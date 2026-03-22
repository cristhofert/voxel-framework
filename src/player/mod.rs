//! 6DOF player system (jetpack / astronaut — zero gravity).
//!
//! Provides a first-person camera that can fly freely in any direction,
//! including full 360° roll and pitch, and resolves capsule collisions
//! against solid voxels.
//!
//! # Quick start
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
//! | Escape       | Release cursor (second: close window)|
//!
//! # State integration
//!
//! [`PlayerPlugin`] adds systems unconditionally to `Startup` and `Update`.
//! If your game uses an `AppState`, wrap the systems in `.run_if(in_state(…))`:
//!
//! ```rust,no_run
//! # use bevy::prelude::*;
//! # use voxel_framework::player::{player_look, player_move};
//! # use voxel_framework::voxel::DefaultVoxel;
//! # use voxel_framework::player::player_resolve_collision;
//! # #[derive(States, Default, Clone, PartialEq, Eq, Hash, Debug)]
//! # enum AppState { #[default] Playing }
//! App::new()
//!     .add_systems(Update,
//!         (player_look, player_move, player_resolve_collision::<DefaultVoxel>)
//!             .chain()
//!             .run_if(in_state(AppState::Playing)),
//!     );
//! ```

use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;

use crate::chunk::{Chunk, CHUNK_SIZE, VOXEL_SIZE};
use crate::voxel::VoxelType;
use crate::world::ViewerPosition;

// ─── Physics constants ────────────────────────────────────────────────────────

/// Radius of the player's collision capsule (world units).
pub const PLAYER_RADIUS: f32 = 0.3;

/// Half-height of the capsule's cylindrical spine (world units).
/// The full capsule height is `2 * (PLAYER_HALF_HEIGHT + PLAYER_RADIUS)`.
pub const PLAYER_HALF_HEIGHT: f32 = 0.6;

/// Lerp factor applied when the player is accelerating.
const ACCEL: f32 = 12.0;

/// Lerp factor applied when the player is decelerating (no input).
const DECEL: f32 = 7.0;

// ─── Component ───────────────────────────────────────────────────────────────

/// Player component. Attach together with `Camera3d` on the same entity.
///
/// The camera entity IS the player — there is no separate hierarchy.
#[derive(Component, Debug, Clone)]
pub struct Player {
    /// Maximum flight speed in world units per second.
    pub speed: f32,
    /// Mouse sensitivity (radians per pixel).
    pub sensitivity: f32,
    /// Roll speed in radians per second (Q / E keys).
    pub roll_speed: f32,
    /// Current velocity, accumulated frame-to-frame.
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

/// Configuration resource consumed by [`spawn_player`].
///
/// Insert this before adding [`PlayerPlugin`] to override defaults:
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::player::PlayerConfig;
///
/// App::new()
///     .insert_resource(PlayerConfig {
///         spawn_position: Vec3::new(0.0, 50.0, 150.0),
///         speed: 8.0,
///         ..Default::default()
///     });
/// ```
#[derive(Resource, Debug, Clone)]
pub struct PlayerConfig {
    /// World-space position where the player spawns.
    pub spawn_position: Vec3,
    /// Maximum flight speed (world units / s).
    pub speed: f32,
    /// Mouse sensitivity (radians / pixel).
    pub sensitivity: f32,
    /// Roll speed (radians / s).
    pub roll_speed: f32,
    /// Initial look-at target (used to orient the camera on spawn).
    pub look_at: Vec3,
    /// Vertical field-of-view in degrees.
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

/// Spawns the player camera entity.
///
/// The system is idempotent: if a [`Player`] entity already exists it returns
/// immediately (safe to call on re-entry from pause / resume).
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

/// Updates the player's orientation from mouse motion and Q/E roll keys.
///
/// Implements true 6DOF rotation — no pitch clamp. The player can rotate
/// 360° around any axis like an astronaut.
///
/// Uses `MessageReader<MouseMotion>` and accumulates the delta each frame
/// to avoid inter-frame drift.
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

/// Applies player movement relative to the current 6DOF orientation.
///
/// Velocity is smoothed with a lerp: [`ACCEL`] when input is held,
/// [`DECEL`] when no key is pressed. Also updates [`ViewerPosition`] so
/// the chunk streaming system keeps up.
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

/// Resolves capsule-vs-voxel collisions for the player.
///
/// The capsule spine follows the player's LOCAL Y axis, so it correctly
/// handles any 6DOF orientation (roll / pitch do not break collision).
///
/// Iterates every loaded chunk and checks the voxels within the bounding
/// box of the capsule. For each solid voxel the player is pushed out via
/// [`push_capsule_out`].
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

/// Locks cursor on left click; first Escape releases, second closes the window.
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

/// Pushes `pos` (capsule center) out of a solid voxel AABB.
///
/// Projects the capsule spine onto the nearest point, finds the closest
/// point on that sphere to the AABB, and applies the minimum separation
/// vector needed to clear the surface.
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

/// 6DOF player plugin.
///
/// Adds [`spawn_player`] on `Startup` and the three update systems
/// (`player_look` → `player_move` → `player_resolve_collision`) chained
/// in order each `Update` frame.
///
/// Systems run unconditionally. To gate them behind an `AppState`, add the
/// systems manually with `.run_if(in_state(…))` and omit this plugin.
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
