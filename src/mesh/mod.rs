//! Mesh generation for voxel chunks.
//!
//! This module implements greedy meshing with LOD support and optional
//! neighbor-aware border culling. The two main entry points are:
//!
//! - [`generate_chunk_mesh`] — generates a mesh for a standalone chunk.
//! - [`generate_chunk_mesh_with_neighbors`] — generates a mesh that culls
//!   border faces against adjacent chunk data, eliminating visible seams.
//!
//! [`MeshPlugin<V>`] wires the mesh and despawn systems into a Bevy app.

mod greedy;
mod neighbor;
pub mod lod;
pub mod systems;

pub use greedy::generate_chunk_mesh;
pub use neighbor::generate_chunk_mesh_with_neighbors;
pub use systems::{chunk_mesh_system, chunk_despawn_system, lod_update_system};

use bevy::prelude::*;

use crate::chunk::CHUNK_SIZE;
use crate::generator::GeneratorSystems;
use crate::voxel::VoxelType;

// ─── MeshPlugin ───────────────────────────────────────────────────────────────

/// Plugin that wires mesh generation and chunk despawn systems into the app.
///
/// Add this plugin alongside [`crate::world::WorldPlugin`] and
/// [`crate::generator::GeneratorPlugin`]:
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::{VoxelPlugin, generator::GeneratorPlugin, mesh::MeshPlugin, voxel::DefaultVoxel};
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(VoxelPlugin)
///     .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())
///     .add_plugins(MeshPlugin::<DefaultVoxel>::new())
///     .run();
/// ```
pub struct MeshPlugin<V: VoxelType> {
    _marker: std::marker::PhantomData<V>,
}

impl<V: VoxelType> MeshPlugin<V> {
    /// Creates a new `MeshPlugin`.
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<V: VoxelType> Default for MeshPlugin<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: VoxelType> Plugin for MeshPlugin<V> {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                systems::lod_update_system::<V>,
                systems::chunk_mesh_system::<V>.after(GeneratorSystems),
                systems::chunk_despawn_system::<V>,
            ),
        );
    }
}

/// Border data from the six cardinal neighbors of a chunk.
///
/// Each entry is an optional flat array of `CHUNK_SIZE * CHUNK_SIZE` voxels
/// representing the outermost face of the adjacent chunk that touches this
/// chunk. The face ordering follows the project convention:
///
/// | Index | Direction | Adjacent face |
/// |-------|-----------|---------------|
/// | 0     | +Y        | bottom face of chunk above |
/// | 1     | -Y        | top face of chunk below    |
/// | 2     | +X        | left face of chunk to the right |
/// | 3     | -X        | right face of chunk to the left |
/// | 4     | +Z        | back face of chunk in front |
/// | 5     | -Z        | front face of chunk behind  |
///
/// When a neighbor slot is `None`, the border is treated as open air.
pub struct ChunkNeighbors<V: VoxelType> {
    /// Border faces from adjacent chunks: `[+Y, -Y, +X, -X, +Z, -Z]`.
    pub faces: [Option<[V; CHUNK_SIZE * CHUNK_SIZE]>; 6],
}

impl<V: VoxelType> Default for ChunkNeighbors<V> {
    fn default() -> Self {
        Self {
            faces: [None, None, None, None, None, None],
        }
    }
}
