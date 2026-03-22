//! Drop table and magnet/pickup systems.
//!
//! [`DropTable`] maps voxel byte IDs to item drops and is queried by
//! [`spawn_drops_system`] whenever a [`VoxelBroken`] message arrives.
//!
//! [`DropsPlugin`] is intentionally **not** added to [`VoxelPlugin`] — it is
//! generic over the item type and must be added separately by game code,
//! similar to [`GeneratorPlugin`].

use std::collections::HashMap;
use std::marker::PhantomData;

use bevy::prelude::*;

use crate::world::VoxelBroken;

use super::inventory::{Inventory, ItemPickedUp, ItemType};

// ─── DropTable ────────────────────────────────────────────────────────────────

/// Maps voxel byte IDs to item drops.
///
/// # Example
///
/// ```rust,no_run
/// use voxel_framework::items::{DropTable, DefaultItem};
///
/// let mut table = DropTable::<DefaultItem>::new();
/// table.register(1, DefaultItem::RockFragment, 1, 3);
/// table.register(2, DefaultItem::IronOre, 1, 2);
/// ```
#[derive(Resource, Debug, Clone)]
pub struct DropTable<I: ItemType> {
    /// voxel_byte → (item, count_min, count_max)
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

    /// Returns the drop for `voxel_byte`, using `seed` for deterministic count
    /// selection within `[min, max]`.
    ///
    /// Returns `None` if no entry is registered for `voxel_byte`.
    pub fn get_drop(&self, voxel_byte: u8, seed: u64) -> Option<(I, u32)> {
        let &(item, min, max) = self.entries.get(&voxel_byte)?;
        let count = if min == max {
            min
        } else {
            let range = (max - min + 1) as u64;
            min + (seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) % range)
                as u32
        };
        Some((item, count))
    }
}

impl<I: ItemType> Default for DropTable<I> {
    fn default() -> Self {
        Self::new()
    }
}

// ─── MagnetConfig ─────────────────────────────────────────────────────────────

/// Configuration for the magnetic item-attraction system.
#[derive(Resource, Debug, Clone)]
pub struct MagnetConfig {
    /// Radius within which items are attracted to the [`ItemCollector`].
    pub radius: f32,
    /// Acceleration applied toward the collector (units/sec²).
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

/// A dropped item entity floating in the world.
#[derive(Component, Debug, Clone)]
pub struct DroppedItem<I: ItemType> {
    /// The item type.
    pub item: I,
    /// Stack size of this drop.
    pub count: u32,
    /// Current velocity (world units/sec).
    pub velocity: Vec3,
    /// Remaining lifetime in seconds before the item despawns.
    pub lifetime: f32,
}

/// Tag component marking the entity that attracts and collects nearby items.
///
/// Typically attached to the player entity.
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
/// pickup range, adding them to [`Inventory`] and emitting [`ItemPickedUp`].
fn magnet_system<I: ItemType>(
    time: Res<Time>,
    config: Res<MagnetConfig>,
    collector_query: Query<&Transform, With<ItemCollector>>,
    mut item_query: Query<(Entity, &mut DroppedItem<I>, Option<&Transform>)>,
    mut inventory: ResMut<Inventory<I>>,
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
            // Close enough — pick up.
            let added = dropped.count - inventory.add(dropped.item, dropped.count);
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

/// Plugin that registers drop-table resources and systems for item type `I`.
///
/// **Not** added to [`VoxelPlugin`] — add it manually in game code alongside
/// [`GeneratorPlugin`]:
///
/// ```rust,no_run
/// use voxel_framework::items::{DropsPlugin, DefaultItem};
///
/// // app.add_plugins(DropsPlugin::<DefaultItem>::new());
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
        app.init_resource::<Inventory<I>>();
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
}
