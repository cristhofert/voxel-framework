//! Voxel mining API.
//!
//! Provides break-time tracking and voxel destruction events.
//! Game code drives mining by calling `start_mining` / `stop_mining`
//! each frame and reacting to [`VoxelBroken`] and [`MiningProgress`] messages.

use bevy::prelude::*;

use crate::chunk::Chunk;
use crate::voxel::VoxelType;

use super::{ChunkManager, ChunkModified};

// ─── Resources ───────────────────────────────────────────────────────────────

/// Configuration for the mining system.
#[derive(Resource, Debug, Clone)]
pub struct MiningConfig {
    /// Time in seconds to break one voxel. Default: 1.5
    pub break_time: f32,
}

impl Default for MiningConfig {
    fn default() -> Self {
        Self { break_time: 1.5 }
    }
}

/// Tracks current mining progress for one target voxel.
#[derive(Resource, Default, Debug)]
pub struct MiningState {
    /// The chunk coord + local coord being mined, if any.
    pub target: Option<(IVec3, (usize, usize, usize))>,
    /// Elapsed break time in seconds [0.0, break_time].
    pub elapsed: f32,
    /// Progress in [0.0, 1.0].
    pub progress: f32,
}

// ─── Messages ────────────────────────────────────────────────────────────────

/// Emitted when a voxel finishes breaking.
#[derive(Message, Debug, Clone)]
pub struct VoxelBroken {
    pub chunk_coord: IVec3,
    pub local: (usize, usize, usize),
    pub world_pos: Vec3,
}

/// Emitted every frame while mining is in progress.
#[derive(Message, Debug, Clone)]
pub struct MiningProgress {
    pub progress: f32,
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

/// Begin or continue mining the voxel at the given chunk+local coord.
///
/// Call this every frame while the mine button is held. When `break_time` is
/// reached the voxel is set to air, a [`VoxelBroken`] message is written, a
/// [`ChunkModified`] message is written to trigger remeshing, and the state is
/// reset. A [`MiningProgress`] message is written every frame while mining.
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

/// Stop mining (button released). Resets progress.
pub fn stop_mining(state: &mut MiningState) {
    state.target = None;
    state.elapsed = 0.0;
    state.progress = 0.0;
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Plugin that registers mining resources and messages.
///
/// Does **not** add any systems — game code calls [`tick_mining`] and
/// [`stop_mining`] from its own systems.
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
