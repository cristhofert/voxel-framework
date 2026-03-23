//! Voxel placement (block-placing) API.
//!
//! # Design — pure functions, no implicit systems
//!
//! This module mirrors the mining module's architecture: all placement logic
//! lives in free functions so game code retains full scheduling control.
//! [`PlacementPlugin`] only registers the required resources and message
//! types; it adds **no** systems of its own.
//!
//! # Integration pattern
//!
//! ```no_run
//! # use bevy::prelude::*;
//! # use voxel_framework::world::placement::{
//! #     PlacementPlugin, PlacementConfig, PlacementState, VoxelPlaced,
//! #     try_place_voxel, tick_placement,
//! # };
//! # use voxel_framework::world::{ChunkManager, ChunkModified};
//! # use voxel_framework::chunk::Chunk;
//! # use voxel_framework::voxel::DefaultVoxel;
//! // 1. Add the plugin once when building the App.
//! //    App::new().add_plugins(PlacementPlugin);
//!
//! // 2. In your input system, call tick_placement every frame and
//! //    try_place_voxel once per button press.
//! fn my_placement_system(
//!     mut state:    ResMut<PlacementState>,
//!     config:       Res<PlacementConfig>,
//!     manager:      Res<ChunkManager>,
//!     mut chunks:   Query<&mut Chunk<DefaultVoxel>>,
//!     mut placed:   MessageWriter<VoxelPlaced>,
//!     mut modified: MessageWriter<ChunkModified>,
//!     time:         Res<Time>,
//! ) {
//!     tick_placement(time.delta_secs(), &mut state, &config);
//!
//!     let place_pressed = true; // replace with real input
//!     if place_pressed {
//!         try_place_voxel(
//!             IVec3::ZERO, (8, 8, 8), DefaultVoxel::Rock,
//!             &mut state, &config, &manager,
//!             &mut chunks, &mut placed, &mut modified,
//!         );
//!     }
//! }
//! ```

use bevy::prelude::*;

use crate::chunk::Chunk;
use crate::voxel::VoxelType;

use super::{ChunkManager, ChunkModified};

// ─── Resources ───────────────────────────────────────────────────────────────

/// Configuration for the block placement system.
///
/// Insert this resource before adding [`PlacementPlugin`], or mutate it at
/// runtime to change placement speed.
#[derive(Resource, Debug, Clone)]
pub struct PlacementConfig {
    /// Minimum time in seconds that must elapse between two successive placements.
    ///
    /// Prevents click-spam from placing blocks faster than intended.
    /// **Default: `0.25` seconds.**
    pub cooldown: f32,
}

impl Default for PlacementConfig {
    fn default() -> Self {
        Self { cooldown: 0.25 }
    }
}

/// Runtime state for the placement cooldown timer.
///
/// Initialised to `cooldown_elapsed = 0.0` (on cooldown) so the player
/// cannot place a block the instant the game starts.  Call
/// [`tick_placement`] every frame to advance the timer.
#[derive(Resource, Default, Debug)]
pub struct PlacementState {
    /// Seconds elapsed since the last successful placement.
    ///
    /// Capped at `config.cooldown`; placement is allowed when this value
    /// equals or exceeds that threshold.
    pub cooldown_elapsed: f32,
}

impl PlacementState {
    /// Returns true if placement is currently allowed (cooldown expired).
    pub fn can_place(&self, config: &PlacementConfig) -> bool {
        self.cooldown_elapsed >= config.cooldown
    }
}

// ─── Messages ────────────────────────────────────────────────────────────────

/// Message fired once when a voxel is successfully placed.
///
/// Emitted by [`try_place_voxel`] on the same frame the voxel is written.
/// Systems that react to new blocks (e.g. sound effects, achievement tracking)
/// should read this message.
#[derive(Message, Debug, Clone)]
pub struct VoxelPlaced {
    /// Chunk-grid coordinate of the chunk that received the new voxel.
    pub chunk_coord: IVec3,
    /// Local voxel index `(x, y, z)` within the chunk.  Each component is in
    /// `[0, CHUNK_SIZE)`.
    pub local: (usize, usize, usize),
    /// World-space center of the newly placed voxel.
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

/// Attempt to place a voxel at the given chunk-local position.
///
/// Call this **once** on the frame the place button is pressed — not every
/// frame.
///
/// The function returns `false` (and does nothing) in two cases:
///
/// 1. **Cooldown active** — `state.cooldown_elapsed < config.cooldown`.  Call
///    [`tick_placement`] every frame so the timer advances.
/// 2. **Target already solid** — the destination voxel is not air; placement
///    into occupied cells is not allowed.
///
/// On success the function:
///
/// 1. Writes `voxel` into the chunk at `(target_local.0, target_local.1,
///    target_local.2)`.
/// 2. Resets `state.cooldown_elapsed` to `0.0`.
/// 3. Writes a [`VoxelPlaced`] message.
/// 4. Writes a [`crate::world::ChunkModified`] message to trigger remeshing.
///
/// # Parameters
///
/// - `target_chunk` — Chunk-grid coordinate of the destination chunk.
/// - `target_local` — Local `(x, y, z)` voxel index within the chunk.
/// - `voxel` — The voxel value to write.
/// - `state` — Mutable reference to the shared [`PlacementState`] resource.
/// - `config` — Read-only placement configuration (cooldown).
/// - `manager` — Chunk registry used to look up the chunk entity.
/// - `chunks` — Mutable query used to write the new voxel.
/// - `placed` — Message writer for [`VoxelPlaced`].
/// - `modified` — Message writer for [`crate::world::ChunkModified`].
///
/// # Return value
///
/// `true` if the voxel was placed, `false` if placement was rejected.
///
/// # Examples
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use voxel_framework::world::placement::{PlacementConfig, PlacementState, VoxelPlaced, try_place_voxel};
/// # use voxel_framework::world::{ChunkManager, ChunkModified};
/// # use voxel_framework::chunk::Chunk;
/// # use voxel_framework::voxel::DefaultVoxel;
/// fn on_click(
///     mut state:    ResMut<PlacementState>,
///     config:       Res<PlacementConfig>,
///     manager:      Res<ChunkManager>,
///     mut chunks:   Query<&mut Chunk<DefaultVoxel>>,
///     mut placed:   MessageWriter<VoxelPlaced>,
///     mut modified: MessageWriter<ChunkModified>,
/// ) {
///     let placed_ok = try_place_voxel(
///         IVec3::ZERO, (8, 8, 8), DefaultVoxel::Rock,
///         &mut state, &config, &manager,
///         &mut chunks, &mut placed, &mut modified,
///     );
///     if !placed_ok {
///         println!("placement rejected (cooldown or occupied)");
///     }
/// }
/// ```
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

/// Advance the placement cooldown timer by `delta_secs`.
///
/// Call this **every frame** unconditionally (regardless of whether the player
/// is pressing the place button) so the cooldown always counts down.  The
/// elapsed value is capped at `config.cooldown` to prevent unbounded growth.
///
/// # Examples
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use voxel_framework::world::placement::{PlacementConfig, PlacementState, tick_placement};
/// fn every_frame(
///     mut state: ResMut<PlacementState>,
///     config:    Res<PlacementConfig>,
///     time:      Res<Time>,
/// ) {
///     tick_placement(time.delta_secs(), &mut state, &config);
/// }
/// ```
pub fn tick_placement(delta_secs: f32, state: &mut PlacementState, config: &PlacementConfig) {
    state.cooldown_elapsed = (state.cooldown_elapsed + delta_secs).min(config.cooldown);
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Bevy plugin that registers the placement subsystem.
///
/// Adds the following to the [`bevy::prelude::App`]:
///
/// - [`PlacementConfig`] resource (initialised with defaults — `cooldown: 0.25 s`)
/// - [`PlacementState`] resource (initialised at default / on cooldown)
/// - [`VoxelPlaced`] message type
///
/// Does **not** add any systems.  Game code is responsible for calling
/// [`try_place_voxel`] and [`tick_placement`] from its own update systems.
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
