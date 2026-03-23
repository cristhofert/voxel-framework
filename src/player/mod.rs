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

use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;

use crate::chunk::{Chunk, CHUNK_SIZE, VOXEL_SIZE};
use crate::voxel::VoxelType;
use crate::world::ViewerPosition;

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
}

impl Default for Player {
    fn default() -> Self {
        Self {
            speed: 4.0,
            sensitivity: 0.0015,
            roll_speed: 1.5,
            velocity: Vec3::ZERO,
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
) {
    if !existing.is_empty() {
        return;
    }

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

    if delta != Vec2::ZERO {
        let yaw_d = -delta.x * player.sensitivity;
        let pitch_d = -delta.y * player.sensitivity;
        // Rotate around LOCAL axes — preserves 6DOF orientation.
        transform.rotate_local_y(yaw_d);
        transform.rotate_local_x(pitch_d);
    }

    // Q / E roll.
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

    // Local-space axes.
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

    let factor = if direction != Vec3::ZERO { ACCEL } else { DECEL };
    player.velocity = player.velocity.lerp(target, (factor * dt).min(1.0));
    transform.translation += player.velocity * dt;

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
    mut player_q: Query<&mut Transform, With<Player>>,
    chunk_q: Query<(&Chunk<V>, &Transform), Without<Player>>,
) {
    let Ok(mut player_tf) = player_q.single_mut() else {
        return;
    };

    let mut pos = player_tf.translation;
    // Local up follows the player's roll/pitch — true 6DOF capsule.
    let up = player_tf.rotation * Vec3::Y;
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
                    pos = push_capsule_out(pos, up, chunk_pos, vx, vy, vz);
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

impl<V: VoxelType> Plugin for PlayerPlugin<V> {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerConfig>();
        app.add_systems(Startup, spawn_player);
        app.add_systems(
            Update,
            (
                player_look,
                player_move,
                player_resolve_collision::<V>,
            )
                .chain(),
        );
        app.add_systems(Update, player_cursor_system);
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
