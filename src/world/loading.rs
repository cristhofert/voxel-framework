//! Initial world loading lifecycle — progress tracking and the ready signal.
//!
//! # Overview
//!
//! When a world first starts, the streaming system begins requesting chunks
//! around the viewer. This module tracks how many of those chunks have been
//! fully generated and meshed, and fires a one-shot signal when the world is
//! ready to display.
//!
//! # What "loaded" means
//!
//! A chunk is considered **loaded** when it is registered in [`ChunkManager`]
//! via [`ChunkManager::register`] — meaning voxel data has been generated
//! **and** the renderable mesh entity has been spawned. The total number of
//! chunks expected in the initial load sphere is computed once from
//! `load_distance` using `count_chunks_in_sphere`.
//!
//! # Event flow
//!
//! ```text
//! WorldPlugin starts
//!     │
//!     ▼
//! loading_progress_system runs each frame (after GeneratorSystems)
//!     │
//!     ├─ emits LoadingProgress { loaded, total, progress }  every frame
//!     │
//!     └─ when progress >= 1.0:
//!            emits WorldReady (once)
//!            sets WorldLoadingState::ready = true → system becomes a no-op
//! ```
//!
//! # Usage
//!
//! Add [`LoadingPlugin`] to your [`App`] alongside [`super::WorldPlugin`].
//! Subscribe to [`LoadingProgress`] to drive a loading screen, and to
//! [`WorldReady`] to transition out of it:
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use voxel_framework::world::{WorldPlugin, LoadingPlugin, LoadingProgress, WorldReady};
//!
//! fn on_world_ready(mut reader: MessageReader<WorldReady>) {
//!     for _ in reader.read() {
//!         println!("world is ready!");
//!     }
//! }
//!
//! App::new()
//!     .add_plugins((WorldPlugin, LoadingPlugin))
//!     .add_systems(Update, on_world_ready)
//!     .run();
//! ```

use bevy::prelude::*;

use crate::chunk::{CHUNK_SIZE, VOXEL_SIZE};
use crate::generator::GeneratorSystems;

use super::ChunkManager;

// ─── Messages ────────────────────────────────────────────────────────────────

/// Emitted every frame while the initial chunk sphere is still loading.
///
/// [`loading_progress_system`] writes this message once per frame from the
/// moment the world starts until [`WorldReady`] fires. Once [`WorldReady`] has
/// been emitted, this message is no longer written.
///
/// - `loaded` — number of chunks currently registered in [`ChunkManager`].
/// - `total` — expected chunk count for the full load sphere (computed once
///   from `load_distance` on the first frame).
/// - `progress` — `loaded / total`, clamped to `[0.0, 1.0]`.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::world::LoadingProgress;
///
/// fn show_progress(mut reader: MessageReader<LoadingProgress>) {
///     for msg in reader.read() {
///         println!("loading: {:.0}%", msg.progress * 100.0);
///     }
/// }
/// ```
#[derive(Message, Debug, Clone)]
pub struct LoadingProgress {
    pub loaded: u32,
    pub total: u32,
    pub progress: f32,
}

/// One-shot signal emitted when the initial chunk sphere is fully loaded.
///
/// [`loading_progress_system`] emits this message exactly once, on the first
/// frame that [`LoadingProgress::progress`] reaches `1.0`. After it fires,
/// [`WorldLoadingState::ready`] is set to `true` and the progress system
/// becomes a no-op — no further [`LoadingProgress`] or `WorldReady` messages
/// are written.
///
/// Use this event to transition your app out of a loading screen, enable
/// gameplay input, or trigger any one-time "world is ready" logic.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::world::WorldReady;
///
/// fn on_world_ready(mut reader: MessageReader<WorldReady>) {
///     for _ in reader.read() {
///         println!("all chunks generated — entering gameplay");
///     }
/// }
///
/// # fn main() {
/// App::new()
///     .add_message::<WorldReady>()
///     .add_systems(Update, on_world_ready)
///     .run();
/// # }
/// ```
#[derive(Message, Debug, Clone)]
pub struct WorldReady;

// ─── Resource ─────────────────────────────────────────────────────────────────

/// Internal resource that guards the one-shot [`WorldReady`] emission.
///
/// Inserted by [`LoadingPlugin`] via `init_resource`. [`loading_progress_system`]
/// reads and mutates this resource each frame:
///
/// - `expected` is computed once on the first frame from [`ChunkManager::load_distance`]
///   and cached here to avoid recomputing it every tick.
/// - `ready` is set to `true` after [`WorldReady`] fires, turning the system
///   into a no-op for the remainder of the session.
///
/// This resource is not intended to be mutated by user code. To check whether
/// the world has finished loading, subscribe to [`WorldReady`] instead.
#[derive(Resource, Default)]
pub struct WorldLoadingState {
    /// Set to `true` after [`WorldReady`] has been emitted.
    pub ready: bool,
    /// Total chunks expected in the initial load sphere (computed once).
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

/// Tracks initial chunk loading and emits [`LoadingProgress`] and [`WorldReady`].
///
/// Scheduled in [`Update`], ordered after [`GeneratorSystems`] so that any
/// chunks completed in the current frame are already counted before progress
/// is computed.
///
/// On the first invocation, the expected chunk count is derived from
/// [`ChunkManager::load_distance`] and cached in [`WorldLoadingState::expected`].
/// Subsequent frames skip this computation and go straight to the progress
/// check.
///
/// Once [`WorldLoadingState::ready`] is `true` (set after [`WorldReady`]
/// fires), the system returns immediately without writing any messages.
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

/// Plugin that tracks initial world loading and emits [`LoadingProgress`] and [`WorldReady`].
///
/// Registers the following on the Bevy [`App`]:
///
/// - [`WorldLoadingState`] resource (via `init_resource`).
/// - [`LoadingProgress`] and [`WorldReady`] messages.
/// - [`loading_progress_system`] scheduled in [`Update`] after [`GeneratorSystems`].
///
/// Must be used together with [`super::WorldPlugin`], which provides the
/// [`ChunkManager`] resource this plugin reads.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::world::{WorldPlugin, LoadingPlugin, WorldReady};
///
/// fn on_ready(mut reader: MessageReader<WorldReady>) {
///     for _ in reader.read() {
///         println!("world ready — starting gameplay");
///     }
/// }
///
/// App::new()
///     .add_plugins((WorldPlugin, LoadingPlugin))
///     .add_systems(Update, on_ready)
///     .run();
/// ```
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
