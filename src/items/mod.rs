//! Item system — inventory, drop tables, hotbar, and persistence.
//!
//! This module provides the complete lifecycle of collectible items: they are
//! spawned as world entities when voxels break, attracted to the player through
//! a magnet system, inserted into a slot-based inventory, displayed on a hotbar,
//! and finally saved to disk between sessions.
//!
//! # Core abstraction
//!
//! [`ItemType`] is the item counterpart of
//! [`VoxelType`](crate::voxel::VoxelType): implement it for any
//! `Copy + Hash + Default` enum to make that enum usable everywhere in the item
//! pipeline.  The framework ships [`DefaultItem`] as a ready-made implementation
//! covering common mineral pickups.
//!
//! # Module overview
//!
//! | Sub-module | Key types | Responsibility |
//! |---|---|---|
//! | [`drops`] | [`DropTable`], [`DroppedItem`], [`DropsPlugin`] | Spawn item entities on [`crate::world::VoxelBroken`]; magnet + pickup |
//! | [`inventory`] | [`Inventory`], [`InventorySlot`], [`ItemPickedUp`] | Slot-based storage resource |
//! | [`hotbar`] | [`HotbarState`], [`HotbarSelectionChanged`], [`HotbarPlugin`] | Selection state and keyboard/scroll input |
//! | [`persistence`] | [`save_inventory`], [`load_inventory`] | JSON serialisation to/from disk |
//!
//! # Relationship to the voxel system
//!
//! Drops fire in response to [`crate::world::VoxelBroken`] messages emitted by
//! the mining subsystem.  [`DropsPlugin`] subscribes to those messages and
//! consults a [`DropTable`] to decide which items — and how many — to spawn.
//! Because [`DropsPlugin`] is generic over `I: ItemType` it cannot be bundled
//! inside [`crate::VoxelPlugin`]; game code must add it explicitly alongside
//! [`crate::generator::GeneratorPlugin`].
//!
//! # Usage pattern
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use voxel_framework::{VoxelPlugin, prelude::*};
//! use voxel_framework::items::{DefaultItem, DropsPlugin, DropTable, ItemCollector};
//!
//! fn main() {
//!     App::new()
//!         // Core framework (includes HotbarPlugin automatically).
//!         .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
//!         // Drop system — must be added by game code because it is generic.
//!         .add_plugins(DropsPlugin::<DefaultItem>::new())
//!         .add_systems(Startup, setup)
//!         .run();
//! }
//!
//! fn setup(
//!     mut drop_table: ResMut<DropTable<DefaultItem>>,
//!     mut commands: Commands,
//! ) {
//!     // Register which items each voxel type drops when broken.
//!     drop_table.register(1, DefaultItem::RockFragment, 1, 3);
//!     drop_table.register(2, DefaultItem::IronOre, 1, 2);
//!
//!     // Mark the player entity so the magnet system knows where to attract items.
//!     // (In practice the player entity is spawned by `voxel_framework::player::spawn_player`
//!     // and `ItemCollector` is attached to it via a separate startup system.)
//!     commands.spawn(ItemCollector);
//! }
//! ```

pub mod drops;
pub mod hotbar;
pub mod inventory;
pub mod persistence;

pub use drops::*;
pub use hotbar::*;
pub use inventory::*;
pub use persistence::*;
