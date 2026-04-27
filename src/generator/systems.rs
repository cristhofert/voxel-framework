//! Systems that implement the two-stage async chunk generation pipeline.
//!
//! # Pipeline overview
//!
//! Stage 1 — **[`chunk_request_system`]**: runs first in [`super::GeneratorSystems`].
//! It drains [`ChunkLoadRequest`] messages and for each one:
//!
//! - Attempts a synchronous disk restore when [`PersistenceConfig`] is present.
//!   On success the entity is spawned and [`ChunkModified`] is emitted
//!   immediately — no async task is needed.
//! - On a cache miss (or when persistence is absent) it clones the
//!   [`super::GeneratorResource`]'s `Arc`, spawns a new bare entity, and
//!   offloads the [`super::ChunkGenerator::generate`] call to
//!   [`AsyncComputeTaskPool`].  The returned [`bevy::tasks::Task`] is stored
//!   as a [`ChunkTask<V>`] component.
//!
//! Stage 2 — **[`chunk_task_poll_system`]**: runs second, chained after stage 1.
//! It iterates every entity with a [`ChunkTask<V>`] component and calls
//! `poll_once` — a non-blocking single-frame check.  When a task resolves:
//!
//! - The [`ChunkTask<V>`] component is removed.
//! - The generated [`Chunk<V>`] and [`ChunkCoordComp`] are inserted.
//! - [`ChunkManager::register`] is called to make the chunk queryable.
//! - [`ChunkModified`] is emitted so [`crate::mesh::systems::chunk_mesh_system`]
//!   builds geometry on the same frame.
//!
//! The two-stage design keeps the main thread free: generation work happens
//! entirely on worker threads, and the main thread only pays a `poll_once` cost
//! per in-flight task per frame.

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};

use crate::chunk::{Chunk, ChunkCoord, ChunkCoordComp};
use crate::voxel::VoxelType;
use crate::world::{ChunkLoadRequest, ChunkManager, ChunkModified, PersistenceConfig, try_load_chunk};

use super::GeneratorResource;

// ─── ChunkTask ────────────────────────────────────────────────────────────────

/// ECS component that tracks an in-flight async chunk generation task.
///
/// [`chunk_request_system`] attaches this component to the chunk entity when
/// it offloads generation to [`AsyncComputeTaskPool`].  The entity has no other
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
    crate::profile_span!("voxel::generator::dispatch");
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
