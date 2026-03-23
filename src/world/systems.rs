//! Bevy systems that implement the chunk streaming loop.
//!
//! # Systems in this module
//!
//! | System | Schedule | Reads | Writes / Emits |
//! |---|---|---|---|
//! | [`streaming_system`] | [`Update`] | [`ViewerPosition`], [`ChunkManager`] | [`ChunkLoadRequest`], [`ChunkUnloadRequest`], `ChunkManager` pending set |
//!
//! # Execution order
//!
//! [`streaming_system`] has no explicit ordering constraints relative to other
//! `Update` systems. It should run **after** your camera/player system has
//! updated [`ViewerPosition`] for the current frame. If you need a hard
//! ordering guarantee, use `.after(YourMovementSystem)` when adding the plugin
//! or configure a system set.
//!
//! # Authoring note
//!
//! [`ViewerPosition`] is defined here (alongside the system that consumes it)
//! because both share the same module-level concern: mapping a world-space
//! position to chunk-grid decisions. It is re-exported from the crate root via
//! [`super`].

use bevy::prelude::*;

use crate::chunk::{ChunkCoord, CHUNK_SIZE};

use super::{ChunkLoadRequest, ChunkManager, ChunkUnloadRequest};

// ─── ViewerPosition ───────────────────────────────────────────────────────────

/// World-space position of the primary viewer (player or camera).
///
/// This resource is read by [`streaming_system`] every frame to decide which
/// chunk coordinates lie within the load sphere and which loaded chunks have
/// drifted beyond the unload sphere. It **must be updated every frame** by
/// your player or camera system — if it is stale, the streaming system will
/// not respond to player movement and chunks near the new position will never
/// be requested.
///
/// The resource defaults to the origin (`Vec3::ZERO`). [`WorldPlugin`](crate::world::WorldPlugin)
/// registers it via `init_resource`, so it is always present when the plugin
/// is active.
///
/// # What happens if it is not updated?
///
/// The streaming system will continue to evaluate distances against the last
/// written position. No new [`ChunkLoadRequest`] events will be emitted for
/// the region around the player's actual position, and no
/// [`ChunkUnloadRequest`] events will fire for chunks the player has left
/// behind. The world will appear frozen at the last known viewer position.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::world::ViewerPosition;
///
/// fn sync_viewer_to_camera(
///     mut viewer: ResMut<ViewerPosition>,
///     camera: Query<&Transform, With<Camera3d>>,
/// ) {
///     if let Ok(transform) = camera.single() {
///         viewer.position = transform.translation;
///     }
/// }
/// ```
///
/// Register your sync system in [`Update`] before the streaming system runs:
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::world::ViewerPosition;
///
/// # fn sync_viewer_to_camera() {}
/// # fn main() {
/// App::new()
///     .add_systems(Update, sync_viewer_to_camera)
///     .run();
/// # }
/// ```
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct ViewerPosition {
    /// Current world-space position of the viewer.
    pub position: Vec3,
}

// ─── streaming_system ────────────────────────────────────────────────────────

/// Drives chunk load and unload decisions based on [`ViewerPosition`].
///
/// Scheduled in [`Update`] by [`super::WorldPlugin`]. Each frame this system:
///
/// 1. Reads [`ViewerPosition`] and converts it to fractional chunk-grid space
///    by dividing by `CHUNK_SIZE`.
/// 2. Computes `load_radius_chunks = ceil(load_distance / CHUNK_SIZE)` and
///    iterates every integer coordinate in the `[-r, r]³` bounding cube.
/// 3. For each coordinate whose world-space center is within `load_distance`
///    and that is neither already loaded nor pending in [`ChunkManager`]:
///    - calls [`ChunkManager::mark_pending`] to prevent duplicate requests,
///    - writes a [`ChunkLoadRequest`] for the generation system to consume.
/// 4. Collects all currently loaded chunk coordinates whose world-space center
///    distance exceeds `unload_distance`, then writes one
///    [`ChunkUnloadRequest`] per coordinate.
///
/// The unload pass uses a separate `Vec` to avoid holding a shared borrow on
/// [`ChunkManager`] while emitting messages that may mutate it in the same
/// frame.
pub fn streaming_system(
    viewer: Res<ViewerPosition>,
    mut manager: ResMut<ChunkManager>,
    mut load_writer: MessageWriter<ChunkLoadRequest>,
    mut unload_writer: MessageWriter<ChunkUnloadRequest>,
) {
    let chunk_size_f = CHUNK_SIZE as f32;

    // Viewer position expressed in chunk-grid units (fractional).
    let viewer_chunk_f = viewer.position / chunk_size_f;

    // ── Emit load requests ────────────────────────────────────────────────

    let load_radius_chunks = (manager.load_distance / chunk_size_f).ceil() as i32;

    for dx in -load_radius_chunks..=load_radius_chunks {
        for dy in -load_radius_chunks..=load_radius_chunks {
            for dz in -load_radius_chunks..=load_radius_chunks {
                let coord: ChunkCoord = IVec3::new(
                    viewer_chunk_f.x.floor() as i32 + dx,
                    viewer_chunk_f.y.floor() as i32 + dy,
                    viewer_chunk_f.z.floor() as i32 + dz,
                );

                // World-space center of this chunk.
                let center = coord.as_vec3() * chunk_size_f + Vec3::splat(chunk_size_f * 0.5);
                let dist = center.distance(viewer.position);

                if dist <= manager.load_distance
                    && !manager.is_loaded(coord)
                    && !manager.is_pending(coord)
                {
                    manager.mark_pending(coord);
                    load_writer.write(ChunkLoadRequest(coord));
                }
            }
        }
    }

    // ── Emit unload requests ──────────────────────────────────────────────

    // Collect coords to unload first to avoid borrowing `manager` mutably
    // while iterating its loaded set.
    let to_unload: Vec<ChunkCoord> = manager
        .iter_loaded()
        .filter_map(|(coord, _entity)| {
            let center = coord.as_vec3() * chunk_size_f + Vec3::splat(chunk_size_f * 0.5);
            if center.distance(viewer.position) > manager.unload_distance {
                Some(coord)
            } else {
                None
            }
        })
        .collect();

    for coord in to_unload {
        unload_writer.write(ChunkUnloadRequest(coord));
    }
}
