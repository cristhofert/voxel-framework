//! Six-degrees-of-freedom (6DOF) player controller for zero-gravity environments.
//!
//! This module implements a first-person camera-player that moves freely in all
//! six directions with no gravity, no floor constraint, and no pitch clamp — the
//! equivalent of an astronaut or jetpack pilot. The camera entity IS the player:
//! there is no separate hierarchy between the logical player and the rendered
//! view.
//!
//! Collisions are resolved against loaded voxel chunks using a capsule-vs-AABB
//! algorithm that correctly handles any 6DOF orientation, including full roll and
//! inverted pitch.
//!
//! # Physics model
//!
//! Velocity is smoothed each frame using exponential lerp:
//!
//! - **Accelerating** (key held): `velocity = lerp(velocity, target, ACCEL * dt)`
//! - **Decelerating** (no input): `velocity = lerp(velocity, Vec3::ZERO, DECEL * dt)`
//!
//! This gives snappy starts and a natural coast-to-stop feel without requiring a
//! fixed physics timestep.
//!
//! # Quick start — [`PlayerPlugin`]
//!
//! The simplest integration: add [`PlayerPlugin`] and everything is wired
//! automatically. Systems run unconditionally every frame.
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use voxel_framework::prelude::*;
//! use voxel_framework::player::PlayerPlugin;
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
//!     .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())
//!     .add_plugins(PlayerPlugin::<DefaultVoxel>::new())
//!     .run();
//! ```
//!
//! # AppState integration — manual systems
//!
//! When your game has an `AppState` enum, skip [`PlayerPlugin`] and register the
//! systems manually so they only run during gameplay:
//!
//! ```rust,no_run
//! # use bevy::prelude::*;
//! # use voxel_framework::player::{spawn_player, player_look, player_move, player_cursor_system};
//! # use voxel_framework::player::player_resolve_collision;
//! # use voxel_framework::voxel::DefaultVoxel;
//! # #[derive(States, Default, Clone, PartialEq, Eq, Hash, Debug)]
//! # enum AppState { #[default] Playing }
//! App::new()
//!     .add_systems(Startup, spawn_player)
//!     .add_systems(Update,
//!         (player_look, player_move::<DefaultVoxel>, player_resolve_collision::<DefaultVoxel>)
//!             .chain()
//!             .run_if(in_state(AppState::Playing)),
//!     )
//!     .add_systems(Update, player_cursor_system);
//! ```
//!
//! # Cursor management contract
//!
//! [`spawn_player`] locks the cursor immediately on startup. [`player_cursor_system`]
//! then maintains the lock state at runtime:
//!
//! - **Left click** while cursor is free → locks cursor and hides it.
//! - **First Escape** while cursor is locked → releases cursor (makes it visible).
//! - **Second Escape** while cursor is already free → closes the window.
//!
//! If you use [`PauseMenuPlugin`](crate::ui::PauseMenuPlugin) alongside this
//! module, do **not** add [`player_cursor_system`] — the pause menu handles cursor
//! management itself.
//!
//! # Controls
//!
//! | Input        | Action                              |
//! |--------------|-------------------------------------|
//! | Mouse        | Yaw + pitch (6DOF, no clamp)        |
//! | Q / E        | Roll left / right                   |
//! | W / S        | Forward / backward                  |
//! | A / D        | Strafe left / right                 |
//! | Space        | Move along local up                 |
//! | Shift        | Move along local down               |
//! | Left click   | Lock cursor                         |
//! | Escape       | Release cursor (second press: close window) |

use bevy::ecs::message::MessageWriter;
use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;

use crate::audio::{SfxEvent, SfxRequest};
use crate::chunk::{CHUNK_SIZE, Chunk, VOXEL_SIZE};
use crate::voxel::VoxelType;
use crate::world::{ChunkManager, ViewerPosition, WorldDataDir, get_voxel_at};

pub mod effects;
pub mod persistence;

// ─── Health & damage messages ─────────────────────────────────────────────────

/// Emitted when the player takes damage from any source.
#[derive(Message, Debug, Clone)]
pub struct PlayerDamaged {
    /// Amount of health points lost.
    pub amount: f32,
}

/// Emitted when the player's health reaches zero and they respawn.
#[derive(Message, Debug, Clone)]
pub struct PlayerDied;

/// Player health and respawn state.
///
/// Inserted as a resource by [`PlayerPlugin`]. Games can read and modify
/// `health` directly (e.g. from environmental hazards); fall damage is applied
/// automatically when landing with high vertical velocity.
///
/// When `health` drops to zero the player is teleported to `spawn_position`
/// and health is restored to `max_health`. A [`PlayerDied`] message is sent.
#[derive(Resource, Debug, Clone)]
pub struct PlayerHealth {
    /// Current health points. Clamped to `[0, max_health]` after damage.
    pub health: f32,
    /// Maximum health points. **Default: `20.0`.**
    pub max_health: f32,
    /// World position used for respawn on death.
    ///
    /// Defaults to the player's spawn position from [`PlayerConfig`]. Games
    /// can update this at runtime (e.g. when placing a bed or checkpoint).
    pub spawn_position: Vec3,
    /// Vertical fall speed (m/s) below which no fall damage is taken.
    ///
    /// Impact speeds below this threshold deal zero damage. **Default: `10.0`.**
    pub fall_safe_speed: f32,
}

impl Default for PlayerHealth {
    fn default() -> Self {
        Self {
            health: 20.0,
            max_health: 20.0,
            spawn_position: Vec3::new(0.0, 30.0, 0.0),
            fall_safe_speed: 10.0,
        }
    }
}

// ─── PlayerCrouching resource ─────────────────────────────────────────────────

/// Tracks whether the player is currently crouching.
///
/// Updated every frame by [`player_move`] in [`PlayerMode::Grounded`] mode
/// based on `ControlLeft` key state. Readable by external systems (e.g. UI,
/// animation) without querying the [`Player`] component directly.
///
/// The resource mirrors [`Player::crouching`] and is kept in sync by
/// [`player_move`] each frame. Modifying it directly has no effect — it is
/// overwritten by the movement system.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerCrouching(pub bool);

// ─── PlayerMode ──────────────────────────────────────────────────────────────

/// Selects the movement model used by [`PlayerPlugin`] and the player systems.
///
/// Set this inside [`PlayerConfig::mode`] before adding [`PlayerPlugin`] (or
/// before the `Startup` schedule runs).  Changing the mode at runtime is
/// **not** supported — the value is copied into the [`Player`] component once
/// on spawn and used from there.
///
/// # Variants
///
/// - [`PlayerMode::FreeFlightSixDof`] — classic 6DOF, no gravity.  Default.
/// - [`PlayerMode::FreeFlightAnchored`] — free flight with a world-axis-anchored horizon.
/// - [`PlayerMode::Grounded`] — gravity + jump, WASD on the XZ plane, pitch
///   clamped to ±89°.  Suitable for Minetest / Minecraft–style games.
#[derive(Debug, Clone)]
pub enum PlayerMode {
    /// Six-degrees-of-freedom flight. No gravity, no pitch clamp, Q/E roll.
    ///
    /// This is the default mode and matches the original behaviour of the
    /// player controller.  All six movement axes (forward, back, left, right,
    /// up, down) are relative to the camera's current orientation.
    FreeFlightSixDof {
        /// How Q/E roll input is applied.
        roll_mode: RollMode,
    },
    /// Six-degrees-of-freedom flight with an FPS-style, world-axis-anchored
    /// horizon. Q/E switch the anchor axis instead of rolling.
    FreeFlightAnchored,
    /// Gravity-based grounded movement.
    ///
    /// - WASD moves on the **world XZ plane** relative to the camera's current
    ///   yaw, ignoring pitch tilt.
    /// - `Space` jumps when [`Player::on_ground`] is `true`.
    /// - Gravity accumulates in [`Player::vertical_velocity`] each frame.
    /// - Mouse yaw rotates the entity around the **world Y axis** (player stays
    ///   upright); pitch rotates the view up/down and is clamped to ±89°.
    /// - Q/E roll is disabled.
    Grounded {
        /// Downward acceleration in world units per second squared.
        ///
        /// Applied every frame as `vertical_velocity -= gravity * dt`.
        /// Typical value: `20.0`.
        gravity: f32,
        /// Upward velocity impulse applied when the player jumps.
        ///
        /// Assigned to [`Player::vertical_velocity`] on the frame `Space` is
        /// pressed while [`Player::on_ground`] is `true`.  Typical value: `8.0`.
        jump_speed: f32,
    },
}

impl Default for PlayerMode {
    fn default() -> Self {
        PlayerMode::FreeFlightSixDof {
            roll_mode: RollMode::Continuous,
        }
    }
}

// ─── Physics constants ────────────────────────────────────────────────────────

/// Radius of the player's collision capsule in world units (≈ 0.6 voxels).
///
/// Each hemispherical end cap of the capsule has this radius. The value keeps
/// the player from clipping single-voxel corners while still fitting through
/// one-voxel-wide gaps when combined with [`PLAYER_HALF_HEIGHT`].
pub const PLAYER_RADIUS: f32 = 0.6;

/// Half-height of the capsule's cylindrical spine in world units (≈ 1.8 voxels).
///
/// The spine runs along the player's local Y axis between offsets
/// `−PLAYER_HALF_HEIGHT` and `+PLAYER_HALF_HEIGHT` from the entity origin.
/// The total capsule height is `2 × (PLAYER_HALF_HEIGHT + PLAYER_RADIUS)` ≈ 4.8
/// world units. In 6DOF mode this axis rolls with the player's orientation.
pub const PLAYER_HALF_HEIGHT: f32 = 1.8;

/// Half-height of the capsule when crouching (≈ 60% of normal).
///
/// Reduces the total capsule height enough to let the player pass through
/// gaps that are one voxel tall. Used by [`player_resolve_collision`] when
/// [`Player::crouching`] is `true` in [`PlayerMode::Grounded`] mode.
pub const CROUCH_HALF_HEIGHT: f32 = PLAYER_HALF_HEIGHT * 0.6;

/// Speed multiplier applied to horizontal movement while sprinting.
///
/// Active in [`PlayerMode::Grounded`] when `Shift` is held and the player is
/// on solid ground. Crouch takes priority — if both are held this multiplier
/// is not applied. See [`player_move`] and [`PlayerCrouching`].
pub const SPRINT_MULTIPLIER: f32 = 1.6;

/// Lerp factor applied when the player is accelerating.
const ACCEL: f32 = 12.0;

/// Lerp factor applied when the player is decelerating (no input).
const DECEL: f32 = 7.0;

/// Internal mouse scale factor applied before the user-facing
/// [`Player::sensitivity`] multiplier.
///
/// Mouse deltas arrive in pixels (often `10+` per frame) so a raw multiplier of
/// `1.0` would spin the camera wildly. This constant converts pixels to a base
/// rad/px rate comparable to Minecraft, so that `sensitivity = 1.0` feels
/// "normal". Users scale around this via the Settings slider (10% → very slow,
/// 100% → default, above 100% would be faster but the slider caps there).
const MOUSE_SCALE: f32 = 0.003;

// ─── Snap roll types ─────────────────────────────────────────────────────────

/// Selects how Q/E roll input is applied in [`PlayerMode::FreeFlightSixDof`].
///
/// - [`RollMode::Continuous`] — roll while the key is held (original behavior).
/// - [`RollMode::Snap`] — each key press animates a snap rotation to the
///   next world-aligned up candidate (a signed world axis projected onto the
///   roll plane).
///
/// Configured by `PlayerMode::FreeFlightSixDof { roll_mode }`; defaults to
/// [`RollMode::Continuous`].
///
/// Ignored by [`PlayerMode::FreeFlightAnchored`]: in anchored flight Q/E switch
/// the anchor axis instead of rolling.
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
pub enum RollMode {
    /// Roll continuously while Q/E is held (default, original behavior).
    #[default]
    Continuous,
    /// Snap-rotate to the next world-aligned up candidate per key press with
    /// easing animation (90° steps when forward is on a world axis).
    Snap,
}

/// Easing curve used by [`RollMode::Snap`] animations.
///
/// Each variant maps to a simple `f(t) -> f32` curve where `t ∈ [0, 1]`.
/// The default ([`EaseOutCubic`](SnapRollEasing::EaseOutCubic)) gives a snappy
/// start that decelerates smoothly.
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
pub enum SnapRollEasing {
    /// `1 − (1 − t)³` — fast start, smooth deceleration (default).
    #[default]
    EaseOutCubic,
    /// `1 − (1 − t)⁴` — slightly slower deceleration than cubic.
    EaseOutQuart,
    /// `t² (3 − 2t)` — symmetric S-curve.
    Smoothstep,
    /// `t` — constant angular velocity.
    Linear,
}

/// Runtime state for an in-progress snap-roll animation.
///
/// Stored in [`Player::snap_anim`]. Created by [`start_or_retarget_snap`],
/// advanced each frame by [`advance_snap`], and cleared when `finished`.
///
/// Angles are *relative* to the orientation captured at press time: the
/// world-aligned candidate is resolved once by [`start_or_retarget_snap`]
/// (via [`world_lattice_angles`] / [`next_lattice_step`]) and baked into
/// `target_angle`; the animation then only applies angle differences through
/// `rotate_local_z`.
#[derive(Copy, Clone, Debug)]
pub(crate) struct SnapRollAnim {
    pub(crate) start_angle: f32,
    pub(crate) target_angle: f32,
    pub(crate) elapsed: f32,
    pub(crate) duration: f32,
    pub(crate) easing: SnapRollEasing,
    pub(crate) current_applied: f32,
}

/// Minimum squared projection length a world axis needs on the roll plane to
/// count as a lattice candidate in [`world_lattice_angles`]. Keeps an axis
/// only when it lies ~45° or more away from local Z, so the axis pair the
/// player is (roughly) looking along never yields near-degenerate up stops —
/// a mostly-axis-aligned view gives 4 candidates, while a diagonal view (all
/// axes at ~54.7°) keeps all 6. The constant is `0.49` rather than the exact
/// `sin²(45°) = 0.5`: on a perfect 45° edge-on view both axis families sit at
/// projection² ≈ 0.5 and float rounding could otherwise exclude BOTH,
/// breaking the ≥4-candidate invariant.
pub(crate) const SNAP_MIN_PROJ_SQ: f32 = 0.49;

/// Angular tolerance in radians within which the reference up already lies ON
/// a candidate; [`next_lattice_step`] skips that candidate so a press always
/// produces visible motion (EPS_ON rule).
pub(crate) const SNAP_EPS_ON: f32 = 1e-3;

// ─── Component ───────────────────────────────────────────────────────────────

/// State component that marks the player camera entity.
///
/// Attach this alongside [`Camera3d`] on a single entity. The camera entity
/// IS the player — there is no separate logical-player / camera hierarchy.
/// All player systems query for this component to locate the controlled entity.
///
/// Prefer constructing through [`PlayerConfig`] via [`spawn_player`] rather
/// than inserting [`Player`] directly, which also sets the initial transform and
/// projection from the resource.
///
/// # Default values
///
/// | Field         | Default  |
/// |---------------|----------|
/// | `speed`       | `4.0`    |
/// | `sensitivity` | `1.0`    |
/// | `roll_speed`  | `1.5`    |
/// | `velocity`    | `ZERO`   |
#[derive(Component, Debug, Clone)]
pub struct Player {
    /// Maximum flight speed in world units per second. Clamped implicitly by
    /// the lerp target: actual speed never exceeds this value regardless of
    /// frame rate.
    pub speed: f32,
    /// Mouse sensitivity multiplier (dimensionless). `1.0` matches the default
    /// "Minecraft-like" feel via the internal `MOUSE_SCALE` constant; lower
    /// values are slower, higher values are faster. The Settings slider exposes
    /// the range `0.10`–`1.00` (10%–100%) to the user.
    pub sensitivity: f32,
    /// Roll angular speed in radians per second applied while Q (roll left) or
    /// E (roll right) is held.
    pub roll_speed: f32,
    /// Current world-space velocity in world units per second. Smoothed each
    /// frame by [`player_move`] using an exponential lerp; do not set this
    /// directly unless you want to override the current momentum.
    pub velocity: Vec3,
    /// Movement mode, copied from [`PlayerConfig::mode`] on spawn.
    ///
    /// Determines which branch of [`player_look`], [`player_move`], and
    /// [`player_resolve_collision`] is executed each frame.
    pub mode: PlayerMode,
    /// Vertical velocity used in [`PlayerMode::Grounded`] mode.
    ///
    /// Gravity accumulates here each frame; a jump impulse sets it to
    /// [`PlayerMode::Grounded::jump_speed`]. Reset to `0.0` when the player
    /// lands (collision pushes upward).  Unused in [`PlayerMode::FreeFlightSixDof`].
    pub vertical_velocity: f32,
    /// Whether the player was on solid ground at the end of the last frame.
    ///
    /// Set to `true` by [`player_resolve_collision`] when a voxel collision
    /// produces an upward push; cleared to `false` at the start of each
    /// collision pass.  Gating jumps on this flag prevents infinite jumping.
    /// Unused in [`PlayerMode::FreeFlightSixDof`].
    pub on_ground: bool,
    /// Absolute landing speed (m/s) recorded when the player just touched ground.
    ///
    /// Set by [`player_resolve_collision`] on the first landing frame;
    /// zero at all other times. Read by [`player_fall_damage`] to compute
    /// and apply damage. Only meaningful in [`PlayerMode::Grounded`].
    pub(crate) impact_speed: f32,
    /// Accumulated seconds since the last footstep sound was emitted.
    ///
    /// Managed by [`player_footstep_system`]. Reset to `0.0` after each
    /// footstep; only meaningful in [`PlayerMode::Grounded`].
    pub(crate) footstep_timer: f32,
    /// Whether the initial chunk load has completed.
    ///
    /// Set to `false` on spawn; flipped to `true` in `player_resolve_collision`
    /// once the chunk at the player's feet is registered in [`ChunkManager`].
    /// While `false`, gravity is suppressed so the player does not fall through
    /// un-loaded terrain during world generation.
    pub(crate) world_loaded: bool,
    /// `true` until the player's first landing after spawn (or respawn).
    ///
    /// Skips fall-damage on that landing so a spawn point slightly above the
    /// terrain surface does not instantly kill the player.  Cleared on first
    /// contact with solid ground.
    pub(crate) spawn_immune: bool,
    /// Whether the player is currently sprinting.
    ///
    /// `true` when `Shift` is held in [`PlayerMode::Grounded`] with
    /// [`Player::on_ground`] `true`, and [`Player::crouching`] is `false`.
    /// Crouch takes priority: if both are held this is `false`.
    /// Always `false` in [`PlayerMode::FreeFlightSixDof`].
    pub sprinting: bool,
    /// Whether the player is currently crouching.
    ///
    /// `true` when `Ctrl` is held in [`PlayerMode::Grounded`] mode.
    /// Reduces the effective capsule half-height to [`CROUCH_HALF_HEIGHT`]
    /// and movement speed to 40% of base speed.
    /// Always `false` in [`PlayerMode::FreeFlightSixDof`].
    pub crouching: bool,
    /// Duration in seconds for a single snap-roll step animation (one hop to
    /// the next world-aligned up candidate).
    /// Only used when `roll_mode == RollMode::Snap`. **Default: `0.5`.**
    pub snap_roll_duration: f32,
    /// Easing curve for snap-roll animations.
    /// Only used when `roll_mode == RollMode::Snap`.
    pub snap_roll_easing: SnapRollEasing,
    /// Active snap-roll animation state (runtime-only, not persisted).
    pub(crate) snap_anim: Option<SnapRollAnim>,
    /// Current anchor axis for [`PlayerMode::FreeFlightAnchored`] look: always one of the
    /// 6 signed world axes. Yaw orbits this axis; pitch is clamped to ±89°
    /// from the plane perpendicular to it. Initialized on spawn to the signed
    /// world axis nearest the spawn orientation's up. Unused in
    /// [`PlayerMode::FreeFlightSixDof`].
    pub anchor_up: Vec3,
    /// Source signed world axis of an in-flight anchor switch, committed to
    /// [`Player::anchor_up`] when the animation finishes (runtime-only).
    pub(crate) pending_anchor: Option<Vec3>,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            speed: 4.0,
            sensitivity: 1.0,
            roll_speed: 1.5,
            velocity: Vec3::ZERO,
            mode: PlayerMode::default(),
            vertical_velocity: 0.0,
            on_ground: false,
            impact_speed: 0.0,
            footstep_timer: 0.0,
            world_loaded: false,
            spawn_immune: true,
            sprinting: false,
            crouching: false,
            snap_roll_duration: 0.5,
            snap_roll_easing: SnapRollEasing::EaseOutCubic,
            snap_anim: None,
            anchor_up: Vec3::Y,
            pending_anchor: None,
        }
    }
}

// ─── Configuration resource ───────────────────────────────────────────────────

/// Startup configuration for [`spawn_player`].
///
/// Insert this resource before [`PlayerPlugin`] (or before the `Startup`
/// schedule runs) to override any default. Fields are consumed once by
/// [`spawn_player`]; changing the resource after spawn has no effect.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::player::PlayerConfig;
///
/// App::new()
///     .insert_resource(PlayerConfig {
///         spawn_position: Vec3::new(0.0, 50.0, 150.0),
///         speed: 8.0,
///         fov_degrees: 80.0,
///         ..Default::default()
///     });
/// ```
///
/// # Default values
///
/// | Field           | Default                   |
/// |-----------------|---------------------------|
/// | `spawn_position`| `(0.0, 0.0, 30.0)`        |
/// | `speed`         | `4.0` wu/s                |
/// | `sensitivity`   | `1.0` (unitless multiplier) |
/// | `roll_speed`    | `1.5` rad/s               |
/// | `look_at`       | `Vec3::ZERO`              |
/// | `fov_degrees`   | `90.0`                    |
#[derive(Resource, Debug, Clone)]
pub struct PlayerConfig {
    /// World-space position at which the player entity is spawned.
    pub spawn_position: Vec3,
    /// Maximum flight speed in world units per second. Copied into
    /// [`Player::speed`] on spawn.
    pub speed: f32,
    /// Mouse sensitivity multiplier (dimensionless). Copied into
    /// [`Player::sensitivity`] on spawn. `1.0` = default "Minecraft-like" feel;
    /// see `MOUSE_SCALE` for the underlying rad/px rate.
    pub sensitivity: f32,
    /// Roll angular speed in radians per second. Copied into
    /// [`Player::roll_speed`] on spawn.
    pub roll_speed: f32,
    /// World-space point the camera looks toward immediately after spawning.
    /// Only affects the initial orientation; the player can look anywhere
    /// afterwards via mouse input.
    pub look_at: Vec3,
    /// Vertical field-of-view in degrees. Valid range: `50.0`–`120.0`.
    /// Converted to radians and stored in the [`PerspectiveProjection`] on
    /// spawn; runtime changes must be made directly on the [`Projection`]
    /// component (e.g., via the Settings menu).
    pub fov_degrees: f32,
    /// Movement model used by the player systems.
    ///
    /// Defaults to [`PlayerMode::FreeFlightSixDof`] for backwards compatibility.
    /// Set to [`PlayerMode::Grounded`] for gravity-based gameplay.
    pub mode: PlayerMode,
    /// Duration in seconds for a single snap-roll step animation (one hop to
    /// the next world-aligned up candidate).
    /// Copied into [`Player::snap_roll_duration`] on spawn. **Default: `0.5`.**
    pub snap_roll_duration_secs: f32,
    /// Easing curve for snap-roll animations.
    /// Copied into [`Player::snap_roll_easing`] on spawn.
    pub snap_roll_easing: SnapRollEasing,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            spawn_position: Vec3::new(0.0, 0.0, 30.0),
            speed: 4.0,
            sensitivity: 1.0,
            roll_speed: 1.5,
            look_at: Vec3::ZERO,
            fov_degrees: 90.0,
            mode: PlayerMode::default(),
            snap_roll_duration_secs: 0.5,
            snap_roll_easing: SnapRollEasing::EaseOutCubic,
        }
    }
}

// ─── Systems ──────────────────────────────────────────────────────────────────

/// Spawns the player camera entity configured by [`PlayerConfig`].
///
/// This system is **idempotent**: if a [`Player`] entity already exists in the
/// world it returns immediately without spawning a second one. This makes it
/// safe to register on `Startup` even when transitioning through `AppState`
/// variants that re-enter the startup flow.
///
/// **Side effect:** locks the cursor on spawn (`CursorGrabMode::Locked`,
/// `visible = false`). This matches the expectation that the game begins in a
/// mouse-captured state. To keep the cursor free on startup, set
/// [`CrosshairConfig::visible`](crate::ui::CrosshairConfig) to `false` and manage
/// cursor state yourself.
pub fn spawn_player(
    mut commands: Commands,
    config: Res<PlayerConfig>,
    existing: Query<(), With<Player>>,
    mut windows: Query<&mut bevy::window::CursorOptions>,
    mut health: ResMut<PlayerHealth>,
) {
    if !existing.is_empty() {
        return;
    }

    // Sync spawn_position with the configured spawn so respawn lands in the right place.
    health.spawn_position = config.spawn_position;

    // Lock the cursor so mouse motion drives the camera.
    for mut cursor in &mut windows {
        cursor.grab_mode = bevy::window::CursorGrabMode::Locked;
        cursor.visible = false;
    }

    let spawn = config.spawn_position;
    let look_at = config.look_at;
    let fov = config.fov_degrees.to_radians();
    let mode = config.mode.clone();

    // Build the transform first so the anchor axis can be derived from the
    // actual spawn orientation.
    let transform = Transform::from_translation(spawn).looking_at(look_at, Vec3::Y);

    commands.spawn((
        Player {
            speed: config.speed,
            sensitivity: config.sensitivity,
            roll_speed: config.roll_speed,
            velocity: Vec3::ZERO,
            mode,
            vertical_velocity: 0.0,
            on_ground: false,
            impact_speed: 0.0,
            footstep_timer: 0.0,
            world_loaded: false,
            spawn_immune: true,
            sprinting: false,
            crouching: false,
            snap_roll_duration: config.snap_roll_duration_secs,
            snap_roll_easing: config.snap_roll_easing,
            snap_anim: None,
            anchor_up: nearest_world_axis(*transform.up()),
            pending_anchor: None,
        },
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov, ..default() }),
        transform,
    ));
}

// ─── Snap roll helpers ──────────────────────────────────────────────────────

pub(crate) fn apply_easing(t: f32, easing: SnapRollEasing) -> f32 {
    match easing {
        SnapRollEasing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
        SnapRollEasing::EaseOutQuart => 1.0 - (1.0 - t).powi(4),
        SnapRollEasing::Smoothstep => t * t * (3.0 - 2.0 * t),
        SnapRollEasing::Linear => t,
    }
}

/// The 6 signed world axes in canonical order.
///
/// The order is load-bearing: it is the candidate iteration order of
/// [`world_lattice_candidates`] and the deterministic tie-break sequence of
/// [`nearest_world_axis`] (first-in-order wins).
pub(crate) const AXES: [Vec3; 6] = [
    Vec3::X,
    Vec3::NEG_X,
    Vec3::Y,
    Vec3::NEG_Y,
    Vec3::Z,
    Vec3::NEG_Z,
];

/// Like [`world_lattice_angles`] but returns `(angle δ, SOURCE signed world
/// axis)` per surviving candidate.
///
/// The candidate set is the 6 signed world axes ([`AXES`]) projected onto
/// the plane perpendicular to `local_z` and normalized. Axes whose projection
/// has length² < [`SNAP_MIN_PROJ_SQ`] are excluded — an axis within 45° of
/// the roll axis is "the one you are looking along" and would yield a
/// perceptually arbitrary up stop (at least 4 candidates always survive).
/// Each angle is `δ = atan2(cross(up, c) · local_z, up · c)` — the
/// [`Transform::rotate_local_z`] convention, so δ feeds it unmodified.
pub(crate) fn world_lattice_candidates(local_z: Vec3, up: Vec3) -> Vec<(f32, Vec3)> {
    let mut candidates = Vec::with_capacity(6);
    for axis in AXES {
        let proj = axis - axis.dot(local_z) * local_z;
        if proj.length_squared() < SNAP_MIN_PROJ_SQ {
            continue;
        }
        let c = proj.normalize();
        candidates.push((up.cross(c).dot(local_z).atan2(up.dot(c)), axis));
    }
    candidates
}

/// Signed roll angles δ ∈ (−π, π] from `up` to each surviving world-axis
/// candidate around the roll axis `local_z` (Bevy's BACK vector).
///
/// Thin wrapper over [`world_lattice_candidates`] that drops each candidate's
/// source axis — see that function for the lattice rule.
pub(crate) fn world_lattice_angles(local_z: Vec3, up: Vec3) -> Vec<f32> {
    world_lattice_candidates(local_z, up)
        .into_iter()
        .map(|(a, _)| a)
        .collect()
}

/// Next lattice step in press direction `dir` (+1 = Q / CCW, −1 = E / CW).
///
/// dir > 0 → min over `{δ > SNAP_EPS_ON ? δ : δ + TAU}`;
/// dir < 0 → max over `{δ < −SNAP_EPS_ON ? δ : δ − TAU}`.
/// Candidates within [`SNAP_EPS_ON`] of the reference up wrap a full turn
/// (EPS_ON rule), so the result is always a nonzero signed angle.
pub(crate) fn next_lattice_step(angles: &[f32], dir: i32) -> f32 {
    use std::f32::consts::TAU;
    if dir > 0 {
        angles
            .iter()
            .map(|&d| if d > SNAP_EPS_ON { d } else { d + TAU })
            .fold(f32::INFINITY, f32::min)
    } else {
        angles
            .iter()
            .map(|&d| if d < -SNAP_EPS_ON { d } else { d - TAU })
            .fold(f32::NEG_INFINITY, f32::max)
    }
}

/// Mirror of [`next_lattice_step`] carrying each candidate's source axis:
/// same [`SNAP_EPS_ON`] wrap, min (dir > 0) / max (dir < 0) by wrapped angle.
/// Returns the wrapped angle together with the winning candidate's SOURCE
/// signed world axis.
///
/// The caller guarantees a non-empty slice (the [`SNAP_MIN_PROJ_SQ`]
/// ≥4-candidate invariant); panics otherwise rather than returning garbage.
pub(crate) fn next_lattice_candidate(cands: &[(f32, Vec3)], dir: i32) -> (f32, Vec3) {
    use std::f32::consts::TAU;
    assert!(
        !cands.is_empty(),
        "next_lattice_candidate: empty candidate slice — SNAP_MIN_PROJ_SQ \
         guarantees at least 4 lattice candidates"
    );
    if dir > 0 {
        cands
            .iter()
            .map(|&(d, axis)| (if d > SNAP_EPS_ON { d } else { d + TAU }, axis))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .unwrap()
    } else {
        cands
            .iter()
            .map(|&(d, axis)| (if d < -SNAP_EPS_ON { d } else { d - TAU }, axis))
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .unwrap()
    }
}

/// Builds (or retargets) a snap-roll animation toward the next world-aligned
/// up candidate in press direction `dir`.
///
/// `local_z` / `up` are `*transform.local_z()` / `*transform.up()` at press
/// time. Every press resolves through one primitive — "next candidate in
/// direction `dir` from a reference up" ([`world_lattice_angles`] +
/// [`next_lattice_step`]):
///
/// - **Fresh press** — reference = `up`; the animation starts at `0.0`
///   toward the first step.
/// - **Same direction mid-animation** — reference = the in-flight target up
///   (`up` advanced by the not-yet-applied residual; exact because
///   `up ⊥ local_z`); retargets one candidate further, resuming from the
///   currently applied angle.
/// - **Opposite direction mid-animation** — reference = `up`; reverses to
///   the nearest candidate behind, resuming from the currently applied angle.
///
/// The target is fixed here at press time and never re-resolved mid-flight.
/// Angles stay relative: [`advance_snap`] applies only differences via
/// `rotate_local_z`, so the animation lands exactly on the world candidate
/// computed from `local_z` / `up`.
pub(crate) fn start_or_retarget_snap(
    existing: Option<SnapRollAnim>,
    local_z: Vec3,
    up: Vec3,
    dir: i32,
    duration: f32,
    easing: SnapRollEasing,
) -> SnapRollAnim {
    match existing {
        None => {
            let step = next_lattice_step(&world_lattice_angles(local_z, up), dir);
            SnapRollAnim {
                start_angle: 0.0,
                target_angle: step,
                elapsed: 0.0,
                duration,
                easing,
                current_applied: 0.0,
            }
        }
        Some(anim) => {
            let same_dir = (dir > 0 && anim.target_angle > anim.start_angle)
                || (dir < 0 && anim.target_angle < anim.start_angle);

            if same_dir {
                // Reference = in-flight target up: advance `up` by the
                // not-yet-applied residual, then extend one candidate further.
                let residual = anim.target_angle - anim.current_applied;
                let ref_up = Quat::from_axis_angle(local_z, residual) * up;
                let step = next_lattice_step(&world_lattice_angles(local_z, ref_up), dir);
                SnapRollAnim {
                    start_angle: anim.current_applied,
                    target_angle: anim.target_angle + step,
                    elapsed: 0.0,
                    duration,
                    easing,
                    current_applied: anim.current_applied,
                }
            } else {
                // Opposite direction: nearest candidate behind current up.
                let step = next_lattice_step(&world_lattice_angles(local_z, up), dir);
                SnapRollAnim {
                    start_angle: anim.current_applied,
                    target_angle: anim.current_applied + step,
                    elapsed: 0.0,
                    duration,
                    easing,
                    current_applied: anim.current_applied,
                }
            }
        }
    }
}

/// The reference up [`start_or_retarget_snap`] resolves candidates from,
/// replicated as a pure helper so the Anchored arm can compute the SOURCE
/// axis of the exact candidate the animation will land on:
///
/// - Same direction mid-animation → `up` advanced by the not-yet-applied
///   residual (`Quat::from_axis_angle(local_z, target − applied) * up`).
/// - Fresh press or opposite direction → `up` unchanged.
pub(crate) fn snap_reference_up(
    existing: Option<SnapRollAnim>,
    local_z: Vec3,
    up: Vec3,
    dir: i32,
) -> Vec3 {
    match existing {
        Some(anim) => {
            let same_dir = (dir > 0 && anim.target_angle > anim.start_angle)
                || (dir < 0 && anim.target_angle < anim.start_angle);
            if same_dir {
                let residual = anim.target_angle - anim.current_applied;
                Quat::from_axis_angle(local_z, residual) * up
            } else {
                up
            }
        }
        None => up,
    }
}

/// Clamps a pitch delta so `asin(forward · anchor_up)` stays within ±89°.
///
/// `pitch = asin(clamp(forward · anchor_up, −1, 1))`; returns
/// `clamp(pitch + pitch_d, ±89°) − pitch`. The clamp-then-subtract form gives
/// exact truncation at the poles for free: at the limit, outward deltas
/// return 0 while inward deltas pass through. Pure and NaN-free (the dot is
/// clamped to asin's domain).
pub(crate) fn clamp_pitch_delta(forward: Vec3, anchor_up: Vec3, pitch_d: f32) -> f32 {
    let limit = 89_f32.to_radians();
    let pitch = forward.dot(anchor_up).clamp(-1.0, 1.0).asin();
    (pitch + pitch_d).clamp(-limit, limit) - pitch
}

/// Signed world axis with the maximum dot product against `v`.
///
/// Iterates [`AXES`] in canonical order keeping the max with strict `>`, so
/// ties break deterministically toward the first axis in the order
/// `[X, NEG_X, Y, NEG_Y, Z, NEG_Z]`. Used to initialize
/// [`Player::anchor_up`] at spawn.
pub(crate) fn nearest_world_axis(v: Vec3) -> Vec3 {
    let mut best = AXES[0];
    let mut best_dot = v.dot(AXES[0]);
    for axis in AXES.into_iter().skip(1) {
        let d = v.dot(axis);
        if d > best_dot {
            best = axis;
            best_dot = d;
        }
    }
    best
}

pub(crate) fn advance_snap(anim: &mut SnapRollAnim, dt: f32) -> (f32, bool) {
    anim.elapsed += dt;
    let t = (anim.elapsed / anim.duration).clamp(0.0, 1.0);
    let eased = apply_easing(t, anim.easing);
    let total = anim.start_angle + (anim.target_angle - anim.start_angle) * eased;
    let delta = total - anim.current_applied;
    anim.current_applied = total;
    let finished = anim.elapsed >= anim.duration;
    if finished {
        // Snap exactly to target to avoid FP residue
        let correction = anim.target_angle - anim.current_applied;
        anim.current_applied = anim.target_angle;
        return (delta + correction, true);
    }
    (delta, false)
}

/// Updates the player orientation from mouse movement and Q/E roll keys.
///
/// Implements **true 6DOF rotation** with no pitch clamp. Each axis rotates
/// around the entity's *local* frame:
///
/// - Mouse X → `rotate_local_y` (yaw relative to current up)
/// - Mouse Y → `rotate_local_x` (pitch relative to current right)
/// - Q / E   → `rotate_local_z` (roll at [`Player::roll_speed`] rad/s)
///
/// Rotating in local space means all three axes compose correctly regardless
/// of the accumulated orientation — the player can pitch past vertical, roll
/// upside-down, and continue steering intuitively without gimbal lock.
///
/// All [`MouseMotion`] events for the frame are accumulated into a single
/// delta before applying rotation, preventing inter-frame jitter at high
/// polling rates.
pub fn player_look(
    mut query: Query<(&mut Player, &mut Transform)>,
    mut mouse_motion: MessageReader<MouseMotion>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    let Ok((mut player, mut transform)) = query.single_mut() else {
        return;
    };

    // Accumulate mouse delta for this frame.
    let mut delta = Vec2::ZERO;
    for event in mouse_motion.read() {
        delta += event.delta;
    }

    match &player.mode {
        PlayerMode::FreeFlightSixDof { roll_mode } => {
            if delta != Vec2::ZERO {
                let yaw_d = -delta.x * MOUSE_SCALE * player.sensitivity;
                let pitch_d = -delta.y * MOUSE_SCALE * player.sensitivity;
                // Rotate around LOCAL axes — preserves 6DOF orientation.
                transform.rotate_local_y(yaw_d);
                transform.rotate_local_x(pitch_d);
            }

            // Q / E roll.
            match roll_mode {
                RollMode::Continuous => {
                    let mut roll = 0.0_f32;
                    if keys.pressed(KeyCode::KeyQ) {
                        roll += 1.0;
                    }
                    if keys.pressed(KeyCode::KeyE) {
                        roll -= 1.0;
                    }
                    if roll != 0.0 {
                        transform.rotate_local_z(roll * player.roll_speed * time.delta_secs());
                    }
                }
                RollMode::Snap => {
                    if keys.just_pressed(KeyCode::KeyQ) {
                        player.snap_anim = Some(start_or_retarget_snap(
                            player.snap_anim,
                            *transform.local_z(),
                            *transform.up(),
                            1,
                            player.snap_roll_duration,
                            player.snap_roll_easing,
                        ));
                    }
                    if keys.just_pressed(KeyCode::KeyE) {
                        player.snap_anim = Some(start_or_retarget_snap(
                            player.snap_anim,
                            *transform.local_z(),
                            *transform.up(),
                            -1,
                            player.snap_roll_duration,
                            player.snap_roll_easing,
                        ));
                    }
                    if let Some(ref mut anim) = player.snap_anim {
                        let (delta, finished) = advance_snap(anim, time.delta_secs());
                        transform.rotate_local_z(delta);
                        if finished {
                            player.snap_anim = None;
                        }
                    }
                }
            }
        }
        PlayerMode::FreeFlightAnchored => {
            if delta != Vec2::ZERO {
                let yaw_d = -delta.x * MOUSE_SCALE * player.sensitivity;
                let pitch_d = -delta.y * MOUSE_SCALE * player.sensitivity;
                // Yaw: WORLD-space rotation around the anchor axis. While a
                // switch animation runs this is still the OLD anchor —
                // pending_anchor is only committed on finish.
                transform.rotate(Quat::from_axis_angle(player.anchor_up, yaw_d));
                // Pitch: local-X rotation, clamped so the elevation against
                // the anchor stays within ±89°.
                let clamped = clamp_pitch_delta(*transform.forward(), player.anchor_up, pitch_d);
                transform.rotate_local_x(clamped);
            }

            // Q / E: switch the anchor to the next lattice candidate
            // (RollMode is ignored in Anchored look).
            if keys.just_pressed(KeyCode::KeyQ) {
                let local_z = *transform.local_z();
                let up = *transform.up();
                let ref_up = snap_reference_up(player.snap_anim, local_z, up, 1);
                let (_, axis) =
                    next_lattice_candidate(&world_lattice_candidates(local_z, ref_up), 1);
                player.pending_anchor = Some(axis);
                player.snap_anim = Some(start_or_retarget_snap(
                    player.snap_anim,
                    local_z,
                    up,
                    1,
                    player.snap_roll_duration,
                    player.snap_roll_easing,
                ));
            }
            if keys.just_pressed(KeyCode::KeyE) {
                let local_z = *transform.local_z();
                let up = *transform.up();
                let ref_up = snap_reference_up(player.snap_anim, local_z, up, -1);
                let (_, axis) =
                    next_lattice_candidate(&world_lattice_candidates(local_z, ref_up), -1);
                player.pending_anchor = Some(axis);
                player.snap_anim = Some(start_or_retarget_snap(
                    player.snap_anim,
                    local_z,
                    up,
                    -1,
                    player.snap_roll_duration,
                    player.snap_roll_easing,
                ));
            }
            if let Some(ref mut anim) = player.snap_anim {
                let (delta, finished) = advance_snap(anim, time.delta_secs());
                transform.rotate_local_z(delta);
                if finished {
                    // Commit the pure signed world axis, then renormalize
                    // the frame: look_to preserves forward and snaps
                    // screen-up onto the new anchor's projection, absorbing
                    // any roll accumulated vs. the new axis mid-animation.
                    if let Some(anchor) = player.pending_anchor.take() {
                        player.anchor_up = anchor;
                    }
                    let fwd = *transform.forward();
                    transform.look_to(fwd, player.anchor_up);
                    player.snap_anim = None;
                }
            }
        }
        PlayerMode::Grounded { .. } => {
            if delta != Vec2::ZERO {
                let yaw_d = -delta.x * MOUSE_SCALE * player.sensitivity;
                let pitch_d = -delta.y * MOUSE_SCALE * player.sensitivity;

                // Yaw: rotate around WORLD Y so the player stays upright.
                transform.rotate_y(yaw_d);

                // Pitch: decompose current rotation, apply clamped pitch.
                let (yaw, pitch, _roll) = transform.rotation.to_euler(EulerRot::YXZ);
                let new_pitch = (pitch + pitch_d).clamp(-89_f32.to_radians(), 89_f32.to_radians());
                transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, new_pitch, 0.0);
            }
            // No roll in grounded mode.
        }
    }
}

/// Translates the player based on keyboard input and updates [`ViewerPosition`].
///
/// Movement axes are derived from the current entity rotation, so all six
/// directions (forward, back, left, right, up, down) are always relative to
/// wherever the player is currently looking — including after roll and pitch.
///
/// **Velocity smoothing** uses exponential lerp clamped to `[0, 1]`:
///
/// ```text
/// factor = if moving { ACCEL } else { DECEL }
/// velocity = lerp(velocity, target, (factor * dt).min(1.0))
/// ```
///
/// This is frame-rate-independent for reasonable frame times and produces a
/// snappy start with a natural coast-to-stop deceleration.
///
/// After translating the entity, the system writes the new world position to
/// [`ViewerPosition`] so the chunk streaming system can load/unload chunks
/// around the player without querying the player transform directly.
pub fn player_move<V: VoxelType>(
    mut query: Query<(&mut Player, &mut Transform)>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut viewer: ResMut<ViewerPosition>,
    manager: Res<ChunkManager>,
    chunks: Query<&Chunk<V>>,
    mut crouch_res: ResMut<PlayerCrouching>,
    applied: Option<Res<effects::AppliedEffectModifiers>>,
) {
    let Ok((mut player, mut transform)) = query.single_mut() else {
        return;
    };

    let dt = time.delta_secs();

    match player.mode.clone() {
        PlayerMode::FreeFlightSixDof { .. } | PlayerMode::FreeFlightAnchored => {
            // Sprint/crouch are not applicable in FreeFlight.
            player.sprinting = false;
            player.crouching = false;
            crouch_res.0 = false;

            // Local-space axes — full 6DOF.
            let forward = transform.rotation * Vec3::NEG_Z;
            let right = transform.rotation * Vec3::X;
            let up = transform.rotation * Vec3::Y;

            let mut direction = Vec3::ZERO;
            if keys.pressed(KeyCode::KeyW) {
                direction += forward;
            }
            if keys.pressed(KeyCode::KeyS) {
                direction -= forward;
            }
            if keys.pressed(KeyCode::KeyA) {
                direction -= right;
            }
            if keys.pressed(KeyCode::KeyD) {
                direction += right;
            }
            if keys.pressed(KeyCode::Space) {
                direction += up;
            }
            if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
                direction -= up;
            }

            let target = if direction != Vec3::ZERO {
                direction.normalize() * player.speed
            } else {
                Vec3::ZERO
            };

            let factor = if direction != Vec3::ZERO {
                ACCEL
            } else {
                DECEL
            };
            player.velocity = player.velocity.lerp(target, (factor * dt).min(1.0));
            transform.translation += player.velocity * dt;
        }
        PlayerMode::Grounded {
            gravity,
            jump_speed,
        } => {
            // Horizontal movement on XZ using yaw-only orientation.
            let yaw = transform.rotation.to_euler(EulerRot::YXZ).0;
            let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
            let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());

            let mut h_dir = Vec3::ZERO;
            if keys.pressed(KeyCode::KeyW) {
                h_dir += forward;
            }
            if keys.pressed(KeyCode::KeyS) {
                h_dir -= forward;
            }
            if keys.pressed(KeyCode::KeyA) {
                h_dir -= right;
            }
            if keys.pressed(KeyCode::KeyD) {
                h_dir += right;
            }

            // Sprint (Shift) and crouch (Ctrl) speed multipliers.
            // Crouch takes priority over sprint if both are held.
            // Sprint only activates when on solid ground (no air-sprinting).
            let crouching =
                keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
            let sprinting = !crouching
                && player.on_ground
                && (keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight));
            player.crouching = crouching;
            player.sprinting = sprinting;
            crouch_res.0 = crouching;
            let speed_mult = if crouching {
                0.5
            } else if sprinting {
                SPRINT_MULTIPLIER
            } else {
                1.0
            };

            // Sample the block under the player's feet to get movement modifiers.
            let foot_pos = transform.translation - Vec3::Y * (PLAYER_HALF_HEIGHT + 0.3);
            let modifier = get_voxel_at::<V>(foot_pos, &manager, &chunks)
                .map(|v| v.movement_modifier())
                .unwrap_or_default();

            let effect_speed = applied.as_deref().map(|a| a.0.speed_mult).unwrap_or(1.0);
            let h_target = if h_dir != Vec3::ZERO {
                h_dir.normalize() * player.speed * speed_mult * modifier.speed_mult * effect_speed
            } else {
                Vec3::ZERO
            };

            let effective_decel = DECEL * modifier.friction_mult;
            let factor = if h_dir != Vec3::ZERO {
                ACCEL
            } else {
                effective_decel
            };
            player.velocity = player.velocity.lerp(h_target, (factor * dt).min(1.0));

            // Freeze gravity until the initial chunk load completes so the
            // player does not fall through un-loaded terrain on startup.
            if !player.world_loaded {
                player.vertical_velocity = 0.0;
                transform.translation += player.velocity * dt;
                viewer.position = transform.translation;
                return;
            }

            // Jump impulse (only when on the ground; disabled while crouching).
            if keys.just_pressed(KeyCode::Space) && player.on_ground && !crouching {
                let effect_jump = applied.as_deref().map(|a| a.0.jump_mult).unwrap_or(1.0);
                player.vertical_velocity = jump_speed * modifier.jump_mult * effect_jump;
            }

            // Apply gravity.
            player.vertical_velocity -= gravity * modifier.gravity_mult * dt;

            // Water buoyancy: if the player's center voxel is transparent (e.g.
            // water), apply a gentle upward push each frame to make them float
            // toward the surface. The movement_modifier already reduces gravity
            // via gravity_mult; buoyancy adds a positive upward impulse.
            let center_voxel = get_voxel_at::<V>(transform.translation, &manager, &chunks);
            if center_voxel.map(|v| v.alpha() < 1.0).unwrap_or(false) {
                player.vertical_velocity += jump_speed * 0.3 * dt;
            }

            // Translate: horizontal from smoothed velocity + vertical from gravity.
            transform.translation += player.velocity * dt;
            transform.translation.y += player.vertical_velocity * dt;
        }
    }

    // Keep the streaming origin at the player position.
    viewer.position = transform.translation;
}

/// Resolves capsule-vs-voxel AABB collisions and repositions the player.
///
/// The collision shape is a capsule whose spine runs along the player's **local
/// Y axis** between `±PLAYER_HALF_HEIGHT`. Because the spine follows the
/// entity rotation, roll and pitch do not break collision detection — the
/// capsule tilts with the player.
///
/// **Algorithm (per chunk, per solid voxel in the AABB):**
///
/// 1. Compute the axis-aligned bounding box of the capsule in chunk-local space.
/// 2. Clamp the AABB to the chunk bounds to skip out-of-range voxels.
/// 3. For each solid voxel in the range, call `push_capsule_out` which:
///    a. Finds the closest point on the capsule spine to the voxel AABB center.
///    b. Treats that point as a sphere center of radius [`PLAYER_RADIUS`].
///    c. Computes the minimum separation vector from the sphere to the voxel AABB.
///    d. Accumulates the push into the running position.
///
/// The final corrected position is written back to the player transform. This
/// system should always run **after** [`player_move`] in the same frame.
pub fn player_resolve_collision<V: VoxelType>(
    mut player_q: Query<(&mut Player, &mut Transform)>,
    chunk_q: Query<(&Chunk<V>, &Transform), Without<Player>>,
    manager: Res<ChunkManager>,
) {
    let Ok((mut player, mut player_tf)) = player_q.single_mut() else {
        return;
    };

    // Grounded mode tracks whether the player is standing on solid ground.
    let is_grounded = matches!(player.mode, PlayerMode::Grounded { .. });
    if is_grounded {
        player.on_ground = false;
        player.impact_speed = 0.0; // reset each frame; set below on landing
    }

    // Use reduced capsule height when crouching so the player fits through
    // 1-block-tall gaps. In FreeFlight mode crouching is always false.
    let half_height = if player.crouching {
        CROUCH_HALF_HEIGHT
    } else {
        PLAYER_HALF_HEIGHT
    };

    // Enable gravity only once the chunk containing the player's feet is
    // registered in ChunkManager — meaning the generator has actually produced
    // terrain data for that chunk.  Checking a distant chunk being present
    // (chunk_q.is_empty) is not sufficient: it fires too early.
    if !player.world_loaded {
        let feet = player_tf.translation - Vec3::Y * half_height;
        let coord = crate::chunk::world_to_chunk_coord(feet);
        if manager.is_loaded(coord) {
            player.world_loaded = true;
        }
    }

    let mut pos = player_tf.translation;
    // FreeFlight: capsule spine follows entity rotation (true 6DOF).
    // Grounded: spine is always world-vertical (Vec3::Y) so the capsule stands upright.
    let up = if is_grounded {
        Vec3::Y
    } else {
        player_tf.rotation * Vec3::Y
    };

    let extent = half_height + PLAYER_RADIUS;
    let voxel_size = VOXEL_SIZE;

    for (chunk, chunk_tf) in &chunk_q {
        let chunk_pos = chunk_tf.translation;
        let local = pos - chunk_pos;

        // AABB of voxels that could possibly overlap the capsule.
        let min_vox = IVec3::new(
            ((local.x - extent) / voxel_size).floor() as i32,
            ((local.y - extent) / voxel_size).floor() as i32,
            ((local.z - extent) / voxel_size).floor() as i32,
        );
        let max_vox = IVec3::new(
            ((local.x + extent) / voxel_size).ceil() as i32,
            ((local.y + extent) / voxel_size).ceil() as i32,
            ((local.z + extent) / voxel_size).ceil() as i32,
        );

        // Clamp to chunk bounds for the loop — is_solid_at returns false for
        // out-of-range coords, but skipping them avoids pointless iterations.
        let lo = min_vox.max(IVec3::ZERO);
        let hi = max_vox.min(IVec3::splat(CHUNK_SIZE as i32 - 1));

        for vx in lo.x..=hi.x {
            for vy in lo.y..=hi.y {
                for vz in lo.z..=hi.z {
                    if !chunk.is_solid_at(vx as usize, vy as usize, vz as usize) {
                        continue;
                    }
                    let new_pos = push_capsule_out(pos, up, half_height, chunk_pos, vx, vy, vz);

                    // In grounded mode detect landing: a positive Y correction
                    // means the player was pushed upward off a solid surface.
                    if is_grounded && new_pos.y > pos.y + 0.001 {
                        if !player.on_ground {
                            // First contact this frame — record impact speed for fall damage.
                            player.impact_speed = player.vertical_velocity.abs();
                        }
                        player.on_ground = true;
                        player.vertical_velocity = 0.0;
                    }

                    pos = new_pos;
                }
            }
        }
    }

    player_tf.translation = pos;
}

// ─── Cursor grab ──────────────────────────────────────────────────────────────

/// Manages cursor lock state and window close on Escape.
///
/// This system implements a two-stage Escape behavior common in first-person
/// games:
///
/// | Trigger | Cursor state before | Effect |
/// |---------|---------------------|--------|
/// | Left click | Free (`None`) | Locks cursor, hides it |
/// | Escape | Locked | Releases cursor, shows it |
/// | Escape | Free | Despawns the window entity (closes the app) |
///
/// Do **not** add this system when using [`PauseMenuPlugin`](crate::ui::PauseMenuPlugin),
/// which provides its own cursor management and a richer Escape flow
/// (Playing → Paused → Playing).
pub fn player_cursor_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut windows: Query<(Entity, &mut bevy::window::CursorOptions)>,
) {
    for (entity, mut cursor) in windows.iter_mut() {
        if mouse.just_pressed(MouseButton::Left)
            && cursor.grab_mode == bevy::window::CursorGrabMode::None
        {
            cursor.grab_mode = bevy::window::CursorGrabMode::Locked;
            cursor.visible = false;
        }

        if keys.just_pressed(KeyCode::Escape) {
            if cursor.grab_mode != bevy::window::CursorGrabMode::None {
                cursor.grab_mode = bevy::window::CursorGrabMode::None;
                cursor.visible = true;
            } else {
                commands.entity(entity).despawn();
            }
        }
    }
}

/// Applies fall damage when the player lands after a significant drop.
///
/// Reads `Player::impact_speed` set by [`player_resolve_collision`] and
/// computes damage as `(impact_speed - fall_safe_speed).max(0) * 2.0`. When
/// health reaches zero the player is teleported to [`PlayerHealth::spawn_position`]
/// and a [`PlayerDied`] message is sent.
///
/// Run this system **after** [`player_resolve_collision`].
pub fn player_fall_damage(
    mut player_q: Query<(&mut Player, &mut Transform)>,
    mut health: ResMut<PlayerHealth>,
    mut damaged_writer: MessageWriter<PlayerDamaged>,
    mut died_writer: MessageWriter<PlayerDied>,
) {
    let Ok((mut player, mut transform)) = player_q.single_mut() else {
        return;
    };

    if player.impact_speed <= 0.0 {
        return;
    }

    // First landing after spawn or respawn is immune to fall damage so the
    // player doesn't die just because the spawn point is a few blocks above
    // the terrain surface.
    if player.spawn_immune {
        player.spawn_immune = false;
        player.impact_speed = 0.0;
        return;
    }

    let damage = (player.impact_speed - health.fall_safe_speed).max(0.0) * 2.0;
    player.impact_speed = 0.0;

    if damage <= 0.0 {
        return;
    }

    health.health = (health.health - damage).max(0.0);
    damaged_writer.write(PlayerDamaged { amount: damage });

    if health.health <= 0.0 {
        health.health = health.max_health;
        transform.translation = health.spawn_position;
        // Reset velocity so the player doesn't inherit falling momentum after respawn.
        player.vertical_velocity = 0.0;
        player.velocity = Vec3::ZERO;
        player.world_loaded = false; // re-freeze gravity until feet chunk reloads
        player.spawn_immune = true; // grant immunity on the next landing too
        died_writer.write(PlayerDied);
    }
}

/// Processes incoming damage messages and runs the death / respawn check.
///
/// Regen and poison are handled exclusively by
/// [`apply_health_effects`](crate::player::effects::apply_health_effects)
/// inside [`StatusEffectsPlugin`](crate::player::effects::StatusEffectsPlugin).
/// This system only reads [`AppliedEffectModifiers`](crate::player::effects::AppliedEffectModifiers) to apply damage resistance
/// to incoming [`PlayerDamaged`] events. `AppliedEffectModifiers` is optional —
/// the system degrades gracefully when no `StatusEffectsPlugin` is registered.
///
/// Run this system **every frame**, regardless of pause state.
pub fn health_system(
    applied: Option<Res<effects::AppliedEffectModifiers>>,
    mut health: ResMut<PlayerHealth>,
    mut damaged_reader: bevy::ecs::message::MessageReader<PlayerDamaged>,
    mut died_writer: MessageWriter<PlayerDied>,
) {
    // Process every pending damage event, reduced by damage resistance.
    for event in damaged_reader.read() {
        let resistance = applied
            .as_ref()
            .map(|a| a.0.damage_resistance)
            .unwrap_or(0.0);
        let actual = event.amount * (1.0 - resistance.clamp(0.0, 1.0));
        health.health = (health.health - actual).max(0.0);
    }

    // Death check — instant respawn (position is handled by respawn_system).
    if health.health <= 0.0 {
        health.health = health.max_health;
        died_writer.write(PlayerDied);
    }
}

/// Teleports the streaming origin to the spawn position on player death.
///
/// Listens for [`PlayerDied`] messages and updates [`ViewerPosition`] so the
/// chunk streaming system starts loading terrain around the respawn location
/// immediately. Physical entity teleportation is handled by
/// [`player_fall_damage`].
///
/// If no [`PlayerConfig`] resource is present, the player is teleported to
/// [`Vec3::ZERO`].
pub fn respawn_system(
    mut died_reader: bevy::ecs::message::MessageReader<PlayerDied>,
    mut viewer: ResMut<ViewerPosition>,
    config: Option<Res<PlayerConfig>>,
) {
    for _ in died_reader.read() {
        let spawn = config
            .as_ref()
            .map(|c| c.spawn_position)
            .unwrap_or(Vec3::ZERO);
        viewer.position = spawn;
    }
}

// ─── Push helper ─────────────────────────────────────────────────────────────

/// Returns the corrected capsule-center position after pushing out of a solid voxel.
///
/// The algorithm treats the capsule as a swept sphere along the spine segment
/// `[pos − up*half_height, pos + up*half_height]`:
///
/// 1. Find the point on the spine segment closest to the voxel AABB center
///    (clamped parametric projection, `t ∈ [0, 1]`).
/// 2. Treat that spine point as a sphere center `sphere_c` with radius
///    [`PLAYER_RADIUS`].
/// 3. Find the point on the voxel AABB closest to `sphere_c`.
/// 4. If the sphere overlaps the AABB (distance² < `PLAYER_RADIUS²`), compute
///    the minimum translation vector and apply it to `pos`.
///
/// A degenerate case (sphere center exactly inside the voxel) is handled by
/// pushing along the dominant axis of the relative offset to avoid a zero-
/// length normal.
fn push_capsule_out(
    pos: Vec3,
    up: Vec3,
    half_height: f32,
    chunk_pos: Vec3,
    vx: i32,
    vy: i32,
    vz: i32,
) -> Vec3 {
    let vmin = chunk_pos + Vec3::new(vx as f32, vy as f32, vz as f32) * VOXEL_SIZE;
    let vmax = vmin + Vec3::splat(VOXEL_SIZE);

    // Capsule spine endpoints.
    let p0 = pos - up * half_height;
    let p1 = pos + up * half_height;
    let spine = p1 - p0;

    // Closest point on spine to the voxel center.
    let vox_center = (vmin + vmax) * 0.5;
    let len_sq = spine.length_squared();
    let sphere_c = if len_sq > f32::EPSILON {
        let t = ((vox_center - p0).dot(spine) / len_sq).clamp(0.0, 1.0);
        p0 + spine * t
    } else {
        pos
    };

    // Sphere-AABB collision.
    let closest = sphere_c.clamp(vmin, vmax);
    let diff = sphere_c - closest;
    let dist_sq = diff.length_squared();

    if dist_sq >= PLAYER_RADIUS * PLAYER_RADIUS {
        return pos;
    }

    let delta = if dist_sq > f32::EPSILON {
        let dist = dist_sq.sqrt();
        (diff / dist) * (PLAYER_RADIUS - dist)
    } else {
        // Degenerate: center exactly inside voxel — push along dominant axis.
        let rel = sphere_c - vox_center;
        let abs_rel = rel.abs();
        let half = VOXEL_SIZE * 0.5 + PLAYER_RADIUS;
        if abs_rel.x >= abs_rel.y && abs_rel.x >= abs_rel.z {
            Vec3::new(half * rel.x.signum() - rel.x, 0.0, 0.0)
        } else if abs_rel.y >= abs_rel.z {
            Vec3::new(0.0, half * rel.y.signum() - rel.y, 0.0)
        } else {
            Vec3::new(0.0, 0.0, half * rel.z.signum() - rel.z)
        }
    };

    pos + delta
}

/// Emits [`SfxRequest`] footstep sounds when the player walks on solid ground.
///
/// Samples the voxel directly below the player's feet and passes its byte
/// representation as [`SfxEvent::Footstep`] so games can map surface types to
/// different audio assets. The step interval scales with horizontal speed.
pub fn player_footstep_system<V: VoxelType>(
    mut query: Query<(&mut Player, &Transform)>,
    time: Res<Time>,
    manager: Res<ChunkManager>,
    chunks: Query<&Chunk<V>>,
    mut sfx: MessageWriter<SfxRequest>,
) {
    let Ok((mut player, transform)) = query.single_mut() else {
        return;
    };

    if !matches!(player.mode, PlayerMode::Grounded { .. }) || !player.on_ground {
        return;
    }

    let h_speed = Vec3::new(player.velocity.x, 0.0, player.velocity.z).length();
    if h_speed < 0.5 {
        player.footstep_timer = 0.0;
        return;
    }

    // Step interval: shorter at higher speeds (clamped between 0.2–0.5s).
    let step_interval = (0.5 / h_speed).clamp(0.2, 0.5);
    player.footstep_timer += time.delta_secs();

    if player.footstep_timer < step_interval {
        return;
    }
    player.footstep_timer = 0.0;

    // Sample voxel just below the player's feet.
    let foot_pos = transform.translation - Vec3::Y * (PLAYER_HALF_HEIGHT + 0.3);
    let voxel_byte = get_voxel_at::<V>(foot_pos, &manager, &chunks)
        .map(|v| v.to_byte())
        .unwrap_or(0);

    sfx.write(SfxRequest {
        event: SfxEvent::Footstep(voxel_byte),
        position: None,
    });
}

// ─── Spawn persistence ────────────────────────────────────────────────────────

/// Startup system that applies a persisted spawn position to [`PlayerConfig`]
/// and [`PlayerHealth`] before the player entity is spawned.
///
/// Reads the world data directory from [`WorldDataDir`] (set by
/// [`PlayerPlugin::with_world`]).  Silent no-op if `player_data.json` does not
/// exist.
fn load_spawn_on_startup(
    mut config: ResMut<PlayerConfig>,
    mut health: ResMut<PlayerHealth>,
    world_data_dir: Res<WorldDataDir>,
) {
    let data_dir = world_data_dir.as_str();
    if let Some(pos) = persistence::load_spawn_position(data_dir) {
        config.spawn_position = pos;
        health.spawn_position = pos;
    } else {
        // First run: persist the configured default so subsequent runs reload it.
        persistence::save_spawn_position(config.spawn_position, data_dir);
    }
}

/// System that saves [`PlayerHealth::spawn_position`] to disk on [`AppExit`].
///
/// This preserves a custom spawn set at runtime (e.g. sleeping in a bed)
/// across sessions.  The [`PlayerConfig::spawn_position`] default is used as
/// fallback when no file exists.
fn save_spawn_on_exit(
    health: Res<PlayerHealth>,
    world_data_dir: Res<WorldDataDir>,
    mut exit_reader: bevy::ecs::message::MessageReader<AppExit>,
) {
    for _ in exit_reader.read() {
        persistence::save_spawn_position(health.spawn_position, world_data_dir.as_str());
    }
}

// ─── Run condition ────────────────────────────────────────────────────────────

/// Returns `true` when gameplay should be active.
///
/// If [`crate::ui::PauseMenuState`] is registered, returns `true`
/// only while in the `Playing` variant. If the state is not present (i.e.
/// [`crate::ui::PauseMenuPlugin`] was not added), always returns `true` so the
/// player systems run unconditionally.
fn gameplay_active(state: Option<Res<bevy::prelude::State<crate::ui::PauseMenuState>>>) -> bool {
    use crate::ui::PauseMenuState;
    state.map_or(true, |s| matches!(s.get(), PauseMenuState::Playing))
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Plugin that wires up the complete 6DOF player controller.
///
/// Registers [`PlayerConfig`] as an init resource, spawns the player camera
/// on `Startup`, and chains the three frame-update systems in the correct
/// dependency order every `Update`:
///
/// ```text
/// player_look → player_move → player_resolve_collision<V>
/// ```
///
/// [`player_cursor_system`] is added as a separate, un-chained `Update` system
/// so cursor events are processed independently of the movement pipeline.
///
/// All systems run **unconditionally** every frame. If your game requires
/// state-gated player control (e.g., pausing during a menu), skip this plugin
/// and register the systems manually:
///
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use voxel_framework::player::{spawn_player, player_look, player_move,
/// #     player_resolve_collision};
/// # use voxel_framework::voxel::DefaultVoxel;
/// # #[derive(States, Default, Clone, PartialEq, Eq, Hash, Debug)]
/// # enum AppState { #[default] Playing }
/// App::new()
///     .add_systems(Startup, spawn_player)
///     .add_systems(Update,
///         (player_look, player_move::<DefaultVoxel>, player_resolve_collision::<DefaultVoxel>)
///             .chain()
///             .run_if(in_state(AppState::Playing)),
///     );
/// ```
///
/// The generic parameter `V` must match the [`VoxelType`] used by the loaded
/// [`Chunk`] entities so the collision system can read
/// solid-voxel data.
pub struct PlayerPlugin<V: VoxelType> {
    _marker: std::marker::PhantomData<V>,
    world_name: String,
}

impl<V: VoxelType> PlayerPlugin<V> {
    /// Creates a `PlayerPlugin` using the default world name (`"world"`).
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
            world_name: String::new(),
        }
    }

    /// Creates a `PlayerPlugin` that stores player data under
    /// `./worlds/{world_name}/player_data.json`.
    pub fn with_world(world_name: impl Into<String>) -> Self {
        Self {
            _marker: std::marker::PhantomData,
            world_name: world_name.into(),
        }
    }
}

impl<V: VoxelType> Default for PlayerPlugin<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: VoxelType> Plugin for PlayerPlugin<V> {
    fn build(&self, app: &mut App) {
        let name = if self.world_name.is_empty() {
            "world"
        } else {
            &self.world_name
        };
        app.insert_resource(WorldDataDir::for_world(name));
        app.init_resource::<PlayerConfig>();
        app.init_resource::<PlayerHealth>();
        app.init_resource::<PlayerCrouching>();
        app.add_message::<PlayerDamaged>();
        app.add_message::<PlayerDied>();
        app.add_systems(Startup, (load_spawn_on_startup, spawn_player).chain());
        app.add_systems(
            Update,
            (
                player_look,
                player_move::<V>,
                player_resolve_collision::<V>,
                player_fall_damage,
                player_footstep_system::<V>,
            )
                .chain()
                .run_if(gameplay_active),
        );
        // health_system and respawn_system run unconditionally (not gated by pause).
        app.add_systems(Update, health_system);
        app.add_systems(Update, respawn_system);
        app.add_systems(Update, player_cursor_system.run_if(gameplay_active));
        app.add_systems(PostUpdate, save_spawn_on_exit);
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_default_values() {
        let p = Player::default();
        assert_eq!(p.speed, 4.0);
        assert_eq!(p.sensitivity, 1.0);
        assert_eq!(p.roll_speed, 1.5);
        assert_eq!(p.velocity, Vec3::ZERO);
    }

    #[test]
    fn player_config_default_values() {
        let c = PlayerConfig::default();
        assert_eq!(c.speed, 4.0);
        assert_eq!(c.fov_degrees, 90.0);
        assert!(matches!(
            c.mode,
            PlayerMode::FreeFlightSixDof {
                roll_mode: RollMode::Continuous
            }
        ));
        assert!((c.snap_roll_duration_secs - 0.5).abs() < f32::EPSILON);
        assert_eq!(c.snap_roll_easing, SnapRollEasing::EaseOutCubic);
    }

    #[test]
    fn snap_roll_completes_to_90deg() {
        let mut anim = start_or_retarget_snap(
            None,
            Vec3::Z,
            Vec3::Y,
            1,
            0.18,
            SnapRollEasing::EaseOutCubic,
        );
        let (delta, finished) = advance_snap(&mut anim, 0.18);
        assert!(finished);
        assert!(
            (delta - std::f32::consts::FRAC_PI_2).abs() < 1e-4,
            "Expected ~PI/2, got {delta}"
        );
    }

    #[test]
    fn snap_roll_easing_monotonic() {
        let mut anim = start_or_retarget_snap(
            None,
            Vec3::Z,
            Vec3::Y,
            1,
            0.18,
            SnapRollEasing::EaseOutCubic,
        );
        let step = 0.045;
        let mut deltas = Vec::new();
        for _ in 0..4 {
            let (d, _) = advance_snap(&mut anim, step);
            assert!(d > 0.0, "Delta should be positive, got {d}");
            deltas.push(d);
        }
        let total: f32 = deltas.iter().sum();
        assert!(
            (total - std::f32::consts::FRAC_PI_2).abs() < 1e-3,
            "Sum should be ~PI/2, got {total}"
        );
        // Ease-out: deltas should be non-increasing (each <= previous)
        for i in 1..deltas.len() {
            assert!(
                deltas[i] <= deltas[i - 1] + 1e-6,
                "Easing not monotonically decreasing: {:?}",
                deltas
            );
        }
    }

    #[test]
    fn snap_roll_same_direction_extends_target() {
        let mut anim = start_or_retarget_snap(
            None,
            Vec3::Z,
            Vec3::Y,
            1,
            0.18,
            SnapRollEasing::EaseOutCubic,
        );
        // Advance ~30%
        advance_snap(&mut anim, 0.054);
        let prev_applied = anim.current_applied;
        // The transform has rotated with the animation: up at press time
        // reflects the applied angle (world-aligned start).
        let up = Quat::from_axis_angle(Vec3::Z, anim.current_applied) * Vec3::Y;
        let new_anim = start_or_retarget_snap(
            Some(anim),
            Vec3::Z,
            up,
            1,
            0.18,
            SnapRollEasing::EaseOutCubic,
        );
        assert!((new_anim.start_angle - prev_applied).abs() < 1e-6);
        assert!(
            (new_anim.target_angle - std::f32::consts::PI).abs() < 1e-4,
            "Expected target ~PI (2x90°), got {}",
            new_anim.target_angle
        );
    }

    #[test]
    fn snap_roll_opposite_direction_reverses_to_lattice() {
        let mut anim = start_or_retarget_snap(
            None,
            Vec3::Z,
            Vec3::Y,
            1,
            0.18,
            SnapRollEasing::EaseOutCubic,
        );
        // Advance ~30% (heading to PI/2 ≈ 1.571)
        advance_snap(&mut anim, 0.054);
        let current = anim.current_applied;
        assert!(current > 0.0 && current < std::f32::consts::FRAC_PI_2);
        // The transform has rotated with the animation: up at press time
        // reflects the applied angle (world-aligned start).
        let up = Quat::from_axis_angle(Vec3::Z, current) * Vec3::Y;
        let new_anim = start_or_retarget_snap(
            Some(anim),
            Vec3::Z,
            up,
            -1,
            0.18,
            SnapRollEasing::EaseOutCubic,
        );
        // Should target 0.0 (nearest lattice below current in negative direction)
        assert!(
            new_anim.target_angle.abs() < 1e-6,
            "Expected target ~0, got {}",
            new_anim.target_angle
        );
    }

    #[test]
    fn continuous_mode_unchanged() {
        let player = Player::default();
        assert!(matches!(
            player.mode,
            PlayerMode::FreeFlightSixDof {
                roll_mode: RollMode::Continuous
            }
        ));
        assert!(player.snap_anim.is_none());
    }

    #[test]
    fn snap_roll_config_copies_to_player() {
        let config = PlayerConfig {
            mode: PlayerMode::FreeFlightSixDof {
                roll_mode: RollMode::Snap,
            },
            snap_roll_duration_secs: 0.3,
            snap_roll_easing: SnapRollEasing::Smoothstep,
            ..default()
        };
        let player = Player {
            mode: config.mode.clone(),
            snap_roll_duration: config.snap_roll_duration_secs,
            snap_roll_easing: config.snap_roll_easing,
            ..default()
        };
        assert!(matches!(
            player.mode,
            PlayerMode::FreeFlightSixDof {
                roll_mode: RollMode::Snap
            }
        ));
        assert!((player.snap_roll_duration - 0.3).abs() < f32::EPSILON);
        assert_eq!(player.snap_roll_easing, SnapRollEasing::Smoothstep);
    }

    #[test]
    fn snap_roll_start_from_none_is_independent() {
        let anim1 = start_or_retarget_snap(
            None,
            Vec3::Z,
            Vec3::Y,
            1,
            0.18,
            SnapRollEasing::EaseOutCubic,
        );
        let anim2 = start_or_retarget_snap(
            None,
            Vec3::Z,
            Vec3::Y,
            1,
            0.18,
            SnapRollEasing::EaseOutCubic,
        );
        assert!((anim1.target_angle - anim2.target_angle).abs() < f32::EPSILON);
        assert!((anim1.start_angle - anim2.start_angle).abs() < f32::EPSILON);
    }

    #[test]
    fn lattice_axis_forward_yields_four_candidates_at_90deg() {
        // Looking straight down: local Z (back) parallel to world Y.
        let mut angles = world_lattice_angles(Vec3::Y, Vec3::Z);
        assert_eq!(angles.len(), 4, "Y axis pair must be excluded: {angles:?}");
        angles.sort_by(f32::total_cmp);
        for pair in angles.windows(2) {
            assert!(
                (pair[1] - pair[0] - std::f32::consts::FRAC_PI_2).abs() < 1e-5,
                "Candidates must be exactly 90° apart: {angles:?}"
            );
        }
    }

    #[test]
    fn lattice_near_axis_forward_still_yields_four_candidates() {
        // Playtest regression: view drifted a few degrees off a world axis
        // must NOT resurrect the near-forward axis pair as junk candidates
        // (6 stops with two perceptually arbitrary ones). 45° dominance rule.
        let local_z = Quat::from_axis_angle(Vec3::X, 8_f32.to_radians()) * Vec3::Z;
        let up = Quat::from_axis_angle(local_z, 5_f32.to_radians())
            * (Vec3::Y - Vec3::Y.dot(local_z) * local_z).normalize();

        let angles = world_lattice_angles(local_z, up);
        assert_eq!(
            angles.len(),
            4,
            "Near-axis view must exclude the ±Z pair, got {angles:?}"
        );

        // Exact 45° edge-on view: both axis families sit at proj² ≈ 0.5.
        // Float rounding must never exclude both (≥4 invariant) — the 0.49
        // threshold keeps the tie inclusive, so all 6 survive.
        let boundary_z = Vec3::new(0.0, 1.0, 1.0).normalize();
        let boundary_up = (Vec3::Y - Vec3::Y.dot(boundary_z) * boundary_z).normalize();
        assert_eq!(
            world_lattice_angles(boundary_z, boundary_up).len(),
            6,
            "At exactly 45° both axis families are equally valid"
        );
    }

    #[test]
    fn lattice_diagonal_forward_yields_six_candidates_cycling_2pi() {
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_3, TAU};
        // Forward = (1,1,1)/√3, so local Z (back) is the opposite diagonal.
        let local_z = -Vec3::ONE.normalize();
        // Start exactly on a candidate: world X projected onto the roll plane.
        let start = (Vec3::X - Vec3::X.dot(local_z) * local_z).normalize();

        let angles = world_lattice_angles(local_z, start);
        assert_eq!(
            angles.len(),
            6,
            "All 6 axes survive on a diagonal: {angles:?}"
        );

        // Six Q presses walk the hexagon: each step ~60° (never 90°),
        // the total is a full turn, and we return to the start candidate.
        let mut up = start;
        let mut total = 0.0_f32;
        for _ in 0..6 {
            let step = next_lattice_step(&world_lattice_angles(local_z, up), 1);
            assert!(
                (step - FRAC_PI_3).abs() < 1e-3,
                "Expected ~60° step, got {step}"
            );
            assert!(
                (step - FRAC_PI_2).abs() > 0.1,
                "Step must not be 90°, got {step}"
            );
            total += step;
            up = Quat::from_axis_angle(local_z, step) * up;
        }
        assert!(
            (total - TAU).abs() < 1e-3,
            "Six presses must sum to 2π, got {total}"
        );
        assert!(
            up.dot(start) > 0.9999,
            "Cycle must return to start, got {up:?}"
        );
    }

    #[test]
    fn lattice_drifted_up_travels_residual() {
        // Up rolled 10° off the Y candidate (CCW, rotate_local_z convention).
        let up = Quat::from_axis_angle(Vec3::Z, 10_f32.to_radians()) * Vec3::Y;
        let angles = world_lattice_angles(Vec3::Z, up);

        let q = next_lattice_step(&angles, 1);
        let e = next_lattice_step(&angles, -1);
        assert!(
            (q - 80_f32.to_radians()).abs() < 1e-4,
            "Q must travel ≈+80°, got {q}"
        );
        assert!(
            (e + 10_f32.to_radians()).abs() < 1e-4,
            "E must travel ≈−10°, got {e}"
        );

        // Both presses land within ε of a world candidate.
        let up_q = Quat::from_axis_angle(Vec3::Z, q) * up;
        let up_e = Quat::from_axis_angle(Vec3::Z, e) * up;
        assert!(
            up_q.dot(Vec3::NEG_X) > 0.9999,
            "Q must land on −X, got {up_q:?}"
        );
        assert!(
            up_e.dot(Vec3::Y) > 0.9999,
            "E must land on +Y, got {up_e:?}"
        );
    }

    #[test]
    fn lattice_eps_on_skips_exact_candidate() {
        // Up exactly on a candidate: the press must travel a full step, not ~0.
        let angles = world_lattice_angles(Vec3::Z, Vec3::Y);
        let q = next_lattice_step(&angles, 1);
        let e = next_lattice_step(&angles, -1);
        assert!(
            (q - std::f32::consts::FRAC_PI_2).abs() < 1e-5,
            "Q from an exact candidate must be +90°, got {q}"
        );
        assert!(
            (e + std::f32::consts::FRAC_PI_2).abs() < 1e-5,
            "E from an exact candidate must be −90°, got {e}"
        );
    }

    #[test]
    fn snap_retarget_drifted_frame_lands_on_candidates() {
        let local_z = Vec3::Z;
        // Start 10° off the Y candidate: fresh Q targets the −X candidate (+80°).
        let up0 = Quat::from_axis_angle(local_z, 10_f32.to_radians()) * Vec3::Y;

        // Same-direction press mid-flight extends to the NEXT candidate (−Y).
        let mut anim =
            start_or_retarget_snap(None, local_z, up0, 1, 0.18, SnapRollEasing::EaseOutCubic);
        advance_snap(&mut anim, 0.054);
        let up_mid = Quat::from_axis_angle(local_z, anim.current_applied) * up0;
        let extended = start_or_retarget_snap(
            Some(anim),
            local_z,
            up_mid,
            1,
            0.18,
            SnapRollEasing::EaseOutCubic,
        );
        let up_ext = Quat::from_axis_angle(local_z, extended.target_angle) * up0;
        assert!(
            up_ext.dot(Vec3::NEG_Y) > 0.9999,
            "Extend must land on −Y, got {up_ext:?}"
        );

        // Opposite press mid-flight reverses to the candidate behind (+Y).
        let mut anim =
            start_or_retarget_snap(None, local_z, up0, 1, 0.18, SnapRollEasing::EaseOutCubic);
        advance_snap(&mut anim, 0.054);
        let up_mid = Quat::from_axis_angle(local_z, anim.current_applied) * up0;
        let reversed = start_or_retarget_snap(
            Some(anim),
            local_z,
            up_mid,
            -1,
            0.18,
            SnapRollEasing::EaseOutCubic,
        );
        let up_rev = Quat::from_axis_angle(local_z, reversed.target_angle) * up0;
        assert!(
            up_rev.dot(Vec3::Y) > 0.9999,
            "Reverse must land on +Y, got {up_rev:?}"
        );
    }

    #[test]
    fn push_capsule_no_collision() {
        // Voxel at origin, player far away — should not move.
        let pos = Vec3::new(10.0, 10.0, 10.0);
        let result = push_capsule_out(pos, Vec3::Y, PLAYER_HALF_HEIGHT, Vec3::ZERO, 0, 0, 0);
        assert_eq!(result, pos);
    }

    #[test]
    fn push_capsule_resolves_overlap() {
        // Player center directly on top of a voxel — must be pushed out.
        let pos = Vec3::new(0.5, 0.5, 0.5); // inside voxel [0,0,0]
        let result = push_capsule_out(pos, Vec3::Y, PLAYER_HALF_HEIGHT, Vec3::ZERO, 0, 0, 0);
        // After push, capsule sphere should clear the voxel.
        let vox_center = Vec3::splat(0.5);
        let closest = result.clamp(Vec3::ZERO, Vec3::ONE);
        let dist = (result - closest).length();
        assert!(
            dist >= PLAYER_RADIUS - 0.001,
            "dist={dist} must be >= PLAYER_RADIUS={PLAYER_RADIUS}"
        );
        let _ = vox_center; // silence unused warning
    }

    // ─── Anchored look: pitch clamp ──────────────────────────────────────────

    #[test]
    fn clamp_pitch_under_limit_passes_through() {
        // Level forward (pitch 0 vs +Y anchor): moderate deltas are untouched.
        let d = clamp_pitch_delta(Vec3::NEG_Z, Vec3::Y, 30_f32.to_radians());
        assert!((d - 30_f32.to_radians()).abs() < 1e-5, "got {d}");
        let d = clamp_pitch_delta(Vec3::NEG_Z, Vec3::Y, -30_f32.to_radians());
        assert!((d + 30_f32.to_radians()).abs() < 1e-5, "got {d}");
    }

    #[test]
    fn clamp_pitch_crossing_limit_truncates() {
        // From +80° elevation a +20° request truncates to +9° (lands at +89°).
        let f_hi = Vec3::new(0.0, 80_f32.to_radians().sin(), -(80_f32.to_radians().cos()));
        let d = clamp_pitch_delta(f_hi, Vec3::Y, 20_f32.to_radians());
        assert!(
            (d - 9_f32.to_radians()).abs() < 1e-4,
            "expected ~+9°, got {d}"
        );

        // Mirror: from −80° a −20° request truncates to −9° (lands at −89°).
        let f_lo = Vec3::new(
            0.0,
            -(80_f32.to_radians().sin()),
            -(80_f32.to_radians().cos()),
        );
        let d = clamp_pitch_delta(f_lo, Vec3::Y, -20_f32.to_radians());
        assert!(
            (d + 9_f32.to_radians()).abs() < 1e-4,
            "expected ~−9°, got {d}"
        );
    }

    #[test]
    fn clamp_pitch_at_limit_blocks_outward_allows_inward() {
        // Exactly at the +89° pole: outward is blocked, inward passes through.
        let f = Vec3::new(0.0, 89_f32.to_radians().sin(), -(89_f32.to_radians().cos()));
        let outward = clamp_pitch_delta(f, Vec3::Y, 5_f32.to_radians());
        assert!(
            outward.abs() < 1e-4,
            "outward delta at the pole must be ~0, got {outward}"
        );
        let inward = clamp_pitch_delta(f, Vec3::Y, -5_f32.to_radians());
        assert!(
            (inward + 5_f32.to_radians()).abs() < 1e-4,
            "inward delta must pass through, got {inward}"
        );
    }

    // ─── Anchored look: lattice candidates ───────────────────────────────────

    #[test]
    fn candidates_axis_forward_have_source_axes() {
        use std::f32::consts::FRAC_PI_2;
        // Axial view (local Z = world Z, up = Y): the ±Z pair is filtered, the
        // four survivors carry their source axes with the expected angles.
        let cands = world_lattice_candidates(Vec3::Z, Vec3::Y);
        assert_eq!(cands.len(), 4, "±Z must be excluded: {cands:?}");
        let angle_of = |axis: Vec3| {
            cands
                .iter()
                .find(|(_, a)| *a == axis)
                .unwrap_or_else(|| panic!("axis {axis:?} missing from {cands:?}"))
                .0
        };
        assert!(
            (angle_of(Vec3::X) + FRAC_PI_2).abs() < 1e-5,
            "X must sit at −π/2"
        );
        assert!(
            (angle_of(Vec3::NEG_X) - FRAC_PI_2).abs() < 1e-5,
            "−X must sit at +π/2"
        );
        assert!(angle_of(Vec3::Y).abs() < 1e-5, "Y must sit at 0");
        assert!(
            (angle_of(Vec3::NEG_Y).abs() - std::f32::consts::PI).abs() < 1e-5,
            "−Y must sit at ±π"
        );
    }

    #[test]
    fn candidates_diagonal_forward_yield_six_axes() {
        // Diagonal view: every signed world axis survives, each exactly once.
        let local_z = -Vec3::ONE.normalize();
        let up = (Vec3::X - Vec3::X.dot(local_z) * local_z).normalize();
        let cands = world_lattice_candidates(local_z, up);
        assert_eq!(
            cands.len(),
            6,
            "all 6 axes survive on a diagonal: {cands:?}"
        );
        for axis in AXES {
            assert_eq!(
                cands.iter().filter(|(_, a)| *a == axis).count(),
                1,
                "axis {axis:?} must appear exactly once: {cands:?}"
            );
        }
    }

    #[test]
    fn lattice_angles_equal_candidate_angles() {
        // The wrapper must be observably identical to the candidate angles.
        let diag_z = -Vec3::ONE.normalize();
        let diag_up = (Vec3::X - Vec3::X.dot(diag_z) * diag_z).normalize();
        for (local_z, up) in [(Vec3::Z, Vec3::Y), (diag_z, diag_up)] {
            let angles = world_lattice_angles(local_z, up);
            let cands = world_lattice_candidates(local_z, up);
            assert_eq!(angles.len(), cands.len());
            for (a, (c, _)) in angles.iter().zip(cands.iter()) {
                assert_eq!(a, c, "wrapper angle {a} != candidate angle {c}");
            }
        }
    }

    // ─── Anchored look: candidate stepping ───────────────────────────────────

    #[test]
    fn next_lattice_candidate_matches_next_lattice_step() {
        // Drifted up (10° off Y): the axis-carrying mirror must agree with the
        // locked next_lattice_step in both directions.
        let up = Quat::from_axis_angle(Vec3::Z, 10_f32.to_radians()) * Vec3::Y;
        let cands = world_lattice_candidates(Vec3::Z, up);
        let angles = world_lattice_angles(Vec3::Z, up);
        for dir in [1, -1] {
            let (cand_angle, _) = next_lattice_candidate(&cands, dir);
            let step = next_lattice_step(&angles, dir);
            assert_eq!(cand_angle, step, "dir {dir}: {cand_angle} != {step}");
        }
    }

    #[test]
    fn next_candidate_q_yields_neg_x_e_yields_x() {
        use std::f32::consts::FRAC_PI_2;
        // From (local_z = Z, up = Y): Q travels +90° to the −X candidate,
        // E travels −90° to the +X candidate.
        let cands = world_lattice_candidates(Vec3::Z, Vec3::Y);
        let (q_angle, q_axis) = next_lattice_candidate(&cands, 1);
        assert!(
            (q_angle - FRAC_PI_2).abs() < 1e-5,
            "Q must travel +π/2, got {q_angle}"
        );
        assert_eq!(q_axis, Vec3::NEG_X);
        let (e_angle, e_axis) = next_lattice_candidate(&cands, -1);
        assert!(
            (e_angle + FRAC_PI_2).abs() < 1e-5,
            "E must travel −π/2, got {e_angle}"
        );
        assert_eq!(e_axis, Vec3::X);
    }

    // ─── Anchored look: reference up and anchor commit ───────────────────────

    #[test]
    fn snap_reference_up_fresh_and_opposite_use_up() {
        let up = Quat::from_axis_angle(Vec3::Z, 10_f32.to_radians()) * Vec3::Y;
        // Fresh press: no animation → reference is the current up.
        assert_eq!(snap_reference_up(None, Vec3::Z, up, 1), up);
        // Opposite-direction press mid-animation → reference is still up.
        let mut anim =
            start_or_retarget_snap(None, Vec3::Z, up, 1, 0.18, SnapRollEasing::EaseOutCubic);
        advance_snap(&mut anim, 0.054);
        assert_eq!(snap_reference_up(Some(anim), Vec3::Z, up, -1), up);
    }

    #[test]
    fn snap_reference_up_same_dir_advances_residual() {
        let up0 = Vec3::Y;
        let mut anim =
            start_or_retarget_snap(None, Vec3::Z, up0, 1, 0.18, SnapRollEasing::EaseOutCubic);
        advance_snap(&mut anim, 0.054);
        // The transform rotated with the animation: up now reflects the
        // applied angle. Same-dir reference = up advanced by the residual,
        // i.e. the in-flight target up.
        let up_mid = Quat::from_axis_angle(Vec3::Z, anim.current_applied) * up0;
        let ref_up = snap_reference_up(Some(anim), Vec3::Z, up_mid, 1);
        let expected =
            Quat::from_axis_angle(Vec3::Z, anim.target_angle - anim.current_applied) * up_mid;
        assert!(
            ref_up.dot(expected) > 0.9999,
            "ref {ref_up:?} != target up {expected:?}"
        );
        // For this fresh anim the in-flight target is the −X candidate.
        assert!(
            ref_up.dot(Vec3::NEG_X) > 0.9999,
            "expected ~−X, got {ref_up:?}"
        );
    }

    #[test]
    fn anchor_commit_matches_pending_projection() {
        let local_z = Vec3::Z;
        let dur = 0.18;
        let ease = SnapRollEasing::EaseOutCubic;
        let up0 = Quat::from_axis_angle(local_z, 10_f32.to_radians()) * Vec3::Y;
        let proj = |axis: Vec3| (axis - axis.dot(local_z) * local_z).normalize();

        // Fresh Q: pending axis −X; the animation must land on its projection.
        let ref_up = snap_reference_up(None, local_z, up0, 1);
        let (_, axis) = next_lattice_candidate(&world_lattice_candidates(local_z, ref_up), 1);
        assert_eq!(axis, Vec3::NEG_X);
        let anim = start_or_retarget_snap(None, local_z, up0, 1, dur, ease);
        let landed = Quat::from_axis_angle(local_z, anim.target_angle) * up0;
        assert!(
            landed.dot(proj(axis)) > 0.9999,
            "fresh: {landed:?} != proj({axis:?})"
        );

        // Extend (same dir mid-anim): pending axis −Y, one candidate further.
        let mut anim = start_or_retarget_snap(None, local_z, up0, 1, dur, ease);
        advance_snap(&mut anim, 0.054);
        let up_mid = Quat::from_axis_angle(local_z, anim.current_applied) * up0;
        let ref_up = snap_reference_up(Some(anim), local_z, up_mid, 1);
        let (_, axis) = next_lattice_candidate(&world_lattice_candidates(local_z, ref_up), 1);
        assert_eq!(axis, Vec3::NEG_Y);
        let extended = start_or_retarget_snap(Some(anim), local_z, up_mid, 1, dur, ease);
        let landed = Quat::from_axis_angle(local_z, extended.target_angle) * up0;
        assert!(
            landed.dot(proj(axis)) > 0.9999,
            "extend: {landed:?} != proj({axis:?})"
        );

        // Reverse (opposite dir mid-anim): pending axis +Y, behind current up.
        let mut anim = start_or_retarget_snap(None, local_z, up0, 1, dur, ease);
        advance_snap(&mut anim, 0.054);
        let up_mid = Quat::from_axis_angle(local_z, anim.current_applied) * up0;
        let ref_up = snap_reference_up(Some(anim), local_z, up_mid, -1);
        let (_, axis) = next_lattice_candidate(&world_lattice_candidates(local_z, ref_up), -1);
        assert_eq!(axis, Vec3::Y);
        let reversed = start_or_retarget_snap(Some(anim), local_z, up_mid, -1, dur, ease);
        let landed = Quat::from_axis_angle(local_z, reversed.target_angle) * up0;
        assert!(
            landed.dot(proj(axis)) > 0.9999,
            "reverse: {landed:?} != proj({axis:?})"
        );
    }

    // ─── Anchored look: spawn anchor and defaults ────────────────────────────

    #[test]
    fn nearest_world_axis_snaps_biased_vectors() {
        assert_eq!(nearest_world_axis(Vec3::new(0.9, 0.1, 0.2)), Vec3::X);
        assert_eq!(nearest_world_axis(Vec3::new(-2.0, 0.5, 0.5)), Vec3::NEG_X);
        assert_eq!(nearest_world_axis(Vec3::new(0.1, -1.5, 0.3)), Vec3::NEG_Y);
        assert_eq!(nearest_world_axis(Vec3::new(-0.1, 0.2, 0.95)), Vec3::Z);
        assert_eq!(nearest_world_axis(Vec3::new(0.2, 0.3, -0.9)), Vec3::NEG_Z);
    }

    #[test]
    fn nearest_world_axis_tie_uses_axes_order() {
        // Strict `>` keeps the first max in AXES order [X, −X, Y, −Y, Z, −Z].
        assert_eq!(nearest_world_axis(Vec3::new(1.0, 1.0, 0.0)), Vec3::X);
        assert_eq!(nearest_world_axis(Vec3::new(0.0, 1.0, 1.0)), Vec3::Y);
        assert_eq!(nearest_world_axis(Vec3::new(-1.0, -1.0, -1.0)), Vec3::NEG_X);
    }

    #[test]
    fn player_mode_defaults_to_sixdof() {
        let p = Player::default();
        assert!(matches!(
            p.mode,
            PlayerMode::FreeFlightSixDof {
                roll_mode: RollMode::Continuous
            }
        ));
        assert_eq!(p.anchor_up, Vec3::Y);
        assert!(p.pending_anchor.is_none());
        assert!(matches!(
            PlayerConfig::default().mode,
            PlayerMode::FreeFlightSixDof {
                roll_mode: RollMode::Continuous
            }
        ));
    }

    #[test]
    fn anchored_mode_config_copies_to_player() {
        // Non-default value must propagate config → component.
        let config = PlayerConfig {
            mode: PlayerMode::FreeFlightAnchored,
            ..default()
        };
        let player = Player {
            mode: config.mode.clone(),
            ..default()
        };
        assert!(matches!(player.mode, PlayerMode::FreeFlightAnchored));
    }

    // ---- Frame-loop integration tests -------------------------------------
    //
    // Three Anchored behaviors are frame-loop properties, not pure functions:
    // a level horizon under diagonal input, mouse-look staying on the OLD
    // anchor while a switch animates, and `RollMode::Continuous` being inert.
    // These drive `player_look` through a headless `App`.

    /// Builds a headless app that runs `player_look` over one Anchored player
    /// starting from the identity transform (forward −Z, up +Y, right +X).
    fn anchored_app() -> (App, Entity) {
        let mut app = App::new();
        app.add_message::<MouseMotion>();
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(Time::<()>::default());
        app.add_systems(Update, player_look);
        let entity = app
            .world_mut()
            .spawn((
                Player {
                    mode: PlayerMode::FreeFlightAnchored,
                    ..default()
                },
                Transform::default(),
            ))
            .id();
        (app, entity)
    }

    /// Advances one frame: sets `dt`, applies the exact held-key set (freshly
    /// added keys register as `just_pressed`, persisting keys do not), injects
    /// mouse motion, then runs the schedule.
    fn step(app: &mut App, mouse: Vec2, held: &[KeyCode], dt: f32) {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(dt));
        {
            let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            input.clear(); // drop last frame's just_pressed / just_released
            let stale: Vec<KeyCode> = input
                .get_pressed()
                .copied()
                .filter(|k| !held.contains(k))
                .collect();
            for k in stale {
                input.release(k);
            }
            for k in held {
                input.press(*k);
            } // re-press keeps held keys out of just_pressed
        }
        if mouse != Vec2::ZERO {
            app.world_mut().write_message(MouseMotion { delta: mouse });
        }
        app.update();
    }

    fn forward_of(app: &App, entity: Entity) -> Vec3 {
        *app.world()
            .entity(entity)
            .get::<Transform>()
            .unwrap()
            .forward()
    }

    #[test]
    fn anchored_diagonal_input_keeps_horizon_level() {
        let (mut app, e) = anchored_app();
        // Strong diagonal input for enough frames to push pitch into the clamp.
        for _ in 0..15 {
            step(&mut app, Vec2::new(60.0, -80.0), &[], 0.016);
            let right = *app.world().entity(e).get::<Transform>().unwrap().right();
            // World-yaw around Y ∘ local-X pitch never rolls the frame: the
            // camera's right vector stays in the horizontal plane.
            assert!(
                right.dot(Vec3::Y).abs() < 1e-4,
                "horizon rolled: right·Y = {}",
                right.dot(Vec3::Y)
            );
        }
        // The ±89° clamp both engaged (we pushed past 88°) and held (never over).
        let elevation = forward_of(&app, e).dot(Vec3::Y).abs();
        assert!(
            elevation >= 88_f32.to_radians().sin(),
            "clamp never engaged: |forward·Y| = {elevation}"
        );
        assert!(
            elevation <= 89_f32.to_radians().sin() + 1e-4,
            "pitch exceeded ±89°: |forward·Y| = {elevation}"
        );
    }

    #[test]
    fn anchored_switch_yaw_uses_old_anchor() {
        let (mut app, e) = anchored_app();
        // Frame 1: tap Q to begin a switch. The default 0.5s duration keeps it
        // running across the next frame (2 × 0.1s = 0.2s < 0.5s).
        step(&mut app, Vec2::ZERO, &[KeyCode::KeyQ], 0.1);
        {
            let p = app.world().entity(e).get::<Player>().unwrap();
            assert!(p.snap_anim.is_some(), "Q should start a switch animation");
            assert!(p.pending_anchor.is_some(), "pending anchor should be set");
            assert_eq!(p.anchor_up, Vec3::Y, "anchor must not commit mid-animation");
        }
        let before = forward_of(&app, e);
        // Frame 2: release Q, yaw with the mouse while the switch still runs.
        step(&mut app, Vec2::new(50.0, 0.0), &[], 0.1);
        let p = app.world().entity(e).get::<Player>().unwrap();
        assert!(p.snap_anim.is_some(), "animation should still be running");
        assert_eq!(
            p.anchor_up,
            Vec3::Y,
            "yaw must not commit the pending anchor"
        );
        let after = forward_of(&app, e);
        // Yaw ran around the OLD anchor (world Y): a rotation about Y preserves
        // forward·Y, and the pending axis is non-Y, so a wrong anchor would
        // tilt forward out of the horizontal plane.
        assert!(
            after.dot(Vec3::Y).abs() < 1e-5,
            "yaw left the Y-plane → wrong anchor"
        );
        assert!(
            (after - before).length() > 1e-3,
            "yaw had no visible effect"
        );
    }

    #[test]
    fn anchored_q_switches_anchor() {
        let (mut app, e) = anchored_app();
        // In SixDof this would roll the frame every tick; in Anchored, Q must
        // instead begin an animated anchor switch — RollMode is inert.
        step(&mut app, Vec2::ZERO, &[KeyCode::KeyQ], 0.1);
        let p = app.world().entity(e).get::<Player>().unwrap();
        assert!(
            p.snap_anim.is_some() && p.pending_anchor.is_some(),
            "Continuous roll_mode must be inert in Anchored — Q takes the switch path"
        );
    }
}
