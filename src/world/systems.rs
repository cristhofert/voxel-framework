//! Streaming system that drives chunk load/unload based on viewer position.
//!
//! The [`streaming_system`] runs every frame, computes which chunk coordinates
//! fall within the load sphere around [`ViewerPosition`], and emits
//! [`ChunkLoadRequest`] or [`ChunkUnloadRequest`] messages accordingly.

use bevy::prelude::*;

use crate::chunk::{ChunkCoord, CHUNK_SIZE};

use super::{ChunkLoadRequest, ChunkManager, ChunkUnloadRequest};

// ─── ViewerPosition ───────────────────────────────────────────────────────────

/// World-space position of the primary viewer (player / camera).
///
/// Update this resource every frame to drive the chunk streaming system.
/// The [`streaming_system`] reads it each tick to determine which chunk
/// coordinates should be loaded or unloaded.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::world::ViewerPosition;
///
/// fn move_camera(mut viewer: ResMut<ViewerPosition>, transform: Query<&Transform, With<Camera3d>>) {
///     if let Ok(t) = transform.single() {
///         viewer.position = t.translation;
///     }
/// }
/// ```
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct ViewerPosition {
    /// Current world-space position of the viewer.
    pub position: Vec3,
}

// ─── streaming_system ────────────────────────────────────────────────────────

/// Drives chunk load and unload decisions based on [`ViewerPosition`].
///
/// Each frame this system:
/// 1. Converts `viewer.position` to a chunk-grid coordinate.
/// 2. Iterates every integer coord within a bounding cube of radius
///    `ceil(load_distance / CHUNK_SIZE)` around the viewer chunk.
/// 3. Emits [`ChunkLoadRequest`] for coords inside the load sphere that are
///    neither already loaded nor pending.
/// 4. Emits [`ChunkUnloadRequest`] for all currently loaded chunks whose
///    world-space center has drifted beyond the unload sphere.
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
