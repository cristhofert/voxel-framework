//! Greedy mesh generation for voxel chunks with LOD and neighbor-aware seam culling.
//!
//! # Greedy meshing
//!
//! Naive voxel renderers emit one quad per visible face, producing O(n³) geometry
//! even for flat surfaces. Greedy meshing collapses adjacent coplanar faces of the
//! same voxel type into a single merged quad, reducing geometry by an order of
//! magnitude for typical terrain.  The algorithm processes each of the six cardinal
//! directions independently: for every slice perpendicular to the current axis it
//! builds a 2-D visibility mask, then scans the mask left-to-right / top-to-bottom
//! extending each run into the largest rectangle of identical voxels before emitting
//! a single quad and clearing the merged region.
//!
//! # Entry points
//!
//! Two public meshing functions are provided:
//!
//! - [`generate_chunk_mesh`] — standalone meshing. Border faces that exit the
//!   chunk boundary are always emitted as visible, which leaves gaps between
//!   adjacent loaded chunks until they are meshed independently.
//! - [`generate_chunk_mesh_with_neighbors`] — seam-free meshing. Border faces are
//!   checked against the corresponding face of each adjacent chunk via
//!   [`ChunkNeighbors`]. A face is culled when the neighboring voxel is solid,
//!   eliminating single-voxel cracks at chunk boundaries.
//!
//! Prefer [`generate_chunk_mesh_with_neighbors`] for in-world rendering.
//! Use [`generate_chunk_mesh`] for isolated previews or when neighbor data is
//! unavailable.
//!
//! # Level of detail (LOD)
//!
//! The `step` parameter controls the LOD resolution.  A step of `1` samples every
//! voxel at full detail.  A step of `2` groups each `2×2×2` block into a single
//! super-voxel (half resolution), halving triangle counts for distant chunks.
//! [`lod::compute_lod`] maps viewer distance to a level; [`lod::lod_to_step`]
//! converts a level to the step value to pass here.
//!
//! | LOD level | Step | Distance threshold     |
//! |-----------|------|------------------------|
//! | `0`       | `1`  | < 200 world units      |
//! | `1`       | `2`  | ≥ 200 world units      |
//!
//! [`lod::compute_lod_with_hysteresis`] adds a ±5-unit hysteresis band around the
//! threshold to prevent rapid LOD oscillation as the viewer moves along the boundary.
//!
//! # Output mesh attributes
//!
//! Every mesh produced by this module contains:
//!
//! | Attribute               | Type          | Description                        |
//! |-------------------------|---------------|------------------------------------|
//! | `ATTRIBUTE_POSITION`    | `Float32x3`   | Vertex positions in chunk-local space |
//! | `ATTRIBUTE_NORMAL`      | `Float32x3`   | Face normals (one of six unit axes) |
//! | `ATTRIBUTE_UV_0`        | `Float32x2`   | UVs scaled by voxel count for tiling |
//! | `ATTRIBUTE_COLOR`       | `Float32x4`   | Per-vertex color from [`VoxelType::color`] |
//! | Indices                 | `U32`         | Triangle list with CCW winding      |
//!
//! [`MeshPlugin<V>`] wires the mesh generation, LOD update, and chunk despawn
//! systems into a Bevy app.

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

/// Bevy [`Plugin`] that registers the mesh generation, LOD update, and chunk
/// despawn systems for a specific voxel type `V`.
///
/// Add this plugin after [`crate::world::WorldPlugin`] and
/// [`crate::generator::GeneratorPlugin`].  It registers three systems in the
/// [`Update`] schedule:
///
/// - [`lod_update_system`] — recomputes LOD levels every frame and emits
///   [`crate::world::ChunkModified`] when a level changes.
/// - [`chunk_mesh_system`] — runs after [`crate::generator::GeneratorSystems`];
///   responds to [`crate::world::ChunkModified`] by (re)building chunk meshes.
/// - [`chunk_despawn_system`] — responds to [`crate::world::ChunkUnloadRequest`]
///   by optionally saving to disk and despawning the entity.
///
/// # Examples
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
    /// Creates a new [`MeshPlugin`] for voxel type `V`.
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

/// Border voxel data from the six cardinal neighbors of a chunk, used for
/// seam-free meshing with [`generate_chunk_mesh_with_neighbors`].
///
/// Each slot holds a flat `CHUNK_SIZE × CHUNK_SIZE` array containing the
/// outermost face of the adjacent chunk that abuts the current chunk on that
/// side.  The array is obtained from `Chunk::border_slice` on the neighbor.
///
/// The face index convention matches [`greedy::FACE_DIRS`]:
///
/// | Index | Direction | Neighbor face stored          |
/// |-------|-----------|-------------------------------|
/// | `0`   | `+Y`      | bottom face of the chunk above |
/// | `1`   | `-Y`      | top face of the chunk below    |
/// | `2`   | `+X`      | left face of the chunk to the right |
/// | `3`   | `-X`      | right face of the chunk to the left |
/// | `4`   | `+Z`      | back face of the chunk in front |
/// | `5`   | `-Z`      | front face of the chunk behind  |
///
/// A slot set to `None` means the neighbor is not loaded; the border face is
/// then treated as open air and the face is emitted unconditionally.
pub struct ChunkNeighbors<V: VoxelType> {
    /// Six border face slices in `[+Y, -Y, +X, -X, +Z, -Z]` order.
    ///
    /// Each `Some` entry is a `CHUNK_SIZE * CHUNK_SIZE` flat array packed in
    /// the same order as `Chunk::border_slice` for that face index.
    pub faces: [Option<[V; CHUNK_SIZE * CHUNK_SIZE]>; 6],
}

impl<V: VoxelType> Default for ChunkNeighbors<V> {
    fn default() -> Self {
        Self {
            faces: [None, None, None, None, None, None],
        }
    }
}
