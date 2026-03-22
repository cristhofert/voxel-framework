//! Item system — inventory, drop tables, hotbar, and persistence.
//!
//! This module mirrors the voxel type architecture: [`ItemType`] is to items
//! what [`VoxelType`] is to voxels. Implement it for any `Copy + Hash` enum.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use voxel_framework::items::{DefaultItem, Inventory, HotbarPlugin};
//! ```

pub mod drops;
pub mod hotbar;
pub mod inventory;
pub mod persistence;

pub use drops::*;
pub use hotbar::*;
pub use inventory::*;
pub use persistence::*;
