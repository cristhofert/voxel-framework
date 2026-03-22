//! World loading state tracking.
//!
//! Provides [`LoadingProgress`] messages and a [`WorldReady`] message
//! emitted once the initial sphere of chunks around the viewer is loaded.

use bevy::prelude::*;

use crate::chunk::{CHUNK_SIZE, VOXEL_SIZE};
use crate::generator::GeneratorSystems;

use super::ChunkManager;

// ─── Messages ────────────────────────────────────────────────────────────────

/// Emitted every frame while the initial chunks are loading.
/// `progress` is 0.0–1.0.
#[derive(Message, Debug, Clone)]
pub struct LoadingProgress {
    pub loaded: u32,
    pub total: u32,
    pub progress: f32,
}

/// Emitted once when all chunks within `load_distance` are loaded for the
/// first time. After this fires, the world is ready to display.
#[derive(Message, Debug, Clone)]
pub struct WorldReady;

// ─── Resource ─────────────────────────────────────────────────────────────────

/// Tracks whether [`WorldReady`] has already been emitted.
#[derive(Resource, Default)]
pub struct WorldLoadingState {
    pub ready: bool,
    /// Total chunks expected in the initial load sphere.
    pub expected: u32,
}

// ─── Helpers (pure, testable) ─────────────────────────────────────────────────

/// Returns the number of chunk coords within a cubic grid of `[-r, r]^3` that
/// also fall inside the sphere of radius `r` (measured in chunk units).
pub(crate) fn count_chunks_in_sphere(radius_in_chunks: i32) -> u32 {
    let r = radius_in_chunks;
    let mut count = 0u32;
    for dx in -r..=r {
        for dy in -r..=r {
            for dz in -r..=r {
                // Use squared distance to stay in integer arithmetic.
                let dist_sq = dx * dx + dy * dy + dz * dz;
                if dist_sq <= r * r {
                    count += 1;
                }
            }
        }
    }
    count
}

/// Computes the loading progress fraction given `loaded` and `expected` counts.
/// Returns 0.0 when `expected` is 0 to avoid division by zero.
pub(crate) fn compute_progress(loaded: u32, expected: u32) -> f32 {
    if expected == 0 {
        0.0
    } else {
        (loaded as f32 / expected as f32).min(1.0)
    }
}

// ─── System ───────────────────────────────────────────────────────────────────

/// Tracks initial chunk loading and emits [`LoadingProgress`] and
/// [`WorldReady`] messages.
///
/// Runs in [`Update`], ordered after [`GeneratorSystems`].
pub fn loading_progress_system(
    manager: Res<ChunkManager>,
    mut loading_state: ResMut<WorldLoadingState>,
    mut progress_writer: MessageWriter<LoadingProgress>,
    mut ready_writer: MessageWriter<WorldReady>,
) {
    // Compute the expected chunk count once.
    if loading_state.expected == 0 {
        let radius_in_chunks =
            (manager.load_distance / (CHUNK_SIZE as f32 * VOXEL_SIZE)).ceil() as i32;
        loading_state.expected = count_chunks_in_sphere(radius_in_chunks);
    }

    if loading_state.ready {
        return;
    }

    let loaded = manager.loaded_count() as u32;
    let expected = loading_state.expected;
    let progress = compute_progress(loaded, expected);

    progress_writer.write(LoadingProgress {
        loaded,
        total: expected,
        progress,
    });

    if progress >= 1.0 {
        ready_writer.write(WorldReady);
        loading_state.ready = true;
    }
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Plugin that tracks initial world loading and emits [`LoadingProgress`] and
/// [`WorldReady`] messages.
pub struct LoadingPlugin;

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldLoadingState>();
        app.add_message::<LoadingProgress>();
        app.add_message::<WorldReady>();
        app.add_systems(Update, loading_progress_system.after(GeneratorSystems));
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loading_progress_zero_when_nothing_loaded() {
        // With nothing loaded, progress should be 0.0.
        let progress = compute_progress(0, 100);
        assert_eq!(progress, 0.0);
    }

    #[test]
    fn test_loading_progress_zero_when_expected_is_zero() {
        // Guard against division by zero when expected hasn't been computed yet.
        let progress = compute_progress(0, 0);
        assert_eq!(progress, 0.0);
    }

    #[test]
    fn test_loading_progress_partial() {
        let progress = compute_progress(50, 100);
        assert!((progress - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_loading_ready_when_fully_loaded() {
        // When loaded >= expected, progress reaches 1.0 — WorldReady condition met.
        let progress = compute_progress(100, 100);
        assert_eq!(progress, 1.0);
        // Simulate the ready trigger.
        assert!(progress >= 1.0);
    }

    #[test]
    fn test_loading_ready_when_overloaded() {
        // loaded can exceed expected briefly; progress is clamped to 1.0.
        let progress = compute_progress(110, 100);
        assert_eq!(progress, 1.0);
    }

    #[test]
    fn test_count_chunks_in_sphere_radius_zero() {
        // Radius 0 → only the origin chunk.
        assert_eq!(count_chunks_in_sphere(0), 1);
    }

    #[test]
    fn test_count_chunks_in_sphere_radius_one() {
        // Radius 1 → 7 chunks (origin + 6 face-adjacent).
        assert_eq!(count_chunks_in_sphere(1), 7);
    }

    #[test]
    fn test_count_chunks_in_sphere_positive_count() {
        // Any positive radius should yield a positive chunk count.
        assert!(count_chunks_in_sphere(5) > 0);
    }
}
