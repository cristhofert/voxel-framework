//! Chunk data model, coordinate utilities, and persistence for the voxel world.
//!
//! ## Overview
//!
//! This module defines the core data structures that represent a piece of the
//! voxel world at rest. The runtime lifecycle (request, generate, mesh, render)
//! lives in [`crate::world`], [`crate::generator`], and [`crate::mesh`]. This
//! module is purely about *what a chunk is* and *where it lives in space*.
//!
//! ## The `Chunk<V>` data model
//!
//! A [`Chunk<V>`] is a 16×16×16 block of voxels stored in a single,
//! heap-allocated flat array of length [`CHUNK_VOLUME`] (4 096 elements).
//! Voxels are indexed in X-major order:
//!
//! ```text
//! flat_index = x * CHUNK_SIZE² + y * CHUNK_SIZE + z
//! ```
//!
//! This layout keeps a full Z-row of voxels contiguous in memory, which is
//! cache-friendly for the greedy meshing algorithm's inner loop. The array is
//! wrapped in a `Box` to keep the chunk's stack footprint to a single pointer
//! regardless of voxel size.
//!
//! Because [`Chunk<V>`] derives [`bevy::prelude::Component`], it can be
//! attached directly to a Bevy entity, making it queryable alongside
//! [`ChunkCoordComp`] and [`ChunkLod`] without any indirection.
//!
//! ## Coordinate systems
//!
//! The framework uses three distinct coordinate spaces:
//!
//! | Space | Type | Unit |
//! |-------|------|------|
//! | **World space** | `Vec3` | One unit = one voxel = [`VOXEL_SIZE`] meters |
//! | **Chunk space** | [`ChunkCoord`] (`IVec3`) | One unit = `CHUNK_SIZE` world units |
//! | **Local space** | `(usize, usize, usize)` | Voxel index within a chunk, each in `[0, CHUNK_SIZE)` |
//!
//! ### Conversion helpers
//!
//! | Function | Input → Output |
//! |----------|----------------|
//! | [`world_to_chunk_coord`] | `Vec3` → [`ChunkCoord`] |
//! | [`world_to_local_coord`] | `Vec3` → `(x, y, z)` local |
//! | [`chunk_origin`] | [`ChunkCoord`] → `Vec3` (min corner) |
//! | [`chunk_center`] | [`ChunkCoord`] → `Vec3` (center) |
//!
//! All conversions use Euclidean division, so negative world coordinates map
//! correctly to negative chunk coordinates without off-by-one errors.
//!
//! ## Constants
//!
//! | Constant | Value | Meaning |
//! |----------|-------|---------|
//! | [`CHUNK_SIZE`] | `16` | Voxels per axis |
//! | [`CHUNK_VOLUME`] | `4 096` | Total voxels per chunk |
//! | [`VOXEL_SIZE`] | `1.0` | World-space size of one voxel |
//!
//! ## Persistence
//!
//! Raw chunk data can be written to and read from disk via the
//! [`persistence`] sub-module, which serializes each voxel with
//! [`crate::voxel::VoxelType::to_byte`] / [`crate::voxel::VoxelType::from_byte`].

mod storage;
pub mod persistence;

pub use storage::{Chunk, face_to_state};

use bevy::prelude::*;

/// Number of voxels along each axis of a chunk.
///
/// A chunk is a cube with `CHUNK_SIZE` voxels per edge. The value `16` is
/// chosen to balance memory footprint (4 096 voxels per chunk), generation
/// granularity, and cache locality during meshing. Increasing this value
/// reduces chunk boundary overhead but raises per-chunk allocation size and
/// generation latency.
pub const CHUNK_SIZE: usize = 16;

/// Total number of voxels in a single chunk (`CHUNK_SIZE³ = 4 096`).
///
/// This is the length of the flat array inside every [`Chunk<V>`].
/// At one byte per voxel (the minimum for `DefaultVoxel`), a fully populated
/// chunk occupies 4 KiB of heap memory before any component overhead.
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

/// World-space size of a single voxel, in Bevy world units.
///
/// Currently `1.0`, meaning one voxel corresponds to one world unit. All
/// coordinate conversion functions in this module factor `VOXEL_SIZE` in so
/// that changing this constant is sufficient to rescale the entire world.
pub const VOXEL_SIZE: f32 = 1.0;

/// Integer coordinate identifying a chunk in the world grid.
///
/// `ChunkCoord` is a type alias for [`bevy::math::IVec3`]. Each component
/// is a signed integer where one unit equals `CHUNK_SIZE * VOXEL_SIZE`
/// world-space units. The minimum corner of the chunk in world space is:
///
/// ```text
/// chunk_origin = coord.as_vec3() * (CHUNK_SIZE as f32 * VOXEL_SIZE)
/// ```
///
/// Chunk `(0, 0, 0)` covers world positions `[0, 16)³`.
/// Chunk `(-1, 0, 0)` covers world positions `[-16, 0)` on the X axis.
///
/// Use [`world_to_chunk_coord`] to convert a world-space position to the
/// chunk that contains it, and [`chunk_origin`] / [`chunk_center`] to go
/// back the other way.
pub type ChunkCoord = IVec3;

/// Bevy [`Component`] that tags a chunk entity with its [`ChunkCoord`].
///
/// Every chunk entity receives this component when generation completes.
/// Having the coordinate on the entity lets systems query
/// `&ChunkCoordComp` directly — without going through
/// [`crate::world::ChunkManager`] — and avoids the lock contention that
/// would arise if all per-chunk logic had to look up coordinates in the
/// manager's `HashMap`.
///
/// The inner [`IVec3`] is the same value as the corresponding entry in
/// [`crate::world::ChunkManager`] and can be passed to any function
/// expecting a [`ChunkCoord`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkCoordComp(pub IVec3);

/// Bevy [`Component`] that stores the current Level of Detail for a chunk entity.
///
/// The LOD level controls the voxel sampling step used during greedy mesh
/// generation — a higher step skips more voxels, producing a lower-resolution
/// mesh that is cheaper to render.
///
/// | `ChunkLod` | Distance from viewer | Sampling step | Effective resolution |
/// |------------|---------------------|---------------|----------------------|
/// | `0`        | < 50 units          | 1 (every voxel) | Full 16³ |
/// | `1`        | 50–100 units        | 2 (every 2nd)   | Effective 8³ |
/// | `2`        | > 100 units         | 4 (every 4th)   | Effective 4³ |
///
/// The value is updated every frame by `lod_update_system` based on the
/// distance between the chunk center and [`crate::world::systems::ViewerPosition`].
/// When the level changes, a [`crate::world::ChunkModified`] event is emitted
/// so that [`crate::mesh::MeshPlugin`] regenerates the geometry at the new
/// detail level on the next frame.
///
/// `Default` produces `ChunkLod(0)` (full detail).
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

/// Converts a world-space position to the [`ChunkCoord`] of the chunk that contains it.
///
/// Each chunk spans `CHUNK_SIZE * VOXEL_SIZE` (`16.0`) world units per axis.
/// Euclidean division is used so that negative positions round toward negative
/// infinity and map to the correct chunk rather than toward zero.
///
/// # Examples
///
/// ```rust,no_run
/// use bevy::math::Vec3;
/// use voxel_framework::chunk::world_to_chunk_coord;
///
/// // Origin is in chunk (0, 0, 0).
/// assert_eq!(world_to_chunk_coord(Vec3::ZERO), bevy::math::IVec3::ZERO);
///
/// // Position (16.0, 0.0, 0.0) is the first voxel of chunk (1, 0, 0).
/// assert_eq!(world_to_chunk_coord(Vec3::new(16.0, 0.0, 0.0)).x, 1);
///
/// // Negative positions map to negative chunks.
/// assert_eq!(world_to_chunk_coord(Vec3::new(-1.0, 0.0, 0.0)).x, -1);
/// ```
pub fn world_to_chunk_coord(world_pos: Vec3) -> ChunkCoord {
    let size = CHUNK_SIZE as f32 * VOXEL_SIZE;
    IVec3::new(
        world_pos.x.div_euclid(size) as i32,
        world_pos.y.div_euclid(size) as i32,
        world_pos.z.div_euclid(size) as i32,
    )
}

/// Converts a world-space position to the local voxel coordinate within its chunk.
///
/// Returns `(x, y, z)` where each component is in `[0, CHUNK_SIZE)`.
/// The result can be passed directly to [`Chunk::get`] or [`Chunk::set`].
///
/// Use [`world_to_chunk_coord`] to find which chunk the position belongs to,
/// then this function to get the offset within that chunk.
///
/// # Examples
///
/// ```rust,no_run
/// use bevy::math::Vec3;
/// use voxel_framework::chunk::world_to_local_coord;
///
/// // Position (0.5, 0.5, 0.5) → local voxel (0, 0, 0).
/// assert_eq!(world_to_local_coord(Vec3::splat(0.5)), (0, 0, 0));
///
/// // Position (15.5, 0.0, 0.0) is in local voxel (15, 0, 0).
/// assert_eq!(world_to_local_coord(Vec3::new(15.5, 0.0, 0.0)).0, 15);
/// ```
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

/// Returns the world-space position of the minimum corner (origin) of a chunk.
///
/// The origin is the corner with the smallest X, Y, and Z values. It equals
/// `coord.as_vec3() * (CHUNK_SIZE as f32 * VOXEL_SIZE)`.
pub fn chunk_origin(coord: ChunkCoord) -> Vec3 {
    coord.as_vec3() * (CHUNK_SIZE as f32 * VOXEL_SIZE)
}

/// Returns the world-space position of the center of a chunk.
///
/// Equivalent to `chunk_origin(coord) + Vec3::splat(CHUNK_SIZE as f32 * VOXEL_SIZE * 0.5)`.
/// Used by the LOD and occlusion systems to measure the distance from a chunk
/// to the viewer or camera.
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
