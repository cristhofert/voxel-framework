//! Simple mob (enemy entity) system for voxel worlds.
//!
//! Provides a complete AI loop — patrol → chase → attack — with voxel
//! collision, gravity, player damage, and death events. Mobs are represented
//! as capsule-shaped entities that collide with the chunk world using
//! [`crate::world::resolve_capsule_collision`].
//!
//! # Quick start
//!
//! ```no_run
//! use bevy::prelude::*;
//! use voxel_framework::VoxelPlugin;
//! use voxel_framework::mobs::{MobPlugin, MobConfig, spawn_mob};
//! use voxel_framework::voxel::DefaultTerrainVoxel;
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(VoxelPlugin::<DefaultTerrainVoxel>::new())
//!     .add_plugins(MobPlugin::<DefaultTerrainVoxel>::new())
//!     .add_systems(Startup, |
//!         mut commands: Commands,
//!         mut meshes: ResMut<Assets<Mesh>>,
//!         mut materials: ResMut<Assets<StandardMaterial>>,
//!     | {
//!         spawn_mob(
//!             Vec3::new(8.0, 70.0, 8.0),
//!             MobConfig::default(),
//!             &mut commands, &mut meshes, &mut materials,
//!         );
//!     })
//!     .run();
//! ```

use bevy::prelude::*;

use crate::chunk::Chunk;
use crate::player::{Player, PlayerHealth};
use crate::voxel::VoxelType;
use crate::world::ChunkManager;
use crate::world::collision::resolve_capsule_collision;

// ─── Constants ────────────────────────────────────────────────────────────────

/// Half-height of the mob capsule along the Y axis (world units).
pub const MOB_HALF_HEIGHT: f32 = 0.5;
/// Radius of the mob capsule (world units).
pub const MOB_RADIUS: f32 = 0.35;

// ─── Components ───────────────────────────────────────────────────────────────

/// AI behaviour state for a [`Mob`] entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MobAiState {
    /// Wanders randomly near the spawn point.
    Patrol,
    /// Has detected the player; moves toward them.
    Chase,
    /// Close enough to strike; swings at the player.
    Attack,
}

/// Core mob component — attached to every mob entity.
#[derive(Component)]
pub struct Mob {
    /// Current hit points. Mob is dead when `<= 0.0`.
    pub health: f32,
    /// Current AI mode.
    pub ai_state: MobAiState,
    /// Horizontal velocity accumulated this frame (m/s).
    pub velocity: Vec3,
    /// Vertical velocity for gravity/jumping (m/s, Y component only).
    pub vertical_velocity: f32,
    /// Whether the mob is resting on solid ground.
    pub on_ground: bool,
    /// Countdown until the next attack swing (seconds).
    pub attack_cooldown: f32,
    /// Accumulated time for patrol direction changes.
    pub patrol_timer: f32,
    /// Current patrol walk direction (horizontal, unnormalised may be zero).
    pub patrol_dir: Vec3,
    /// World-space position where this mob was spawned (patrol centre).
    pub home_position: Vec3,
}

// ─── Config ───────────────────────────────────────────────────────────────────

/// Per-mob-type configuration.
///
/// Insert as many [`MobConfig`] instances as you need — one per mob variety.
/// Pass one to [`spawn_mob`] when spawning an entity.
#[derive(Clone, Debug)]
pub struct MobConfig {
    /// Initial (and maximum) health points.
    pub max_health: f32,
    /// Horizontal movement speed while chasing (m/s).
    pub chase_speed: f32,
    /// Horizontal movement speed while patrolling (m/s).
    pub patrol_speed: f32,
    /// Distance at which the mob detects the player.
    pub detection_radius: f32,
    /// Distance at which melee attacks are triggered.
    pub attack_range: f32,
    /// Damage dealt per attack swing.
    pub attack_damage: f32,
    /// Minimum seconds between attacks.
    pub attack_cooldown: f32,
    /// How far the mob wanders from its spawn point while patrolling.
    pub patrol_radius: f32,
    /// Distance from the player at which the mob despawns (unloads).
    pub despawn_distance: f32,
    /// Gravity acceleration (m/s²). Positive = downward.
    ///
    /// `Some(g)` enables grounded behaviour: the mob accelerates downward each
    /// frame and snaps to the ground via capsule collision (Minecraft-style).
    /// `None` disables gravity and ground-snapping entirely so the mob floats
    /// freely in 3D — useful for zero-g / space games where mobs should drift
    /// alongside a [`crate::player::PlayerMode::FreeFlight`] player.
    ///
    /// In zero-g mode the mob's `vertical_velocity` field stays at `0.0`;
    /// only horizontal AI velocity drives motion.
    pub gravity: Option<f32>,
    /// RGB color used for the default box mesh.
    pub color: Color,
}

impl Default for MobConfig {
    fn default() -> Self {
        Self {
            max_health: 20.0,
            chase_speed: 3.5,
            patrol_speed: 1.2,
            detection_radius: 12.0,
            attack_range: 1.5,
            attack_damage: 4.0,
            attack_cooldown: 1.2,
            patrol_radius: 6.0,
            despawn_distance: 64.0,
            gravity: Some(20.0),
            color: Color::srgb(0.6, 0.1, 0.1),
        }
    }
}

// ─── Events ───────────────────────────────────────────────────────────────────

/// Emitted when a mob's health drops to zero or below.
///
/// Systems that handle loot drops or achievements should read this message.
#[derive(bevy::ecs::message::Message, Debug, Clone)]
pub struct MobDied {
    /// World-space position of the mob at the moment of death.
    pub position: Vec3,
}

// ─── Spawn ────────────────────────────────────────────────────────────────────

/// Spawns a mob entity at `position` using the given [`MobConfig`].
///
/// The mob is rendered as a colored box mesh (1×1×1 voxel).  For custom
/// visuals, spawn the mob yourself and attach the [`Mob`] component instead.
pub fn spawn_mob(
    position: Vec3,
    config: MobConfig,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Entity {
    let mesh = meshes.add(Cuboid::new(
        MOB_RADIUS * 2.0,
        (MOB_HALF_HEIGHT + MOB_RADIUS) * 2.0,
        MOB_RADIUS * 2.0,
    ));
    let material = materials.add(StandardMaterial {
        base_color: config.color,
        ..default()
    });
    commands.spawn((
        Mob {
            health: config.max_health,
            ai_state: MobAiState::Patrol,
            velocity: Vec3::ZERO,
            vertical_velocity: 0.0,
            on_ground: false,
            attack_cooldown: 0.0,
            patrol_timer: 0.0,
            patrol_dir: Vec3::ZERO,
            home_position: position,
        },
        MobConfigComponent(config),
        Transform::from_translation(position),
        Visibility::default(),
        Mesh3d(mesh),
        MeshMaterial3d(material),
    )).id()
}

/// Internal component that stores the [`MobConfig`] on the entity so systems
/// can read it without a separate resource lookup.
#[derive(Component)]
pub struct MobConfigComponent(pub MobConfig);

// ─── Systems ──────────────────────────────────────────────────────────────────

fn mob_ai_system(
    player_q: Query<&Transform, With<Player>>,
    mut mob_q: Query<(&Transform, &mut Mob, &MobConfigComponent)>,
    time: Res<Time>,
) {
    let Ok(player_tf) = player_q.single() else { return };
    let player_pos = player_tf.translation;
    let dt = time.delta_secs();

    for (mob_tf, mut mob, cfg) in mob_q.iter_mut() {
        let mob_pos = mob_tf.translation;
        let dist = mob_pos.distance(player_pos);
        let config = &cfg.0;

        mob.attack_cooldown = (mob.attack_cooldown - dt).max(0.0);
        mob.patrol_timer += dt;

        match mob.ai_state {
            MobAiState::Patrol => {
                if dist < config.detection_radius {
                    mob.ai_state = MobAiState::Chase;
                } else {
                    // Change patrol direction every 3-5 s using timer.
                    if mob.patrol_timer > 3.0 {
                        mob.patrol_timer = 0.0;
                        // Simple hash-based pseudo-random direction.
                        let t = mob_pos.x * 13.7 + mob_pos.z * 7.3 + time.elapsed_secs() * 2.3;
                        mob.patrol_dir = Vec3::new(t.sin(), 0.0, t.cos()).normalize_or_zero();
                        // Stop walking if too far from home.
                        if mob_pos.distance(mob.home_position) > config.patrol_radius {
                            let home_dir = (mob.home_position - mob_pos).normalize_or_zero();
                            mob.patrol_dir = Vec3::new(home_dir.x, 0.0, home_dir.z);
                        }
                    }
                    mob.velocity = mob.patrol_dir * config.patrol_speed;
                }
            }
            MobAiState::Chase => {
                if dist > config.detection_radius * 1.5 {
                    mob.ai_state = MobAiState::Patrol;
                    mob.patrol_dir = Vec3::ZERO;
                } else if dist < config.attack_range {
                    mob.ai_state = MobAiState::Attack;
                    mob.velocity = Vec3::ZERO;
                } else {
                    let dir = (player_pos - mob_pos).normalize_or_zero();
                    mob.velocity = Vec3::new(dir.x, 0.0, dir.z) * config.chase_speed;
                }
            }
            MobAiState::Attack => {
                if dist > config.attack_range * 1.5 {
                    mob.ai_state = MobAiState::Chase;
                }
                // Slow creep toward player even in attack state.
                let dir = (player_pos - mob_pos).normalize_or_zero();
                mob.velocity = Vec3::new(dir.x, 0.0, dir.z) * (config.chase_speed * 0.3);
            }
        }
    }
}

fn mob_attack_system(
    player_q: Query<&Transform, With<Player>>,
    mut mob_q: Query<(&Transform, &mut Mob, &MobConfigComponent)>,
    mut player_health: ResMut<PlayerHealth>,
) {
    let Ok(player_tf) = player_q.single() else { return };
    let player_pos = player_tf.translation;

    for (mob_tf, mut mob, cfg) in mob_q.iter_mut() {
        if mob.ai_state != MobAiState::Attack {
            continue;
        }
        if mob.attack_cooldown > 0.0 {
            continue;
        }
        let dist = mob_tf.translation.distance(player_pos);
        if dist <= cfg.0.attack_range {
            player_health.health = (player_health.health - cfg.0.attack_damage).max(0.0);
            mob.attack_cooldown = cfg.0.attack_cooldown;
        }
    }
}

fn mob_physics_system<V: VoxelType>(
    mut mob_q: Query<(&mut Transform, &mut Mob, &MobConfigComponent)>,
    manager: Res<ChunkManager>,
    chunks: Query<&Chunk<V>>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (mut tf, mut mob, cfg) in mob_q.iter_mut() {
        match cfg.0.gravity {
            // ── Grounded mode (Minecraft-style) ───────────────────────────
            Some(g) => {
                // Gravity.
                if !mob.on_ground {
                    mob.vertical_velocity -= g * dt;
                }

                let move_delta = (mob.velocity + Vec3::Y * mob.vertical_velocity) * dt;
                tf.translation += move_delta;

                // Capsule collision.
                let half = MOB_HALF_HEIGHT;
                let bottom = tf.translation - Vec3::Y * half;
                let top    = tf.translation + Vec3::Y * half;
                let push = resolve_capsule_collision::<V, ()>(bottom, top, MOB_RADIUS, &manager, &chunks);
                tf.translation += push;

                // Ground detection.
                if push.y > 0.01 {
                    mob.on_ground = true;
                    mob.vertical_velocity = 0.0;
                } else {
                    mob.on_ground = false;
                }
            }
            // ── Zero-gravity mode (space / FreeFlight) ────────────────────
            None => {
                // No gravity, no ground-snap. The AI's horizontal velocity
                // drives motion in full 3D. `vertical_velocity` stays at 0
                // and `on_ground` is meaningless here.
                mob.vertical_velocity = 0.0;
                mob.on_ground = false;

                let move_delta = mob.velocity * dt;
                tf.translation += move_delta;

                // Still resolve capsule collision so mobs can't tunnel
                // through chunks, but apply the push directly without
                // ground inference.
                let half = MOB_HALF_HEIGHT;
                let bottom = tf.translation - Vec3::Y * half;
                let top    = tf.translation + Vec3::Y * half;
                let push = resolve_capsule_collision::<V, ()>(bottom, top, MOB_RADIUS, &manager, &chunks);
                tf.translation += push;
            }
        }
    }
}

fn mob_death_system(
    mob_q: Query<(Entity, &Transform, &Mob)>,
    mut commands: Commands,
    mut died_writer: bevy::ecs::message::MessageWriter<MobDied>,
) {
    for (entity, tf, mob) in mob_q.iter() {
        if mob.health <= 0.0 {
            died_writer.write(MobDied { position: tf.translation });
            commands.entity(entity).despawn();
        }
    }
}

fn mob_despawn_system(
    mob_q: Query<(Entity, &Transform, &MobConfigComponent)>,
    player_q: Query<&Transform, With<Player>>,
    mut commands: Commands,
) {
    let Ok(player_tf) = player_q.single() else { return };
    let player_pos = player_tf.translation;
    for (entity, tf, cfg) in mob_q.iter() {
        if tf.translation.distance(player_pos) > cfg.0.despawn_distance {
            commands.entity(entity).despawn();
        }
    }
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Plugin that wires up all mob systems for voxel type `V`.
///
/// Add [`MobPlugin`] after your core voxel plugin. It relies on
/// [`crate::world::ChunkManager`], [`crate::player::Player`], and
/// [`crate::player::PlayerHealth`] being present.
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use voxel_framework::VoxelPlugin;
/// use voxel_framework::mobs::MobPlugin;
/// use voxel_framework::voxel::DefaultTerrainVoxel;
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(VoxelPlugin::<DefaultTerrainVoxel>::new())
///     .add_plugins(MobPlugin::<DefaultTerrainVoxel>::new())
///     .run();
/// ```
pub struct MobPlugin<V: VoxelType> {
    _m: std::marker::PhantomData<V>,
}

impl<V: VoxelType> MobPlugin<V> {
    /// Creates a new [`MobPlugin`] for voxel type `V`.
    pub fn new() -> Self {
        Self { _m: std::marker::PhantomData }
    }
}

impl<V: VoxelType> Default for MobPlugin<V> {
    fn default() -> Self { Self::new() }
}

impl<V: VoxelType> Plugin for MobPlugin<V> {
    fn build(&self, app: &mut App) {
        app.add_message::<MobDied>();
        app.add_systems(
            Update,
            (
                mob_ai_system,
                mob_attack_system,
                mob_physics_system::<V>,
                mob_death_system,
                mob_despawn_system,
            ),
        );
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Pure-Rust mirror of the per-mob branch in [`mob_physics_system`] with
    /// no chunk collision (push = 0). This lets us unit-test the gravity
    /// branching logic without spinning up a full Bevy app, matching the
    /// `src/fragments/mod.rs` test convention of simulating the loop manually.
    fn step(mob: &mut Mob, tf: &mut Transform, gravity: Option<f32>, dt: f32) {
        match gravity {
            Some(g) => {
                if !mob.on_ground {
                    mob.vertical_velocity -= g * dt;
                }
                let move_delta = (mob.velocity + Vec3::Y * mob.vertical_velocity) * dt;
                tf.translation += move_delta;
                // No chunks → push is zero → never grounded in this test.
                mob.on_ground = false;
            }
            None => {
                mob.vertical_velocity = 0.0;
                mob.on_ground = false;
                tf.translation += mob.velocity * dt;
            }
        }
    }

    fn make_mob(velocity: Vec3) -> Mob {
        Mob {
            health: 20.0,
            ai_state: MobAiState::Patrol,
            velocity,
            vertical_velocity: 0.0,
            on_ground: false,
            attack_cooldown: 0.0,
            patrol_timer: 0.0,
            patrol_dir: Vec3::ZERO,
            home_position: Vec3::ZERO,
        }
    }

    #[test]
    fn mob_default_has_gravity_enabled() {
        // Grounded behaviour is preserved as the default — existing games
        // built against MobConfig::default() keep falling under gravity.
        assert_eq!(MobConfig::default().gravity, Some(20.0));
    }

    #[test]
    fn mob_zero_gravity_does_not_fall() {
        // A mob with `gravity: None` and zero velocity must stay put across
        // many ticks — vertical_velocity is forced to 0 every step.
        let mut mob = make_mob(Vec3::ZERO);
        let mut tf = Transform::from_translation(Vec3::new(0.0, 100.0, 0.0));

        for _ in 0..100 {
            step(&mut mob, &mut tf, None, 0.1);
        }

        assert_eq!(tf.translation.y, 100.0, "zero-g mob must not fall");
        assert_eq!(mob.vertical_velocity, 0.0);
        assert!(!mob.on_ground);
    }

    #[test]
    fn mob_zero_gravity_drifts_with_velocity() {
        // Horizontal velocity carries through; Y is untouched.
        let mut mob = make_mob(Vec3::new(2.0, 0.0, 0.0));
        let mut tf = Transform::from_translation(Vec3::new(0.0, 100.0, 0.0));

        // 10 ticks × 0.1 s × 2 m/s = 2.0 m drift.
        for _ in 0..10 {
            step(&mut mob, &mut tf, None, 0.1);
        }

        assert!((tf.translation.x - 2.0).abs() < 1e-4, "x drift = {}", tf.translation.x);
        assert_eq!(tf.translation.y, 100.0, "y must not change in zero-g");
    }

    #[test]
    fn mob_with_gravity_falls() {
        // Sanity check the Some(g) branch: gravity accumulates and the mob
        // falls when no ground push is applied.
        let mut mob = make_mob(Vec3::ZERO);
        let mut tf = Transform::from_translation(Vec3::new(0.0, 100.0, 0.0));

        for _ in 0..10 {
            step(&mut mob, &mut tf, Some(20.0), 0.1);
        }

        assert!(tf.translation.y < 100.0, "grounded mob must fall (got y={})", tf.translation.y);
        assert!(mob.vertical_velocity < 0.0, "vertical velocity must accumulate downward");
    }
}
