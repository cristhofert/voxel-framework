//! Asteroid chunk generator for [`DefaultVoxel`].
//!
//! Produces asteroid-shaped voxel formations using FBM noise for surface
//! deformation and mineral vein distribution. Asteroid positions and radii
//! are deterministically derived from the seed using the noise module's
//! [`hash3`] function (no `rand` crate dependency).

use bevy::math::{IVec3, Vec3};

use crate::chunk::Chunk;
use crate::chunk::CHUNK_SIZE;
use crate::noise::{fbm, hash3};
use crate::voxel::DefaultVoxel;

use super::ChunkGenerator;

/// Distribution frequencies for mineral types.
///
/// Values should sum to approximately 1.0. Each value represents the
/// cumulative probability band for that mineral type.
#[derive(Clone, Debug)]
pub struct MineralFrequencies {
    /// Frequency of rare mineral nodes (default: 0.05).
    pub rare_mineral: f32,
    /// Frequency of silicon crystal veins (default: 0.15).
    pub silicon_crystal: f32,
    /// Frequency of iron ore deposits (default: 0.25).
    pub iron_ore: f32,
    /// Frequency of common rock (default: 0.55).
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

/// Configuration for asteroid generation.
///
/// Controls the number, size, and noise parameters of generated asteroids.
/// All values are deterministic given the same seed.
#[derive(Clone, Debug)]
pub struct AsteroidConfig {
    /// Global seed for deterministic generation.
    pub seed: u32,
    /// Number of asteroids to generate.
    pub asteroid_count: usize,
    /// Minimum asteroid radius in world units.
    pub min_radius: f32,
    /// Maximum asteroid radius in world units.
    pub max_radius: f32,
    /// Number of FBM octaves for surface deformation (typically 4).
    pub noise_octaves: u32,
    /// Base frequency for surface FBM noise (typically 2.5).
    pub noise_frequency: f32,
    /// Surface noise amplitude as a fraction of radius (typically 0.34).
    pub noise_amplitude: f32,
    /// Mineral distribution frequencies.
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

/// Procedural asteroid generator for [`DefaultVoxel`].
///
/// Creates asteroid-shaped voxel formations with FBM-deformed surfaces and
/// noise-driven mineral distribution. Asteroid positions are deterministically
/// computed from the seed at construction time.
pub struct AsteroidGenerator {
    config: AsteroidConfig,
    asteroids: Vec<Asteroid>,
}

impl AsteroidGenerator {
    /// Creates a new asteroid generator with the given configuration.
    ///
    /// Asteroid positions and radii are computed deterministically from the
    /// config seed using hash-based pseudo-random generation (no `rand` crate).
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
                let hf = hash3(i as i32, 4, 0, per_seed);

                let spread_xz = config.max_radius * 10.0;
                let spread_y = config.max_radius * 5.0;

                let x = (hx * 2.0 - 1.0) * spread_xz;
                let y = (hy * 2.0 - 1.0) * spread_y;
                let z = (hz * 2.0 - 1.0) * spread_xz;

                let r = config.min_radius + hr * (config.max_radius - config.min_radius);
                let flatten = 0.5 + hf * 0.5; // [0.5, 1.0]

                (Vec3::new(x, y, z), r, flatten)
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
