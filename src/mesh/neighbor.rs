//! Neighbor-aware greedy meshing with cross-chunk border culling.
//!
//! The standalone mesher in [`super::greedy`] always emits border faces as
//! visible, which produces single-voxel cracks between adjacent loaded chunks.
//! This module plugs a custom `face_visible` callback into [`super::greedy::build_mesh`]
//! that consults [`super::ChunkNeighbors`] for voxels that cross chunk boundaries.
//!
//! # Culling strategy
//!
//! For each candidate face the callback checks whether the neighbor cell falls
//! inside or outside the current chunk:
//!
//! - **Interior** — the neighbor is within bounds; the standard solid-check
//!   (`!super_voxel.is_solid()`) is used, identical to the standalone mesher.
//! - **Border** — the neighbor is outside bounds; the corresponding slot in
//!   [`super::ChunkNeighbors::faces`] is consulted.  If the slot is `None`
//!   (neighbor not loaded), the face is treated as visible.  If the slot is
//!   `Some`, the voxel at the matching `(u, v)` position in the border slice
//!   is checked; the face is culled when that voxel is solid.
//!
//! This approach requires no changes to the greedy merge loop itself — only
//! the face-visibility predicate changes, making it zero-cost for interior faces.

use bevy::mesh::Mesh;

use crate::chunk::{Chunk, CHUNK_SIZE};
use crate::voxel::VoxelType;

use super::ChunkNeighbors;
use super::greedy::{build_mesh, super_voxel, FaceDir};

/// Generates a seam-free [`Mesh`] for a chunk by culling border faces against
/// adjacent chunk data supplied in `neighbors`.
///
/// This is the preferred meshing entry point for in-world rendering.  It
/// produces the same greedy-merged geometry as [`super::generate_chunk_mesh`]
/// but eliminates single-voxel gaps at chunk boundaries by consulting
/// [`ChunkNeighbors`] before emitting any border face.
///
/// # Parameters
///
/// - `chunk` — The [`Chunk<V>`] to mesh.
/// - `neighbors` — Border voxel data for the six adjacent chunks.  Slots set
///   to `None` are treated as open air; the corresponding border faces are
///   always emitted.  Build this from [`Chunk::border_slice`] on each loaded
///   neighbor or use [`ChunkNeighbors::default`] to treat all borders as air.
/// - `step` — LOD step size.  `1` = full detail; `2` = half detail.  Use
///   [`super::lod::lod_to_step`] to derive this from a LOD level.
///
/// # Returns
///
/// See [`super::generate_chunk_mesh`] for the full attribute list.  The mesh
/// is otherwise identical; only face culling at chunk borders differs.
///
/// # Examples
///
/// ```rust,no_run
/// use voxel_framework::{
///     chunk::Chunk,
///     mesh::{ChunkNeighbors, generate_chunk_mesh_with_neighbors},
///     voxel::DefaultVoxel,
/// };
///
/// let chunk = Chunk::<DefaultVoxel>::new();
/// // All neighbors absent — borders treated as open air.
/// let neighbors = ChunkNeighbors::<DefaultVoxel>::default();
/// let mesh = generate_chunk_mesh_with_neighbors(&chunk, &neighbors, 1);
/// ```
pub fn generate_chunk_mesh_with_neighbors<V: VoxelType>(
    chunk: &Chunk<V>,
    neighbors: &ChunkNeighbors<V>,
    step: usize,
) -> Mesh {
    crate::profile_span!("voxel::mesh::neighbor");
    let face_visible =
        |chunk: &Chunk<V>, x: usize, y: usize, z: usize, face: &FaceDir, step: usize| -> bool {
            is_face_visible(chunk, neighbors, x, y, z, face, step)
        };
    build_mesh(chunk, step, &face_visible)
}

/// Determines whether a face is visible, checking neighbor data at borders.
///
/// For interior voxels, this is equivalent to `!neighbor_voxel.is_solid()`.
/// For border voxels, the neighbor border slice is consulted.
#[inline]
fn is_face_visible<V: VoxelType>(
    chunk: &Chunk<V>,
    neighbors: &ChunkNeighbors<V>,
    x: usize,
    y: usize,
    z: usize,
    face: &FaceDir,
    step: usize,
) -> bool {
    let coords = [x, y, z];
    let axis = face.axis;

    // Check if the neighbor in this direction falls outside the chunk.
    let neighbor_on_axis = if face.positive {
        coords[axis].wrapping_add(step)
    } else {
        coords[axis].wrapping_sub(step)
    };

    if neighbor_on_axis < CHUNK_SIZE {
        // Neighbor is within the chunk — check directly.
        // A neighbor occludes only when it is solid AND fully opaque (alpha >= 1.0),
        // consistent with the build_mask condition (color()[3] > 0.0).
        let mut nc = [x, y, z];
        nc[axis] = neighbor_on_axis;
        let sv = super_voxel(chunk, nc[0], nc[1], nc[2], step);
        return !sv.is_solid() || sv.color()[3] < 1.0;
    }

    // Neighbor is outside the chunk — consult the border slice.
    let face_index = face_dir_to_index(face);
    match &neighbors.faces[face_index] {
        None => true, // No neighbor data — treat as air.
        Some(slice) => {
            // At step > 1 (LOD 1/2) the current super-voxel covers a step×step
            // region on the neighbor's border face.  Check ALL step×step neighbor
            // voxels — the face is visible if ANY of them is non-solid or
            // transparent (i.e. cull only when the entire region is fully solid).
            //
            // Checking only the corner voxel (u_coord, v_coord) caused incorrect
            // face culling when the corner was solid but other voxels in the region
            // were air, producing transparent holes at LOD 1+ chunk boundaries.
            let u_coord = coords[face.u_axis];
            let v_coord = coords[face.v_axis];

            for du in 0..step {
                for dv in 0..step {
                    let u = u_coord + du;
                    let v = v_coord + dv;
                    if u >= CHUNK_SIZE || v >= CHUNK_SIZE {
                        return true; // out of bounds → treat as air → face visible
                    }
                    let slice_index = border_slice_index(face_index, u, v);
                    let neighbor = slice[slice_index];
                    if !neighbor.is_solid() || neighbor.color()[3] < 1.0 {
                        return true; // any gap in the step×step region → face visible
                    }
                }
            }
            false // entire step×step region is solid and opaque → cull
        }
    }
}

/// Maps a `FaceDir` to its conventional face index (0..6).
#[inline]
fn face_dir_to_index(face: &FaceDir) -> usize {
    // FACE_DIRS[0] = +Y, [1] = -Y, [2] = +X, [3] = -X, [4] = +Z, [5] = -Z
    match (face.axis, face.positive) {
        (1, true) => 0,  // +Y
        (1, false) => 1, // -Y
        (0, true) => 2,  // +X
        (0, false) => 3, // -X
        (2, true) => 4,  // +Z
        (2, false) => 5, // -Z
        _ => unreachable!(),
    }
}

/// Computes the index into a `border_slice` array for a given face and
/// local (u, v) coordinates.
///
/// The packing matches `Chunk::border_slice`:
/// - Face 0 (+Y): `x * CHUNK_SIZE + z`
/// - Face 1 (-Y): `x * CHUNK_SIZE + z`
/// - Face 2 (+X): `y * CHUNK_SIZE + z`
/// - Face 3 (-X): `y * CHUNK_SIZE + z`
/// - Face 4 (+Z): `x * CHUNK_SIZE + y`
/// - Face 5 (-Z): `x * CHUNK_SIZE + y`
///
/// The `u_coord` and `v_coord` here are in the FaceDir's tangent-axis space.
#[inline]
fn border_slice_index(face_index: usize, u_coord: usize, v_coord: usize) -> usize {
    // FaceDir tangent axes vs border_slice packing:
    //   Face 0/1 (+Y/-Y): u=X(0), v=Z(2) -> border_slice packs [x * CS + z] -> u * CS + v
    //   Face 2/3 (+X/-X): u=Z(2), v=Y(1) -> border_slice packs [y * CS + z] -> v * CS + u
    //   Face 4/5 (+Z/-Z): u=X(0), v=Y(1) -> border_slice packs [x * CS + y] -> u * CS + v
    match face_index {
        0 | 1 => u_coord * CHUNK_SIZE + v_coord,
        2 | 3 => v_coord * CHUNK_SIZE + u_coord,
        4 | 5 => u_coord * CHUNK_SIZE + v_coord,
        _ => unreachable!(),
    }
}
