//! High-level operations for reading and writing voxels at world-space coordinates.

use bevy::prelude::*;

use crate::chunk::{Chunk, ChunkCoord, world_to_chunk_coord, world_to_local_coord};
use crate::voxel::VoxelType;
use crate::world::ChunkModified;

use super::ChunkManager;

/// Read the voxel at a world-space position.
/// Returns `None` if the chunk is not loaded.
pub fn get_voxel_at<V: VoxelType>(
    world_pos: Vec3,
    manager: &ChunkManager,
    chunks: &Query<&Chunk<V>>,
) -> Option<V> {
    let coord: ChunkCoord = world_to_chunk_coord(world_pos);
    let entity = manager.get_entity(coord)?;
    let chunk = chunks.get(entity).ok()?;
    let (x, y, z) = world_to_local_coord(world_pos);
    Some(chunk.get(x, y, z))
}

/// Write a voxel at a world-space position.
/// Returns `false` if the chunk is not loaded.
pub fn set_voxel_at<V: VoxelType>(
    world_pos: Vec3,
    voxel: V,
    manager: &ChunkManager,
    chunks: &mut Query<&mut Chunk<V>>,
    modified: &mut MessageWriter<ChunkModified>,
) -> bool {
    let coord: ChunkCoord = world_to_chunk_coord(world_pos);
    let Some(entity) = manager.get_entity(coord) else {
        return false;
    };
    let Ok(mut chunk) = chunks.get_mut(entity) else {
        return false;
    };
    let (x, y, z) = world_to_local_coord(world_pos);
    chunk.set(x, y, z, voxel);
    modified.write(ChunkModified(coord));
    true
}
