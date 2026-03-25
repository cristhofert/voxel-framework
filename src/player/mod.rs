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
//!     .add_plugins(VoxelPlugin)
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
//!         (player_look, player_move, player_resolve_collision::<DefaultVoxel>)
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
use crate::chunk::{Chunk, CHUNK_SIZE, VOXEL_SIZE};
use crate::voxel::VoxelType;
use crate::world::{ChunkManager, ViewerPosition, WorldReady, get_voxel_at};

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
/// - [`PlayerMode::FreeFlight`] — classic 6DOF, no gravity.  Default.
/// - [`PlayerMode::Grounded`] — gravity + jump, WASD on the XZ plane, pitch
///   clamped to ±89°.  Suitable for Minetest / Minecraft–style games.
#[derive(Debug, Clone)]
pub enum PlayerMode {
    /// Six-degrees-of-freedom flight. No gravity, no pitch clamp, Q/E roll.
    ///
    /// This is the default mode and matches the original behaviour of the
    /// player controller.  All six movement axes (forward, back, left, right,
    /// up, down) are relative to the camera's current orientation.
    FreeFlight,
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
        PlayerMode::FreeFlight
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

/// Lerp factor applied when the player is accelerating.
const ACCEL: f32 = 12.0;

/// Lerp factor applied when the player is decelerating (no input).
const DECEL: f32 = 7.0;

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
/// | `sensitivity` | `0.0015` |
/// | `roll_speed`  | `1.5`    |
/// | `velocity`    | `ZERO`   |
#[derive(Component, Debug, Clone)]
pub struct Player {
    /// Maximum flight speed in world units per second. Clamped implicitly by
    /// the lerp target: actual speed never exceeds this value regardless of
    /// frame rate.
    pub speed: f32,
    /// Mouse sensitivity in radians per pixel of cursor movement. Lower values
    /// feel slower and more precise; values above `0.003` feel very fast for
    /// most players.
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
    /// lands (collision pushes upward).  Unused in [`PlayerMode::FreeFlight`].
    pub vertical_velocity: f32,
    /// Whether the player was on solid ground at the end of the last frame.
    ///
    /// Set to `true` by [`player_resolve_collision`] when a voxel collision
    /// produces an upward push; cleared to `false` at the start of each
    /// collision pass.  Gating jumps on this flag prevents infinite jumping.
    /// Unused in [`PlayerMode::FreeFlight`].
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
    /// Set to `false` on spawn and `true` when [`crate::world::WorldReady`]
    /// fires. While `false`, gravity is suppressed so the player does not
    /// fall through un-loaded chunks before the world has finished generating.
    pub(crate) world_loaded: bool,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            speed: 4.0,
            sensitivity: 0.0015,
            roll_speed: 1.5,
            velocity: Vec3::ZERO,
            mode: PlayerMode::FreeFlight,
            vertical_velocity: 0.0,
            on_ground: false,
            impact_speed: 0.0,
            footstep_timer: 0.0,
            world_loaded: false,
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
/// | `sensitivity`   | `0.0015` rad/px           |
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
    /// Mouse sensitivity in radians per pixel. Copied into
    /// [`Player::sensitivity`] on spawn.
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
    /// Defaults to [`PlayerMode::FreeFlight`] for backwards compatibility.
    /// Set to [`PlayerMode::Grounded`] for gravity-based gameplay.
    pub mode: PlayerMode,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            spawn_position: Vec3::new(0.0, 0.0, 30.0),
            speed: 4.0,
            sensitivity: 0.0015,
            roll_speed: 1.5,
            look_at: Vec3::ZERO,
            fov_degrees: 90.0,
            mode: PlayerMode::FreeFlight,
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

    commands.spawn((
        Player {
            speed: config.speed,
            sensitivity: config.sensitivity,
            roll_speed: config.roll_speed,
            velocity: Vec3::ZERO,
            mode: config.mode.clone(),
            vertical_velocity: 0.0,
            on_ground: false,
            impact_speed: 0.0,
            footstep_timer: 0.0,
            world_loaded: false, // gravity frozen until WorldReady fires
        },
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov,
            ..default()
        }),
        Transform::from_translation(spawn).looking_at(look_at, Vec3::Y),
    ));
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
    mut query: Query<(&Player, &mut Transform)>,
    mut mouse_motion: MessageReader<MouseMotion>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    let Ok((player, mut transform)) = query.single_mut() else {
        return;
    };

    // Accumulate mouse delta for this frame.
    let mut delta = Vec2::ZERO;
    for event in mouse_motion.read() {
        delta += event.delta;
    }

    match &player.mode {
        PlayerMode::FreeFlight => {
            if delta != Vec2::ZERO {
                let yaw_d   = -delta.x * player.sensitivity;
                let pitch_d = -delta.y * player.sensitivity;
                // Rotate around LOCAL axes — preserves 6DOF orientation.
                transform.rotate_local_y(yaw_d);
                transform.rotate_local_x(pitch_d);
            }

            // Q / E roll.
            let mut roll = 0.0_f32;
            if keys.pressed(KeyCode::KeyQ) { roll += 1.0; }
            if keys.pressed(KeyCode::KeyE) { roll -= 1.0; }
            if roll != 0.0 {
                transform.rotate_local_z(roll * player.roll_speed * time.delta_secs());
            }
        }
        PlayerMode::Grounded { .. } => {
            if delta != Vec2::ZERO {
                let yaw_d   = -delta.x * player.sensitivity;
                let pitch_d = -delta.y * player.sensitivity;

                // Yaw: rotate around WORLD Y so the player stays upright.
                transform.rotate_y(yaw_d);

                // Pitch: decompose current rotation, apply clamped pitch.
                let (yaw, pitch, _roll) = transform.rotation.to_euler(EulerRot::YXZ);
                let new_pitch = (pitch + pitch_d).clamp(
                    -89_f32.to_radians(),
                     89_f32.to_radians(),
                );
                transform.rotation =
                    Quat::from_euler(EulerRot::YXZ, yaw, new_pitch, 0.0);
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
pub fn player_move(
    mut query: Query<(&mut Player, &mut Transform)>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut viewer: ResMut<ViewerPosition>,
) {
    let Ok((mut player, mut transform)) = query.single_mut() else {
        return;
    };

    let dt = time.delta_secs();

    match player.mode.clone() {
        PlayerMode::FreeFlight => {
            // Local-space axes — full 6DOF.
            let forward = transform.rotation * Vec3::NEG_Z;
            let right   = transform.rotation * Vec3::X;
            let up      = transform.rotation * Vec3::Y;

            let mut direction = Vec3::ZERO;
            if keys.pressed(KeyCode::KeyW)      { direction += forward; }
            if keys.pressed(KeyCode::KeyS)      { direction -= forward; }
            if keys.pressed(KeyCode::KeyA)      { direction -= right;   }
            if keys.pressed(KeyCode::KeyD)      { direction += right;   }
            if keys.pressed(KeyCode::Space)     { direction += up;      }
            if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
                direction -= up;
            }

            let target = if direction != Vec3::ZERO {
                direction.normalize() * player.speed
            } else {
                Vec3::ZERO
            };

            let factor = if direction != Vec3::ZERO { ACCEL } else { DECEL };
            player.velocity = player.velocity.lerp(target, (factor * dt).min(1.0));
            transform.translation += player.velocity * dt;
        }
        PlayerMode::Grounded { gravity, jump_speed } => {
            // Horizontal movement on XZ using yaw-only orientation.
            let yaw = transform.rotation.to_euler(EulerRot::YXZ).0;
            let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
            let right   = Vec3::new( yaw.cos(), 0.0, -yaw.sin());

            let mut h_dir = Vec3::ZERO;
            if keys.pressed(KeyCode::KeyW) { h_dir += forward; }
            if keys.pressed(KeyCode::KeyS) { h_dir -= forward; }
            if keys.pressed(KeyCode::KeyA) { h_dir -= right;   }
            if keys.pressed(KeyCode::KeyD) { h_dir += right;   }

            // Sprint (ShiftLeft) and crouch (ControlLeft) speed multipliers.
            // Crouch takes priority over sprint if both are held.
            let crouching = keys.pressed(KeyCode::ControlLeft);
            let sprinting = !crouching && keys.pressed(KeyCode::ShiftLeft);
            let speed_mult = if crouching {
                0.5
            } else if sprinting {
                1.6
            } else {
                1.0
            };

            let h_target = if h_dir != Vec3::ZERO {
                h_dir.normalize() * player.speed * speed_mult
            } else {
                Vec3::ZERO
            };

            let factor = if h_dir != Vec3::ZERO { ACCEL } else { DECEL };
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
                player.vertical_velocity = jump_speed;
            }

            // Apply gravity.
            player.vertical_velocity -= gravity * dt;

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

    let mut pos = player_tf.translation;
    // FreeFlight: capsule spine follows entity rotation (true 6DOF).
    // Grounded: spine is always world-vertical (Vec3::Y) so the capsule stands upright.
    let up = if is_grounded {
        Vec3::Y
    } else {
        player_tf.rotation * Vec3::Y
    };

    let extent = PLAYER_HALF_HEIGHT + PLAYER_RADIUS;
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
                    let new_pos = push_capsule_out(pos, up, chunk_pos, vx, vy, vz);

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
/// Reads [`Player::impact_speed`] set by [`player_resolve_collision`] and
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
        died_writer.write(PlayerDied);
    }
}

// ─── Push helper ─────────────────────────────────────────────────────────────

/// Returns the corrected capsule-center position after pushing out of a solid voxel.
///
/// The algorithm treats the capsule as a swept sphere along the spine segment
/// `[pos − up*PLAYER_HALF_HEIGHT, pos + up*PLAYER_HALF_HEIGHT]`:
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
    chunk_pos: Vec3,
    vx: i32,
    vy: i32,
    vz: i32,
) -> Vec3 {
    let vmin = chunk_pos + Vec3::new(vx as f32, vy as f32, vz as f32) * VOXEL_SIZE;
    let vmax = vmin + Vec3::splat(VOXEL_SIZE);

    // Capsule spine endpoints.
    let p0 = pos - up * PLAYER_HALF_HEIGHT;
    let p1 = pos + up * PLAYER_HALF_HEIGHT;
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

/// Default filename written to the working directory for spawn persistence.
const SPAWN_FILE: &str = "spawn.dat";

/// Saves the spawn position to a plain-text file.
///
/// The file contains three space-separated `f32` values (X Y Z) on one line.
/// Creates the file and any missing parent directories.
///
/// # Errors
///
/// Returns an error if the file cannot be created or written.
pub fn save_spawn_position(
    path: &std::path::Path,
    pos: Vec3,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, format!("{} {} {}", pos.x, pos.y, pos.z))?;
    Ok(())
}

/// Loads a spawn position previously saved with [`save_spawn_position`].
///
/// Returns `None` if the file does not exist or cannot be parsed.
pub fn load_spawn_position(path: &std::path::Path) -> Option<Vec3> {
    let data = std::fs::read_to_string(path).ok()?;
    let mut parts = data.split_whitespace();
    let x: f32 = parts.next()?.parse().ok()?;
    let y: f32 = parts.next()?.parse().ok()?;
    let z: f32 = parts.next()?.parse().ok()?;
    Some(Vec3::new(x, y, z))
}

/// Startup system that applies a persisted spawn position to [`PlayerConfig`]
/// and [`PlayerHealth`] before the player entity is spawned.
///
/// Runs before [`spawn_player`] so the loaded position takes effect on the
/// initial spawn.  Silent no-op if `spawn.dat` does not exist.
fn load_spawn_on_startup(
    mut config: ResMut<PlayerConfig>,
    mut health: ResMut<PlayerHealth>,
) {
    if let Some(pos) = load_spawn_position(std::path::Path::new(SPAWN_FILE)) {
        config.spawn_position = pos;
        health.spawn_position = pos;
    }
}

/// System that saves [`PlayerHealth::spawn_position`] to disk on [`AppExit`].
///
/// This preserves a custom spawn set at runtime (e.g. sleeping in a bed)
/// across sessions.  The [`PlayerConfig::spawn_position`] default is used as
/// fallback when no file exists.
fn save_spawn_on_exit(
    health: Res<PlayerHealth>,
    mut exit_reader: bevy::ecs::message::MessageReader<AppExit>,
) {
    for _ in exit_reader.read() {
        let _ = save_spawn_position(
            std::path::Path::new(SPAWN_FILE),
            health.spawn_position,
        );
    }
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
///         (player_look, player_move, player_resolve_collision::<DefaultVoxel>)
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
}

impl<V: VoxelType> PlayerPlugin<V> {
    pub fn new() -> Self {
        Self { _marker: std::marker::PhantomData }
    }
}

impl<V: VoxelType> Default for PlayerPlugin<V> {
    fn default() -> Self {
        Self::new()
    }
}

fn on_world_ready(
    mut reader: bevy::ecs::message::MessageReader<WorldReady>,
    mut player_q: Query<&mut Player>,
) {
    if reader.read().next().is_some() {
        for mut player in player_q.iter_mut() {
            player.world_loaded = true;
        }
    }
}

impl<V: VoxelType> Plugin for PlayerPlugin<V> {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerConfig>();
        app.init_resource::<PlayerHealth>();
        app.add_message::<PlayerDamaged>();
        app.add_message::<PlayerDied>();
        app.add_systems(Startup, (load_spawn_on_startup, spawn_player).chain());
        app.add_systems(
            Update,
            (
                player_look,
                player_move,
                player_resolve_collision::<V>,
                player_fall_damage,
                player_footstep_system::<V>,
            )
                .chain(),
        );
        app.add_systems(Update, player_cursor_system);
        app.add_systems(PostUpdate, save_spawn_on_exit);
        app.add_systems(Update, on_world_ready);
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
        assert_eq!(p.sensitivity, 0.0015);
        assert_eq!(p.roll_speed, 1.5);
        assert_eq!(p.velocity, Vec3::ZERO);
    }

    #[test]
    fn player_config_default_values() {
        let c = PlayerConfig::default();
        assert_eq!(c.speed, 4.0);
        assert_eq!(c.fov_degrees, 90.0);
    }

    #[test]
    fn push_capsule_no_collision() {
        // Voxel at origin, player far away — should not move.
        let pos = Vec3::new(10.0, 10.0, 10.0);
        let result = push_capsule_out(pos, Vec3::Y, Vec3::ZERO, 0, 0, 0);
        assert_eq!(result, pos);
    }

    #[test]
    fn push_capsule_resolves_overlap() {
        // Player center directly on top of a voxel — must be pushed out.
        let pos = Vec3::new(0.5, 0.5, 0.5); // inside voxel [0,0,0]
        let result = push_capsule_out(pos, Vec3::Y, Vec3::ZERO, 0, 0, 0);
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
}
