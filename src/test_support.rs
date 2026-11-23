//! Shared helpers for unit tests. Compiled only under `cfg(test)`.
//!
//! Prefer these over building a `World` by hand in each test module so that
//! world-level tests (collision, raycast, placement, voxel ops) share one setup.

use bevy::ecs::world::World;
use bevy::math::IVec3;

use crate::chunk::{CHUNK_SIZE, Chunk};
use crate::voxel::VoxelType;
use crate::world::ChunkManager;

/// Returns a chunk where every voxel is `voxel`.
pub(crate) fn filled_chunk<V: VoxelType>(voxel: V) -> Chunk<V> {
    let mut chunk = Chunk::<V>::new();
    for x in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                chunk.set(x, y, z, voxel);
            }
        }
    }
    chunk
}

/// Builds a bare `World` with a [`ChunkManager`] resource and one spawned,
/// registered entity per `(coord, chunk)` pair.
///
/// Pair it with `SystemState` to call functions that take
/// `Res<ChunkManager>` and `Query<&Chunk<V>>`.
pub(crate) fn world_with_chunks<V: VoxelType>(
    chunks: impl IntoIterator<Item = (IVec3, Chunk<V>)>,
) -> World {
    let mut world = World::new();
    let mut manager = ChunkManager::default();
    for (coord, chunk) in chunks {
        let entity = world.spawn(chunk).id();
        manager.register(coord, entity);
    }
    world.insert_resource(manager);
    world
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::DefaultVoxel;

    #[test]
    fn filled_chunk_is_fully_solid() {
        assert!(filled_chunk(DefaultVoxel::Rock).is_fully_solid());
    }

    #[test]
    fn world_with_chunks_registers_every_coord() {
        let coords = [IVec3::ZERO, IVec3::new(-1, 0, 2)];
        let world = world_with_chunks(coords.map(|coord| (coord, Chunk::<DefaultVoxel>::new())));
        let manager = world.resource::<ChunkManager>();
        for coord in coords {
            let entity = manager
                .get_entity(coord)
                .expect("coord should be registered");
            assert!(world.entity(entity).get::<Chunk<DefaultVoxel>>().is_some());
        }
    }
}
