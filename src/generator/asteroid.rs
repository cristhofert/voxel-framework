//! Procedural asteroid generator for [`DefaultVoxel`].
//!
//! # Algorithm
//!
//! [`AsteroidGenerator`] populates chunks using a density-field approach:
//!
//! 1. **Asteroid placement** — at construction time, [`AsteroidConfig::asteroid_count`]
//!    asteroid centers and radii are derived deterministically from the seed
//!    using [`hash3`] with decorrelated per-component inputs.  No `rand` crate
//!    is involved, which avoids version-drift and guarantees identical output
//!    across platforms.  The first asteroid is always pinned at the world
//!    origin so there is always a guaranteed spawn point.
//!
//! 2. **Surface deformation** — for each voxel candidate the distance to the
//!    nearest asteroid center is computed.  A surface displacement is sampled
//!    from an FBM noise field (using [`fbm`] with configurable octaves,
//!    frequency, and amplitude) evaluated at world-space coordinates normalized
//!    by `2 × radius`.  The density is `radius − dist + surface_displacement`;
//!    voxels with positive density are solid.
//!
//! 3. **Mineral distribution** — three independent FBM passes with seed offsets
//!    of `+100`, `+200`, and `+300` produce separate noise fields for rare
//!    minerals, silicon crystals, and iron ore respectively.  Threshold
//!    comparisons on those fields determine which [`DefaultVoxel`] variant is
//!    placed (see [`MineralFrequencies`] for the default thresholds).
//!
//! 4. **First-asteroid-wins** — when multiple asteroids overlap, the first one
//!    in the precomputed list claims the voxel.  This keeps the logic O(n) per
//!    voxel and produces seamless asteroid interiors.

use bevy::math::{IVec3, Vec3};

use crate::chunk::Chunk;
use crate::chunk::CHUNK_SIZE;
use crate::noise::{fbm, hash3};
use crate::voxel::DefaultVoxel;

use super::ChunkGenerator;

/// Approximate volumetric distribution frequencies for each mineral type within
/// a solid asteroid.
///
/// The values are not used as direct probability weights; rather they document
/// the intended proportions that the FBM noise thresholds in
/// [`AsteroidGenerator::determine_mineral`] were tuned to produce.  Changing
/// these fields alone has no effect — they are for documentation and future
/// tooling that may auto-tune thresholds to match target frequencies.
///
/// Values represent the fraction of solid voxels expected to contain each
/// mineral.  The sum should be approximately `1.0`.
#[derive(Clone, Debug)]
pub struct MineralFrequencies {
    /// Expected fraction of solid voxels that are rare mineral nodes.
    ///
    /// Default: `0.05` (~5 %).  Achieved by requiring `rare_noise > 0.55`.
    pub rare_mineral: f32,
    /// Expected fraction of solid voxels that are silicon crystal veins.
    ///
    /// Default: `0.15` (~15 %).  Achieved by requiring `crystal_noise > 0.30`.
    pub silicon_crystal: f32,
    /// Expected fraction of solid voxels that are iron ore deposits.
    ///
    /// Default: `0.25` (~25 %).  Achieved by requiring `iron_noise > 0.20`.
    pub iron_ore: f32,
    /// Expected fraction of solid voxels that are common rock (the fallback).
    ///
    /// Default: `0.55` (~55 %).
    pub rock: f32,
}

impl Default for MineralFrequencies {
    fn default() -> Self {
        Self {
            rare_mineral: 0.05,
            silicon_crystal: 0.15,
            iron_ore: 0.25,
            rock: 0.55,
        }
    }
}

/// Configuration for the [`AsteroidGenerator`].
///
/// Controls asteroid count, size distribution, surface noise parameters, and
/// mineral frequencies.  All generation is fully deterministic: the same
/// `AsteroidConfig` always produces the same world.
///
/// Construct via [`AsteroidConfig::default`] and override individual fields:
///
/// ```rust,no_run
/// use voxel_framework::generator::asteroid::AsteroidConfig;
///
/// let config = AsteroidConfig {
///     seed: 42,
///     asteroid_count: 12,
///     min_radius: 15.0,
///     max_radius: 80.0,
///     ..AsteroidConfig::default()
/// };
/// ```
#[derive(Clone, Debug)]
pub struct AsteroidConfig {
    /// Seed for the hash-based pseudo-random number generator.
    ///
    /// Changing the seed produces a completely different asteroid field.
    pub seed: u32,

    /// Number of asteroids to pre-compute and place in the world.
    ///
    /// The first asteroid is always at the origin.  Valid range: `1..`.
    pub asteroid_count: usize,

    /// Minimum asteroid radius in world units.
    ///
    /// Must satisfy `min_radius ≤ max_radius`.  Valid range: `> 0.0`.
    pub min_radius: f32,

    /// Maximum asteroid radius in world units.
    ///
    /// Must satisfy `max_radius ≥ min_radius`.
    pub max_radius: f32,

    /// Number of FBM octaves used for surface deformation.
    ///
    /// More octaves add finer surface detail at the cost of more noise
    /// evaluations per voxel.  Typical range: `2..=6`.  Default: `4`.
    pub noise_octaves: u32,

    /// Base sampling frequency for the surface FBM field.
    ///
    /// Higher values produce a rougher, more pitted surface.  The field is
    /// sampled at world coordinates divided by `2 × radius`, so the actual
    /// spatial frequency scales with asteroid size.  Default: `2.5`.
    pub noise_frequency: f32,

    /// Surface displacement amplitude as a fraction of the asteroid radius.
    ///
    /// A value of `0.34` means the surface can deviate by up to ±34 % of the
    /// radius.  Valid range: `0.0..=1.0`.  Default: `0.34`.
    pub noise_amplitude: f32,

    /// Intended mineral distribution frequencies (documentation only).
    ///
    /// See [`MineralFrequencies`] for details.
    pub mineral_frequencies: MineralFrequencies,
}

impl Default for AsteroidConfig {
    fn default() -> Self {
        Self {
            seed: 12345,
            asteroid_count: 8,
            min_radius: 20.0,
            max_radius: 60.0,
            noise_octaves: 4,
            noise_frequency: 2.5,
            noise_amplitude: 0.34,
            mineral_frequencies: MineralFrequencies::default(),
        }
    }
}

/// Pre-computed asteroid with center position and radius.
#[derive(Clone, Debug)]
struct Asteroid {
    center: Vec3,
    radius: f32,
    /// Per-asteroid seed derived from global seed + index.
    seed: u32,
    /// Y-axis flattening factor in [0.5, 1.0]. 0.5 = flat disk, 1.0 = sphere.
    flatten_y: f32,
}

/// Procedural generator that fills the world with asteroid-shaped voxel
/// formations, implementing [`ChunkGenerator<DefaultVoxel>`].
///
/// See the [module-level documentation](self) for a full description of the
/// generation algorithm.  Asteroid positions and radii are computed once at
/// construction time via [`AsteroidGenerator::new`]; subsequent calls to
/// [`ChunkGenerator::generate`] are read-only and safe to call concurrently
/// from multiple worker threads.
///
/// # Examples
///
/// ```rust,no_run
/// use std::sync::Arc;
/// use voxel_framework::generator::{AsteroidGenerator, GeneratorResource};
/// use voxel_framework::generator::asteroid::AsteroidConfig;
/// use voxel_framework::voxel::DefaultVoxel;
///
/// let config = AsteroidConfig { seed: 99, asteroid_count: 5, ..AsteroidConfig::default() };
/// let generator = Arc::new(AsteroidGenerator::new(config));
///
/// // Insert into Bevy as a resource:
/// // app.insert_resource(GeneratorResource::<DefaultVoxel> { generator });
/// ```
pub struct AsteroidGenerator {
    config: AsteroidConfig,
    asteroids: Vec<Asteroid>,
}

impl AsteroidGenerator {
    /// Creates an [`AsteroidGenerator`] from the given [`AsteroidConfig`].
    ///
    /// All `config.asteroid_count` asteroid centers and radii are computed
    /// immediately using [`hash3`]-based pseudo-random generation.  Construction
    /// is O(n) in `asteroid_count` and allocates a single `Vec`; it is fast
    /// enough to run on the main thread at app startup.
    pub fn new(config: AsteroidConfig) -> Self {
        let asteroids = Self::build_asteroids(&config);
        Self { config, asteroids }
    }

    /// Returns a reference to the generator's configuration.
    pub fn config(&self) -> &AsteroidConfig {
        &self.config
    }

    /// Returns the number of pre-computed asteroids.
    pub fn asteroid_count(&self) -> usize {
        self.asteroids.len()
    }

    /// Deterministically generates asteroid positions and radii from the seed.
    fn build_asteroids(config: &AsteroidConfig) -> Vec<Asteroid> {
        let mut asteroids = Vec::with_capacity(config.asteroid_count);

        for i in 0..config.asteroid_count {
            let per_seed = config.seed.wrapping_add(i as u32 * 97 + 1);

            let (center, radius, flatten_y) = if i == 0 {
                // First asteroid always at origin (guaranteed spawn reference point).
                let r = (config.min_radius + config.max_radius) * 0.5;
                (Vec3::ZERO, r, 1.0)
            } else {
                // Hash the seed + index for each component.
                // Use different z-values to decorrelate x, y, z, radius, flatten.
                let hx = hash3(i as i32, 0, 0, per_seed);
                let hy = hash3(i as i32, 1, 0, per_seed);
                let hz = hash3(i as i32, 2, 0, per_seed);
                let hr = hash3(i as i32, 3, 0, per_seed);

                let spread_xz = config.max_radius * 10.0;
                let spread_y = config.max_radius * 5.0;

                let x = (hx * 2.0 - 1.0) * spread_xz;
                let y = (hy * 2.0 - 1.0) * spread_y;
                let z = (hz * 2.0 - 1.0) * spread_xz;

                let r = config.min_radius + hr * (config.max_radius - config.min_radius);

                (Vec3::new(x, y, z), r, 1.0_f32)
            };

            asteroids.push(Asteroid {
                center,
                radius,
                seed: per_seed,
                flatten_y,
            });
        }

        asteroids
    }

    /// Determines the mineral type for a solid voxel based on noise at the
    /// given world position.
    ///
    /// Uses three independent FBM noise passes with different seeds to create
    /// natural-looking mineral veins. The thresholds are based on the
    /// architecture specification.
    #[inline]
    fn determine_mineral(world_pos: Vec3, asteroid_seed: u32) -> DefaultVoxel {
        // Mineral vein FBM parameters (from ARCHITECTURE.md):
        // 3 octaves, frequency 5.0, lacunarity 2.0, gain 0.5
        let mineral_octaves = 3;
        let mineral_freq = 5.0;
        let lacunarity = 2.0;
        let gain = 0.5;

        // Three independent noise passes with different seeds
        let rare_noise = fbm(
            world_pos.x,
            world_pos.y,
            world_pos.z,
            asteroid_seed.wrapping_add(100),
            mineral_octaves,
            mineral_freq,
            lacunarity,
            gain,
        );
        let crystal_noise = fbm(
            world_pos.x,
            world_pos.y,
            world_pos.z,
            asteroid_seed.wrapping_add(200),
            mineral_octaves,
            mineral_freq,
            lacunarity,
            gain,
        );
        let iron_noise = fbm(
            world_pos.x,
            world_pos.y,
            world_pos.z,
            asteroid_seed.wrapping_add(300),
            mineral_octaves,
            mineral_freq,
            lacunarity,
            gain,
        );

        // FBM returns [-1, 1]; thresholds from ARCHITECTURE.md:
        // rare_noise > 0.55 => RareMineral (~5%)
        // crystal_noise > 0.30 => SiliconCrystal (~15%)
        // iron_noise > 0.20 => IronOre (~25%)
        // default => Rock (~55%)
        if rare_noise > 0.55 {
            DefaultVoxel::RareMineral
        } else if crystal_noise > 0.30 {
            DefaultVoxel::SiliconCrystal
        } else if iron_noise > 0.20 {
            DefaultVoxel::IronOre
        } else {
            DefaultVoxel::Rock
        }
    }
}

impl ChunkGenerator<DefaultVoxel> for AsteroidGenerator {
    fn generate(&self, coord: IVec3, chunk: &mut Chunk<DefaultVoxel>) {
        let chunk_world = coord.as_vec3() * CHUNK_SIZE as f32;

        for lx in 0..CHUNK_SIZE {
            for ly in 0..CHUNK_SIZE {
                for lz in 0..CHUNK_SIZE {
                    let world_pos = chunk_world
                        + Vec3::new(lx as f32 + 0.5, ly as f32 + 0.5, lz as f32 + 0.5);

                    for asteroid in &self.asteroids {
                        let dx = world_pos.x - asteroid.center.x;
                        let dy = (world_pos.y - asteroid.center.y) / asteroid.flatten_y;
                        let dz = world_pos.z - asteroid.center.z;
                        let dist = (dx * dx + dy * dy + dz * dz).sqrt();

                        // Quick reject: if too far, skip noise calculation.
                        // Max surface extent is radius + radius * amplitude.
                        let max_extent =
                            asteroid.radius + asteroid.radius * self.config.noise_amplitude;
                        if dist > max_extent + 1.0 {
                            continue;
                        }

                        // Normalized coordinates for FBM sampling
                        let norm = world_pos / (asteroid.radius * 2.0);

                        let surface_noise = fbm(
                            norm.x,
                            norm.y,
                            norm.z,
                            asteroid.seed,
                            self.config.noise_octaves,
                            self.config.noise_frequency,
                            2.0, // lacunarity
                            0.5, // gain
                        ) * asteroid.radius
                            * self.config.noise_amplitude;

                        let density = asteroid.radius - dist + surface_noise;

                        if density > 0.0 {
                            let voxel = Self::determine_mineral(world_pos, asteroid.seed);
                            chunk.set(lx, ly, lz, voxel);
                            // First asteroid wins for this voxel position
                            break;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_generation() {
        let config = AsteroidConfig::default();
        let generator = AsteroidGenerator::new(config.clone());

        let coord = IVec3::new(0, 0, 0);

        let mut chunk_a = Chunk::<DefaultVoxel>::new();
        generator.generate(coord, &mut chunk_a);

        let mut chunk_b = Chunk::<DefaultVoxel>::new();
        generator.generate(coord, &mut chunk_b);

        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    assert_eq!(
                        chunk_a.get(x, y, z),
                        chunk_b.get(x, y, z),
                        "Voxel mismatch at ({x}, {y}, {z}): generation must be deterministic"
                    );
                }
            }
        }
    }

    #[test]
    fn empty_space_produces_air() {
        let config = AsteroidConfig {
            asteroid_count: 1,
            min_radius: 10.0,
            max_radius: 10.0,
            ..AsteroidConfig::default()
        };
        let generator = AsteroidGenerator::new(config);

        // Chunk very far from the origin asteroid (which is at Vec3::ZERO)
        let coord = IVec3::new(1000, 1000, 1000);
        let mut chunk = Chunk::<DefaultVoxel>::new();
        generator.generate(coord, &mut chunk);

        assert!(
            chunk.is_empty(),
            "Chunk far from any asteroid should be entirely air"
        );
    }

    #[test]
    fn asteroid_interior_has_solid_voxels() {
        let config = AsteroidConfig {
            asteroid_count: 1,
            min_radius: 30.0,
            max_radius: 30.0,
            ..AsteroidConfig::default()
        };
        let generator = AsteroidGenerator::new(config);

        // Chunk at origin — center of the first asteroid
        let coord = IVec3::new(0, 0, 0);
        let mut chunk = Chunk::<DefaultVoxel>::new();
        generator.generate(coord, &mut chunk);

        let solid_count = (0..CHUNK_SIZE)
            .flat_map(|x| {
                (0..CHUNK_SIZE)
                    .flat_map(move |y| (0..CHUNK_SIZE).map(move |z| (x, y, z)))
            })
            .filter(|&(x, y, z)| chunk.is_solid_at(x, y, z))
            .count();

        assert!(
            solid_count > 0,
            "Chunk at asteroid center should contain solid voxels, but found none"
        );
        // The center of a radius-30 asteroid should have many solid voxels
        assert!(
            solid_count > CHUNK_SIZE * CHUNK_SIZE,
            "Expected substantial solid voxels at asteroid center, got only {solid_count}"
        );
    }

    #[test]
    fn mineral_distribution_has_variety() {
        let config = AsteroidConfig {
            asteroid_count: 1,
            min_radius: 40.0,
            max_radius: 40.0,
            ..AsteroidConfig::default()
        };
        let generator = AsteroidGenerator::new(config);

        // Generate several chunks around the asteroid center to get a good sample
        let mut has_rock = false;
        let mut has_iron = false;
        let mut has_silicon = false;
        let mut has_rare = false;

        for cx in -1..=1 {
            for cy in -1..=1 {
                for cz in -1..=1 {
                    let coord = IVec3::new(cx, cy, cz);
                    let mut chunk = Chunk::<DefaultVoxel>::new();
                    generator.generate(coord, &mut chunk);

                    for x in 0..CHUNK_SIZE {
                        for y in 0..CHUNK_SIZE {
                            for z in 0..CHUNK_SIZE {
                                match chunk.get(x, y, z) {
                                    DefaultVoxel::Rock => has_rock = true,
                                    DefaultVoxel::IronOre => has_iron = true,
                                    DefaultVoxel::SiliconCrystal => has_silicon = true,
                                    DefaultVoxel::RareMineral => has_rare = true,
                                    DefaultVoxel::Air => {}
                                }
                            }
                        }
                    }
                }
            }
        }

        assert!(has_rock, "Should contain Rock voxels");
        assert!(has_iron, "Should contain IronOre voxels");
        assert!(has_silicon, "Should contain SiliconCrystal voxels");
        assert!(has_rare, "Should contain RareMineral voxels");
    }
}
