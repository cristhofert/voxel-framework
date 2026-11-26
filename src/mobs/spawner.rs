//! Rule-driven mob spawning.
//!
//! [`MobSpawnerPlugin`] periodically evaluates the rules in [`MobSpawnRules`]
//! and spawns registered mob kinds around every [`Player`], depending on the
//! time of day, the terrain at the candidate position and an optional zone
//! predicate.
//!
//!
//! See [`MobSpawnerPlugin`] for a complete example.

use std::marker::PhantomData;
use std::sync::Arc;

use bevy::prelude::*;

use super::{MOB_HALF_HEIGHT, MOB_RADIUS, MobKind, MobRegistry, spawn_mob_kind};
use crate::chunk::Chunk;
use crate::noise::hash3;
use crate::player::Player;
use crate::voxel::VoxelType;
use crate::world::ChunkManager;
use crate::world::daycycle::DayCycleConfig;
use crate::world::voxel_ops::get_voxel_at;

/// `time_of_day` at which [`TimeWindow::Day`] starts (dawn).
pub const DAY_START: f32 = 0.25;
/// `time_of_day` at which [`TimeWindow::Day`] ends (dusk).
pub const DAY_END: f32 = 0.75;

/// Voxels searched above and below the player's height for a ground surface.
const GROUND_SEARCH_RANGE: i32 = 16;
/// Gap left between the floor and a freshly spawned mob's capsule.
const SPAWN_CLEARANCE: f32 = 0.01;
/// Fraction of a kind's `despawn_distance` that spawn rings are capped to, so
/// a mob is not despawned by distance right after spawning.
const DESPAWN_MARGIN: f32 = 0.9;
/// Narrowest ring a ground rule can use. Ground spawns snap to voxel column
/// centres (up to ~0.71 away), so a thinner ring may contain no centre at all.
const MIN_GROUND_RING_WIDTH: f32 = 1.5;

// ─── Rules ────────────────────────────────────────────────────────────────────

/// Part of the day in which a [`SpawnRule`] applies.
///
/// Times use [`DayCycleConfig::time_of_day`]: `0.0` midnight, `0.5` noon.
/// Without a [`DayCycleConfig`] resource only [`TimeWindow::Any`] applies.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TimeWindow {
    /// Always applies, with or without a day cycle.
    #[default]
    Any,
    /// From dawn to dusk: `[0.25, 0.75)`.
    Day,
    /// From dusk to dawn: everything outside [`TimeWindow::Day`].
    Night,
    /// The half-open range `[start, end)`. Bounds are wrapped into `[0, 1)`,
    /// so `0.9..0.1` and `0.9..1.1` both mean "from 0.9 past midnight to
    /// 0.1". Empty when `start == end`, the whole day when the span is `1.0`
    /// or more, and never applies with a non-finite bound.
    Range {
        /// First `time_of_day` in the window.
        start: f32,
        /// First `time_of_day` after the window.
        end: f32,
    },
}

impl TimeWindow {
    /// Returns `true` when the window applies at `time_of_day`.
    ///
    /// `None` means there is no day cycle; only [`TimeWindow::Any`] applies.
    pub fn contains(&self, time_of_day: Option<f32>) -> bool {
        if matches!(self, Self::Any) {
            return true;
        }
        let Some(time) = time_of_day.filter(|t| t.is_finite()) else {
            return false;
        };
        let time = wrap_time(time);
        match *self {
            Self::Any => true,
            Self::Day => (DAY_START..DAY_END).contains(&time),
            Self::Night => !(DAY_START..DAY_END).contains(&time),
            Self::Range { start, end } => {
                if !(start.is_finite() && end.is_finite()) || start == end {
                    return false;
                }
                if end - start >= 1.0 {
                    return true;
                }
                let (start, end) = (wrap_time(start), wrap_time(end));
                if start <= end {
                    (start..end).contains(&time)
                } else {
                    time >= start || time < end
                }
            }
        }
    }
}

/// Wraps a finite time of day into `[0, 1)`.
fn wrap_time(time: f32) -> f32 {
    let wrapped = time.rem_euclid(1.0);
    // `rem_euclid` rounds tiny negative inputs up to exactly 1.0.
    if wrapped >= 1.0 { 0.0 } else { wrapped }
}

/// Where a [`SpawnRule`] places mobs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpawnPlacement {
    /// Standing on a solid voxel with two free voxels above it, at the centre
    /// of a voxel column. The ring's `min` is measured horizontally and its
    /// `max` in 3D, so nothing spawns on ground far below or above the player.
    #[default]
    Ground,
    /// Floating in free space (zero-gravity mobs). The ring is measured in 3D.
    Air,
}

/// Distance band around a player in which a rule spawns mobs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnRing {
    /// Closest a mob spawns to the player.
    pub min: f32,
    /// Farthest a mob spawns from the player. Also the radius in which live
    /// mobs count toward [`SpawnRule::cap`].
    pub max: f32,
}

impl SpawnRing {
    /// Creates a ring. Negative and `NaN` distances become `0`, and `max` is
    /// raised to `min` if it is smaller. An infinite `max` means "as far as
    /// the kind's despawn distance allows".
    pub fn new(min: f32, max: f32) -> Self {
        let clean = |d: f32| if d.is_nan() { 0.0 } else { d.max(0.0) };
        let min = clean(min).min(f32::MAX);
        Self {
            min,
            max: clean(max).max(min),
        }
    }
}

impl Default for SpawnRing {
    /// `[12, 32]`: out of arm's reach, well inside the default despawn distance.
    fn default() -> Self {
        Self {
            min: 12.0,
            max: 32.0,
        }
    }
}

/// Candidate spawn handed to a [`SpawnRule::zone`] predicate.
#[derive(Clone, Copy, Debug)]
pub struct SpawnContext<'a> {
    /// Where the mob would spawn (its centre).
    pub position: Vec3,
    /// Position of the player the mob would spawn around.
    pub player_position: Vec3,
    /// Current time of day, or `None` without a day cycle.
    pub time_of_day: Option<f32>,
    /// Kind that would be spawned.
    pub kind: &'a MobKind,
}

/// Predicate deciding whether a candidate position is inside a rule's zone.
pub type SpawnZone = Arc<dyn Fn(&SpawnContext<'_>) -> bool + Send + Sync>;

/// One spawning rule: which kind appears, where, when and how many.
///
/// Built with chained setters:
///
/// ```
/// use voxel_framework::mobs::{SpawnRule, TimeWindow};
///
/// let rule = SpawnRule::new("night_slime")
///     .time(TimeWindow::Night)
///     .ring(12.0, 32.0)
///     .cap(6)
///     .zone(|ctx| ctx.position.y < 80.0)
///     .despawn_outside_window(true);
/// assert_eq!(rule.cap, 6);
/// ```
#[derive(Clone)]
pub struct SpawnRule {
    /// Kind to spawn; must be registered in [`MobRegistry`].
    pub kind: MobKind,
    /// Relative chance against other rules that apply. **Default: `1.0`.**
    pub weight: f32,
    /// Distance band around the player. **Default: `[12, 32]`.** Capped
    /// below the kind's `despawn_distance`; ground rules need a ring at least
    /// 1.5 units wide.
    pub ring: SpawnRing,
    /// Part of the day in which the rule applies. **Default: `Any`.**
    pub time: TimeWindow,
    /// Ground or free-space placement. **Default: `Ground`.**
    pub placement: SpawnPlacement,
    /// Most live mobs of this kind allowed within `ring.max` of one player.
    /// **Default: `4`.**
    pub cap: usize,
    /// Optional extra condition on the candidate position (the hook for
    /// biomes or regions). **Default: none.**
    pub zone: Option<SpawnZone>,
    /// Despawn mobs spawned by this rule once `time` stops applying.
    /// **Default: `false`.**
    pub despawn_outside_window: bool,
}

impl SpawnRule {
    /// Creates a rule for `kind` with the defaults listed on each field.
    pub fn new(kind: impl Into<MobKind>) -> Self {
        Self {
            kind: kind.into(),
            weight: 1.0,
            ring: SpawnRing::default(),
            time: TimeWindow::Any,
            placement: SpawnPlacement::Ground,
            cap: 4,
            zone: None,
            despawn_outside_window: false,
        }
    }

    /// Sets the relative weight.
    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    /// Sets the distance band; see [`SpawnRing::new`].
    pub fn ring(mut self, min: f32, max: f32) -> Self {
        self.ring = SpawnRing::new(min, max);
        self
    }

    /// Sets the time window.
    pub fn time(mut self, time: TimeWindow) -> Self {
        self.time = time;
        self
    }

    /// Sets the placement.
    pub fn placement(mut self, placement: SpawnPlacement) -> Self {
        self.placement = placement;
        self
    }

    /// Sets the per-player cap.
    pub fn cap(mut self, cap: usize) -> Self {
        self.cap = cap;
        self
    }

    /// Sets the zone predicate.
    pub fn zone(
        mut self,
        zone: impl Fn(&SpawnContext<'_>) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.zone = Some(Arc::new(zone));
        self
    }

    /// Sets whether mobs despawn once the time window stops applying.
    pub fn despawn_outside_window(mut self, despawn: bool) -> Self {
        self.despawn_outside_window = despawn;
        self
    }
}

impl std::fmt::Debug for SpawnRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpawnRule")
            .field("kind", &self.kind)
            .field("weight", &self.weight)
            .field("ring", &self.ring)
            .field("time", &self.time)
            .field("placement", &self.placement)
            .field("cap", &self.cap)
            .field("zone", &self.zone.as_ref().map(|_| "<predicate>"))
            .field("despawn_outside_window", &self.despawn_outside_window)
            .finish()
    }
}

/// Spawn rules and pacing evaluated by [`MobSpawnerPlugin`].
///
/// Insert it before or after adding the plugin, or mutate it at runtime.
#[derive(Resource, Clone, Debug)]
pub struct MobSpawnRules {
    /// Rules, evaluated independently for every player.
    pub rules: Vec<SpawnRule>,
    /// Seconds between evaluations; `<= 0` pauses spawning.
    /// **Default: `2.0`.**
    pub interval_secs: f32,
    /// Candidate positions tried per player per evaluation.
    /// **Default: `8`.**
    pub attempts_per_player: u32,
    /// Seed of the spawner's deterministic random sequence. **Default: `0`.**
    pub seed: u32,
}

impl Default for MobSpawnRules {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            interval_secs: 2.0,
            attempts_per_player: 8,
            seed: 0,
        }
    }
}

impl MobSpawnRules {
    /// Adds a rule (builder style).
    pub fn with_rule(mut self, rule: SpawnRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Adds a rule.
    pub fn add(&mut self, rule: SpawnRule) -> &mut Self {
        self.rules.push(rule);
        self
    }
}

/// Marks a mob that is despawned once its time window stops applying.
///
/// Inserted by the spawner for rules with
/// [`SpawnRule::despawn_outside_window`]. It stores the window itself, so
/// editing the rules later does not change mobs that already exist.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct DespawnOutsideWindow(pub TimeWindow);

// ─── Pure decision logic ──────────────────────────────────────────────────────

/// Deterministic random sequence keyed by seed, evaluation and player index.
struct SpawnRng {
    seed: u32,
    evaluation: i32,
    player: i32,
    counter: i32,
}

impl SpawnRng {
    fn new(seed: u32, evaluation: u32, player: usize) -> Self {
        Self {
            // Pre-mixed so the seed is not interchangeable with the
            // evaluation number inside `hash3`.
            seed: seed.wrapping_mul(0x9e37_79b9).rotate_left(16) ^ 0x85eb_ca6b,
            evaluation: evaluation as i32,
            player: player as i32,
            counter: 0,
        }
    }

    /// Next value in `[0, 1)`.
    fn next_f32(&mut self) -> f32 {
        let value = hash3(self.evaluation, self.player, self.counter, self.seed);
        self.counter = self.counter.wrapping_add(1);
        value.clamp(0.0, 1.0 - f32::EPSILON)
    }
}

/// Picks an index with probability proportional to its weight.
///
/// `roll` is in `[0, 1)`. Non-positive and non-finite weights never win.
fn pick_weighted(weights: &[f32], roll: f32) -> Option<usize> {
    let usable = |w: f32| w.is_finite() && w > 0.0;
    let total: f32 = weights.iter().copied().filter(|w| usable(*w)).sum();
    if !total.is_finite() || total <= 0.0 {
        return None;
    }
    let target = roll.clamp(0.0, 1.0) * total;
    let mut acc = 0.0;
    let mut last = None;
    for (index, &weight) in weights.iter().enumerate() {
        if !usable(weight) {
            continue;
        }
        acc += weight;
        last = Some(index);
        if target < acc {
            return last;
        }
    }
    // Float rounding left `target` at the very end of the range.
    last
}

/// Caps `ring` below the distance at which the kind despawns. Returns `None`
/// when no usable ring is left: nothing inside the cap, an unbounded ring, or
/// a ground ring too thin to contain a voxel column centre.
fn effective_ring(
    ring: SpawnRing,
    placement: SpawnPlacement,
    despawn_distance: f32,
) -> Option<SpawnRing> {
    let limit = despawn_distance * DESPAWN_MARGIN;
    let max = if limit.is_nan() {
        ring.max
    } else {
        ring.max.min(limit)
    };
    let min_width = match placement {
        SpawnPlacement::Ground => MIN_GROUND_RING_WIDTH,
        SpawnPlacement::Air => 0.0,
    };
    (max.is_finite() && ring.min <= max && max - ring.min >= min_width)
        .then_some(SpawnRing { min: ring.min, max })
}

/// Weight of every rule for one player: the rule's weight when it is
/// eligible, `0.0` otherwise.
fn rule_weights(
    rules: &[SpawnRule],
    registry: &MobRegistry,
    time_of_day: Option<f32>,
    player_position: Vec3,
    live_mobs: &[(MobKind, Vec3)],
) -> Vec<f32> {
    rules
        .iter()
        .map(|rule| {
            if !rule.time.contains(time_of_day) {
                return 0.0;
            }
            let Some(ring) = registry.get(&rule.kind).and_then(|config| {
                effective_ring(rule.ring, rule.placement, config.despawn_distance)
            }) else {
                return 0.0;
            };
            let nearby = live_mobs
                .iter()
                .filter(|(kind, position)| {
                    *kind == rule.kind && position.distance(player_position) <= ring.max
                })
                .count();
            if nearby < rule.cap { rule.weight } else { 0.0 }
        })
        .collect()
}

/// Random offset from the player inside `ring`, uniform over the ring's area
/// (ground) or volume (air).
fn candidate_offset(ring: SpawnRing, placement: SpawnPlacement, rng: &mut SpawnRng) -> Vec3 {
    let angle = rng.next_f32() * std::f32::consts::TAU;
    let t = rng.next_f32();
    match placement {
        SpawnPlacement::Ground => {
            let (min, max) = (ring.min * ring.min, ring.max * ring.max);
            let distance = (min + (max - min) * t).sqrt();
            Vec3::new(angle.cos() * distance, 0.0, angle.sin() * distance)
        }
        SpawnPlacement::Air => {
            let (min, max) = (ring.min.powi(3), ring.max.powi(3));
            let distance = (min + (max - min) * t).cbrt();
            let y = rng.next_f32() * 2.0 - 1.0;
            let horizontal = (1.0 - y * y).max(0.0).sqrt();
            Vec3::new(angle.cos() * horizontal, y, angle.sin() * horizontal) * distance
        }
    }
}

/// Finds where a mob can stand in the voxel column containing `(x, z)`.
///
/// Searches outward from `near_y` and returns the centre of a mob resting on
/// the nearest surface: solid voxel below, two non-solid voxels above, all
/// loaded. `solid` returns `None` for voxels in unloaded chunks.
fn find_ground_spawn(
    x: f32,
    z: f32,
    near_y: f32,
    solid: impl Fn(IVec3) -> Option<bool>,
) -> Option<Vec3> {
    if !(x.is_finite() && z.is_finite() && near_y.is_finite()) {
        return None;
    }
    let (vx, vz, y0) = (x.floor() as i32, z.floor() as i32, near_y.floor() as i32);
    let stands_at = |y: i32| {
        solid(IVec3::new(vx, y - 1, vz)) == Some(true)
            && solid(IVec3::new(vx, y, vz)) == Some(false)
            && solid(IVec3::new(vx, y + 1, vz)) == Some(false)
    };
    (0..=GROUND_SEARCH_RANGE)
        .flat_map(|d| [y0.saturating_add(d), y0.saturating_sub(d)])
        .find(|&y| stands_at(y))
        .map(|y| {
            // Column centre keeps the capsule clear of neighbouring walls.
            Vec3::new(
                vx as f32 + 0.5,
                y as f32 + MOB_RADIUS + MOB_HALF_HEIGHT + SPAWN_CLEARANCE,
                vz as f32 + 0.5,
            )
        })
}

/// Returns `true` when every voxel touched by a mob capsule centred at
/// `position` is loaded and not solid.
fn air_spawn_clear(position: Vec3, solid: impl Fn(IVec3) -> Option<bool>) -> bool {
    if !position.is_finite() {
        return false;
    }
    let extent = Vec3::new(MOB_RADIUS, MOB_HALF_HEIGHT + MOB_RADIUS, MOB_RADIUS);
    let min = (position - extent).floor().as_ivec3();
    let max = (position + extent).floor().as_ivec3();
    (min.x..=max.x).all(|x| {
        (min.y..=max.y).all(|y| (min.z..=max.z).all(|z| solid(IVec3::new(x, y, z)) == Some(false)))
    })
}

/// Tries up to `attempts` candidates and returns the first valid spawn
/// position for `rule` around `player_position`.
fn find_spawn_position(
    rule: &SpawnRule,
    ring: SpawnRing,
    player_position: Vec3,
    time_of_day: Option<f32>,
    attempts: u32,
    rng: &mut SpawnRng,
    solid: impl Fn(IVec3) -> Option<bool>,
) -> Option<Vec3> {
    (0..attempts).find_map(|_| {
        let candidate = player_position + candidate_offset(ring, rule.placement, rng);
        let position = match rule.placement {
            SpawnPlacement::Ground => {
                let position =
                    find_ground_spawn(candidate.x, candidate.z, player_position.y, &solid)?;
                // Snapping to the column centre can leave the ring, and the
                // surface can be far above or below the player. Keeping the
                // 3D distance inside `max` means the mob counts toward the
                // cap and is not despawned by distance right away.
                let horizontal = position.xz().distance(player_position.xz());
                let inside =
                    horizontal >= ring.min && position.distance(player_position) <= ring.max;
                inside.then_some(position)?
            }
            SpawnPlacement::Air => air_spawn_clear(candidate, &solid).then_some(candidate)?,
        };
        let context = SpawnContext {
            position,
            player_position,
            time_of_day,
            kind: &rule.kind,
        };
        rule.zone
            .as_ref()
            .is_none_or(|zone| zone(&context))
            .then_some(position)
    })
}

// ─── Systems ──────────────────────────────────────────────────────────────────

/// Pacing state of the spawner.
#[derive(Resource, Default)]
struct MobSpawnerState {
    elapsed: f32,
    evaluations: u32,
}

#[allow(clippy::too_many_arguments)]
fn mob_spawner_system<V: VoxelType>(
    time: Res<Time>,
    rules: Res<MobSpawnRules>,
    registry: Res<MobRegistry>,
    mut state: ResMut<MobSpawnerState>,
    day_cycle: Option<Res<DayCycleConfig>>,
    players: Query<(Entity, &Transform), With<Player>>,
    mobs: Query<(&MobKind, &Transform)>,
    manager: Res<ChunkManager>,
    chunks: Query<&Chunk<V>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let interval = rules.interval_secs;
    if !(interval.is_finite() && interval > 0.0) || rules.rules.is_empty() {
        return;
    }
    state.elapsed += time.delta_secs();
    if state.elapsed < interval {
        return;
    }
    // One evaluation per interval, even after a long frame.
    state.elapsed %= interval;
    state.evaluations = state.evaluations.wrapping_add(1);

    let time_of_day = day_cycle.map(|day| day.time_of_day);
    let solid = |voxel: IVec3| {
        get_voxel_at::<V>(voxel.as_vec3() + Vec3::splat(0.5), &manager, &chunks)
            .map(|v| v.is_solid())
    };

    // Stable player order keeps the random sequence reproducible.
    let mut player_positions: Vec<(Entity, Vec3)> = players
        .iter()
        .map(|(entity, transform)| (entity, transform.translation))
        .collect();
    player_positions.sort_by_key(|(entity, _)| *entity);

    let mut live_mobs: Vec<(MobKind, Vec3)> = mobs
        .iter()
        .map(|(kind, transform)| (kind.clone(), transform.translation))
        .collect();

    for (index, (_, player_position)) in player_positions.into_iter().enumerate() {
        let mut rng = SpawnRng::new(rules.seed, state.evaluations, index);
        let weights = rule_weights(
            &rules.rules,
            &registry,
            time_of_day,
            player_position,
            &live_mobs,
        );
        let Some(rule) = pick_weighted(&weights, rng.next_f32()).map(|i| &rules.rules[i]) else {
            continue;
        };
        // Eligibility already proved the kind is registered and the ring valid.
        let Some(ring) = registry
            .get(&rule.kind)
            .and_then(|config| effective_ring(rule.ring, rule.placement, config.despawn_distance))
        else {
            continue;
        };
        let Some(position) = find_spawn_position(
            rule,
            ring,
            player_position,
            time_of_day,
            rules.attempts_per_player,
            &mut rng,
            solid,
        ) else {
            continue;
        };
        let Some(entity) = spawn_mob_kind(
            &rule.kind,
            &registry,
            position,
            &mut commands,
            &mut meshes,
            &mut materials,
        ) else {
            continue;
        };
        if rule.despawn_outside_window {
            commands
                .entity(entity)
                .insert(DespawnOutsideWindow(rule.time));
        }
        // Count it right away so the next player sees the updated total.
        live_mobs.push((rule.kind.clone(), position));
    }
}

fn mob_window_despawn_system(
    day_cycle: Option<Res<DayCycleConfig>>,
    mobs: Query<(Entity, &DespawnOutsideWindow)>,
    mut commands: Commands,
) {
    // Without a readable clock nothing can be "outside" its window.
    let Some(time_of_day) = day_cycle
        .map(|day| day.time_of_day)
        .filter(|time| time.is_finite())
    else {
        return;
    };
    for (entity, window) in mobs.iter() {
        if !window.0.contains(Some(time_of_day)) {
            // Another system may despawn the same mob this frame.
            commands.entity(entity).try_despawn();
        }
    }
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Spawns registered mob kinds around players according to [`MobSpawnRules`].
///
/// Add it next to [`MobPlugin`](super::MobPlugin) for the same voxel type,
/// register kinds in [`MobRegistry`] and insert a [`MobSpawnRules`] resource.
///
/// - It relies on [`ChunkManager`] and `Time` (the core voxel plugins and
///   Bevy's time plugin). Without `Assets<Mesh>` and
///   `Assets<StandardMaterial>` (headless apps) the spawner stays idle.
/// - `Day`, `Night` and `Range` rules read [`DayCycleConfig`]; without
///   [`DayCyclePlugin`](crate::world::DayCyclePlugin) only
///   [`TimeWindow::Any`] rules ever apply.
/// - Every [`Player`] gets its own spawns, and mobs despawn by distance to
///   the nearest player. Mob AI still assumes a single player.
///
/// ```no_run
/// use bevy::prelude::*;
/// use voxel_framework::VoxelPlugin;
/// use voxel_framework::mobs::{
///     MobConfig, MobPlugin, MobRegistry, MobSpawnRules, MobSpawnerPlugin, SpawnRule, TimeWindow,
/// };
/// use voxel_framework::voxel::DefaultTerrainVoxel;
/// use voxel_framework::world::DayCyclePlugin;
///
/// let mut registry = MobRegistry::default();
/// registry.register("slime", MobConfig::default());
/// registry
///     .register_variant("night_slime", "slime", |config| config.chase_speed = 5.0)
///     .unwrap();
///
/// let rules = MobSpawnRules::default()
///     .with_rule(SpawnRule::new("slime").time(TimeWindow::Day).cap(3))
///     .with_rule(
///         SpawnRule::new("night_slime")
///             .time(TimeWindow::Night)
///             .ring(12.0, 32.0)
///             .cap(6)
///             .despawn_outside_window(true),
///     );
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(VoxelPlugin::<DefaultTerrainVoxel>::new())
///     .add_plugins(DayCyclePlugin)
///     .add_plugins(MobPlugin::<DefaultTerrainVoxel>::new())
///     .add_plugins(MobSpawnerPlugin::<DefaultTerrainVoxel>::new())
///     .insert_resource(registry)
///     .insert_resource(rules)
///     .run();
/// ```
pub struct MobSpawnerPlugin<V: VoxelType> {
    _m: PhantomData<V>,
}

impl<V: VoxelType> MobSpawnerPlugin<V> {
    /// Creates the plugin.
    pub fn new() -> Self {
        Self { _m: PhantomData }
    }
}

impl<V: VoxelType> Default for MobSpawnerPlugin<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: VoxelType> Plugin for MobSpawnerPlugin<V> {
    fn build(&self, app: &mut App) {
        app.init_resource::<MobRegistry>()
            .init_resource::<MobSpawnRules>()
            .init_resource::<MobSpawnerState>()
            .add_systems(
                Update,
                (
                    mob_spawner_system::<V>.run_if(
                        resource_exists::<Assets<Mesh>>
                            .and(resource_exists::<Assets<StandardMaterial>>),
                    ),
                    mob_window_despawn_system,
                ),
            );
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::MobConfig;
    use crate::test_support::{filled_chunk, world_with_chunks};
    use crate::voxel::DefaultVoxel;
    use bevy::ecs::system::RunSystemOnce;
    use std::time::Duration;

    /// Flat world: everything below `y = 0` is solid, everything loaded.
    fn flat(voxel: IVec3) -> Option<bool> {
        Some(voxel.y < 0)
    }

    const REST: f32 = MOB_RADIUS + MOB_HALF_HEIGHT + SPAWN_CLEARANCE;

    fn registry() -> MobRegistry {
        let mut registry = MobRegistry::default();
        registry.register("slime", MobConfig::default());
        registry.register("bat", MobConfig::default());
        registry
    }

    // ── Time windows ──────────────────────────────────────────────────────

    #[test]
    fn day_and_night_split_at_dawn_and_dusk() {
        for (time, is_day) in [(0.0, false), (0.24, false), (0.25, true), (0.5, true)] {
            assert_eq!(TimeWindow::Day.contains(Some(time)), is_day, "t={time}");
            assert_eq!(TimeWindow::Night.contains(Some(time)), !is_day, "t={time}");
        }
        assert!(!TimeWindow::Day.contains(Some(0.75)));
        assert!(TimeWindow::Night.contains(Some(0.99)));
    }

    #[test]
    fn range_wraps_across_midnight() {
        let late = TimeWindow::Range {
            start: 0.9,
            end: 0.1,
        };
        assert!(late.contains(Some(0.95)));
        assert!(late.contains(Some(0.05)));
        assert!(late.contains(Some(0.9)));
        assert!(!late.contains(Some(0.1)));
        assert!(!late.contains(Some(0.5)));

        let morning = TimeWindow::Range {
            start: 0.2,
            end: 0.4,
        };
        assert!(morning.contains(Some(0.3)));
        assert!(!morning.contains(Some(0.4)));
        assert!(
            !TimeWindow::Range {
                start: 0.3,
                end: 0.3
            }
            .contains(Some(0.3))
        );
    }

    #[test]
    fn range_bounds_are_wrapped_into_one_day() {
        let range = |start, end| TimeWindow::Range { start, end };
        // Written without wrapping, meaning the same as 0.9..0.1.
        for window in [range(0.9, 1.1), range(-0.1, 0.1)] {
            assert!(window.contains(Some(0.95)), "{window:?}");
            assert!(window.contains(Some(0.05)), "{window:?}");
            assert!(!window.contains(Some(0.5)), "{window:?}");
        }
        // A span of a whole day or more always applies.
        for time in [0.0, 0.5, 0.99] {
            assert!(range(0.0, 1.0).contains(Some(time)));
            assert!(range(0.2, 1.7).contains(Some(time)));
        }
        // Dusk to midnight keeps working when `end` is written as 1.0.
        assert!(range(0.75, 1.0).contains(Some(0.9)));
        assert!(!range(0.75, 1.0).contains(Some(0.1)));
        // A clock a hair below zero is midnight, not "1.0".
        assert!(range(0.0, 0.25).contains(Some(-1e-9)));
        for window in [range(f32::NAN, 0.5), range(0.1, f32::INFINITY)] {
            assert!(!window.contains(Some(0.2)), "{window:?}");
        }
    }

    #[test]
    fn only_any_applies_without_a_usable_clock() {
        for time in [None, Some(f32::NAN), Some(f32::INFINITY)] {
            assert!(TimeWindow::Any.contains(time));
            assert!(!TimeWindow::Day.contains(time));
            assert!(!TimeWindow::Night.contains(time));
        }
        // Out-of-range clocks wrap like the day cycle does.
        assert!(TimeWindow::Day.contains(Some(1.5)));
        assert!(TimeWindow::Night.contains(Some(-0.1)));
    }

    // ── Weighted choice ───────────────────────────────────────────────────

    #[test]
    fn pick_weighted_follows_the_weights() {
        let weights = [1.0, 0.0, 3.0];
        assert_eq!(pick_weighted(&weights, 0.0), Some(0));
        assert_eq!(pick_weighted(&weights, 0.24), Some(0));
        assert_eq!(pick_weighted(&weights, 0.25), Some(2));
        assert_eq!(pick_weighted(&weights, 0.999), Some(2));
        assert_eq!(pick_weighted(&weights, 1.0), Some(2));
    }

    #[test]
    fn pick_weighted_ignores_unusable_weights() {
        assert_eq!(pick_weighted(&[], 0.5), None);
        assert_eq!(pick_weighted(&[0.0, -2.0, f32::NAN], 0.5), None);
        assert_eq!(
            pick_weighted(&[f32::NAN, 2.0, f32::INFINITY, -1.0], 0.9),
            Some(1)
        );
    }

    #[test]
    fn rng_is_deterministic_and_varies_by_key() {
        let draw = |seed, evaluation, player| {
            let mut rng = SpawnRng::new(seed, evaluation, player);
            [rng.next_f32(), rng.next_f32(), rng.next_f32()]
        };
        assert_eq!(draw(7, 3, 0), draw(7, 3, 0));
        assert_ne!(draw(7, 3, 0), draw(8, 3, 0));
        assert_ne!(draw(7, 3, 0), draw(7, 4, 0));
        assert_ne!(draw(7, 3, 0), draw(7, 3, 1));
        assert!(draw(7, 3, 0).iter().all(|v| (0.0..1.0).contains(v)));
    }

    // ── Eligibility ───────────────────────────────────────────────────────

    #[test]
    fn rule_is_ineligible_outside_its_time_window() {
        let rules = [SpawnRule::new("slime").time(TimeWindow::Night)];
        let weights = |time| rule_weights(&rules, &registry(), time, Vec3::ZERO, &[]);
        assert_eq!(weights(Some(0.5)), [0.0]);
        assert_eq!(weights(Some(0.9)), [1.0]);
        assert_eq!(weights(None), [0.0]);
    }

    #[test]
    fn rule_is_ineligible_for_an_unregistered_kind() {
        let rules = [SpawnRule::new("ghost"), SpawnRule::new("slime").weight(2.5)];
        let weights = rule_weights(&rules, &registry(), None, Vec3::ZERO, &[]);
        assert_eq!(weights, [0.0, 2.5]);
    }

    #[test]
    fn cap_counts_only_same_kind_mobs_inside_the_ring() {
        let rules = [SpawnRule::new("slime").ring(4.0, 20.0).cap(2)];
        let player = Vec3::new(100.0, 0.0, 0.0);
        let near = |kind: &str, dx: f32| (MobKind::new(kind), player + Vec3::X * dx);
        let weights =
            |mobs: &[(MobKind, Vec3)]| rule_weights(&rules, &registry(), None, player, mobs)[0];

        assert_eq!(weights(&[near("slime", 5.0)]), 1.0);
        assert_eq!(weights(&[near("slime", 5.0), near("slime", 19.0)]), 0.0);
        // Other kinds and far-away mobs do not count.
        assert_eq!(weights(&[near("slime", 5.0), near("bat", 6.0)]), 1.0);
        assert_eq!(weights(&[near("slime", 5.0), near("slime", 25.0)]), 1.0);
    }

    #[test]
    fn ring_is_capped_below_the_despawn_distance() {
        let ground = |min, max, despawn| {
            effective_ring(SpawnRing::new(min, max), SpawnPlacement::Ground, despawn)
        };
        assert_eq!(
            ground(10.0, 100.0, 50.0),
            Some(SpawnRing {
                min: 10.0,
                max: 45.0
            })
        );
        assert_eq!(ground(10.0, 30.0, 50.0), Some(SpawnRing::new(10.0, 30.0)));
        assert_eq!(ground(45.0, 100.0, 50.0), None);
        // "As far as allowed": an infinite max takes the despawn cap.
        assert_eq!(
            ground(12.0, f32::INFINITY, 64.0),
            Some(SpawnRing {
                min: 12.0,
                max: 57.6
            })
        );
        // Nothing bounds the ring.
        assert_eq!(ground(12.0, f32::INFINITY, f32::INFINITY), None);
        assert_eq!(
            ground(12.0, 30.0, f32::NAN),
            Some(SpawnRing::new(12.0, 30.0))
        );

        let mut registry = MobRegistry::default();
        registry.register(
            "slime",
            MobConfig {
                despawn_distance: 10.0,
                ..Default::default()
            },
        );
        let rules = [SpawnRule::new("slime").ring(12.0, 32.0)];
        assert_eq!(
            rule_weights(&rules, &registry, None, Vec3::ZERO, &[]),
            [0.0]
        );
    }

    #[test]
    fn ground_rules_need_a_ring_wide_enough_for_a_column_centre() {
        let rules = [
            SpawnRule::new("slime").ring(12.0, 12.0),
            SpawnRule::new("slime").ring(12.0, 13.5),
            SpawnRule::new("slime")
                .ring(12.0, 12.0)
                .placement(SpawnPlacement::Air),
        ];
        assert_eq!(
            rule_weights(&rules, &registry(), None, Vec3::ZERO, &[]),
            [0.0, 1.0, 1.0]
        );
    }

    #[test]
    fn spawn_ring_sanitizes_its_bounds() {
        assert_eq!(
            SpawnRing::new(-3.0, f32::NAN),
            SpawnRing { min: 0.0, max: 0.0 }
        );
        assert_eq!(SpawnRing::new(8.0, 2.0), SpawnRing { min: 8.0, max: 8.0 });
        assert_eq!(SpawnRing::new(8.0, f32::INFINITY).max, f32::INFINITY);
    }

    // ── Positions ─────────────────────────────────────────────────────────

    #[test]
    fn candidate_offsets_stay_inside_the_ring() {
        let ring = SpawnRing::new(8.0, 16.0);
        for evaluation in 0..200 {
            let mut rng = SpawnRng::new(1, evaluation, 0);
            let ground = candidate_offset(ring, SpawnPlacement::Ground, &mut rng);
            assert_eq!(ground.y, 0.0);
            let d = ground.length();
            assert!((8.0 - 1e-3..=16.0 + 1e-3).contains(&d), "ground d={d}");

            let air = candidate_offset(ring, SpawnPlacement::Air, &mut rng);
            let d = air.length();
            assert!((8.0 - 1e-3..=16.0 + 1e-3).contains(&d), "air d={d}");
        }
    }

    #[test]
    fn ground_spawn_rests_on_the_floor_at_the_column_centre() {
        let position = find_ground_spawn(3.9, -2.1, 5.0, flat).unwrap();
        assert_eq!(position, Vec3::new(3.5, REST, -2.5));
    }

    #[test]
    fn ground_spawn_needs_loaded_terrain_and_headroom() {
        // Nothing loaded.
        assert_eq!(find_ground_spawn(0.5, 0.5, 2.0, |_| None), None);
        // Floor loaded but the air above it is not.
        let floor_only = |v: IVec3| (v.y < 0).then_some(true);
        assert_eq!(find_ground_spawn(0.5, 0.5, 2.0, floor_only), None);
        // Only one free voxel above the floor.
        let low_ceiling = |v: IVec3| Some(v.y != 0);
        assert_eq!(find_ground_spawn(0.5, 0.5, 0.0, low_ceiling), None);
        // No surface within the search range.
        assert_eq!(find_ground_spawn(0.5, 0.5, 40.0, flat), None);
        assert_eq!(find_ground_spawn(f32::NAN, 0.5, 0.0, flat), None);
    }

    #[test]
    fn ground_spawn_prefers_the_surface_nearest_to_the_player() {
        // Cave: floor below y = 0, open for y in 0..3, rock for 3..10, open sky above.
        let cave = |v: IVec3| Some(v.y < 0 || (3..10).contains(&v.y));
        let in_cave = find_ground_spawn(0.5, 0.5, 1.0, cave).unwrap();
        assert!((in_cave.y - REST).abs() < 1e-6, "y={}", in_cave.y);
        let on_top = find_ground_spawn(0.5, 0.5, 11.0, cave).unwrap();
        assert!((on_top.y - (10.0 + REST)).abs() < 1e-6, "y={}", on_top.y);
    }

    #[test]
    fn air_spawn_needs_a_free_loaded_capsule() {
        assert!(air_spawn_clear(Vec3::new(0.5, 5.0, 0.5), flat));
        // Feet inside the floor.
        assert!(!air_spawn_clear(Vec3::new(0.5, 0.5, 0.5), flat));
        assert!(!air_spawn_clear(Vec3::new(0.5, 5.0, 0.5), |_| None));
        assert!(!air_spawn_clear(Vec3::splat(f32::NAN), flat));
    }

    #[test]
    fn spawn_position_respects_ring_and_zone() {
        let rule = SpawnRule::new("slime").ring(8.0, 16.0);
        let player = Vec3::new(40.2, 1.0, -7.7);
        for evaluation in 0..100 {
            let mut rng = SpawnRng::new(3, evaluation, 0);
            if let Some(position) =
                find_spawn_position(&rule, rule.ring, player, None, 8, &mut rng, flat)
            {
                let d = position.xz().distance(player.xz());
                assert!((8.0..=16.0).contains(&d), "d={d}");
                assert!((position.y - REST).abs() < 1e-6);
            }
        }

        let east_only = rule
            .clone()
            .zone(|ctx| ctx.position.x > ctx.player_position.x);
        let mut found = 0;
        for evaluation in 0..100 {
            let mut rng = SpawnRng::new(3, evaluation, 0);
            if let Some(position) =
                find_spawn_position(&east_only, rule.ring, player, None, 8, &mut rng, flat)
            {
                assert!(position.x > player.x);
                found += 1;
            }
        }
        assert!(found > 50, "zone rejected too much: {found}");

        let nowhere = rule.clone().zone(|_| false);
        let mut rng = SpawnRng::new(3, 0, 0);
        assert_eq!(
            find_spawn_position(&nowhere, rule.ring, player, None, 8, &mut rng, flat),
            None
        );
    }

    #[test]
    fn ground_spawn_far_below_the_player_is_rejected() {
        // Player 14 units above the floor: a ground mob inside the horizontal
        // ring would be farther than `max` in 3D, so it would dodge the cap.
        let rule = SpawnRule::new("slime").ring(2.0, 6.0);
        let player = Vec3::new(0.5, 14.0, 0.5);
        for evaluation in 0..50 {
            let mut rng = SpawnRng::new(3, evaluation, 0);
            assert_eq!(
                find_spawn_position(&rule, rule.ring, player, None, 8, &mut rng, flat),
                None
            );
        }
        // A wider ring still spawns, always within `max` in 3D.
        let wide = SpawnRule::new("slime").ring(2.0, 24.0);
        let mut found = 0;
        for evaluation in 0..50 {
            let mut rng = SpawnRng::new(3, evaluation, 0);
            if let Some(position) =
                find_spawn_position(&wide, wide.ring, player, None, 8, &mut rng, flat)
            {
                assert!(position.distance(player) <= 24.0);
                found += 1;
            }
        }
        assert!(found > 0);
    }

    #[test]
    fn air_spawn_stays_in_the_ring_and_in_free_loaded_space() {
        let rule = SpawnRule::new("bat")
            .ring(8.0, 16.0)
            .placement(SpawnPlacement::Air);
        let player = Vec3::new(0.5, 30.0, 0.5);
        let mut found = 0;
        for evaluation in 0..100 {
            let mut rng = SpawnRng::new(9, evaluation, 0);
            if let Some(position) =
                find_spawn_position(&rule, rule.ring, player, None, 8, &mut rng, flat)
            {
                let d = position.distance(player);
                assert!((8.0 - 1e-3..=16.0 + 1e-3).contains(&d), "d={d}");
                assert!(air_spawn_clear(position, flat));
                found += 1;
            }
        }
        assert!(found > 0);

        let mut rng = SpawnRng::new(9, 0, 0);
        assert_eq!(
            find_spawn_position(&rule, rule.ring, player, None, 8, &mut rng, |_| None),
            None,
            "nothing loaded"
        );
    }

    #[test]
    fn same_inputs_give_the_same_spawn_position() {
        let rule = SpawnRule::new("slime").ring(8.0, 16.0);
        let find = |seed| {
            let mut rng = SpawnRng::new(seed, 5, 0);
            find_spawn_position(&rule, rule.ring, Vec3::ZERO, None, 8, &mut rng, flat)
        };
        assert_eq!(find(11), find(11));
        assert_ne!(find(11), find(12));
    }

    // ── Systems ───────────────────────────────────────────────────────────

    const FLOOR_TOP: f32 = 16.0;

    /// Solid chunk with a loaded empty chunk above, plus everything the
    /// spawner system needs. One evaluation per [`evaluate`] call.
    fn spawner_world(rules: MobSpawnRules) -> World {
        let mut world = world_with_chunks([
            (IVec3::ZERO, filled_chunk(DefaultVoxel::Rock)),
            (IVec3::Y, Chunk::<DefaultVoxel>::new()),
        ]);
        world.insert_resource(Time::<()>::default());
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(MobSpawnerState::default());
        world.insert_resource(registry());
        world.insert_resource(rules);
        world
    }

    fn spawn_player(world: &mut World, position: Vec3) -> Entity {
        world
            .spawn((Player::default(), Transform::from_translation(position)))
            .id()
    }

    fn evaluate(world: &mut World, times: usize) {
        let interval = world.resource::<MobSpawnRules>().interval_secs;
        for _ in 0..times {
            world
                .resource_mut::<Time>()
                .advance_by(Duration::from_secs_f32(interval));
            world
                .run_system_once(mob_spawner_system::<DefaultVoxel>)
                .unwrap();
            world.run_system_once(mob_window_despawn_system).unwrap();
        }
    }

    fn mobs_of(world: &mut World, kind: &str) -> Vec<Vec3> {
        let mut query = world.query::<(&MobKind, &Transform)>();
        query
            .iter(world)
            .filter(|(k, _)| k.as_str() == kind)
            .map(|(_, t)| t.translation)
            .collect()
    }

    fn set_time(world: &mut World, time_of_day: f32) {
        world.insert_resource(DayCycleConfig {
            time_of_day,
            ..Default::default()
        });
    }

    #[test]
    fn spawner_places_mobs_on_the_floor_inside_the_ring_up_to_the_cap() {
        let rules =
            MobSpawnRules::default().with_rule(SpawnRule::new("slime").ring(2.0, 6.0).cap(3));
        let mut world = spawner_world(rules);
        let player = Vec3::new(8.0, FLOOR_TOP + 1.0, 8.0);
        spawn_player(&mut world, player);

        evaluate(&mut world, 1);
        assert_eq!(mobs_of(&mut world, "slime").len(), 1, "one per evaluation");

        evaluate(&mut world, 20);
        let mobs = mobs_of(&mut world, "slime");
        assert_eq!(mobs.len(), 3, "cap reached and held");
        for position in mobs {
            let d = position.xz().distance(player.xz());
            assert!((2.0..=6.0).contains(&d), "d={d}");
            assert!((position.y - (FLOOR_TOP + REST)).abs() < 1e-5);
        }
    }

    #[test]
    fn spawner_waits_for_the_interval() {
        let rules = MobSpawnRules::default().with_rule(SpawnRule::new("slime").ring(2.0, 6.0));
        let mut world = spawner_world(rules);
        spawn_player(&mut world, Vec3::new(8.0, FLOOR_TOP + 1.0, 8.0));

        for _ in 0..3 {
            world
                .resource_mut::<Time>()
                .advance_by(Duration::from_secs_f32(0.5));
            world
                .run_system_once(mob_spawner_system::<DefaultVoxel>)
                .unwrap();
        }
        assert!(mobs_of(&mut world, "slime").is_empty(), "1.5 s < 2 s");

        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(0.6));
        world
            .run_system_once(mob_spawner_system::<DefaultVoxel>)
            .unwrap();
        assert_eq!(mobs_of(&mut world, "slime").len(), 1);
    }

    #[test]
    fn spawner_follows_the_time_of_day() {
        let rules = MobSpawnRules::default()
            .with_rule(SpawnRule::new("slime").ring(2.0, 6.0).time(TimeWindow::Day))
            .with_rule(
                SpawnRule::new("bat")
                    .ring(2.0, 6.0)
                    .time(TimeWindow::Night)
                    .despawn_outside_window(true),
            );
        let mut world = spawner_world(rules);
        spawn_player(&mut world, Vec3::new(8.0, FLOOR_TOP + 1.0, 8.0));

        // No day cycle: neither rule applies.
        evaluate(&mut world, 5);
        assert!(mobs_of(&mut world, "slime").is_empty());
        assert!(mobs_of(&mut world, "bat").is_empty());

        set_time(&mut world, 0.9);
        evaluate(&mut world, 5);
        assert!(mobs_of(&mut world, "slime").is_empty());
        let bats = mobs_of(&mut world, "bat").len();
        assert!(bats > 0, "night rule spawns at night");

        // Dawn: night mobs marked for it disappear, day mobs appear and stay.
        set_time(&mut world, 0.25);
        evaluate(&mut world, 5);
        assert!(mobs_of(&mut world, "bat").is_empty());
        let slimes = mobs_of(&mut world, "slime").len();
        assert!(slimes > 0);

        set_time(&mut world, 0.9);
        evaluate(&mut world, 1);
        assert_eq!(
            mobs_of(&mut world, "slime").len(),
            slimes,
            "day mobs are not tied to their window"
        );
    }

    #[test]
    fn spawner_serves_every_player() {
        let rules =
            MobSpawnRules::default().with_rule(SpawnRule::new("slime").ring(1.0, 2.5).cap(1));
        let mut world = spawner_world(rules);
        let a = Vec3::new(3.5, FLOOR_TOP + 1.0, 3.5);
        let b = Vec3::new(12.5, FLOOR_TOP + 1.0, 12.5);
        spawn_player(&mut world, a);
        spawn_player(&mut world, b);

        evaluate(&mut world, 10);

        let mobs = mobs_of(&mut world, "slime");
        assert_eq!(mobs.len(), 2, "one per player at cap 1");
        for player in [a, b] {
            assert!(
                mobs.iter().any(|m| m.xz().distance(player.xz()) <= 2.5),
                "no mob near {player}"
            );
        }
    }

    #[test]
    fn spawner_is_deterministic_for_a_seed() {
        let run = |seed| {
            let mut rules = MobSpawnRules::default()
                .with_rule(SpawnRule::new("slime").ring(2.0, 6.0).cap(8))
                .with_rule(SpawnRule::new("bat").ring(2.0, 6.0).cap(8));
            rules.seed = seed;
            let mut world = spawner_world(rules);
            spawn_player(&mut world, Vec3::new(8.0, FLOOR_TOP + 1.0, 8.0));
            evaluate(&mut world, 6);
            let mut all = mobs_of(&mut world, "slime");
            all.extend(mobs_of(&mut world, "bat"));
            all.sort_by(|p, q| p.to_array().partial_cmp(&q.to_array()).unwrap());
            (mobs_of(&mut world, "slime").len(), all)
        };
        assert_eq!(run(42), run(42));
        assert_ne!(run(42), run(43));
    }

    #[test]
    fn spawner_does_nothing_when_paused_or_without_players() {
        let rules = MobSpawnRules::default().with_rule(SpawnRule::new("slime").ring(2.0, 6.0));
        let mut world = spawner_world(rules);
        evaluate(&mut world, 5);
        assert!(mobs_of(&mut world, "slime").is_empty(), "no players");

        spawn_player(&mut world, Vec3::new(8.0, FLOOR_TOP + 1.0, 8.0));
        world.resource_mut::<MobSpawnRules>().interval_secs = 0.0;
        for _ in 0..5 {
            world
                .resource_mut::<Time>()
                .advance_by(Duration::from_secs(5));
            world
                .run_system_once(mob_spawner_system::<DefaultVoxel>)
                .unwrap();
        }
        assert!(mobs_of(&mut world, "slime").is_empty(), "paused");
    }

    #[test]
    fn cap_holds_while_the_player_hovers_above_the_ground() {
        let rules =
            MobSpawnRules::default().with_rule(SpawnRule::new("slime").ring(2.0, 9.0).cap(2));
        let mut world = spawner_world(rules);
        // 6 units above the floor: only part of the ring reaches the ground.
        spawn_player(&mut world, Vec3::new(8.0, FLOOR_TOP + 6.0, 8.0));

        evaluate(&mut world, 40);

        assert_eq!(mobs_of(&mut world, "slime").len(), 2);
    }

    #[test]
    fn plugin_runs_in_an_app_and_idles_without_asset_stores() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ChunkManager>()
            .add_plugins(MobSpawnerPlugin::<DefaultVoxel>::new())
            .insert_resource(
                MobSpawnRules::default().with_rule(SpawnRule::new("slime").ring(2.0, 6.0)),
            );
        // No `Assets<Mesh>`: the spawner must skip instead of panicking.
        app.update();

        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>();
        app.update();
    }

    #[test]
    fn plugin_initializes_its_resources() {
        let mut app = App::new();
        app.add_plugins(MobSpawnerPlugin::<DefaultVoxel>::new());
        assert!(app.world().contains_resource::<MobSpawnRules>());
        assert!(app.world().contains_resource::<MobRegistry>());
        assert!(app.world().resource::<MobSpawnRules>().rules.is_empty());
    }
}
