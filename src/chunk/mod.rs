//! Chunk storage, constants, and I/O for the voxel world.

mod storage;
pub mod persistence;

pub use storage::Chunk;

use bevy::prelude::*;

/// Number of voxels along each axis of a chunk.
pub const CHUNK_SIZE: usize = 16;

/// Total number of voxels in a chunk (`CHUNK_SIZE ^ 3`).
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

/// Size of a single voxel in world-space units.
pub const VOXEL_SIZE: f32 = 1.0;

/// Integer coordinate identifying a chunk in the world grid.
///
/// Each unit corresponds to `CHUNK_SIZE` world-space units. The world-space
/// position of a chunk's minimum corner is `coord.as_vec3() * CHUNK_SIZE as f32`.
pub type ChunkCoord = IVec3;

/// Component that tags a chunk entity with its grid coordinate.
///
/// Attached to every chunk entity after generation completes so that systems
/// can query `&ChunkCoordComp` alongside other chunk components without going
/// through [`crate::world::ChunkManager`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkCoordComp(pub IVec3);

/// Component that stores the current Level of Detail for a chunk entity.
///
/// | Value | Distance from viewer | Detail |
/// |-------|---------------------|--------|
/// | `0`   | < 50 units          | Full (step = 1) |
/// | `1`   | 50–100 units        | Reduced (step = 2) |
/// | `2`   | > 100 units         | Coarse (step = 4) |
///
/// Updated every frame by `lod_update_system`. When the value changes,
/// a [`crate::world::ChunkModified`] message is emitted so that the mesh
/// system regenerates the geometry at the new detail level.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChunkLod(pub u8);

impl ChunkCoordComp {
    /// Returns the underlying `IVec3` coordinate.
    #[inline]
    pub fn coord(&self) -> IVec3 {
        self.0
    }
}

// ─── Coordinate utilities ─────────────────────────────────────────────────────

/// Convert a world-space position to the chunk coordinate that contains it.
///
/// Each chunk spans `CHUNK_SIZE * VOXEL_SIZE` world units along each axis.
/// Uses Euclidean division so negative coordinates map to the correct chunk.
pub fn world_to_chunk_coord(world_pos: Vec3) -> ChunkCoord {
    let size = CHUNK_SIZE as f32 * VOXEL_SIZE;
    IVec3::new(
        world_pos.x.div_euclid(size) as i32,
        world_pos.y.div_euclid(size) as i32,
        world_pos.z.div_euclid(size) as i32,
    )
}

/// Convert a world-space position to the local voxel index within its chunk.
///
/// Returns `(x, y, z)` each in `[0, CHUNK_SIZE)`.
pub fn world_to_local_coord(world_pos: Vec3) -> (usize, usize, usize) {
    let size = CHUNK_SIZE as f32 * VOXEL_SIZE;
    let local = Vec3::new(
        world_pos.x.rem_euclid(size),
        world_pos.y.rem_euclid(size),
        world_pos.z.rem_euclid(size),
    );
    (
        (local.x / VOXEL_SIZE) as usize,
        (local.y / VOXEL_SIZE) as usize,
        (local.z / VOXEL_SIZE) as usize,
    )
}

/// Returns the world-space origin (minimum corner) of a chunk.
pub fn chunk_origin(coord: ChunkCoord) -> Vec3 {
    coord.as_vec3() * (CHUNK_SIZE as f32 * VOXEL_SIZE)
}

/// Returns the world-space center of a chunk.
pub fn chunk_center(coord: ChunkCoord) -> Vec3 {
    chunk_origin(coord) + Vec3::splat(CHUNK_SIZE as f32 * VOXEL_SIZE * 0.5)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_to_chunk_coord_at_origin() {
        assert_eq!(
            world_to_chunk_coord(Vec3::ZERO),
            IVec3::new(0, 0, 0)
        );
    }

    #[test]
    fn world_to_chunk_coord_one_chunk_over() {
        // CHUNK_SIZE = 16, VOXEL_SIZE = 1.0 → chunk boundary at 16.0
        assert_eq!(
            world_to_chunk_coord(Vec3::new(16.0, 0.0, 0.0)),
            IVec3::new(1, 0, 0)
        );
    }

    #[test]
    fn world_to_local_coord_at_origin() {
        assert_eq!(world_to_local_coord(Vec3::new(0.5, 0.5, 0.5)), (0, 0, 0));
    }

    #[test]
    fn world_to_local_coord_second_chunk() {
        // 16.5 is 0.5 into the second chunk → local x = 0
        assert_eq!(world_to_local_coord(Vec3::new(16.5, 0.0, 0.0)), (0, 0, 0));
    }
}
