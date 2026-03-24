//! High-level world-space voxel accessors.
//!
//! This module provides the two primary entry points for **reading and writing
//! individual voxels by world-space position**.  It sits above the chunk-local
//! getter/setter API (e.g. `Chunk::get`, `Chunk::set`) and above the
//! chunk-coordinate helpers (`world_to_chunk_coord`, `world_to_local_coord`),
//! combining them into a single ergonomic call.
//!
//! # Relationship to chunk-local API
//!
//! When you know the exact chunk coordinate and local voxel index (e.g. inside
//! mining or placement logic that received a [`crate::world::raycast::RaycastHit`]),
//! prefer the chunk-local API — it avoids the coordinate conversion overhead.
//! Use this module when you only have a world-space `Vec3` and do not want to
//! manage chunk boundaries manually.
//!
//! # Examples
//!
//! ```no_run
//! # use bevy::prelude::*;
//! # use voxel_framework::world::{ChunkManager, ChunkModified};
//! # use voxel_framework::world::voxel_ops::{get_voxel_at, set_voxel_at};
//! # use voxel_framework::chunk::Chunk;
//! # use voxel_framework::voxel::DefaultVoxel;
//! # fn example(
//! #     manager: &ChunkManager,
//! #     chunks: &Query<&Chunk<DefaultVoxel>>,
//! #     mut_chunks: &mut Query<&mut Chunk<DefaultVoxel>>,
//! #     modified: &mut MessageWriter<ChunkModified>,
//! # ) {
//! let pos = Vec3::new(12.0, 64.0, 8.0);
//!
//! // Read
//! if let Some(voxel) = get_voxel_at(pos, manager, chunks) {
//!     println!("voxel at {pos}: {voxel:?}");
//! }
//!
//! // Write
//! let written = set_voxel_at(pos, DefaultVoxel::Rock, manager, mut_chunks, modified);
//! assert!(written, "chunk must be loaded for the write to succeed");
//! # }
//! ```

use bevy::prelude::*;

use crate::chunk::{Chunk, ChunkCoord, world_to_chunk_coord, world_to_local_coord};
use crate::voxel::VoxelType;
use crate::world::ChunkModified;

use super::ChunkManager;

/// Read the voxel at a world-space position.
///
/// Converts `world_pos` to a chunk coordinate and a local voxel index, looks
/// up the chunk entity in `manager`, and returns the voxel stored at that
/// position.
///
/// # Parameters
///
/// - `world_pos` — World-space position anywhere inside the target voxel.
///   The function uses the **floor** of each component to determine the voxel
///   cell, so any point inside the voxel's unit cube maps to the same result.
/// - `manager` — Chunk registry used to find the entity for the enclosing chunk.
/// - `chunks` — Read query for chunk voxel data.
///
/// # Return value
///
/// `Some(V)` containing the voxel value at `world_pos`, or `None` when the
/// enclosing chunk is not currently loaded.
///
/// # Examples
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use voxel_framework::world::{ChunkManager, voxel_ops::get_voxel_at};
/// # use voxel_framework::chunk::Chunk;
/// # use voxel_framework::voxel::DefaultVoxel;
/// # fn example(manager: &ChunkManager, chunks: &Query<&Chunk<DefaultVoxel>>) {
/// let pos = Vec3::new(8.0, 64.5, 4.0);
/// match get_voxel_at(pos, manager, chunks) {
///     Some(v) => println!("voxel: {v:?}"),
///     None    => println!("chunk not loaded"),
/// }
/// # }
/// ```
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
///
/// Converts `world_pos` to a chunk coordinate and a local voxel index, then
/// writes `voxel` into the chunk.  As a side effect, emits a
/// [`crate::world::ChunkModified`] message for the containing chunk so that
/// the meshing and occlusion systems know to rebuild it.
///
/// # Parameters
///
/// - `world_pos` — World-space position inside the target voxel cell.
/// - `voxel` — The voxel value to write.
/// - `manager` — Chunk registry used to find the entity for the enclosing chunk.
/// - `chunks` — Mutable query for chunk voxel data.
/// - `modified` — Message writer; receives a [`crate::world::ChunkModified`]
///   event for the affected chunk on success.
///
/// # Return value
///
/// `true` if the write succeeded, `false` if the enclosing chunk is not loaded.
///
/// # Side effects
///
/// On success, writes one [`crate::world::ChunkModified`] message.  Downstream
/// systems (mesher, occlusion, persistence) react to this event — it must not
/// be suppressed.
///
/// # Examples
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use voxel_framework::world::{ChunkManager, ChunkModified, voxel_ops::set_voxel_at};
/// # use voxel_framework::chunk::Chunk;
/// # use voxel_framework::voxel::DefaultVoxel;
/// # fn example(
/// #     manager: &ChunkManager,
/// #     mut chunks: &mut Query<&mut Chunk<DefaultVoxel>>,
/// #     mut modified: &mut MessageWriter<ChunkModified>,
/// # ) {
/// let pos = Vec3::new(8.0, 64.0, 4.0);
/// let ok = set_voxel_at(pos, DefaultVoxel::Rock, manager, &mut chunks, &mut modified);
/// if !ok { eprintln!("chunk at {pos} is not loaded"); }
/// # }
/// ```
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

/// Read the state byte of the voxel at a world-space position.
///
/// Returns `None` if the enclosing chunk is not loaded. Returns `0` for voxels
/// whose state has never been explicitly set (generator-filled voxels).
///
/// See [`crate::chunk::Chunk::get_state`] for the state-byte encoding.
pub fn get_state_at<V: VoxelType>(
    world_pos: Vec3,
    manager: &ChunkManager,
    chunks: &Query<&Chunk<V>>,
) -> Option<u8> {
    let coord: ChunkCoord = world_to_chunk_coord(world_pos);
    let entity = manager.get_entity(coord)?;
    let chunk = chunks.get(entity).ok()?;
    let (x, y, z) = world_to_local_coord(world_pos);
    Some(chunk.get_state(x, y, z))
}

/// Write the state byte of the voxel at a world-space position.
///
/// Does NOT emit a `ChunkModified` event — state changes do not affect
/// geometry, so remeshing is not triggered. Returns `true` on success, `false`
/// if the enclosing chunk is not loaded.
///
/// See [`crate::chunk::Chunk::set_state`] for the state-byte encoding.
pub fn set_state_at<V: VoxelType>(
    world_pos: Vec3,
    state: u8,
    manager: &ChunkManager,
    chunks: &mut Query<&mut Chunk<V>>,
) -> bool {
    let coord: ChunkCoord = world_to_chunk_coord(world_pos);
    let Some(entity) = manager.get_entity(coord) else {
        return false;
    };
    let Ok(mut chunk) = chunks.get_mut(entity) else {
        return false;
    };
    let (x, y, z) = world_to_local_coord(world_pos);
    chunk.set_state(x, y, z, state);
    true
}
