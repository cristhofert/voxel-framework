//! Neighbor-aware mesh generation with border culling.
//!
//! Identical to the basic greedy mesher, but when a voxel sits at the chunk
//! border, the outward face is checked against the neighbor chunk's border
//! slice rather than being unconditionally emitted.

use bevy::mesh::Mesh;

use crate::chunk::{Chunk, CHUNK_SIZE};
use crate::voxel::VoxelType;

use super::ChunkNeighbors;
use super::greedy::{build_mesh, super_voxel, FaceDir};

/// Generates a mesh for a chunk with neighbor-aware border culling.
///
/// When a voxel is at the edge of the chunk and its outward face would
/// normally be emitted (because there is no neighbor *within* the chunk),
/// this variant checks the corresponding neighbor border slice. If the
/// adjacent voxel in the neighbor chunk is solid, the face is culled.
///
/// If a neighbor slot is `None`, the border is treated as open air and the
/// face is emitted normally.
///
/// # Arguments
///
/// * `chunk` — The chunk to mesh.
/// * `neighbors` — Border data from the six adjacent chunks.
/// * `step` — LOD step size (`1` for full detail, `2` for half).
pub fn generate_chunk_mesh_with_neighbors<V: VoxelType>(
    chunk: &Chunk<V>,
    neighbors: &ChunkNeighbors<V>,
    step: usize,
) -> Mesh {
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
        let mut nc = [x, y, z];
        nc[axis] = neighbor_on_axis;
        let sv = super_voxel(chunk, nc[0], nc[1], nc[2], step);
        return !sv.is_solid();
    }

    // Neighbor is outside the chunk — consult the border slice.
    let face_index = face_dir_to_index(face);
    match &neighbors.faces[face_index] {
        None => true, // No neighbor data — treat as air.
        Some(slice) => {
            // Look up the voxel in the neighbor slice.
            let u_coord = coords[face.u_axis];
            let v_coord = coords[face.v_axis];

            let slice_index = border_slice_index(face_index, u_coord, v_coord);
            !slice[slice_index].is_solid()
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
