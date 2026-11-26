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

use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::chunk::{Chunk, world_to_chunk_coord};
use crate::player::{Player, PlayerHealth};
use crate::voxel::VoxelType;
use crate::world::ChunkManager;
use crate::world::collision::resolve_capsule_collision;

mod spawner;

pub use spawner::{
    DAY_END, DAY_START, DespawnOutsideWindow, MobSpawnRules, MobSpawnerPlugin, SpawnContext,
    SpawnPlacement, SpawnRing, SpawnRule, SpawnZone, TimeWindow,
};

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
    /// alongside a [`crate::player::PlayerMode::FreeFlightSixDof`] player.
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
    commands
        .spawn((
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
        ))
        .id()
}

/// Internal component that stores the [`MobConfig`] on the entity so systems
/// can read it without a separate resource lookup.
#[derive(Component)]
pub struct MobConfigComponent(pub MobConfig);

// ─── Mob kinds ────────────────────────────────────────────────────────────────

/// Named mob type, such as `"slime"` or `"night_slime"`.
///
/// Mobs spawned through [`spawn_mob_kind`] carry this component so games and
/// spawners can count or filter mobs by type. Mobs spawned with the plain
/// [`spawn_mob`] have no kind.
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MobKind(String);

impl MobKind {
    /// Creates a kind from its name.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Returns the kind's name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for MobKind {
    fn from(name: &str) -> Self {
        Self::new(name)
    }
}

impl From<String> for MobKind {
    fn from(name: String) -> Self {
        Self::new(name)
    }
}

impl AsRef<str> for MobKind {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MobKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Error returned by [`MobRegistry::register_variant`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MobRegistryError {
    /// The base kind a variant derives from is not registered.
    UnknownBase(MobKind),
}

impl std::fmt::Display for MobRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownBase(kind) => write!(f, "unknown base mob kind `{kind}`"),
        }
    }
}

impl std::error::Error for MobRegistryError {}

/// Registry of named mob types, each backed by a [`MobConfig`].
///
/// Every mob type is a configuration of the same base mob (the colored box
/// with patrol/chase/attack AI). Use [`MobRegistry::register_variant`] to
/// derive a new type from an existing one and change only what differs.
///
/// [`MobPlugin`] initializes an empty registry; games fill it at startup.
///
/// ```
/// use bevy::prelude::Color;
/// use voxel_framework::mobs::{MobConfig, MobKind, MobRegistry};
///
/// let mut registry = MobRegistry::default();
/// registry.register("slime", MobConfig::default());
/// registry
///     .register_variant("night_slime", "slime", |config| {
///         config.chase_speed = 5.0;
///         config.color = Color::srgb(0.3, 0.1, 0.6);
///     })
///     .unwrap();
///
/// assert_eq!(registry.get(&MobKind::new("night_slime")).unwrap().chase_speed, 5.0);
/// ```
#[derive(Resource, Clone, Debug, Default)]
pub struct MobRegistry {
    kinds: BTreeMap<MobKind, MobConfig>,
}

impl MobRegistry {
    /// Registers (or replaces) a mob type.
    pub fn register(&mut self, kind: impl Into<MobKind>, config: MobConfig) -> &mut Self {
        self.kinds.insert(kind.into(), config);
        self
    }

    /// Registers `kind` as a copy of `base`'s config modified by `customize`.
    ///
    /// Later changes to `base` do not affect the variant.
    pub fn register_variant(
        &mut self,
        kind: impl Into<MobKind>,
        base: impl Into<MobKind>,
        customize: impl FnOnce(&mut MobConfig),
    ) -> Result<&mut Self, MobRegistryError> {
        let base = base.into();
        let mut config = self
            .kinds
            .get(&base)
            .cloned()
            .ok_or(MobRegistryError::UnknownBase(base))?;
        customize(&mut config);
        Ok(self.register(kind, config))
    }

    /// Returns the config for `kind`, if registered.
    pub fn get(&self, kind: &MobKind) -> Option<&MobConfig> {
        self.kinds.get(kind)
    }

    /// Returns `true` if `kind` is registered.
    pub fn contains(&self, kind: &MobKind) -> bool {
        self.kinds.contains_key(kind)
    }

    /// Iterates registered kinds in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&MobKind, &MobConfig)> {
        self.kinds.iter()
    }

    /// Number of registered kinds.
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    /// Returns `true` when no kinds are registered.
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }
}

/// Spawns a mob of a registered `kind` at `position`.
///
/// Works like [`spawn_mob`] with a copy of the kind's current [`MobConfig`]
/// and also inserts the [`MobKind`] component; later registry changes do not
/// affect mobs already spawned. Returns `None` when `kind` is not registered.
pub fn spawn_mob_kind(
    kind: &MobKind,
    registry: &MobRegistry,
    position: Vec3,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Option<Entity> {
    let config = registry.get(kind)?.clone();
    let entity = spawn_mob(position, config, commands, meshes, materials);
    commands.entity(entity).insert(kind.clone());
    Some(entity)
}

// ─── Systems ──────────────────────────────────────────────────────────────────

fn mob_ai_system(
    player_q: Query<&Transform, With<Player>>,
    mut mob_q: Query<(&Transform, &mut Mob, &MobConfigComponent)>,
    time: Res<Time>,
) {
    let Ok(player_tf) = player_q.single() else {
        return;
    };
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
    let Ok(player_tf) = player_q.single() else {
        return;
    };
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

/// Downward distance probed below a mob's capsule to decide whether it is
/// standing on something, and the farthest it is snapped down onto the floor.
/// Small enough that mobs still fall off ledges instead of floating.
const GROUND_PROBE: f32 = 0.05;
/// Minimum vertical push-out treated as contact (filters float noise).
const CONTACT_EPSILON: f32 = 1e-4;
/// Longest distance a mob moves between two collision passes. Kept below
/// [`MOB_RADIUS`] so the capsule spine does not end a step inside a voxel,
/// where the resolver can only guess which face to push out of. Holds for up
/// to `MAX_STEP * MAX_SUBSTEPS` (11.2) units of travel per tick.
const MAX_STEP: f32 = MOB_RADIUS * 0.5;
/// Upper bound on collision passes per tick (a long frame hitch at full speed).
const MAX_SUBSTEPS: usize = 64;
/// Fastest a mob falls (m/s); bounds the substep count together with
/// [`MAX_SUBSTEPS`].
const TERMINAL_FALL_SPEED: f32 = 40.0;

/// Number of equal substeps needed so `delta` moves at most [`MAX_STEP`] each.
fn substep_count(delta: Vec3) -> usize {
    let steps = (delta.abs().max_element() / MAX_STEP).ceil();
    // `as usize` saturates and maps NaN to 0, so the clamp covers both.
    (steps as usize).clamp(1, MAX_SUBSTEPS)
}

/// Advances one physics tick for a single mob and returns its new position.
///
/// `resolve` maps a capsule spine `(bottom, top)` to the corrected `bottom`
/// position, exactly like [`resolve_capsule_collision`]. Keeping the world
/// lookup behind a closure lets the grounded/zero-g logic be unit-tested
/// without chunks.
///
/// Motion is split into substeps of at most [`MAX_STEP`]. In grounded mode
/// each substep resolves horizontal motion first, keeping only the horizontal
/// part of that push, and vertical motion second, keeping only the vertical
/// part: a wall then never reads as a floor, even though the seam between two
/// adjacent voxels pushes diagonally. A short downward probe at the end makes a resting mob
/// report `on_ground == true` on every frame.
fn mob_physics_step(
    mob: &mut Mob,
    mut pos: Vec3,
    gravity: Option<f32>,
    dt: f32,
    mut resolve: impl FnMut(Vec3, Vec3) -> Vec3,
) -> Vec3 {
    // A degenerate time step moves nothing and must not corrupt the mob.
    if !dt.is_finite() || dt <= 0.0 {
        return pos;
    }
    let half = Vec3::Y * MOB_HALF_HEIGHT;
    let mut push_out = |center: Vec3| resolve(center - half, center + half) - (center - half);

    match gravity {
        // ── Grounded mode (Minecraft-style) ───────────────────────────────
        Some(g) => {
            if !mob.on_ground || mob.vertical_velocity > 0.0 {
                mob.vertical_velocity = (mob.vertical_velocity - g * dt).max(-TERMINAL_FALL_SPEED);
            } else {
                mob.vertical_velocity = 0.0;
            }

            let delta = (mob.velocity + Vec3::Y * mob.vertical_velocity) * dt;
            let steps = substep_count(delta);
            let mut step = delta / steps as f32;

            for _ in 0..steps {
                // Horizontal pass: walls stop the mob but never lift it.
                pos.x += step.x;
                pos.z += step.z;
                let push = push_out(pos);
                pos.x += push.x;
                pos.z += push.z;

                // Vertical pass: floors and ceilings stop the mob but never
                // shove it sideways. Landing or bumping a ceiling cancels
                // vertical speed for the rest of the tick.
                pos.y += step.y;
                let push = push_out(pos);
                pos.y += push.y;
                if (push.y > CONTACT_EPSILON && step.y < 0.0)
                    || (push.y < -CONTACT_EPSILON && step.y > 0.0)
                {
                    mob.vertical_velocity = 0.0;
                    step.y = 0.0;
                }
            }

            // Ground probe: is there solid ground just below the feet?
            mob.on_ground = false;
            if mob.vertical_velocity <= 0.0 {
                let probe = pos - Vec3::Y * GROUND_PROBE;
                let probe_push = push_out(probe);
                if probe_push.y > CONTACT_EPSILON {
                    mob.on_ground = true;
                    mob.vertical_velocity = 0.0;
                    // Snap down onto the contact surface.
                    pos.y = probe.y + probe_push.y;
                }
            }
        }
        // ── Zero-gravity mode (space / FreeFlight) ────────────────────────
        None => {
            // No gravity, no ground-snap. The AI velocity drives motion in
            // full 3D. `vertical_velocity` stays at 0 and `on_ground` is
            // meaningless here. Collision still runs so mobs cannot tunnel.
            mob.vertical_velocity = 0.0;
            mob.on_ground = false;
            let delta = mob.velocity * dt;
            let steps = substep_count(delta);
            let step = delta / steps as f32;
            for _ in 0..steps {
                pos += step;
                pos += push_out(pos);
            }
        }
    }
    pos
}

/// Returns `true` when the chunks holding the ground under a mob are loaded.
///
/// Checks the chunk just below the capsule's feet and the one `fall` units
/// further down (how far the mob can drop this tick). The centre's chunk is
/// not enough: a mob standing on a floor whose top face lies on a chunk
/// boundary sits in the chunk above the one that holds the floor.
fn mob_ground_loaded(pos: Vec3, fall: f32, is_loaded: impl Fn(IVec3) -> bool) -> bool {
    let feet = pos - Vec3::Y * (MOB_HALF_HEIGHT + MOB_RADIUS + GROUND_PROBE);
    is_loaded(world_to_chunk_coord(feet))
        && is_loaded(world_to_chunk_coord(feet - Vec3::Y * fall.max(0.0)))
}

fn mob_physics_system<V: VoxelType>(
    mut mob_q: Query<(&mut Transform, &mut Mob, &MobConfigComponent)>,
    manager: Res<ChunkManager>,
    chunks: Query<&Chunk<V>>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (mut tf, mut mob, cfg) in mob_q.iter_mut() {
        // Freeze mobs with gravity while the ground under them is not loaded
        // yet so they don't fall through terrain that is still streaming in.
        // Same rule as the player, whose gravity waits for the feet chunk.
        if let Some(g) = cfg.0.gravity {
            let fall = (g * dt - mob.vertical_velocity).clamp(0.0, TERMINAL_FALL_SPEED) * dt;
            if !mob_ground_loaded(tf.translation, fall, |coord| manager.is_loaded(coord)) {
                continue;
            }
        }
        tf.translation = mob_physics_step(
            &mut mob,
            tf.translation,
            cfg.0.gravity,
            dt,
            |bottom, top| {
                resolve_capsule_collision::<V, ()>(bottom, top, MOB_RADIUS, &manager, &chunks)
            },
        );
    }
}

fn mob_death_system(
    mob_q: Query<(Entity, &Transform, &Mob)>,
    mut commands: Commands,
    mut died_writer: bevy::ecs::message::MessageWriter<MobDied>,
) {
    for (entity, tf, mob) in mob_q.iter() {
        if mob.health <= 0.0 {
            died_writer.write(MobDied {
                position: tf.translation,
            });
            commands.entity(entity).despawn();
        }
    }
}

/// Despawns mobs farther than their `despawn_distance` from every player.
fn mob_despawn_system(
    mob_q: Query<(Entity, &Transform, &MobConfigComponent)>,
    player_q: Query<&Transform, With<Player>>,
    mut commands: Commands,
) {
    // Without players there is nothing to measure against; keep the mobs.
    if player_q.is_empty() {
        return;
    }
    for (entity, tf, cfg) in mob_q.iter() {
        let nearest = player_q
            .iter()
            .map(|player| player.translation.distance(tf.translation))
            .fold(f32::INFINITY, f32::min);
        if nearest > cfg.0.despawn_distance {
            // Another system may despawn the same mob this frame.
            commands.entity(entity).try_despawn();
        }
    }
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Plugin that wires up all mob systems for voxel type `V`.
///
/// Initializes an empty [`MobRegistry`]. Add [`MobPlugin`] after your core
/// voxel plugin. It relies on
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
        Self {
            _m: std::marker::PhantomData,
        }
    }
}

impl<V: VoxelType> Default for MobPlugin<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: VoxelType> Plugin for MobPlugin<V> {
    fn build(&self, app: &mut App) {
        app.add_message::<MobDied>();
        app.init_resource::<MobRegistry>();
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
    use crate::test_support::{filled_chunk, world_with_chunks};
    use crate::voxel::DefaultVoxel;
    use bevy::ecs::system::{RunSystemOnce, SystemState};

    const DT: f32 = 1.0 / 60.0;

    /// Collision stand-in for a flat floor whose top surface is at `y = 0`
    /// (everything below is solid). Returns the corrected capsule `bottom`
    /// like [`resolve_capsule_collision`].
    fn flat_floor(bottom: Vec3, _top: Vec3) -> Vec3 {
        Vec3::new(bottom.x, bottom.y.max(MOB_RADIUS), bottom.z)
    }

    /// No collision at all (empty space).
    fn empty(bottom: Vec3, _top: Vec3) -> Vec3 {
        bottom
    }

    /// Height of a mob's centre when resting on [`flat_floor`].
    const REST_Y: f32 = MOB_RADIUS + MOB_HALF_HEIGHT;

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
        let mut mob = make_mob(Vec3::ZERO);
        let mut pos = Vec3::new(0.0, 100.0, 0.0);
        for _ in 0..100 {
            pos = mob_physics_step(&mut mob, pos, None, 0.1, empty);
        }
        assert_eq!(pos.y, 100.0, "zero-g mob must not fall");
        assert_eq!(mob.vertical_velocity, 0.0);
        assert!(!mob.on_ground);
    }

    #[test]
    fn mob_zero_gravity_drifts_with_velocity() {
        let mut mob = make_mob(Vec3::new(2.0, 0.0, 0.0));
        let mut pos = Vec3::new(0.0, 100.0, 0.0);
        // 10 ticks × 0.1 s × 2 m/s = 2.0 m drift.
        for _ in 0..10 {
            pos = mob_physics_step(&mut mob, pos, None, 0.1, empty);
        }
        assert!((pos.x - 2.0).abs() < 1e-4, "x drift = {}", pos.x);
        assert_eq!(pos.y, 100.0, "y must not change in zero-g");
    }

    #[test]
    fn mob_with_gravity_falls_in_empty_space() {
        let mut mob = make_mob(Vec3::ZERO);
        let mut pos = Vec3::new(0.0, 100.0, 0.0);
        for _ in 0..10 {
            pos = mob_physics_step(&mut mob, pos, Some(20.0), 0.1, empty);
        }
        assert!(pos.y < 100.0, "grounded mob must fall (got y={})", pos.y);
        assert!(mob.vertical_velocity < 0.0);
        assert!(!mob.on_ground);
    }

    #[test]
    fn collision_moves_mob_by_push_out_not_by_absolute_position() {
        // Regression: the physics system used to add the *resolved bottom
        // position* to the translation, teleporting mobs far away. A mob
        // resting on the floor at a large coordinate must stay put.
        let mut mob = make_mob(Vec3::ZERO);
        let start = Vec3::new(500.0, REST_Y, -300.0);
        let pos = mob_physics_step(&mut mob, start, Some(20.0), DT, flat_floor);
        assert!(pos.distance(start) < 0.01, "mob moved to {pos}");
    }

    #[test]
    fn standing_mob_stays_grounded_every_frame() {
        // Regression: a resting mob used to flip `on_ground` every frame.
        for dt in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 144.0, 1.0 / 1000.0] {
            let mut mob = make_mob(Vec3::ZERO);
            let mut pos = Vec3::new(0.0, 3.0, 0.0);
            // Fall and land.
            for _ in 0..((2.0 / dt) as usize) {
                pos = mob_physics_step(&mut mob, pos, Some(20.0), dt, flat_floor);
            }
            for frame in 0..200 {
                pos = mob_physics_step(&mut mob, pos, Some(20.0), dt, flat_floor);
                assert!(mob.on_ground, "dt={dt}: lost ground on frame {frame}");
                assert_eq!(mob.vertical_velocity, 0.0);
                assert!((pos.y - REST_Y).abs() < 1e-3, "dt={dt}: y={}", pos.y);
            }
        }
    }

    #[test]
    fn walking_mob_stays_grounded_on_flat_floor() {
        let mut mob = make_mob(Vec3::new(1.2, 0.0, 0.0));
        mob.on_ground = true;
        let mut pos = Vec3::new(0.0, REST_Y, 0.0);
        for _ in 0..120 {
            pos = mob_physics_step(&mut mob, pos, Some(20.0), DT, flat_floor);
            assert!(mob.on_ground);
        }
        assert!((pos.x - 2.4).abs() < 1e-3, "x={}", pos.x);
    }

    #[test]
    fn mob_walks_off_a_ledge_and_falls() {
        // Floor only where x < 0.
        let ledge = |bottom: Vec3, _top: Vec3| {
            if bottom.x < 0.0 {
                flat_floor(bottom, _top)
            } else {
                bottom
            }
        };
        let mut mob = make_mob(Vec3::new(2.0, 0.0, 0.0));
        mob.on_ground = true;
        let mut pos = Vec3::new(-1.0, REST_Y, 0.0);
        for _ in 0..90 {
            pos = mob_physics_step(&mut mob, pos, Some(20.0), DT, ledge);
        }
        assert!(!mob.on_ground);
        assert!(
            pos.y < REST_Y - 0.5,
            "mob must fall past the ledge, y={}",
            pos.y
        );
    }

    // ── Real voxel collision ──────────────────────────────────────────────
    //
    // The closure stand-ins above cannot reproduce per-voxel artifacts of
    // `resolve_capsule_collision` (seams between stacked voxels, the
    // inside-AABB branch), so these tests run against real chunks.

    /// Top surface of the solid chunk used by [`floor_world`].
    const FLOOR_TOP: f32 = 16.0;

    /// Solid chunk at the origin with a loaded, empty chunk above it.
    fn floor_world() -> World {
        world_with_chunks([
            (IVec3::ZERO, filled_chunk(DefaultVoxel::Rock)),
            (IVec3::Y, Chunk::<DefaultVoxel>::new()),
        ])
    }

    /// Runs `f` with a resolver backed by the chunks in `world`.
    fn with_real_collision<R>(
        world: &mut World,
        f: impl FnOnce(&mut dyn FnMut(Vec3, Vec3) -> Vec3) -> R,
    ) -> R {
        let mut state: SystemState<(Res<ChunkManager>, Query<&Chunk<DefaultVoxel>>)> =
            SystemState::new(world);
        let (manager, chunks) = state.get(world);
        let mut resolve = |bottom: Vec3, top: Vec3| {
            resolve_capsule_collision::<DefaultVoxel, ()>(
                bottom, top, MOB_RADIUS, &manager, &chunks,
            )
        };
        f(&mut resolve)
    }

    #[test]
    fn falling_mob_pressed_against_a_wall_keeps_falling() {
        // Regression: the seam between two stacked wall voxels returns a
        // diagonal push with a small +y component. Treating it as a landing
        // left a chasing mob hanging on the wall in mid-air.
        let mut wall = Chunk::<DefaultVoxel>::new();
        for x in 8..16 {
            for y in 0..16 {
                for z in 0..16 {
                    wall.set(x, y, z, DefaultVoxel::Rock);
                }
            }
        }
        let mut world = world_with_chunks([(IVec3::ZERO, wall)]);
        with_real_collision(&mut world, |resolve| {
            let mut mob = make_mob(Vec3::new(3.5, 0.0, 0.0));
            let start = Vec3::new(8.0 - MOB_RADIUS, 12.0, 8.5);
            let mut pos = start;
            // 0.5 s of free fall is 2.5 units.
            for _ in 0..30 {
                pos = mob_physics_step(&mut mob, pos, Some(20.0), DT, &mut *resolve);
                assert!(!mob.on_ground, "a wall is not ground (y={})", pos.y);
            }
            assert!(pos.y < start.y - 2.0, "mob hung on the wall at y={}", pos.y);
            assert!(pos.x <= start.x + 1e-3, "mob entered the wall, x={}", pos.x);
        });
    }

    #[test]
    fn fast_falling_mob_lands_on_top_of_the_floor() {
        // Regression: one big step could put the capsule spine inside a floor
        // voxel, where the resolver pushes along the nearest face (often
        // sideways) and the mob kept sinking.
        let mut world = floor_world();
        with_real_collision(&mut world, |resolve| {
            for x in [8.02, 8.5, 8.98] {
                for fall_speed in [15.0, 30.0, TERMINAL_FALL_SPEED] {
                    let mut mob = make_mob(Vec3::ZERO);
                    mob.vertical_velocity = -fall_speed;
                    let mut pos = Vec3::new(x, FLOOR_TOP + REST_Y + 0.5, 8.5);
                    for _ in 0..30 {
                        pos =
                            mob_physics_step(&mut mob, pos, Some(20.0), 1.0 / 30.0, &mut *resolve);
                    }
                    assert!(
                        mob.on_ground,
                        "x={x} v={fall_speed}: not grounded, y={}",
                        pos.y
                    );
                    assert!(
                        (pos.y - (FLOOR_TOP + REST_Y)).abs() < 1e-2,
                        "x={x} v={fall_speed}: rests at y={}",
                        pos.y
                    );
                    assert!(
                        (pos.x - x).abs() < 1e-2,
                        "x={x} v={fall_speed}: pushed to {}",
                        pos.x
                    );
                }
            }
        });
    }

    #[test]
    fn mob_bumping_a_ceiling_loses_upward_speed() {
        // Solid everywhere above y = 2 (capsule top sphere stops at 2).
        let ceiling = |bottom: Vec3, top: Vec3| {
            let limit = 2.0 - MOB_RADIUS;
            bottom - Vec3::Y * (top.y - limit).max(0.0)
        };
        let mut mob = make_mob(Vec3::ZERO);
        mob.vertical_velocity = 8.0;
        let mut pos = Vec3::new(0.0, 1.0, 0.0);
        for _ in 0..10 {
            pos = mob_physics_step(&mut mob, pos, Some(20.0), DT, ceiling);
            assert!(pos.y + MOB_HALF_HEIGHT + MOB_RADIUS <= 2.0 + 1e-4);
        }
        assert!(mob.vertical_velocity <= 0.0, "v={}", mob.vertical_velocity);
        assert!(!mob.on_ground);
    }

    #[test]
    fn upward_speed_lifts_a_grounded_mob() {
        // A game that sets `vertical_velocity` on a grounded mob (a jump) must
        // not have it cancelled by the ground probe.
        let mut mob = make_mob(Vec3::ZERO);
        mob.on_ground = true;
        mob.vertical_velocity = 6.0;
        let pos = mob_physics_step(
            &mut mob,
            Vec3::new(0.0, REST_Y, 0.0),
            Some(20.0),
            DT,
            flat_floor,
        );
        assert!(pos.y > REST_Y + 0.05, "y={}", pos.y);
        assert!(!mob.on_ground);
        assert!(mob.vertical_velocity > 0.0);
    }

    #[test]
    fn degenerate_time_steps_leave_the_mob_untouched() {
        let start = Vec3::new(0.0, 5.0, 0.0);
        for dt in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let mut mob = make_mob(Vec3::new(1.0, 0.0, 0.0));
            let pos = mob_physics_step(&mut mob, start, Some(20.0), dt, empty);
            assert_eq!(pos, start, "dt={dt}");
            assert_eq!(mob.vertical_velocity, 0.0, "dt={dt}");
        }
        assert_eq!(substep_count(Vec3::splat(f32::NAN)), 1);
        assert_eq!(substep_count(Vec3::splat(f32::INFINITY)), MAX_SUBSTEPS);
        assert_eq!(substep_count(Vec3::ZERO), 1);
    }

    #[test]
    fn falling_speed_is_capped() {
        let mut mob = make_mob(Vec3::ZERO);
        let mut pos = Vec3::new(0.0, 1000.0, 0.0);
        for _ in 0..600 {
            pos = mob_physics_step(&mut mob, pos, Some(20.0), DT, empty);
        }
        assert_eq!(mob.vertical_velocity, -TERMINAL_FALL_SPEED);
    }

    #[test]
    fn ground_is_loaded_only_when_the_chunk_under_the_feet_is() {
        // Mob resting on a floor whose top face is the chunk boundary y = 16:
        // its centre is in chunk (0, 1, 0), the floor in chunk (0, 0, 0).
        let pos = Vec3::new(8.0, FLOOR_TOP + REST_Y, 8.0);
        let air = IVec3::Y;
        let floor = IVec3::ZERO;
        assert!(mob_ground_loaded(pos, 0.0, |c| c == floor));
        assert!(!mob_ground_loaded(pos, 0.0, |c| c == air));
        // A mob falling toward the boundary needs the chunk it will reach.
        let above = pos + Vec3::Y * 0.5;
        assert!(mob_ground_loaded(above, 0.0, |c| c == air));
        assert!(!mob_ground_loaded(above, 0.6, |c| c == air));
        assert!(mob_ground_loaded(above, 0.6, |c| c == air || c == floor));
    }

    /// Spawns a bare mob (no mesh) and returns its entity.
    fn spawn_bare_mob(world: &mut World, pos: Vec3, config: MobConfig) -> Entity {
        world
            .spawn((
                Transform::from_translation(pos),
                make_mob(Vec3::ZERO),
                MobConfigComponent(config),
            ))
            .id()
    }

    fn run_physics_frames(world: &mut World, frames: usize) {
        for _ in 0..frames {
            world
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(DT));
            world
                .run_system_once(mob_physics_system::<DefaultVoxel>)
                .unwrap();
        }
    }

    #[test]
    fn physics_system_rests_mob_on_real_floor_far_from_origin() {
        // End-to-end regression for the position-vs-offset bug: the system
        // itself, the real resolver, and a chunk away from the origin.
        let base = IVec3::new(20, 3, -15);
        let mut world = world_with_chunks([
            (base, filled_chunk(DefaultVoxel::Rock)),
            (base + IVec3::Y, Chunk::<DefaultVoxel>::new()),
        ]);
        world.insert_resource(Time::<()>::default());
        let top = (base.y + 1) as f32 * 16.0;
        let start = Vec3::new(
            base.x as f32 * 16.0 + 8.5,
            top + REST_Y + 1.0,
            base.z as f32 * 16.0 + 8.5,
        );
        let entity = spawn_bare_mob(&mut world, start, MobConfig::default());

        run_physics_frames(&mut world, 120);

        let pos = world.get::<Transform>(entity).unwrap().translation;
        assert!((pos.y - (top + REST_Y)).abs() < 1e-2, "y={}", pos.y);
        assert!(
            (pos.x - start.x).abs() < 1e-3 && (pos.z - start.z).abs() < 1e-3,
            "pos={pos}"
        );
        assert!(world.get::<Mob>(entity).unwrap().on_ground);
    }

    #[test]
    fn physics_system_freezes_mob_until_its_floor_chunk_loads() {
        // Only the air chunk is loaded; the floor chunk below is not.
        let mut world = world_with_chunks([(IVec3::Y, Chunk::<DefaultVoxel>::new())]);
        world.insert_resource(Time::<()>::default());
        let start = Vec3::new(8.5, FLOOR_TOP + REST_Y, 8.5);
        let entity = spawn_bare_mob(&mut world, start, MobConfig::default());

        run_physics_frames(&mut world, 30);
        assert_eq!(world.get::<Transform>(entity).unwrap().translation, start);

        // Once the floor arrives the mob stands on it.
        let floor = world.spawn(filled_chunk(DefaultVoxel::Rock)).id();
        world
            .resource_mut::<ChunkManager>()
            .register(IVec3::ZERO, floor);
        run_physics_frames(&mut world, 30);
        let pos = world.get::<Transform>(entity).unwrap().translation;
        assert!((pos.y - start.y).abs() < 1e-2, "y={}", pos.y);
        assert!(world.get::<Mob>(entity).unwrap().on_ground);
    }

    #[test]
    fn physics_system_stops_a_falling_mob_above_an_unloaded_floor_chunk() {
        // The mob must freeze before entering the unloaded chunk, not one
        // tick of fall inside it (where the floor will appear around it).
        let mut world = world_with_chunks([(IVec3::Y, Chunk::<DefaultVoxel>::new())]);
        world.insert_resource(Time::<()>::default());
        let entity = spawn_bare_mob(
            &mut world,
            Vec3::new(8.2, FLOOR_TOP + 6.0, 8.5),
            MobConfig::default(),
        );
        world.get_mut::<Mob>(entity).unwrap().vertical_velocity = -25.0;

        run_physics_frames(&mut world, 60);

        let pos = world.get::<Transform>(entity).unwrap().translation;
        assert!(pos.y >= FLOOR_TOP + REST_Y, "sank to y={}", pos.y);
    }

    #[test]
    fn physics_system_moves_mobs_on_a_world_with_no_chunk_above() {
        // A finite world: one solid chunk and nothing registered above it.
        let mut world = world_with_chunks([(IVec3::ZERO, filled_chunk(DefaultVoxel::Rock))]);
        world.insert_resource(Time::<()>::default());
        let entity = spawn_bare_mob(
            &mut world,
            Vec3::new(8.5, FLOOR_TOP + REST_Y + 0.02, 8.5),
            MobConfig::default(),
        );
        world.get_mut::<Mob>(entity).unwrap().velocity = Vec3::new(1.0, 0.0, 0.0);

        run_physics_frames(&mut world, 60);

        let pos = world.get::<Transform>(entity).unwrap().translation;
        assert!((pos.x - 9.5).abs() < 1e-2, "x={}", pos.x);
        assert!(world.get::<Mob>(entity).unwrap().on_ground);
    }

    #[test]
    fn physics_system_lets_zero_gravity_mobs_drift_without_chunks() {
        let mut world = world_with_chunks::<DefaultVoxel>([]);
        world.insert_resource(Time::<()>::default());
        let config = MobConfig {
            gravity: None,
            ..Default::default()
        };
        let entity = spawn_bare_mob(&mut world, Vec3::ZERO, config);
        world.get_mut::<Mob>(entity).unwrap().velocity = Vec3::new(0.0, 2.0, 0.0);

        run_physics_frames(&mut world, 60);

        let pos = world.get::<Transform>(entity).unwrap().translation;
        assert!((pos.y - 2.0).abs() < 1e-2, "y={}", pos.y);
    }

    #[test]
    fn registry_variant_derives_from_base() {
        let mut registry = MobRegistry::default();
        registry.register("slime", MobConfig::default());
        registry
            .register_variant("night_slime", "slime", |c| c.chase_speed = 6.0)
            .unwrap();

        let base = registry.get(&"slime".into()).unwrap();
        let variant = registry.get(&"night_slime".into()).unwrap();
        assert_eq!(variant.chase_speed, 6.0);
        assert_eq!(variant.max_health, base.max_health);
        assert_eq!(base.chase_speed, MobConfig::default().chase_speed);
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn registry_iterates_in_name_order_and_replaces_on_reregister() {
        let mut registry = MobRegistry::default();
        for name in ["slime", "bat", "zombie"] {
            registry.register(name, MobConfig::default());
        }
        let names: Vec<&str> = registry.iter().map(|(kind, _)| kind.as_str()).collect();
        assert_eq!(names, ["bat", "slime", "zombie"]);

        registry.register(
            "slime",
            MobConfig {
                chase_speed: 9.0,
                ..Default::default()
            },
        );
        assert_eq!(registry.len(), 3);
        assert_eq!(registry.get(&"slime".into()).unwrap().chase_speed, 9.0);
    }

    #[test]
    fn mob_plugin_initializes_an_empty_registry() {
        let mut app = App::new();
        app.add_plugins(MobPlugin::<DefaultVoxel>::new());
        assert!(app.world().resource::<MobRegistry>().is_empty());
    }

    #[test]
    fn mobs_despawn_by_distance_to_the_nearest_player() {
        let mut world = World::new();
        let config = MobConfig {
            despawn_distance: 10.0,
            ..Default::default()
        };
        let near_a = spawn_bare_mob(&mut world, Vec3::new(5.0, 0.0, 0.0), config.clone());
        let near_b = spawn_bare_mob(&mut world, Vec3::new(105.0, 0.0, 0.0), config.clone());
        let stranded = spawn_bare_mob(&mut world, Vec3::new(50.0, 0.0, 0.0), config);

        // No players: nothing is despawned.
        world.run_system_once(mob_despawn_system).unwrap();
        assert!(world.get_entity(stranded).is_ok());

        world.spawn((Player::default(), Transform::from_xyz(0.0, 0.0, 0.0)));
        world.spawn((Player::default(), Transform::from_xyz(100.0, 0.0, 0.0)));
        world.run_system_once(mob_despawn_system).unwrap();

        assert!(world.get_entity(near_a).is_ok());
        assert!(world.get_entity(near_b).is_ok());
        assert!(world.get_entity(stranded).is_err());
    }

    #[test]
    fn registry_variant_of_unknown_base_fails() {
        let mut registry = MobRegistry::default();
        let err = registry
            .register_variant("ghost", "slime", |_| {})
            .unwrap_err();
        assert_eq!(err, MobRegistryError::UnknownBase("slime".into()));
        assert!(!registry.contains(&"ghost".into()));
    }

    #[test]
    fn spawn_mob_kind_inserts_kind_component() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>();
        let mut registry = MobRegistry::default();
        registry.register("slime", MobConfig::default());
        app.insert_resource(registry);

        let spawned = app
            .world_mut()
            .run_system_once(
                |mut commands: Commands,
                 registry: Res<MobRegistry>,
                 mut meshes: ResMut<Assets<Mesh>>,
                 mut materials: ResMut<Assets<StandardMaterial>>| {
                    let slime = spawn_mob_kind(
                        &"slime".into(),
                        &registry,
                        Vec3::ZERO,
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                    );
                    let unknown = spawn_mob_kind(
                        &"ghost".into(),
                        &registry,
                        Vec3::ZERO,
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                    );
                    (slime, unknown)
                },
            )
            .unwrap();

        assert!(spawned.1.is_none());
        let entity = spawned.0.expect("registered kind spawns");
        let kind = app.world().get::<MobKind>(entity).unwrap();
        assert_eq!(kind.as_str(), "slime");
    }
}
