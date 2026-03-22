//! Voxel placement API.
//!
//! Provides cooldown tracking and voxel placement events.
//! Game code drives placement by calling [`try_place_voxel`] once when the
//! place button is pressed and [`tick_placement`] every frame, then reacting
//! to [`VoxelPlaced`] messages.

use bevy::prelude::*;

use crate::chunk::Chunk;
use crate::voxel::VoxelType;

use super::{ChunkManager, ChunkModified};

// ─── Resources ───────────────────────────────────────────────────────────────

/// Configuration for the block placement system.
#[derive(Resource, Debug, Clone)]
pub struct PlacementConfig {
    /// Minimum time in seconds between placements. Default: 0.25
    pub cooldown: f32,
}

impl Default for PlacementConfig {
    fn default() -> Self {
        Self { cooldown: 0.25 }
    }
}

/// Tracks cooldown state for block placement.
#[derive(Resource, Default, Debug)]
pub struct PlacementState {
    /// Elapsed time since last placement.
    pub cooldown_elapsed: f32,
}

impl PlacementState {
    /// Returns true if placement is currently allowed (cooldown expired).
    pub fn can_place(&self, config: &PlacementConfig) -> bool {
        self.cooldown_elapsed >= config.cooldown
    }
}

// ─── Messages ────────────────────────────────────────────────────────────────

/// Emitted when a voxel is successfully placed.
#[derive(Message, Debug, Clone)]
pub struct VoxelPlaced {
    pub chunk_coord: IVec3,
    pub local: (usize, usize, usize),
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

/// Attempt to place a voxel at the given chunk+local position.
/// Returns false if on cooldown or position is already solid.
///
/// Call this once when the place button is pressed (not every frame).
#[allow(clippy::too_many_arguments)]
pub fn try_place_voxel<V: VoxelType>(
    target_chunk: IVec3,
    target_local: (usize, usize, usize),
    voxel: V,
    state: &mut PlacementState,
    config: &PlacementConfig,
    manager: &ChunkManager,
    chunks: &mut Query<&mut Chunk<V>>,
    placed: &mut MessageWriter<VoxelPlaced>,
    modified: &mut MessageWriter<ChunkModified>,
) -> bool {
    if !state.can_place(config) {
        return false;
    }

    let Some(entity) = manager.get_entity(target_chunk) else {
        return false;
    };

    let Ok(mut chunk) = chunks.get_mut(entity) else {
        return false;
    };

    let (x, y, z) = target_local;

    // Only place into empty (non-solid) voxels.
    if chunk.get(x, y, z).is_solid() {
        return false;
    }

    chunk.set(x, y, z, voxel);
    state.cooldown_elapsed = 0.0;

    let world_pos = voxel_world_pos(target_chunk, target_local);

    placed.write(VoxelPlaced {
        chunk_coord: target_chunk,
        local: target_local,
        world_pos,
    });

    modified.write(ChunkModified(target_chunk));

    true
}

/// Advance the placement cooldown timer. Call every frame.
pub fn tick_placement(delta_secs: f32, state: &mut PlacementState, config: &PlacementConfig) {
    state.cooldown_elapsed = (state.cooldown_elapsed + delta_secs).min(config.cooldown);
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Plugin that registers placement resources and messages.
///
/// Does **not** add any systems — game code calls [`try_place_voxel`] and
/// [`tick_placement`] from its own systems.
pub struct PlacementPlugin;

impl Plugin for PlacementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlacementConfig>();
        app.init_resource::<PlacementState>();
        app.add_message::<VoxelPlaced>();
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> PlacementConfig {
        PlacementConfig { cooldown: 0.25 }
    }

    // ── can_place ─────────────────────────────────────────────────────────────

    #[test]
    fn test_can_place_when_cooldown_expired() {
        let config = default_config();
        let state = PlacementState {
            cooldown_elapsed: 0.25,
        };
        assert!(state.can_place(&config));
    }

    #[test]
    fn test_cannot_place_when_on_cooldown() {
        let config = default_config();
        let state = PlacementState {
            cooldown_elapsed: 0.10,
        };
        assert!(!state.can_place(&config));
    }

    // ── tick_placement ────────────────────────────────────────────────────────

    #[test]
    fn test_tick_advances_cooldown() {
        let config = default_config();
        let mut state = PlacementState { cooldown_elapsed: 0.0 };
        tick_placement(0.10, &mut state, &config);
        assert!((state.cooldown_elapsed - 0.10).abs() < f32::EPSILON);
    }

    #[test]
    fn test_tick_clamps_to_cooldown_max() {
        let config = default_config();
        let mut state = PlacementState { cooldown_elapsed: 0.20 };
        // Adding 0.20 would give 0.40, but max is 0.25.
        tick_placement(0.20, &mut state, &config);
        assert_eq!(state.cooldown_elapsed, config.cooldown);
    }
}
