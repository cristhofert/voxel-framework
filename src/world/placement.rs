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
//!             IVec3::Y, // face the block was placed against
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
    /// Integer face normal of the surface the block was placed against.
    ///
    /// Each component is `−1`, `0`, or `1`.  Use [`crate::chunk::face_to_state`]
    /// to convert this to a compact 3-bit state value.
    pub face_normal: IVec3,
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
///         IVec3::Y, // face normal: placed on top surface
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
    face_normal: IVec3,
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
    chunk.set_state(x, y, z, crate::chunk::face_to_state(face_normal));
    state.cooldown_elapsed = 0.0;

    let world_pos = voxel_world_pos(target_chunk, target_local);

    placed.write(VoxelPlaced {
        chunk_coord: target_chunk,
        local: target_local,
        world_pos,
        face_normal,
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

// ─── PlacementInputPlugin ─────────────────────────────────────────────────────

/// Convenience plugin that wires mouse input to the placement API automatically.
///
/// `PlacementInputPlugin<V>` adds a single [`Update`] system that:
///
/// 1. Advances the placement cooldown timer every frame via [`tick_placement`].
/// 2. Raycasts from the active [`Camera3d`] (max `6.0` world units) on a
///    **right mouse button** press.
/// 3. Calls [`try_place_voxel`] with the block type derived from the active
///    [`crate::items::HotbarState`] slot: `V::from_byte(selected + 1)`.
///    (Slot 0 → byte 1, slot 1 → byte 2, …)
///
/// # Prerequisites
///
/// [`PlacementPlugin`] must be added **before** this plugin so that the
/// required resources ([`PlacementState`], [`PlacementConfig`]) and message
/// channels ([`VoxelPlaced`]) are already registered.
///
/// [`crate::items::HotbarPlugin`] must also be added for hotbar slot selection.
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use voxel_framework::{VoxelPlugin, generator::{GeneratorPlugin, GeneratorResource}};
/// use voxel_framework::world::placement::PlacementInputPlugin;
/// use voxel_framework::voxel::DefaultTerrainVoxel;
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(VoxelPlugin::<DefaultTerrainVoxel>::new())
///     .add_plugins(PlacementInputPlugin::<DefaultTerrainVoxel>::new())
///     .run();
/// ```
pub struct PlacementInputPlugin<V: VoxelType> {
    _m: std::marker::PhantomData<V>,
}

impl<V: VoxelType> PlacementInputPlugin<V> {
    /// Creates a new [`PlacementInputPlugin`] for voxel type `V`.
    pub fn new() -> Self {
        Self { _m: std::marker::PhantomData }
    }
}

impl<V: VoxelType> Default for PlacementInputPlugin<V> {
    fn default() -> Self { Self::new() }
}

impl<V: VoxelType> Plugin for PlacementInputPlugin<V> {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, placement_input_system::<V>);
    }
}

/// System added by [`PlacementInputPlugin`].
///
/// Ticks the cooldown every frame, then raycasts and places on right-click.
/// Added automatically — do not add this system manually when using
/// [`PlacementInputPlugin`].
pub fn placement_input_system<V: VoxelType>(
    buttons: Res<ButtonInput<MouseButton>>,
    camera_q: Query<&GlobalTransform, With<Camera3d>>,
    manager: Res<ChunkManager>,
    mut chunks: Query<&mut crate::chunk::Chunk<V>>,
    mut state: ResMut<PlacementState>,
    config: Res<PlacementConfig>,
    hotbar: Res<crate::items::hotbar::HotbarState>,
    mut placed: MessageWriter<VoxelPlaced>,
    mut modified: MessageWriter<ChunkModified>,
    time: Res<Time>,
) {
    tick_placement(time.delta_secs(), &mut state, &config);

    if !buttons.just_pressed(MouseButton::Right) {
        return;
    }

    let Ok(cam_tf) = camera_q.single() else {
        return;
    };

    let origin    = cam_tf.translation();
    let direction = cam_tf.forward().as_vec3();

    // `transmute_lens` gives a read-only view without a second conflicting query.
    let mut lens = chunks.transmute_lens::<&crate::chunk::Chunk<V>>();
    let Some(hit) = super::raycast::raycast(
        origin, direction, 6.0, &manager, &lens.query(),
    ) else {
        return;
    };

    // Place one step back along the face normal (adjacent air voxel).
    let place_world = hit.position + hit.face_normal;
    let place_chunk = crate::chunk::world_to_chunk_coord(place_world);
    let place_local = crate::chunk::world_to_local_coord(place_world);

    // Map hotbar slot to voxel type: slot 0 → byte 1, slot 1 → byte 2, etc.
    let voxel = V::from_byte(hotbar.selected as u8 + 1);
    let face_normal = hit.face_normal.as_ivec3();

    try_place_voxel(
        place_chunk,
        place_local,
        voxel,
        face_normal,
        &mut state,
        &config,
        &manager,
        &mut chunks,
        &mut placed,
        &mut modified,
    );
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
