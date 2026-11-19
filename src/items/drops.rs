//! Drop table, world-entity lifecycle, and magnet pickup systems.
//!
//! When a voxel is destroyed the mining subsystem emits a
//! [`crate::world::VoxelBroken`] message.  This module listens to that message
//! and drives the full item-drop pipeline:
//!
//! 1. **Spawn** — `spawn_drops_system` consults [`DropTable`] and spawns a
//!    [`DroppedItem`] entity at the broken voxel's world position.
//! 2. **Attract** — `magnet_system` applies a configurable acceleration toward
//!    any entity tagged with [`ItemCollector`] once the item is within
//!    [`MagnetConfig::radius`] world units.
//! 3. **Collect** — when the item reaches within 0.5 units of the collector,
//!    `magnet_system` calls [`crate::items::inventory::Inventory::add`], emits
//!    [`crate::items::inventory::ItemPickedUp`] only for accepted items, and
//!    retains the unaccepted remainder. It despawns the entity only when empty.
//!    A full inventory leaves the drop available and emits no pickup message.
//! 4. **Expire** — items that are never collected despawn automatically after
//!    their [`DroppedItem::lifetime`] reaches zero (default 60 seconds).
//!
//! # Plugin registration
//!
//! [`DropsPlugin`] is intentionally **not** bundled inside
//! [`crate::VoxelPlugin`] because it is generic over `I: `[`ItemType`].
//! Add it explicitly in game code, alongside
//! [`crate::generator::GeneratorPlugin`]:
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use voxel_framework::items::{DefaultItem, DropsPlugin};
//!
//! App::new().add_plugins(DropsPlugin::<DefaultItem>::new());
//! ```

use std::collections::HashMap;
use std::marker::PhantomData;

use bevy::prelude::*;

use crate::world::VoxelBroken;

use super::inventory::{ContainerComponent, ItemPickedUp, ItemType, PrimaryContainer};

// ─── DropTable ────────────────────────────────────────────────────────────────

/// One runtime drop entry for a voxel byte.
///
/// This is the runtime counterpart of
/// [`crate::content::BlockDropDefinition`]. It is intentionally keyed by voxel
/// byte because [`DropsPlugin`] reacts to [`crate::world::VoxelBroken`] events,
/// and those events carry the destroyed voxel byte.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DropTableEntry<I: ItemType> {
    /// Item produced by this entry.
    pub item: I,
    /// Minimum count, inclusive.
    pub min_count: u32,
    /// Maximum count, inclusive.
    pub max_count: u32,
    /// Probability in `0.0..=1.0`.
    pub chance: f32,
}

impl<I: ItemType> DropTableEntry<I> {
    /// Creates a guaranteed drop entry.
    pub fn new(item: I, min_count: u32, max_count: u32) -> Self {
        Self::with_chance(item, min_count, max_count, 1.0)
    }

    /// Creates a drop entry with an explicit probability.
    pub fn with_chance(item: I, min_count: u32, max_count: u32, chance: f32) -> Self {
        Self {
            item,
            min_count: min_count.min(max_count),
            max_count: min_count.max(max_count),
            chance: chance.clamp(0.0, 1.0),
        }
    }
}

/// Mapping from voxel byte IDs to item drop definitions.
///
/// Each entry associates a voxel byte value (the `u8` returned by
/// [`crate::voxel::VoxelType::to_byte`]) with an item type and a drop-count
/// range `[min, max]`. Runtime entries may also carry a chance, and a voxel may
/// have multiple entries. When a matching voxel is broken,
/// `spawn_drops_system` queries this table and selects counts/chance outcomes
/// using deterministic pseudo-random values seeded from the voxel's world
/// position. If `min == max` the count is always fixed.
///
/// [`DropTable`] is initialized empty; register entries before the first
/// [`crate::world::VoxelBroken`] message is processed.  It is available as a
/// Bevy [`Resource`] after [`DropsPlugin`] is added.
///
/// # Examples
///
/// ```rust,no_run
/// use voxel_framework::items::{DropTable, DefaultItem};
///
/// let mut table = DropTable::<DefaultItem>::new();
/// // Voxel byte 1 drops 1–3 rock fragments.
/// table.register(1, DefaultItem::RockFragment, 1, 3);
/// // Voxel byte 2 always drops exactly 1 iron ore.
/// table.register(2, DefaultItem::IronOre, 1, 1);
/// // Voxel byte 3 can produce multiple runtime entries.
/// table.add_entry(3, voxel_framework::items::DropTableEntry::new(DefaultItem::Cobblestone, 1, 1));
/// table.add_entry(3, voxel_framework::items::DropTableEntry::with_chance(DefaultItem::Coal, 1, 2, 0.25));
/// ```
#[derive(Resource, Debug, Clone)]
pub struct DropTable<I: ItemType> {
    /// Internal map: `voxel_byte` → runtime drop entries.
    entries: HashMap<u8, Vec<DropTableEntry<I>>>,
}

impl<I: ItemType> DropTable<I> {
    /// Creates an empty drop table.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Registers a drop for the given voxel byte.
    ///
    /// When a voxel with `voxel_byte` is broken, between `min` and `max` items
    /// (inclusive) of `item` will be dropped. If `min == max` the count is
    /// always fixed.
    ///
    /// This replaces any previous runtime entries for the voxel byte, preserving
    /// the historical one-entry behavior of [`DropTable::register`]. Use
    /// [`DropTable::add_entry`] when a voxel should produce multiple drops.
    pub fn register(&mut self, voxel_byte: u8, item: I, min: u32, max: u32) {
        self.replace_entries(voxel_byte, [DropTableEntry::new(item, min, max)]);
    }

    /// Registers a single probabilistic drop for the given voxel byte.
    ///
    /// Like [`DropTable::register`], this replaces any previous entries for the
    /// voxel byte.
    pub fn register_with_chance(
        &mut self,
        voxel_byte: u8,
        item: I,
        min: u32,
        max: u32,
        chance: f32,
    ) {
        self.replace_entries(
            voxel_byte,
            [DropTableEntry::with_chance(item, min, max, chance)],
        );
    }

    /// Appends one runtime entry without removing existing entries for the same
    /// voxel byte.
    pub fn add_entry(&mut self, voxel_byte: u8, entry: DropTableEntry<I>) {
        self.entries.entry(voxel_byte).or_default().push(entry);
    }

    /// Replaces all runtime entries for one voxel byte.
    pub fn replace_entries(
        &mut self,
        voxel_byte: u8,
        entries: impl IntoIterator<Item = DropTableEntry<I>>,
    ) {
        self.entries
            .insert(voxel_byte, entries.into_iter().collect());
    }

    /// Iterates over all explicitly registered drop entries.
    ///
    /// Yields `(voxel_byte, item, min_count, max_count)` for each registration.
    /// Identity-fallback entries (not explicitly registered) are not included.
    pub fn iter_entries(&self) -> impl Iterator<Item = (u8, I, u32, u32)> + '_ {
        self.iter_drop_entries()
            .map(|(voxel_byte, entry)| (voxel_byte, entry.item, entry.min_count, entry.max_count))
    }

    /// Iterates over all explicitly registered runtime drop entries, including
    /// chance metadata.
    pub fn iter_drop_entries(&self) -> impl Iterator<Item = (u8, &DropTableEntry<I>)> + '_ {
        self.entries
            .iter()
            .flat_map(|(&voxel_byte, entries)| entries.iter().map(move |entry| (voxel_byte, entry)))
    }

    /// Returns the drop for `voxel_byte`, using `seed` for deterministic count
    /// selection within `[min, max]`.
    ///
    /// Explicit registrations always win. If no entry is registered for
    /// `voxel_byte`, falls back to the identity mapping: `I::from_byte(voxel_byte)`
    /// with count 1. If the resulting item is the empty sentinel (i.e.
    /// `item.to_byte() == 0`), returns `None` — meaning no drop.
    pub fn get_drop(&self, voxel_byte: u8, seed: u64) -> Option<(I, u32)> {
        self.get_drops(voxel_byte, seed).into_iter().next()
    }

    /// Returns all drops produced for `voxel_byte` with deterministic chance
    /// rolls and count selection.
    pub fn get_drops(&self, voxel_byte: u8, seed: u64) -> Vec<(I, u32)> {
        if let Some(entries) = self.entries.get(&voxel_byte) {
            return entries
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| entry.roll(seed, index as u64))
                .collect();
        }

        // Identity fallback: try mapping the voxel byte directly to an item.
        let item = I::from_byte(voxel_byte);
        if !item.is_empty() {
            vec![(item, 1)]
        } else {
            Vec::new()
        }
    }
}

impl<I: ItemType> DropTableEntry<I> {
    fn roll(&self, seed: u64, salt: u64) -> Option<(I, u32)> {
        if self.item.is_empty() || self.max_count == 0 || self.chance <= 0.0 {
            return None;
        }

        let chance_roll = unit_interval(seed, salt);
        if chance_roll > self.chance {
            return None;
        }

        let count = if self.min_count == self.max_count {
            self.min_count
        } else {
            let range = (self.max_count - self.min_count + 1) as u64;
            self.min_count + (lcg(seed, salt.wrapping_add(1)) % range) as u32
        };

        (count > 0).then_some((self.item, count))
    }
}

fn lcg(seed: u64, salt: u64) -> u64 {
    seed.wrapping_add(salt.wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407)
}

fn unit_interval(seed: u64, salt: u64) -> f32 {
    let value = lcg(seed, salt) >> 40;
    value as f32 / ((1u64 << 24) - 1) as f32
}

impl<I: ItemType> Default for DropTable<I> {
    fn default() -> Self {
        Self::new()
    }
}

// ─── MagnetConfig ─────────────────────────────────────────────────────────────

/// Configuration for the magnetic item-attraction system.
///
/// This resource is inserted by [`DropsPlugin`] with [`Default`] values and
/// can be overwritten after plugin registration to tune pickup feel.
///
/// # Examples
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::items::MagnetConfig;
///
/// fn setup(mut config: ResMut<MagnetConfig>) {
///     config.radius = 8.0;
///     config.force  = 16.0;
/// }
/// ```
#[derive(Resource, Debug, Clone)]
pub struct MagnetConfig {
    /// Distance (in world units) within which [`DroppedItem`] entities are
    /// attracted toward the [`ItemCollector`].
    ///
    /// Items outside this radius are neither attracted nor collected.
    /// Default: `4.0`.
    pub radius: f32,
    /// Acceleration magnitude applied toward the collector each second
    /// (world units/sec²).
    ///
    /// Higher values make items snap to the player faster.  Default: `8.0`.
    pub force: f32,
}

impl Default for MagnetConfig {
    fn default() -> Self {
        Self {
            radius: 4.0,
            force: 8.0,
        }
    }
}

// ─── Components ───────────────────────────────────────────────────────────────

/// Component on a world entity representing an uncollected item drop.
///
/// `DroppedItem` entities are spawned by `spawn_drops_system` in response to
/// [`crate::world::VoxelBroken`] messages and despawned by `magnet_system`
/// when either collected or expired.
///
/// The plugin spawns each drop with a [`Transform`] at the broken voxel's
/// world-space center, ready for spatial magnet logic. Manually spawned drops
/// must also provide a [`Transform`] to participate in spatial attraction.
///
/// # Lifecycle
///
/// 1. Spawned with `velocity` set to `(0, 2, 0)` (small upward bounce).
/// 2. Attracted toward the [`ItemCollector`] once within [`MagnetConfig::radius`].
/// 3. Collected when within 0.5 world units of the collector. A full inventory
///    that accepts no items leaves the drop available until pickup or expiry.
///    Partial pickups leave the unaccepted remainder in the world.
/// 4. Despawned after `lifetime` seconds regardless of collection.
#[derive(Component, Debug, Clone)]
pub struct DroppedItem<I: ItemType> {
    /// The item type carried by this drop entity.
    pub item: I,
    /// Number of items in this drop stack.  Always `>= 1`.
    pub count: u32,
    /// Current velocity in world space (units/sec).
    ///
    /// The magnet system accelerates drops inside the attraction radius and
    /// advances their position by velocity times elapsed time while a collector
    /// is available. Outside the radius, existing velocity is preserved.
    /// Game code can modify velocity for custom gravity or bounce; it should
    /// not integrate the same movement a second time.
    pub velocity: Vec3,
    /// Seconds remaining before the entity is automatically despawned.
    ///
    /// Decremented each frame.  Default: `60.0`.
    pub lifetime: f32,
}

/// Tag component marking the entity that attracts and collects nearby [`DroppedItem`] entities.
///
/// Add this component to the player entity after spawning it.  Only one
/// collector entity is supported; `magnet_system` uses
/// [`bevy::ecs::query::QuerySingleError`] and silently skips collection if
/// zero or more than one entity carries this tag.
///
/// # Examples
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::items::ItemCollector;
///
/// fn spawn_player(mut commands: Commands) {
///     commands.spawn((Transform::default(), ItemCollector));
/// }
/// ```
#[derive(Component, Default)]
pub struct ItemCollector;

// ─── Systems ─────────────────────────────────────────────────────────────────

/// Reads [`VoxelBroken`] messages and spawns [`DroppedItem`] entities using
/// [`DropTable`].
fn spawn_drops_system<I: ItemType>(
    mut broken_reader: MessageReader<VoxelBroken>,
    drop_table: Res<DropTable<I>>,
    mut commands: Commands,
) {
    for event in broken_reader.read() {
        // Use world position bits as a deterministic seed.
        let seed = {
            let x = event.world_pos.x.to_bits() as u64;
            let y = event.world_pos.y.to_bits() as u64;
            let z = event.world_pos.z.to_bits() as u64;
            x.wrapping_mul(1000003)
                .wrapping_add(y.wrapping_mul(1000033))
                .wrapping_add(z)
        };

        for (item, count) in drop_table.get_drops(event.voxel_byte, seed) {
            commands.spawn((
                DroppedItem {
                    item,
                    count,
                    velocity: Vec3::new(0.0, 2.0, 0.0),
                    lifetime: 60.0,
                },
                Transform::from_translation(event.world_pos),
            ));
        }
    }
}

/// Applies magnet force toward [`ItemCollector`] and picks up items within
/// pickup range, adding them to the [`ContainerComponent`] on the
/// [`PrimaryContainer`] entity and emitting [`ItemPickedUp`].
fn magnet_system<I: ItemType>(
    time: Res<Time>,
    config: Res<MagnetConfig>,
    collector_query: Query<&Transform, (With<ItemCollector>, Without<DroppedItem<I>>)>,
    mut item_query: Query<(Entity, &mut DroppedItem<I>, Option<&mut Transform>)>,
    mut container_query: Query<&mut ContainerComponent<I>, With<PrimaryContainer>>,
    mut pickup_writer: MessageWriter<ItemPickedUp<I>>,
    mut commands: Commands,
) {
    let collector_pos = collector_query
        .single()
        .ok()
        .map(|transform| transform.translation);
    let dt = time.delta_secs();

    for (entity, mut dropped, transform_opt) in item_query.iter_mut() {
        // Tick lifetime.
        dropped.lifetime -= dt;
        if dropped.lifetime <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }

        // Expiration is independent of the single-collector spatial contract.
        let Some(collector_pos) = collector_pos else {
            continue;
        };

        // If entity has no transform we can't do spatial logic — skip.
        let Some(mut transform) = transform_opt else {
            continue;
        };

        let item_pos = transform.translation;
        let diff = collector_pos - item_pos;
        let dist = diff.length();

        if dist < 0.5 {
            // Close enough — pick up via PrimaryContainer entity.
            let Ok(mut container) = container_query.single_mut() else {
                continue;
            };
            let added = dropped.count - container.0.add(dropped.item, dropped.count);
            if added == 0 {
                continue;
            }
            dropped.count -= added;
            pickup_writer.write(ItemPickedUp {
                item: dropped.item,
                count: added,
            });
            if dropped.count == 0 {
                commands.entity(entity).despawn();
            }
            continue;
        } else if dist < config.radius {
            // Apply attraction force.
            let dir = diff / dist;
            dropped.velocity += dir * config.force * dt;
        }
        transform.translation += dropped.velocity * dt;
    }
}

// ─── DropsPlugin ─────────────────────────────────────────────────────────────

/// Plugin that wires up the full item-drop pipeline for item type `I`.
///
/// Registering this plugin inserts the following resources (with defaults) and
/// systems into the Bevy [`App`]:
///
/// | Resource | Purpose |
/// |---|---|
/// | [`DropTable<I>`] | Empty table; populate it in a `Startup` system |
/// | [`MagnetConfig`] | Tunable attraction radius and force |
/// | [`crate::items::inventory::Inventory<I>`] | Player's item storage |
///
/// It also registers the [`crate::items::inventory::ItemPickedUp<I>`] message
/// type and adds `spawn_drops_system` and `magnet_system` to the `Update`
/// schedule.
///
/// **This plugin is NOT included in [`crate::VoxelPlugin`]** — because it is
/// generic over `I: `[`ItemType`], it must be added separately by game code.
/// Add it alongside [`crate::generator::GeneratorPlugin`]:
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::items::{DefaultItem, DropsPlugin};
///
/// App::new().add_plugins(DropsPlugin::<DefaultItem>::new());
/// ```
pub struct DropsPlugin<I: ItemType> {
    _marker: PhantomData<I>,
}

impl<I: ItemType> DropsPlugin<I> {
    /// Creates a new `DropsPlugin`.
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<I: ItemType> Default for DropsPlugin<I> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: ItemType> Plugin for DropsPlugin<I> {
    fn build(&self, app: &mut App) {
        app.init_resource::<DropTable<I>>();
        app.init_resource::<MagnetConfig>();
        app.add_message::<ItemPickedUp<I>>();
        app.add_systems(Update, spawn_drops_system::<I>);
        app.add_systems(Update, magnet_system::<I>);
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::DefaultItem;

    #[test]
    fn drops_expire_without_a_unique_collector_or_transform() {
        for collectors in [0, 2] {
            let mut app = App::new();
            app.init_resource::<Time>()
                .add_message::<VoxelBroken>()
                .add_plugins(DropsPlugin::<DefaultItem>::new());
            for _ in 0..collectors {
                app.world_mut().spawn((ItemCollector, Transform::default()));
            }
            let mut entities = Vec::new();
            for positioned in [false, true] {
                let mut entity = app.world_mut().spawn(DroppedItem {
                    item: DefaultItem::RockFragment,
                    count: 3,
                    velocity: Vec3::ZERO,
                    lifetime: 0.2,
                });
                if positioned {
                    entity.insert(Transform::default());
                }
                entities.push(entity.id());
            }
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_millis(100));
            app.update();
            for &entity in &entities {
                let drop = app.world().get::<DroppedItem<DefaultItem>>(entity).unwrap();
                assert!((drop.lifetime - 0.1).abs() < 0.0001);
                assert_eq!(drop.count, 3);
            }
            app.update();
            for entity in entities {
                assert!(app.world().get_entity(entity).is_err());
            }
            assert!(
                app.world()
                    .resource::<Messages<ItemPickedUp<DefaultItem>>>()
                    .is_empty()
            );
        }
    }

    #[test]
    fn broken_voxel_spawns_one_mapped_drop_at_world_position() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .add_message::<VoxelBroken>()
            .add_plugins(DropsPlugin::<DefaultItem>::new());
        app.world_mut()
            .resource_mut::<DropTable<DefaultItem>>()
            .register(1, DefaultItem::RockFragment, 1, 1);
        let position = Vec3::new(18.5, -12.5, 36.5);
        app.world_mut().write_message(VoxelBroken {
            chunk_coord: IVec3::new(1, -1, 2),
            local: (2, 3, 4),
            world_pos: position,
            voxel_byte: 1,
        });

        app.update();

        let world = app.world_mut();
        let mut query = world.query::<(&DroppedItem<DefaultItem>, Option<&Transform>)>();
        let drops: Vec<_> = query.iter(world).collect();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].0.item, DefaultItem::RockFragment);
        assert_eq!(drops[0].0.count, 1);
        assert_eq!(
            drops[0]
                .1
                .expect("spawned drop must have a world position")
                .translation,
            position
        );
    }

    #[test]
    fn nearby_drop_moves_and_is_collected_once_without_attracting_distant_drop() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .add_message::<VoxelBroken>()
            .add_plugins(DropsPlugin::<DefaultItem>::new());
        let player = app
            .world_mut()
            .spawn((
                Transform::default(),
                ItemCollector,
                PrimaryContainer,
                ContainerComponent::<DefaultItem>::new(9),
            ))
            .id();
        let spawn_drop = |world: &mut World, position| {
            world
                .spawn((
                    DroppedItem {
                        item: DefaultItem::RockFragment,
                        count: 1,
                        velocity: Vec3::ZERO,
                        lifetime: 60.0,
                    },
                    Transform::from_translation(position),
                ))
                .id()
        };
        let nearby = spawn_drop(app.world_mut(), Vec3::new(2.0, 0.0, 0.0));
        let distant_position = Vec3::new(8.0, 0.0, 0.0);
        let distant = spawn_drop(app.world_mut(), distant_position);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(100));

        app.update();
        assert!(
            app.world()
                .get::<Transform>(nearby)
                .unwrap()
                .translation
                .length()
                < 2.0,
            "nearby drop must move toward the collector"
        );

        let mut pickups = Vec::new();
        for _ in 0..30 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_millis(100));
            app.update();
            pickups.extend(
                app.world_mut()
                    .resource_mut::<Messages<ItemPickedUp<DefaultItem>>>()
                    .drain(),
            );
        }
        assert!(app.world().get_entity(nearby).is_err());
        assert_eq!(
            app.world()
                .get::<ContainerComponent<DefaultItem>>(player)
                .unwrap()
                .0
                .count_of(DefaultItem::RockFragment),
            1
        );
        assert_eq!(pickups.len(), 1);
        assert_eq!(pickups[0].item, DefaultItem::RockFragment);
        assert_eq!(pickups[0].count, 1);
        assert_eq!(
            app.world().get::<Transform>(distant).unwrap().translation,
            distant_position
        );
        assert_eq!(
            app.world()
                .get::<DroppedItem<DefaultItem>>(distant)
                .unwrap()
                .velocity,
            Vec3::ZERO
        );
    }

    #[test]
    fn partial_pickup_retains_remainder_until_inventory_has_room() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .add_message::<VoxelBroken>()
            .add_plugins(DropsPlugin::<DefaultItem>::new());
        let item = DefaultItem::RockFragment;
        let mut container = ContainerComponent::<DefaultItem>::new(1);
        // Published inventories use u32 aggregate quantities: leave room for one.
        assert_eq!(container.0.add(item, u32::MAX - 1), 0);
        let player = app
            .world_mut()
            .spawn((
                Transform::default(),
                ItemCollector,
                PrimaryContainer,
                container,
            ))
            .id();
        let drop = app
            .world_mut()
            .spawn((
                DroppedItem {
                    item,
                    count: 3,
                    velocity: Vec3::ZERO,
                    lifetime: 60.0,
                },
                Transform::from_xyz(0.25, 0.0, 0.0),
            ))
            .id();

        let mut total_picked_up = 0;
        for (expected_pickup, expected_remainder) in [(1, 2), (0, 2), (2, 0), (0, 0), (0, 0)] {
            if expected_pickup == 2 {
                assert_eq!(
                    app.world_mut()
                        .get_mut::<ContainerComponent<DefaultItem>>(player)
                        .unwrap()
                        .0
                        .remove(item, 2),
                    2
                );
            }
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_millis(100));
            app.update();
            assert_eq!(
                app.world()
                    .get::<ContainerComponent<DefaultItem>>(player)
                    .unwrap()
                    .0
                    .count_of(item),
                u32::MAX
            );
            if expected_remainder == 0 {
                assert!(app.world().get_entity(drop).is_err());
            } else {
                let retained = app
                    .world()
                    .get::<DroppedItem<DefaultItem>>(drop)
                    .expect("partial pickup must preserve the remainder");
                assert_eq!(retained.item, item);
                assert_eq!(retained.count, expected_remainder);
            }
            let pickups: Vec<_> = app
                .world_mut()
                .resource_mut::<Messages<ItemPickedUp<DefaultItem>>>()
                .drain()
                .collect();
            if expected_pickup == 0 {
                assert!(pickups.is_empty());
            } else {
                assert_eq!(pickups.len(), 1);
                assert_eq!(pickups[0].item, item);
                assert_eq!(pickups[0].count, expected_pickup);
                total_picked_up += pickups[0].count;
            }
            assert_eq!(
                total_picked_up + expected_remainder,
                3,
                "no units lost or duplicated"
            );
        }
        assert_eq!(total_picked_up, 3);
    }

    #[test]
    fn full_inventory_retains_drop_without_reporting_pickup() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .add_message::<VoxelBroken>()
            .add_plugins(DropsPlugin::<DefaultItem>::new());
        let mut container = ContainerComponent::<DefaultItem>::new(1);
        assert_eq!(container.0.add(DefaultItem::IronOre, 1), 0);
        let player = app
            .world_mut()
            .spawn((
                Transform::default(),
                ItemCollector,
                PrimaryContainer,
                container,
            ))
            .id();
        let drop = app
            .world_mut()
            .spawn((
                DroppedItem {
                    item: DefaultItem::RockFragment,
                    count: 3,
                    velocity: Vec3::ZERO,
                    lifetime: 60.0,
                },
                Transform::from_xyz(0.25, 0.0, 0.0),
            ))
            .id();

        for _ in 0..3 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_millis(100));
            app.update();
            let retained = app
                .world()
                .get::<DroppedItem<DefaultItem>>(drop)
                .expect("full inventory must retain the unaccepted drop");
            assert_eq!(retained.item, DefaultItem::RockFragment);
            assert_eq!(retained.count, 3);
            let inventory = &app
                .world()
                .get::<ContainerComponent<DefaultItem>>(player)
                .unwrap()
                .0;
            assert_eq!(inventory.count_of(DefaultItem::IronOre), 1);
            assert_eq!(inventory.count_of(DefaultItem::RockFragment), 0);
            assert_eq!(
                app.world_mut()
                    .resource_mut::<Messages<ItemPickedUp<DefaultItem>>>()
                    .drain()
                    .count(),
                0
            );
        }

        assert_eq!(
            app.world_mut()
                .get_mut::<ContainerComponent<DefaultItem>>(player)
                .unwrap()
                .0
                .remove(DefaultItem::IronOre, 1),
            1
        );
        for expected_messages in [1, 0, 0] {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_millis(100));
            app.update();
            assert!(app.world().get_entity(drop).is_err());
            let inventory = &app
                .world()
                .get::<ContainerComponent<DefaultItem>>(player)
                .unwrap()
                .0;
            assert_eq!(inventory.count_of(DefaultItem::RockFragment), 3);
            assert_eq!(inventory.count_of(DefaultItem::IronOre), 0);
            let pickups: Vec<_> = app
                .world_mut()
                .resource_mut::<Messages<ItemPickedUp<DefaultItem>>>()
                .drain()
                .collect();
            assert_eq!(pickups.len(), expected_messages);
            if let Some(pickup) = pickups.first() {
                assert_eq!(pickup.item, DefaultItem::RockFragment);
                assert_eq!(pickup.count, 3);
            }
        }
    }

    #[test]
    fn test_drop_table_get_drop() {
        let mut table = DropTable::<DefaultItem>::new();
        table.register(1, DefaultItem::RockFragment, 1, 3);

        // With seed 0: deterministic count should be in [1, 3].
        let result = table.get_drop(1, 0);
        assert!(result.is_some());
        let (item, count) = result.unwrap();
        assert_eq!(item, DefaultItem::RockFragment);
        assert!((1..=3).contains(&count));
    }

    #[test]
    fn test_drop_table_no_entry_returns_none() {
        let table = DropTable::<DefaultItem>::new();
        assert!(table.get_drop(99, 42).is_none());
    }

    #[test]
    fn test_drop_table_fixed_count() {
        let mut table = DropTable::<DefaultItem>::new();
        table.register(2, DefaultItem::IronOre, 5, 5);
        let (_, count) = table.get_drop(2, 12345).unwrap();
        assert_eq!(count, 5);
    }

    #[test]
    fn test_drop_table_identity_fallback_with_other_entries() {
        let mut table = DropTable::<DefaultItem>::new();
        table.register(2, DefaultItem::Dirt, 1, 1);
        let (item, count) = table.get_drop(11, 0).unwrap();
        assert_eq!(item, DefaultItem::IronOre);
        assert_eq!(count, 1);
    }

    #[test]
    fn drop_table_supports_multiple_runtime_entries() {
        let mut table = DropTable::<DefaultItem>::new();
        table.add_entry(3, DropTableEntry::new(DefaultItem::Cobblestone, 1, 1));
        table.add_entry(3, DropTableEntry::new(DefaultItem::Coal, 2, 2));

        let drops = table.get_drops(3, 123);
        assert_eq!(drops.len(), 2);
        assert_eq!(drops[0], (DefaultItem::Cobblestone, 1));
        assert_eq!(drops[1], (DefaultItem::Coal, 2));
    }

    #[test]
    fn register_preserves_single_entry_replacement_behavior() {
        let mut table = DropTable::<DefaultItem>::new();
        table.add_entry(3, DropTableEntry::new(DefaultItem::Cobblestone, 1, 1));
        table.add_entry(3, DropTableEntry::new(DefaultItem::Coal, 1, 1));

        table.register(3, DefaultItem::IronOre, 1, 1);

        let drops = table.get_drops(3, 0);
        assert_eq!(drops, vec![(DefaultItem::IronOre, 1)]);
    }

    #[test]
    fn drop_table_entry_chance_can_disable_a_drop() {
        let mut table = DropTable::<DefaultItem>::new();
        table.add_entry(
            3,
            DropTableEntry::with_chance(DefaultItem::Cobblestone, 1, 1, 0.0),
        );
        table.add_entry(3, DropTableEntry::with_chance(DefaultItem::Coal, 1, 1, 1.0));

        let drops = table.get_drops(3, 0);
        assert_eq!(drops, vec![(DefaultItem::Coal, 1)]);
    }

    #[test]
    fn iter_drop_entries_includes_chance_metadata() {
        let mut table = DropTable::<DefaultItem>::new();
        table.register_with_chance(3, DefaultItem::Cobblestone, 1, 2, 0.25);

        let entries: Vec<_> = table.iter_drop_entries().collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, 3);
        assert_eq!(entries[0].1.item, DefaultItem::Cobblestone);
        assert_eq!(entries[0].1.min_count, 1);
        assert_eq!(entries[0].1.max_count, 2);
        assert_eq!(entries[0].1.chance, 0.25);
    }
}
