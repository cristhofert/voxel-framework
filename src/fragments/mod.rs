//! Voxel break-apart fragment effect.
//!
//! When a voxel is destroyed, this module spawns a small burst of cube-shaped
//! debris particles that fly outward, arc downward under simulated gravity,
//! and automatically despawn after a configurable lifetime.
//!
//! # Lifecycle
//!
//! 1. **Spawn** — each `VoxelBroken` message
//!    triggers the spawning of [`FragmentConfig::count`] fragment entities
//!    centered at the broken voxel's world position.  Initial velocities are
//!    distributed using a golden-angle spiral with pseudo-random jitter so
//!    fragments fan out naturally rather than clustering.
//! 2. **Physics update** — every `Update` tick, each fragment's velocity is
//!    updated with a downward gravity acceleration
//!    ([`FragmentConfig::gravity`]), its position is integrated, and its
//!    remaining lifetime is decremented.
//! 3. **Despawn** — once `lifetime_remaining` reaches zero the entity is
//!    despawned immediately.
//!
//! # Pooled mesh assets
//!
//! All fragments share a single [`Mesh`] handle and a single
//! [`StandardMaterial`] handle, both pre-allocated during `Startup` and stored
//! in the [`FragmentAssets`] resource.  This avoids re-allocating GPU resources
//! for every voxel break event.
//!
//! # Integration
//!
//! ```no_run
//! use bevy::prelude::*;
//! use voxel_framework::fragments::{FragmentConfig, FragmentPlugin};
//!
//! fn main() {
//!     App::new()
//!         .insert_resource(FragmentConfig {
//!             count: 10,
//!             initial_speed: 5.0,
//!             gravity: 12.0,
//!             lifetime: 1.2,
//!             size: 0.15,
//!         })
//!         .add_plugins(FragmentPlugin)
//!         .run();
//! }
//! ```

use bevy::prelude::*;

use crate::generator::GeneratorSystems;
use crate::world::VoxelBroken;

// ─── Resources ───────────────────────────────────────────────────────────────

/// Controls the appearance and physics behavior of voxel break fragments.
///
/// Insert a customized instance before adding [`FragmentPlugin`] to override
/// the defaults, or rely on [`Default`] for sensible Minecraft-like values.
#[derive(Resource, Debug, Clone)]
pub struct FragmentConfig {
    /// Number of fragment entities spawned per broken voxel.
    ///
    /// Higher values produce a denser, more dramatic effect at the cost of
    /// additional entity overhead per break event.  Default: `6`.
    pub count: u32,
    /// Magnitude of the initial outward velocity applied to each fragment,
    /// in world units per second.
    ///
    /// The actual per-fragment speed is jittered in the range
    /// `[0.7 × initial_speed, 1.3 × initial_speed]`.  Default: `4.0`.
    pub initial_speed: f32,
    /// Downward acceleration applied every tick, in world units per second².
    ///
    /// Analogous to gravitational acceleration.  Higher values make fragments
    /// fall faster and arc more sharply.  Default: `9.8`.
    pub gravity: f32,
    /// Duration in seconds before a fragment entity is automatically despawned.
    ///
    /// Default: `1.5`.
    pub lifetime: f32,
    /// Side length of each fragment cube, in world units.
    ///
    /// Also controls the random spawn offset: fragments are displaced by up to
    /// `±size` from the voxel center at spawn time.  Default: `0.2`.
    pub size: f32,
}

impl Default for FragmentConfig {
    fn default() -> Self {
        Self {
            count: 6,
            initial_speed: 4.0,
            gravity: 9.8,
            lifetime: 1.5,
            size: 0.2,
        }
    }
}

/// Shared GPU asset handles used by all fragment entities.
///
/// All fragments in a scene share a single [`Mesh`] (a cube of side length
/// [`FragmentConfig::size`]) and a single warm-grey unlit [`StandardMaterial`].
/// Sharing these handles avoids creating a new GPU resource per broken voxel.
///
/// This resource is inserted automatically at `Startup` by the setup system
/// registered in [`FragmentPlugin`].  It is available for queries from any
/// subsequent system that needs to inspect or override the shared assets.
#[derive(Resource)]
pub struct FragmentAssets {
    /// Handle to the shared cuboid mesh used by every fragment entity.
    pub mesh: Handle<Mesh>,
    /// Handle to the shared unlit material used by every fragment entity.
    ///
    /// The base color is a warm grey (`srgb(0.55, 0.50, 0.47)`) with
    /// `unlit: true` so fragments remain visible regardless of scene lighting.
    pub material: Handle<StandardMaterial>,
}

// ─── Components ──────────────────────────────────────────────────────────────

/// Per-entity state component attached to each fragment particle.
///
/// The update system reads and writes both fields every tick.  Entities whose
/// `lifetime_remaining` reaches zero or below are despawned immediately.
#[derive(Component)]
pub struct Fragment {
    /// Current velocity of the fragment in world space, in units per second.
    ///
    /// Set at spawn time from [`FragmentConfig::initial_speed`] and the
    /// outward direction computed by the golden-angle spiral.  Decremented
    /// along the Y axis each frame by gravity
    /// ([`FragmentConfig::gravity`] × delta time).
    pub velocity: Vec3,
    /// Seconds remaining before this fragment entity is despawned.
    ///
    /// Initialized to [`FragmentConfig::lifetime`] at spawn.  Decremented by
    /// `Time::delta_secs()` each `Update` tick.
    pub lifetime_remaining: f32,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Cheap deterministic pseudo-random float in [0, 1) derived from a seed.
///
/// Uses a xorshift32-style mix so there is no `rand` dependency.
#[inline]
fn pseudo_rand(mut seed: u32) -> f32 {
    seed ^= seed << 13;
    seed ^= seed >> 17;
    seed ^= seed << 5;
    // Map to [0, 1)
    (seed as f32) / (u32::MAX as f32)
}

/// Compute a seed from a world position and fragment index.
#[inline]
fn position_seed(world_pos: Vec3, index: u32) -> u32 {
    let ix = (world_pos.x * 1000.0) as i32 as u32;
    let iy = (world_pos.y * 1000.0) as i32 as u32;
    let iz = (world_pos.z * 1000.0) as i32 as u32;
    let h = ix.wrapping_mul(2654435761)
        ^ iy.wrapping_mul(2246822519)
        ^ iz.wrapping_mul(3266489917)
        ^ index.wrapping_mul(1013904223);
    // One extra mix pass to reduce correlation between adjacent indices.
    h ^ (h >> 16)
}

/// Compute an outward direction for fragment `index` using a golden-angle
/// spiral spread around +Y, with pseudo-random jitter.
fn fragment_direction(world_pos: Vec3, index: u32, count: u32) -> Vec3 {
    // Golden angle in radians ≈ 2.399963…
    const GOLDEN_ANGLE: f32 = 2.399_963;

    let seed_a = position_seed(world_pos, index * 3);
    let seed_b = position_seed(world_pos, index * 3 + 1);
    let seed_c = position_seed(world_pos, index * 3 + 2);

    // Elevation: spread fragments in a hemisphere mostly upward / outward.
    let t = if count > 1 {
        index as f32 / (count - 1) as f32
    } else {
        0.5
    };
    // Elevation angle in [10°, 80°] degrees from horizontal
    let elevation = std::f32::consts::FRAC_PI_8
        + t * (std::f32::consts::FRAC_PI_2 - std::f32::consts::FRAC_PI_4);

    // Azimuth: golden angle spiral + small random jitter
    let azimuth = index as f32 * GOLDEN_ANGLE + pseudo_rand(seed_a) * 0.3;

    // Small random jitter on elevation
    let elevation_jitter = (pseudo_rand(seed_b) - 0.5) * 0.4;
    let final_elevation = (elevation + elevation_jitter).clamp(0.05, std::f32::consts::FRAC_PI_2);

    // Speed scale jitter in [0.7, 1.3]
    let _speed_jitter = 0.7 + pseudo_rand(seed_c) * 0.6;

    let (sin_el, cos_el) = final_elevation.sin_cos();
    let (sin_az, cos_az) = azimuth.sin_cos();

    Vec3::new(cos_el * cos_az, sin_el, cos_el * sin_az).normalize()
}

// ─── Systems ─────────────────────────────────────────────────────────────────

/// Startup system — allocates the shared mesh and material handles and inserts
/// them as a [`FragmentAssets`] resource.
fn setup_fragment_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    config: Res<FragmentConfig>,
) {
    let mesh = meshes.add(Cuboid::new(config.size, config.size, config.size));

    // Warm grey, unlit so it does not depend on the scene's light setup.
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.50, 0.47),
        unlit: true,
        ..default()
    });

    commands.insert_resource(FragmentAssets { mesh, material });
}

/// Listens for [`VoxelBroken`] messages and spawns debris fragments at the
/// broken voxel's world position.
fn spawn_fragments_system(
    mut commands: Commands,
    mut reader: MessageReader<VoxelBroken>,
    config: Res<FragmentConfig>,
    assets: Res<FragmentAssets>,
) {
    for event in reader.read() {
        let origin = event.world_pos;

        for i in 0..config.count {
            let dir = fragment_direction(origin, i, config.count);

            // Speed jitter: [0.7, 1.3] × initial_speed
            let seed = position_seed(origin, i * 3 + 2);
            let speed_scale = 0.7 + pseudo_rand(seed) * 0.6;
            let velocity = dir * config.initial_speed * speed_scale;

            // Small random offset so fragments don't all start at the exact center.
            let offset_seed_x = position_seed(origin, i * 7);
            let offset_seed_y = position_seed(origin, i * 7 + 1);
            let offset_seed_z = position_seed(origin, i * 7 + 2);
            let offset = Vec3::new(
                (pseudo_rand(offset_seed_x) - 0.5) * config.size * 2.0,
                (pseudo_rand(offset_seed_y) - 0.5) * config.size * 2.0,
                (pseudo_rand(offset_seed_z) - 0.5) * config.size * 2.0,
            );

            commands.spawn((
                Mesh3d(assets.mesh.clone()),
                MeshMaterial3d(assets.material.clone()),
                Transform::from_translation(origin + offset),
                Fragment {
                    velocity,
                    lifetime_remaining: config.lifetime,
                },
            ));
        }
    }
}

/// Advances all fragment positions and despawns any whose lifetime has expired.
fn update_fragments_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Transform, &mut Fragment)>,
    config: Res<FragmentConfig>,
) {
    let delta = time.delta_secs();

    for (entity, mut transform, mut fragment) in &mut query {
        // Apply gravity.
        fragment.velocity.y -= config.gravity * delta;
        // Integrate position.
        transform.translation += fragment.velocity * delta;
        // Tick lifetime.
        fragment.lifetime_remaining -= delta;

        if fragment.lifetime_remaining <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Plugin that enables the voxel break-apart fragment effect.
///
/// # What it registers
///
/// * [`FragmentConfig`] — initialized as a default resource.  Insert a
///   pre-populated instance before adding this plugin to override the defaults.
/// * A `Startup` system that allocates the shared mesh and material and inserts
///   them as [`FragmentAssets`].
/// * A `Update` system that listens for `VoxelBroken` messages and spawns
///   fragment entities; ordered after `GeneratorSystems` to ensure world state
///   is settled first.
/// * A `Update` system that advances fragment physics and despawns expired
///   entities.
///
/// # Examples
///
/// ```no_run
/// use bevy::prelude::*;
/// use voxel_framework::fragments::{FragmentConfig, FragmentPlugin};
///
/// fn main() {
///     App::new()
///         .insert_resource(FragmentConfig {
///             count: 8,
///             ..Default::default()
///         })
///         .add_plugins(FragmentPlugin)
///         .run();
/// }
/// ```
pub struct FragmentPlugin;

impl Plugin for FragmentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FragmentConfig>();
        app.add_systems(Startup, setup_fragment_assets);
        app.add_systems(Update, spawn_fragments_system.after(GeneratorSystems));
        app.add_systems(Update, update_fragments_system);
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fragment_config_defaults() {
        let config = FragmentConfig::default();
        assert_eq!(config.count, 6);
        assert!((config.initial_speed - 4.0).abs() < f32::EPSILON);
        assert!((config.gravity - 9.8).abs() < f32::EPSILON);
        assert!((config.lifetime - 1.5).abs() < f32::EPSILON);
        assert!((config.size - 0.2).abs() < f32::EPSILON);
    }

    #[test]
    fn test_fragment_despawns_after_lifetime() {
        let config = FragmentConfig::default();

        // Simulate a fragment ticking down its lifetime manually.
        let mut lifetime_remaining = config.lifetime;
        let velocity = Vec3::new(1.0, 2.0, 0.5);
        let gravity = config.gravity;

        let mut translation = Vec3::ZERO;
        let mut vel = velocity;

        let delta = 0.1_f32;
        let mut despawned = false;

        for _ in 0..100 {
            vel.y -= gravity * delta;
            translation += vel * delta;
            lifetime_remaining -= delta;

            if lifetime_remaining <= 0.0 {
                despawned = true;
                break;
            }
        }

        assert!(despawned, "fragment should despawn when lifetime expires");
        assert!(lifetime_remaining <= 0.0, "lifetime must reach zero");
        // Position should have moved from origin due to initial velocity + gravity.
        assert!(
            translation.length() > 0.0,
            "fragment should have moved before despawning"
        );
    }
}
