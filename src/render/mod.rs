//! Rendering support for voxel chunks.
//!
//! Provides a shared `StandardMaterial` optimized for vertex-colored voxels
//! and marker components for tracking render state on chunk entities.

use bevy::prelude::*;

use crate::voxel::VoxelType;

pub mod material;

pub use material::ChunkMaterial;
pub use material::ChunkMaterialConfig;
pub use material::ChunkRendered;

/// Plugin that sets up shared rendering resources for voxel chunks.
///
/// Creates a shared `StandardMaterial` optimized for vertex-colored voxels
/// and provides systems to attach render components to chunk entities.
pub struct RenderPlugin<V: VoxelType> {
    _marker: std::marker::PhantomData<V>,
}

impl<V: VoxelType> RenderPlugin<V> {
    /// Creates a new `RenderPlugin`.
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<V: VoxelType> Default for RenderPlugin<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: VoxelType> Plugin for RenderPlugin<V> {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMaterial>();
        app.init_resource::<ChunkMaterialConfig>();
        app.add_systems(Startup, material::setup_chunk_material);
    }
}
