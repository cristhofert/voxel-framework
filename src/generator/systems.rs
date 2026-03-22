//! Async chunk generation pipeline systems.
//!
//! Two systems form the pipeline:
//!
//! - [`chunk_request_system`] — listens for [`ChunkLoadRequest`] messages and
//!   spawns an async generation task on [`AsyncComputeTaskPool`] for each one.
//! - [`chunk_task_poll_system`] — polls in-flight tasks every frame; when a
//!   task finishes it attaches the [`Chunk<V>`] to the entity, registers it in
//!   [`ChunkManager`], and emits a [`ChunkModified`] message so the mesh
//!   system knows to build geometry.

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};

use crate::chunk::{Chunk, ChunkCoord, ChunkCoordComp};
use crate::voxel::VoxelType;
use crate::world::{ChunkLoadRequest, ChunkManager, ChunkModified, PersistenceConfig, try_load_chunk};

use super::GeneratorResource;

// ─── ChunkTask ────────────────────────────────────────────────────────────────

/// Component that holds an in-flight async chunk generation task.
///
/// Spawned onto a chunk entity by [`chunk_request_system`] and removed by
/// [`chunk_task_poll_system`] once the task completes.
#[derive(Component)]
pub struct ChunkTask<V: VoxelType> {
    /// The async task producing a fully populated [`Chunk<V>`].
    pub task: Task<(ChunkCoord, Chunk<V>)>,
}

// ─── chunk_request_system ────────────────────────────────────────────────────

/// Responds to [`ChunkLoadRequest`] messages by either loading from disk or
/// spawning async generation tasks.
///
/// For each incoming request this system:
/// 1. If [`PersistenceConfig`] is present, attempts to load the chunk from
///    disk via [`try_load_chunk`].  On success the chunk entity is spawned
///    immediately (no async task), the manager is updated, and a
///    [`ChunkModified`] message is emitted so the mesh system builds geometry.
/// 2. If no saved file exists (or [`PersistenceConfig`] is absent), the
///    original async generation path is used:
///    a. Spawns a new Bevy entity.
///    b. Clones the generator `Arc` so the task owns it without holding the `World`.
///    c. Offloads chunk generation to [`AsyncComputeTaskPool`], attaching the
///    returned [`Task`] as a [`ChunkTask<V>`] component.
///
/// The entity produced by the async path has no other components until
/// [`chunk_task_poll_system`] completes the task and attaches the real
/// [`Chunk<V>`] data.
pub fn chunk_request_system<V: VoxelType>(
    mut commands: Commands,
    mut load_reader: MessageReader<ChunkLoadRequest>,
    generator_res: Option<Res<GeneratorResource<V>>>,
    persistence_config: Option<Res<PersistenceConfig>>,
    mut manager: ResMut<ChunkManager>,
    mut modified_writer: MessageWriter<ChunkModified>,
) {
    let pool = AsyncComputeTaskPool::get();

    for &ChunkLoadRequest(coord) in load_reader.read() {
        // ── 1. Try loading from disk first ───────────────────────────────
        if let Some(ref cfg) = persistence_config
            && let Some(chunk) = try_load_chunk::<V>(coord, &cfg.save_dir)
        {
            // Disk hit — spawn entity directly without an async task.
            let entity = commands
                .spawn((chunk, ChunkCoordComp(coord)))
                .id();
            manager.register(coord, entity);
            modified_writer.write(ChunkModified(coord));
            continue;
        }

        // ── 2. Fall back to async generation ─────────────────────────────
        let Some(ref generator_res) = generator_res else {
            // No generator registered — silently skip (framework-only mode).
            continue;
        };

        // Clone the Arc — cheap, no allocation of the generator itself.
        let generator = generator_res.generator.clone();

        let task = pool.spawn(async move {
            let mut chunk = Chunk::<V>::new();
            generator.generate(coord, &mut chunk);
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
/// 2. Inserts the generated [`Chunk<V>`] and a [`ChunkCoordComp`] component.
/// 3. Calls [`ChunkManager::register`] so the streaming system knows the chunk
///    is fully loaded.
/// 4. Emits a [`ChunkModified`] message to trigger mesh generation.
pub fn chunk_task_poll_system<V: VoxelType>(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut ChunkTask<V>)>,
    mut manager: ResMut<ChunkManager>,
    mut modified_writer: MessageWriter<ChunkModified>,
) {
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
