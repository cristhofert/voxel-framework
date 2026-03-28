//! Rendering support for voxel chunks.
//!
//! Provides either a custom texture-array material ([`ChunkMaterial`]) or a
//! shared `StandardMaterial` for vertex-colored voxels, depending on whether
//! [`ChunkTextureArray`] is inserted before the plugin initialises.

use bevy::asset::embedded_asset;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;

use crate::voxel::VoxelType;

pub mod material;

pub use material::build_texture_array;
pub use material::chunk_aabb;
pub use material::upgrade_to_texture_material;
pub use material::ChunkMaterial;
pub use material::ChunkMaterialConfig;
pub use material::ChunkMaterialHandle;
pub use material::ChunkRendered;
pub use material::ChunkTextureArray;

/// Plugin that sets up shared rendering resources for voxel chunks.
///
/// If [`ChunkTextureArray`] is present as a resource when the plugin builds,
/// chunks will use the embedded WGSL shader with texture array support.
/// Otherwise a shared `StandardMaterial` with vertex colors is used.
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
        // Embed the chunk WGSL shader into the binary.
        // The resulting asset path is: embedded://voxel_framework/render/chunk.wgsl
        embedded_asset!(app, "chunk.wgsl");

        app.add_plugins(MaterialPlugin::<ChunkMaterial>::default());
        app.init_resource::<ChunkMaterialHandle>();
        app.init_resource::<ChunkMaterialConfig>();
        app.add_systems(Startup, material::setup_chunk_material);
        app.add_systems(Update, material::upgrade_to_texture_material);
    }
}
