//! Systems that implement the two-stage async chunk generation pipeline.
//!
//! # Pipeline overview
//!
//! Stage 1 — **[`chunk_request_system`]**: runs first in [`super::GeneratorSystems`].
//! It drains [`ChunkLoadRequest`] messages and for each one:
//!
//! - Clones the [`super::GeneratorResource`]'s `Arc`, spawns a new bare
//!   entity, and offloads the load to [`AsyncComputeTaskPool`].  The returned
//!   [`bevy::tasks::Task`] is stored as a [`ChunkTask<V>`] component.
//! - Inside the task, a chunk saved under [`PersistenceConfig::save_dir`] is
//!   restored with [`try_load_chunk`]; otherwise
//!   [`super::ChunkGenerator::generate`] runs. Neither blocks the main thread.
//! - Without a generator, a saved chunk is restored on the calling thread
//!   instead (see [`chunk_request_system`]).
//!
//! Stage 2 — **[`chunk_task_poll_system`]**: runs second, chained after stage 1.
//! It iterates every entity with a [`ChunkTask<V>`] component and calls
//! `poll_once` — a non-blocking single-frame check.  When a task resolves:
//!
//! - The [`ChunkTask<V>`] component is removed.
//! - The loaded [`Chunk<V>`] and [`ChunkCoordComp`] are inserted.
//! - [`ChunkManager::register`] is called to make the chunk queryable.
//! - [`ChunkModified`] is emitted so [`crate::mesh::systems::chunk_mesh_system`]
//!   builds geometry on the same frame.
//!
//! The two-stage design keeps the main thread free: disk restore and
//! generation happen entirely on worker threads, and the main thread only pays a `poll_once` cost
//! per in-flight task per frame.

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};

use crate::chunk::{Chunk, ChunkCoord, ChunkCoordComp};
use crate::voxel::VoxelType;
use crate::world::{
    ChunkLoadRequest, ChunkManager, ChunkModified, PersistenceConfig, try_load_chunk,
};

use super::GeneratorResource;

// ─── ChunkTask ────────────────────────────────────────────────────────────────

/// ECS component that tracks an in-flight async chunk load task.
///
/// [`chunk_request_system`] attaches this component to the chunk entity when
/// it offloads the load (disk restore, or generation) to
/// [`AsyncComputeTaskPool`].  The entity has no other
/// meaningful components until [`chunk_task_poll_system`] resolves the task and
/// replaces this component with the fully populated [`Chunk<V>`].
///
/// The inner [`Task`] yields `(ChunkCoord, Chunk<V>)` so the coord is
/// available after the task completes without requiring an additional query.
#[derive(Component)]
pub struct ChunkTask<V: VoxelType> {
    /// The async task producing a fully populated [`Chunk<V>`].
    pub task: Task<(ChunkCoord, Chunk<V>)>,
}

// ─── chunk_request_system ────────────────────────────────────────────────────

/// Responds to [`ChunkLoadRequest`] messages by spawning a background task
/// that restores the chunk from disk or generates it.
///
/// For each incoming request this system clones the generator `Arc` (and the
/// save directory when [`PersistenceConfig`] is present) and offloads the load
/// to [`AsyncComputeTaskPool`], attaching the returned [`Task`] as a
/// [`ChunkTask<V>`] component on a new entity. The task:
/// 1. Tries [`try_load_chunk`] when persistence is configured. A saved chunk
///    wins over the generator.
/// 2. Otherwise runs the generator.
///
/// No file is read on the main thread. Either way the resulting chunk is
/// clean ([`Chunk::is_dirty`] is `false`), so it is not saved until edited.
///
/// The entity has no other components until [`chunk_task_poll_system`]
/// completes the task and attaches the real [`Chunk<V>`] data.
///
/// ## Without a generator
///
/// When no [`GeneratorResource`] is registered there is nothing to fall back
/// to, so a saved chunk is read immediately on the calling thread and
/// registered in the same run; a request without a saved file is skipped.
pub fn chunk_request_system<V: VoxelType>(
    mut commands: Commands,
    mut load_reader: MessageReader<ChunkLoadRequest>,
    generator_res: Option<Res<GeneratorResource<V>>>,
    persistence_config: Option<Res<PersistenceConfig>>,
    mut manager: ResMut<ChunkManager>,
    mut modified_writer: MessageWriter<ChunkModified>,
) {
    crate::profile_span!("voxel::generator::dispatch");
    let pool = AsyncComputeTaskPool::get();

    for &ChunkLoadRequest(coord) in load_reader.read() {
        let save_dir = persistence_config.as_ref().map(|cfg| cfg.save_dir.clone());

        let Some(ref generator_res) = generator_res else {
            // No generator registered (framework-only mode): a task could not
            // report "no saved chunk", so restore on this thread or skip.
            if let Some(save_dir) = save_dir
                && let Some(chunk) = try_load_chunk::<V>(coord, &save_dir)
            {
                let entity = commands.spawn((chunk, ChunkCoordComp(coord))).id();
                manager.register(coord, entity);
                modified_writer.write(ChunkModified(coord));
            }
            continue;
        };

        // Clone the Arc — cheap, no allocation of the generator itself.
        let generator = generator_res.generator.clone();

        let task = pool.spawn(async move {
            // A saved chunk wins over the generator. The read blocks, which is
            // why it happens here and not in the system.
            if let Some(save_dir) = save_dir
                && let Some(chunk) = try_load_chunk::<V>(coord, &save_dir)
            {
                return (coord, chunk);
            }

            let mut chunk = Chunk::<V>::new();
            generator.generate(coord, &mut chunk);
            // Generated terrain can be produced again, so it has nothing to save.
            chunk.mark_clean();
            (coord, chunk)
        });

        commands.spawn(ChunkTask::<V> { task });
    }
}

// ─── chunk_task_poll_system ───────────────────────────────────────────────────

/// Polls all in-flight [`ChunkTask<V>`] components.
///
/// When a task is ready this system:
/// 1. Removes the [`ChunkTask`] component from the entity.
/// 2. Inserts the loaded [`Chunk<V>`] and a [`ChunkCoordComp`] component.
/// 3. Calls [`ChunkManager::register`] so the streaming system knows the chunk
///    is fully loaded.
/// 4. Emits a [`ChunkModified`] message to trigger mesh generation.
pub fn chunk_task_poll_system<V: VoxelType>(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut ChunkTask<V>)>,
    mut manager: ResMut<ChunkManager>,
    mut modified_writer: MessageWriter<ChunkModified>,
) {
    crate::profile_span!("voxel::generator::poll");
    for (entity, mut chunk_task) in tasks.iter_mut() {
        // Poll the task exactly once — non-blocking.
        if let Some((coord, chunk)) = block_on(poll_once(&mut chunk_task.task)) {
            // Remove the task component; insert the real chunk data.
            commands
                .entity(entity)
                .remove::<ChunkTask<V>>()
                .insert(chunk)
                .insert(ChunkCoordComp(coord));

            // Register in the manager and notify the mesh system.
            manager.register(coord, entity);
            modified_writer.write(ChunkModified(coord));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bevy::ecs::system::RunSystemOnce;
    use bevy::tasks::TaskPool;

    use super::*;
    use crate::chunk::persistence::{chunk_filename, save_chunk};
    use crate::generator::ChunkGenerator;
    use crate::voxel::DefaultVoxel;

    type V = DefaultVoxel;

    struct CountingGenerator(Arc<AtomicUsize>);

    impl ChunkGenerator<V> for CountingGenerator {
        fn generate(&self, _coord: IVec3, chunk: &mut Chunk<V>) {
            self.0.fetch_add(1, Ordering::SeqCst);
            chunk.set(0, 0, 0, DefaultVoxel::IronOre);
        }
    }

    fn request_world(save_dir: Option<&std::path::Path>) -> World {
        AsyncComputeTaskPool::get_or_init(TaskPool::new);
        let mut world = World::new();
        world.init_resource::<Messages<ChunkLoadRequest>>();
        world.init_resource::<Messages<ChunkModified>>();
        world.insert_resource(ChunkManager::default());
        if let Some(dir) = save_dir {
            world.insert_resource(PersistenceConfig {
                save_dir: dir.to_string_lossy().into_owned(),
                auto_save: true,
            });
        }
        world
    }

    fn with_generator(world: &mut World) -> Arc<AtomicUsize> {
        let calls = Arc::new(AtomicUsize::new(0));
        world.insert_resource(GeneratorResource::<V> {
            generator: Arc::new(CountingGenerator(calls.clone())),
        });
        calls
    }

    fn save_marker(dir: &std::path::Path, coord: IVec3) {
        let mut chunk = Chunk::<V>::new();
        chunk.set(1, 2, 3, DefaultVoxel::RareMineral);
        save_chunk(&chunk, dir.join(chunk_filename(coord))).unwrap();
    }

    fn request(world: &mut World, coord: IVec3) {
        world.write_message(ChunkLoadRequest(coord));
        world
            .run_system_once(chunk_request_system::<V>)
            .expect("chunk_request_system should run");
    }

    /// Polls until `coord` is loaded, giving background tasks time to finish.
    fn wait_loaded(world: &mut World, coord: IVec3) -> Entity {
        for _ in 0..2000 {
            world
                .run_system_once(chunk_task_poll_system::<V>)
                .expect("chunk_task_poll_system should run");
            if let Some(entity) = world.resource::<ChunkManager>().get_entity(coord) {
                return entity;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("chunk {coord} never finished loading");
    }

    fn task_count(world: &mut World) -> usize {
        world.query::<&ChunkTask<V>>().iter(world).count()
    }

    #[test]
    fn saved_chunk_is_restored_by_a_background_task() {
        let dir = tempfile::tempdir().unwrap();
        let coord = IVec3::new(4, -1, 2);
        save_marker(dir.path(), coord);
        let mut world = request_world(Some(dir.path()));
        let calls = with_generator(&mut world);

        request(&mut world, coord);

        assert_eq!(task_count(&mut world), 1, "the disk read is a task");
        assert!(
            !world.resource::<ChunkManager>().is_loaded(coord),
            "the request system must not read the file itself"
        );

        let entity = wait_loaded(&mut world, coord);
        let chunk = world.get::<Chunk<V>>(entity).unwrap();
        assert_eq!(chunk.get(1, 2, 3), DefaultVoxel::RareMineral);
        assert!(!chunk.is_dirty());
        assert_eq!(calls.load(Ordering::SeqCst), 0, "the saved chunk wins");
    }

    #[test]
    fn generated_chunk_has_no_unsaved_changes() {
        let dir = tempfile::tempdir().unwrap();
        let coord = IVec3::ZERO;
        let mut world = request_world(Some(dir.path()));
        let calls = with_generator(&mut world);

        request(&mut world, coord);
        let entity = wait_loaded(&mut world, coord);

        let chunk = world.get::<Chunk<V>>(entity).unwrap();
        assert_eq!(chunk.get(0, 0, 0), DefaultVoxel::IronOre);
        assert!(!chunk.is_dirty(), "generated terrain can be regenerated");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn unreadable_saved_chunk_falls_back_to_the_generator() {
        let dir = tempfile::tempdir().unwrap();
        let coord = IVec3::ZERO;
        std::fs::write(dir.path().join(chunk_filename(coord)), b"short").unwrap();
        let mut world = request_world(Some(dir.path()));
        let calls = with_generator(&mut world);

        request(&mut world, coord);
        let entity = wait_loaded(&mut world, coord);

        let chunk = world.get::<Chunk<V>>(entity).unwrap();
        assert_eq!(chunk.get(0, 0, 0), DefaultVoxel::IronOre);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn without_a_generator_a_saved_chunk_loads_immediately_and_clean() {
        let dir = tempfile::tempdir().unwrap();
        let coord = IVec3::new(0, 3, 0);
        save_marker(dir.path(), coord);
        let mut world = request_world(Some(dir.path()));

        request(&mut world, coord);

        let entity = world
            .resource::<ChunkManager>()
            .get_entity(coord)
            .expect("loaded in the same run");
        let chunk = world.get::<Chunk<V>>(entity).unwrap();
        assert_eq!(chunk.get(1, 2, 3), DefaultVoxel::RareMineral);
        assert!(!chunk.is_dirty());
        assert_eq!(task_count(&mut world), 0);
    }

    #[test]
    fn without_a_generator_or_a_saved_chunk_nothing_is_spawned() {
        let dir = tempfile::tempdir().unwrap();
        let mut world = request_world(Some(dir.path()));

        request(&mut world, IVec3::ZERO);

        assert_eq!(world.entities().len(), 0);
        assert!(!world.resource::<ChunkManager>().is_loaded(IVec3::ZERO));
    }
}
