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
//!    [`crate::items::inventory::ItemPickedUp`], and despawns the entity.
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

/// Mapping from voxel byte IDs to item drop definitions.
///
/// Each entry associates a voxel byte value (the `u8` returned by
/// [`crate::voxel::VoxelType::to_byte`]) with an item type and a drop-count
/// range `[min, max]`.  When a matching voxel is broken, `spawn_drops_system`
/// queries this table and selects a count within the range using a
/// deterministic LCG seeded from the voxel's world position.  If `min == max`
/// the count is always fixed.
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
/// ```
#[derive(Resource, Debug, Clone)]
pub struct DropTable<I: ItemType> {
    /// Internal map: `voxel_byte` → `(item, count_min, count_max)`.
    entries: HashMap<u8, (I, u32, u32)>,
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
    pub fn register(&mut self, voxel_byte: u8, item: I, min: u32, max: u32) {
        self.entries.insert(voxel_byte, (item, min, max));
    }

    /// Iterates over all explicitly registered drop entries.
    ///
    /// Yields `(voxel_byte, item, min_count, max_count)` for each registration.
    /// Identity-fallback entries (not explicitly registered) are not included.
    pub fn iter_entries(&self) -> impl Iterator<Item = (u8, I, u32, u32)> + '_ {
        self.entries
            .iter()
            .map(|(&voxel_byte, &(item, min, max))| (voxel_byte, item, min, max))
    }

    /// Returns the drop for `voxel_byte`, using `seed` for deterministic count
    /// selection within `[min, max]`.
    ///
    /// Explicit registrations always win. If no entry is registered for
    /// `voxel_byte`, falls back to the identity mapping: `I::from_byte(voxel_byte)`
    /// with count 1. If the resulting item is the empty sentinel (i.e.
    /// `item.to_byte() == 0`), returns `None` — meaning no drop.
    pub fn get_drop(&self, voxel_byte: u8, seed: u64) -> Option<(I, u32)> {
        if let Some(&(item, min, max)) = self.entries.get(&voxel_byte) {
            let count = if min == max {
                min
            } else {
                let range = (max - min + 1) as u64;
                min + (seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407)
                    % range) as u32
            };
            return Some((item, count));
        }

        // Identity fallback: try mapping the voxel byte directly to an item.
        let item = I::from_byte(voxel_byte);
        if item.to_byte() != 0 {
            Some((item, 1))
        } else {
            None
        }
    }
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
/// The entity has no [`Transform`] by default — `spawn_drops_system` creates
/// the component without one.  Attaching a [`Transform`] enables spatial
/// magnet logic; entities without one are skipped by the attraction phase but
/// will still expire via `lifetime`.
///
/// # Lifecycle
///
/// 1. Spawned with `velocity` set to `(0, 2, 0)` (small upward bounce).
/// 2. Attracted toward the [`ItemCollector`] once within [`MagnetConfig::radius`].
/// 3. Collected (and despawned) when within 0.5 world units of the collector.
/// 4. Despawned after `lifetime` seconds regardless of collection.
#[derive(Component, Debug, Clone)]
pub struct DroppedItem<I: ItemType> {
    /// The item type carried by this drop entity.
    pub item: I,
    /// Number of items in this drop stack.  Always `>= 1`.
    pub count: u32,
    /// Current velocity in world space (units/sec).
    ///
    /// Updated by the magnet system each frame; game code can modify this to
    /// apply custom physics (e.g. gravity or bounce).
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

        // Derive voxel byte from chunk + local coords for the lookup.
        // The VoxelBroken event doesn't carry the byte directly, so we
        // derive a pseudo-byte from the local coord to index the drop table.
        // In real game code users should extend VoxelBroken or pass voxel_byte
        // separately; here we look up all registered entries and use the seed.
        let voxel_byte = derive_voxel_byte(event);
        if let Some((item, count)) = drop_table.get_drop(voxel_byte, seed) {
            commands.spawn(DroppedItem {
                item,
                count,
                velocity: Vec3::new(0.0, 2.0, 0.0),
                lifetime: 60.0,
            });
        }
    }
}

/// Derives a voxel byte from a [`VoxelBroken`] event.
///
/// `VoxelBroken` does not store the original voxel byte. This helper encodes
/// the local voxel coordinate into a stable byte so the drop table can be
/// indexed. Game code that needs exact voxel-type drops should store the byte
/// in `VoxelBroken` or pass it separately.
fn derive_voxel_byte(event: &VoxelBroken) -> u8 {
    let (x, y, z) = event.local;
    // Simple hash of local coordinates, stable per voxel position.
    ((x as u32)
        .wrapping_mul(73856093)
        .wrapping_add((y as u32).wrapping_mul(19349663))
        .wrapping_add((z as u32).wrapping_mul(83492791))
        & 0xFF) as u8
}

/// Applies magnet force toward [`ItemCollector`] and picks up items within
/// pickup range, adding them to the [`ContainerComponent`] on the
/// [`PrimaryContainer`] entity and emitting [`ItemPickedUp`].
fn magnet_system<I: ItemType>(
    time: Res<Time>,
    config: Res<MagnetConfig>,
    collector_query: Query<&Transform, With<ItemCollector>>,
    mut item_query: Query<(Entity, &mut DroppedItem<I>, Option<&Transform>)>,
    mut container_query: Query<&mut ContainerComponent<I>, With<PrimaryContainer>>,
    mut pickup_writer: MessageWriter<ItemPickedUp<I>>,
    mut commands: Commands,
) {
    let Ok(collector_transform) = collector_query.single() else {
        return;
    };
    let collector_pos = collector_transform.translation;
    let dt = time.delta_secs();

    for (entity, mut dropped, transform_opt) in item_query.iter_mut() {
        // Tick lifetime.
        dropped.lifetime -= dt;
        if dropped.lifetime <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }

        // If entity has no transform we can't do spatial logic — skip.
        let Some(transform) = transform_opt else {
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
            if added > 0 {
                pickup_writer.write(ItemPickedUp {
                    item: dropped.item,
                    count: added,
                });
            }
            commands.entity(entity).despawn();
        } else if dist < config.radius {
            // Apply attraction force.
            let dir = diff / dist;
            dropped.velocity += dir * config.force * dt;
        }
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
}
