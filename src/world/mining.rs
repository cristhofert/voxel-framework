//! Voxel mining (block-breaking) API.
//!
//! # Design — pure functions, no implicit systems
//!
//! All logic lives in free functions ([`tick_mining`], [`stop_mining`]) so
//! that game code retains full control over when and how mining occurs.
//! [`MiningPlugin`] only registers the required resources and message types;
//! it adds **no** systems of its own.
//!
//! # Integration pattern
//!
//! ```no_run
//! # use bevy::prelude::*;
//! # use voxel_framework::world::mining::{MiningPlugin, MiningConfig, MiningState,
//! #     MiningProgress, VoxelBroken, tick_mining, stop_mining};
//! # use voxel_framework::world::{ChunkManager, ChunkModified};
//! # use voxel_framework::chunk::Chunk;
//! # use voxel_framework::voxel::DefaultVoxel;
//! // 1. Add the plugin once when building the App.
//! //    App::new().add_plugins(MiningPlugin);
//!
//! // 2. In your input system, call tick_mining every frame while the
//! //    mine button is held, or stop_mining when it is released.
//! fn my_mining_system(
//!     mut state:    ResMut<MiningState>,
//!     config:       Res<MiningConfig>,
//!     manager:      Res<ChunkManager>,
//!     mut chunks:   Query<&mut Chunk<DefaultVoxel>>,
//!     mut broken:   MessageWriter<VoxelBroken>,
//!     mut progress: MessageWriter<MiningProgress>,
//!     mut modified: MessageWriter<ChunkModified>,
//!     time:         Res<Time>,
//! ) {
//!     let mining = true; // replace with real input check
//!     if mining {
//!         tick_mining(
//!             IVec3::ZERO, (8, 8, 8),
//!             time.delta_secs(),
//!             &mut state, &config, &manager,
//!             &mut chunks, &mut broken, &mut progress, &mut modified,
//!         );
//!     } else {
//!         stop_mining(&mut state);
//!     }
//! }
//! ```

use bevy::prelude::*;

use crate::chunk::Chunk;
use crate::voxel::VoxelType;

use super::{ChunkManager, ChunkModified};

// ─── Resources ───────────────────────────────────────────────────────────────

/// Configuration for the mining system.
///
/// Insert this resource before adding [`MiningPlugin`], or mutate it at
/// runtime to adjust difficulty.
#[derive(Resource, Debug, Clone)]
pub struct MiningConfig {
    /// Time in seconds required to break a single voxel.
    ///
    /// Must be greater than zero.  **Default: `1.5` seconds.**
    pub break_time: f32,
}

impl Default for MiningConfig {
    fn default() -> Self {
        Self { break_time: 1.5 }
    }
}

/// Runtime state for the mining system.
///
/// Tracks which voxel is currently being mined and how much time has
/// accumulated toward breaking it.  There is at most one active target at a
/// time; switching targets resets elapsed time automatically inside
/// [`tick_mining`].
///
/// Call [`stop_mining`] whenever the mine button is released — failing to do
/// so leaves stale progress that will carry over to the next target.
#[derive(Resource, Default, Debug)]
pub struct MiningState {
    /// The chunk coordinate and local voxel index `(x, y, z)` currently being
    /// mined, or `None` when no mining is in progress.
    pub target: Option<(IVec3, (usize, usize, usize))>,
    /// Accumulated break time in seconds, in the range `[0.0, break_time]`.
    pub elapsed: f32,
    /// Normalised break progress in the range `[0.0, 1.0]`.
    ///
    /// Equals `elapsed / break_time`.  Reaches `1.0` when the voxel breaks.
    pub progress: f32,
}

// ─── Messages ────────────────────────────────────────────────────────────────

/// Message fired once when a voxel's break time is fully elapsed.
///
/// Emitted by [`tick_mining`] in the same frame the voxel is destroyed.
/// Systems that react to block destruction (e.g. particle effects, inventory
/// updates) should read this message.
#[derive(Message, Debug, Clone)]
pub struct VoxelBroken {
    /// Chunk-grid coordinate of the chunk that contained the broken voxel.
    pub chunk_coord: IVec3,
    /// Local voxel index `(x, y, z)` within the chunk.  Each component is in
    /// `[0, CHUNK_SIZE)`.
    pub local: (usize, usize, usize),
    /// World-space center of the broken voxel.
    pub world_pos: Vec3,
}

/// Message sent every frame while a voxel is being mined.
///
/// Useful for driving crack-overlay animations, progress bars, or audio
/// feedback.  Emitted by [`tick_mining`] on every frame where mining is
/// active, including the frame the voxel breaks (where `progress` is `1.0`).
#[derive(Message, Debug, Clone)]
pub struct MiningProgress {
    /// Normalised break progress in `[0.0, 1.0]`.
    ///
    /// `0.0` on the first frame, `1.0` on the frame the voxel breaks.
    pub progress: f32,
    /// World-space center of the voxel being mined.
    pub world_pos: Vec3,
}

// ─── Free functions ───────────────────────────────────────────────────────────

/// Compute the world-space center of a voxel given its chunk coord and local index.
fn voxel_world_pos(chunk_coord: IVec3, local: (usize, usize, usize)) -> Vec3 {
    let chunk_origin = chunk_coord.as_vec3() * crate::chunk::CHUNK_SIZE as f32;
    chunk_origin
        + Vec3::new(
            local.0 as f32 + 0.5,
            local.1 as f32 + 0.5,
            local.2 as f32 + 0.5,
        )
}

/// Advance mining by one frame toward breaking the target voxel.
///
/// Call this **every frame** while the mine button is held.  When the
/// accumulated time reaches `config.break_time` the function:
///
/// 1. Sets the target voxel to air (if the chunk is loaded).
/// 2. Writes a [`VoxelBroken`] message.
/// 3. Writes a [`ChunkModified`] message to trigger remeshing.
/// 4. Resets `state` so a new target can start fresh.
///
/// A [`MiningProgress`] message is written on **every** call, including the
/// break frame.
///
/// If `target_chunk` / `target_local` differs from the previous frame, elapsed
/// time is reset to zero automatically — no need to call [`stop_mining`] when
/// switching targets mid-frame.
///
/// # Parameters
///
/// - `target_chunk` — Chunk-grid coordinate of the chunk containing the target
///   voxel.
/// - `target_local` — Local `(x, y, z)` voxel index within the chunk.
/// - `delta_secs` — Frame delta time in seconds (typically `time.delta_secs()`).
/// - `state` — Mutable reference to the shared [`MiningState`] resource.
/// - `config` — Read-only mining configuration (break time).
/// - `manager` — Chunk registry used to look up the chunk entity.
/// - `chunks` — Mutable query used to set voxels to air on break.
/// - `broken` — Message writer for [`VoxelBroken`].
/// - `progress_writer` — Message writer for [`MiningProgress`].
/// - `modified` — Message writer for [`crate::world::ChunkModified`].
///
/// # Examples
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use voxel_framework::world::mining::{MiningConfig, MiningState, MiningProgress, VoxelBroken, tick_mining};
/// # use voxel_framework::world::{ChunkManager, ChunkModified};
/// # use voxel_framework::chunk::Chunk;
/// # use voxel_framework::voxel::DefaultVoxel;
/// fn my_system(
///     mut state:    ResMut<MiningState>,
///     config:       Res<MiningConfig>,
///     manager:      Res<ChunkManager>,
///     mut chunks:   Query<&mut Chunk<DefaultVoxel>>,
///     mut broken:   MessageWriter<VoxelBroken>,
///     mut progress: MessageWriter<MiningProgress>,
///     mut modified: MessageWriter<ChunkModified>,
///     time:         Res<Time>,
/// ) {
///     tick_mining(
///         IVec3::ZERO, (4, 0, 4),
///         time.delta_secs(),
///         &mut state, &config, &manager,
///         &mut chunks, &mut broken, &mut progress, &mut modified,
///     );
/// }
/// ```
#[allow(clippy::too_many_arguments)]
pub fn tick_mining<V: VoxelType>(
    target_chunk: IVec3,
    target_local: (usize, usize, usize),
    delta_secs: f32,
    state: &mut MiningState,
    config: &MiningConfig,
    manager: &ChunkManager,
    chunks: &mut Query<&mut Chunk<V>>,
    broken: &mut MessageWriter<VoxelBroken>,
    progress_writer: &mut MessageWriter<MiningProgress>,
    modified: &mut MessageWriter<ChunkModified>,
) {
    // Reset elapsed when target changes.
    if state.target != Some((target_chunk, target_local)) {
        state.elapsed = 0.0;
        state.progress = 0.0;
        state.target = Some((target_chunk, target_local));
    }

    state.elapsed += delta_secs;
    state.progress = (state.elapsed / config.break_time).min(1.0);

    let world_pos = voxel_world_pos(target_chunk, target_local);

    progress_writer.write(MiningProgress {
        progress: state.progress,
        world_pos,
    });

    if state.progress >= 1.0 {
        // Destroy the voxel if the chunk is loaded.
        if let Some(entity) = manager.get_entity(target_chunk) {
            if let Ok(mut chunk) = chunks.get_mut(entity) {
                let (x, y, z) = target_local;
                chunk.set(x, y, z, V::air());
            }
        }

        broken.write(VoxelBroken {
            chunk_coord: target_chunk,
            local: target_local,
            world_pos,
        });

        modified.write(ChunkModified(target_chunk));

        // Reset state.
        state.target = None;
        state.elapsed = 0.0;
        state.progress = 0.0;
    }
}

/// Stop mining and reset all accumulated progress.
///
/// Call this whenever the mine button is released.  Failing to call
/// `stop_mining` means any partial progress persists, and the next target
/// will appear to start partially broken.
///
/// # Examples
///
/// ```no_run
/// # use voxel_framework::world::mining::{MiningState, stop_mining};
/// fn on_button_release(mut state: bevy::prelude::ResMut<MiningState>) {
///     stop_mining(&mut state);
/// }
/// ```
pub fn stop_mining(state: &mut MiningState) {
    state.target = None;
    state.elapsed = 0.0;
    state.progress = 0.0;
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Bevy plugin that registers the mining subsystem.
///
/// Adds the following to the [`bevy::prelude::App`]:
///
/// - [`MiningConfig`] resource (initialised with defaults — `break_time: 1.5 s`)
/// - [`MiningState`] resource (initialised at default / idle)
/// - [`VoxelBroken`] message type
/// - [`MiningProgress`] message type
///
/// Does **not** add any systems.  Game code is responsible for calling
/// [`tick_mining`] and [`stop_mining`] from its own update systems.
pub struct MiningPlugin;

impl Plugin for MiningPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MiningConfig>();
        app.init_resource::<MiningState>();
        app.add_message::<VoxelBroken>();
        app.add_message::<MiningProgress>();
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> MiningConfig {
        MiningConfig { break_time: 1.5 }
    }

    // ── stop_mining ──────────────────────────────────────────────────────────

    #[test]
    fn stop_mining_resets_state() {
        let mut state = MiningState {
            target: Some((IVec3::ZERO, (1, 2, 3))),
            elapsed: 0.8,
            progress: 0.5,
        };
        stop_mining(&mut state);
        assert!(state.target.is_none());
        assert_eq!(state.elapsed, 0.0);
        assert_eq!(state.progress, 0.0);
    }

    // ── progress calculation ──────────────────────────────────────────────────

    #[test]
    fn test_mining_progress_accumulates() {
        let config = default_config();
        let mut state = MiningState::default();

        // Manually simulate two ticks without Bevy context.
        let target = (IVec3::ZERO, (0usize, 0usize, 0usize));

        // First tick: set target and advance elapsed.
        if state.target != Some(target) {
            state.elapsed = 0.0;
            state.progress = 0.0;
            state.target = Some(target);
        }
        state.elapsed += 0.3;
        state.progress = (state.elapsed / config.break_time).min(1.0);

        assert!((state.elapsed - 0.3).abs() < f32::EPSILON);

        // Second tick: same target, keep accumulating.
        state.elapsed += 0.4;
        state.progress = (state.elapsed / config.break_time).min(1.0);

        assert!((state.elapsed - 0.7).abs() < f32::EPSILON);
        assert!(state.progress > 0.0 && state.progress < 1.0);
    }

    #[test]
    fn test_mining_resets_on_target_change() {
        let config = default_config();
        let mut state = MiningState::default();

        let target_a = (IVec3::ZERO, (0usize, 0usize, 0usize));
        let target_b = (IVec3::ONE, (1usize, 1usize, 1usize));

        // Mine target A for a bit.
        state.target = Some(target_a);
        state.elapsed += 0.5;
        state.progress = (state.elapsed / config.break_time).min(1.0);

        assert!(state.elapsed > 0.0);

        // Switch to target B — should reset.
        if state.target != Some(target_b) {
            state.elapsed = 0.0;
            state.progress = 0.0;
            state.target = Some(target_b);
        }

        assert_eq!(state.elapsed, 0.0);
        assert_eq!(state.progress, 0.0);
        assert_eq!(state.target, Some(target_b));
    }

    #[test]
    fn test_mining_completes_at_break_time() {
        let config = default_config();
        let mut state = MiningState::default();

        // Simulate ticks until elapsed >= break_time.
        let target = (IVec3::ZERO, (0usize, 0usize, 0usize));
        state.target = Some(target);

        let mut completed = false;
        let tick = 0.5_f32;
        for _ in 0..10 {
            state.elapsed += tick;
            state.progress = (state.elapsed / config.break_time).min(1.0);
            if state.progress >= 1.0 {
                completed = true;
                break;
            }
        }

        assert!(completed, "mining should complete within 10 ticks of 0.5s");
        assert_eq!(state.progress, 1.0);
    }

    #[test]
    fn progress_is_clamped_to_one() {
        let config = default_config();
        let mut state = MiningState::default();

        // Way more elapsed than break_time.
        state.elapsed = 99.0;
        state.progress = (state.elapsed / config.break_time).min(1.0);

        assert_eq!(state.progress, 1.0);
    }
}
