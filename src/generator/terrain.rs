//! Procedural heightmap terrain generator for [`DefaultTerrainVoxel`].
//!
//! `TerrainGenerator` uses fractal Brownian motion (FBM) noise to build a
//! classic layered terrain: grass on top, dirt below, stone underneath, and
//! sand at low elevations near the water line.
//!
//! # Integration
//!
//! ```no_run
//! use std::sync::Arc;
//! use bevy::prelude::*;
//! use voxel_framework::{
//!     VoxelPlugin,
//!     generator::{GeneratorPlugin, GeneratorResource},
//!     generator::terrain::{TerrainConfig, TerrainGenerator},
//!     voxel::DefaultTerrainVoxel,
//! };
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(VoxelPlugin::<DefaultTerrainVoxel>::new())
//!     .add_plugins(GeneratorPlugin::<DefaultTerrainVoxel>::new())
//!     .insert_resource(GeneratorResource::<DefaultTerrainVoxel> {
//!         generator: Arc::new(TerrainGenerator::new(TerrainConfig::default())),
//!     })
//!     .run();
//! ```

use bevy::math::IVec3;

use crate::chunk::{CHUNK_SIZE, Chunk};
use crate::noise::{fbm, hash3};
use crate::voxel::DefaultTerrainVoxel;

use super::ChunkGenerator;

/// Configuration for [`TerrainGenerator`].
///
/// Controls the overall shape and scale of the generated terrain. All fields
/// have sensible defaults via [`Default`]; call [`TerrainConfig::default`] for
/// a ready-to-use configuration.
#[derive(Debug, Clone)]
pub struct TerrainConfig {
    /// Seed value passed to the FBM noise function.
    ///
    /// Different seeds produce entirely different terrain layouts.
    /// **Default: `42`.**
    pub seed: u32,
    /// Maximum terrain height in world units.
    ///
    /// The FBM output (normalized to `[0, 1]`) is multiplied by this value to
    /// produce the surface Y coordinate. **Default: `48.0`.**
    pub height_scale: f32,
    /// Base frequency of the FBM noise.
    ///
    /// Lower values produce broader, smoother hills; higher values produce
    /// jagged, fine-grained terrain. **Default: `0.008`.**
    pub frequency: f32,
    /// World Y below which sand replaces the normal grass/dirt surface.
    ///
    /// Voxels at or below this threshold and within the surface layers are
    /// filled with `Sand` instead, simulating a beach or desert floor.
    /// **Default: `5.0`.**
    pub sand_level: f32,
    /// World Y up to which air voxels at or below the surface are filled with
    /// [`DefaultTerrainVoxel::Water`].
    ///
    /// Columns whose surface height is at or below this level get their
    /// above-surface air replaced with water up to `water_level`. Set to a
    /// negative value to disable water entirely. **Default: `4.0`.**
    pub water_level: f32,
    /// Whether to place procedural trees on grass surfaces.
    ///
    /// When `true`, roughly 1 in 20 grass-topped columns receive a tree (trunk
    /// + leaf crown). **Default: `true`.**
    pub trees_enabled: bool,
    /// Whether to vary terrain surface type and height by biome.
    ///
    /// When `true`, a low-frequency noise layer assigns one of four biomes
    /// (Desert, Plains, Forest, Snow) per region. Each biome changes the
    /// surface voxel, height scale multiplier, and tree density.
    /// **Default: `true`.**
    pub biomes_enabled: bool,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            height_scale: 48.0,
            frequency: 0.008,
            sand_level: 5.0,
            water_level: 4.0,
            trees_enabled: true,
            biomes_enabled: true,
        }
    }
}

/// Procedural terrain generator that fills chunks using FBM heightmap data.
///
/// For each XZ column the generator computes a surface height from
/// [`fbm`] noise and layers voxel materials as follows:
///
/// | World Y relative to surface | Material |
/// |---|---|
/// | Above surface | `Air` |
/// | Top layer (surface - 1) | `Grass` (or `Sand` below `sand_level`) |
/// | Next 4 layers | `Dirt` (or `Sand` below `sand_level`) |
/// | Anything deeper | `Stone` |
///
/// The generator implements [`ChunkGenerator<DefaultTerrainVoxel>`] and can be
/// stored in a [`crate::generator::GeneratorResource`].
pub struct TerrainGenerator {
    config: TerrainConfig,
}

impl TerrainGenerator {
    /// Creates a new generator with the given configuration.
    pub fn new(config: TerrainConfig) -> Self {
        Self { config }
    }
}

// ─── Biome helpers ────────────────────────────────────────────────────────────

/// Biome type for a terrain column.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Biome {
    /// Flat sandy desert with no trees and lower height variation.
    Desert,
    /// Grassy plains with occasional trees and moderate height.
    Plains,
    /// Dense forest with many trees and taller, rougher hills.
    Forest,
    /// Snowy peaks with no trees and the tallest height scale.
    Snow,
}

impl Biome {
    /// Returns the height scale multiplier for this biome (applied on top of
    /// [`TerrainConfig::height_scale`]).
    fn height_multiplier(self) -> f32 {
        match self {
            Biome::Desert => 0.5,
            Biome::Plains => 0.8,
            Biome::Forest => 1.0,
            Biome::Snow => 1.4,
        }
    }

    /// Returns the surface and sub-surface voxel for this biome.
    /// `(surface_top, sub_surface)` — top layer and the 4 layers below it.
    fn surface_voxels(self) -> (DefaultTerrainVoxel, DefaultTerrainVoxel) {
        match self {
            Biome::Desert => (DefaultTerrainVoxel::Sand, DefaultTerrainVoxel::Sand),
            Biome::Plains => (DefaultTerrainVoxel::Grass, DefaultTerrainVoxel::Dirt),
            Biome::Forest => (DefaultTerrainVoxel::Grass, DefaultTerrainVoxel::Dirt),
            Biome::Snow => (DefaultTerrainVoxel::Snow, DefaultTerrainVoxel::Gravel),
        }
    }

    /// Returns whether trees should grow in this biome and the modulo divisor
    /// controlling density (lower = denser). `None` means no trees.
    fn tree_density(self) -> Option<u32> {
        match self {
            Biome::Desert => None,
            Biome::Plains => Some(30), // ~1 in 30 columns
            Biome::Forest => Some(8),  // ~1 in 8 columns — dense
            Biome::Snow => None,
        }
    }
}

/// Samples the biome at a world XZ coordinate using a very low-frequency FBM.
///
/// The biome noise uses `seed + 1000` so it is independent of the heightmap
/// noise. Returns one of four [`Biome`] variants.
fn sample_biome(wx: f32, wz: f32, seed: u32) -> Biome {
    // Very slow noise (0.001) so biome regions span ~300 world units each.
    let n = fbm(wx, 0.0, wz, seed.wrapping_add(1000), 2, 0.001, 2.0, 0.5);
    if n < -0.33 {
        Biome::Desert
    } else if n < 0.2 {
        Biome::Plains
    } else if n < 0.6 {
        Biome::Forest
    } else {
        Biome::Snow
    }
}

/// Places a tree centered at the given local column `(lx, lz)` with the trunk
/// base at local Y `trunk_base_ly`. Only voxels whose local Y falls within
/// `[0, CHUNK_SIZE)` are written; out-of-range positions are silently skipped.
/// Leaves do not overwrite non-air voxels so trunks and solid terrain win.
fn place_tree(chunk: &mut Chunk<DefaultTerrainVoxel>, lx: usize, lz: usize, trunk_base_ly: usize) {
    const TRUNK_HEIGHT: usize = 4;
    const CROWN_RADIUS: i32 = 2;
    const CROWN_HEIGHT: usize = 4;

    // Trunk: 4 Wood blocks stacked directly above the surface.
    for h in 0..TRUNK_HEIGHT {
        let ly = trunk_base_ly + h;
        if lx < CHUNK_SIZE && lz < CHUNK_SIZE && ly < CHUNK_SIZE {
            chunk.set(lx, ly, lz, DefaultTerrainVoxel::Wood);
        }
    }

    // Crown: Chebyshev-distance sphere of Leaves centered above the trunk top.
    let crown_center_ly = trunk_base_ly + TRUNK_HEIGHT;
    for dy in 0..CROWN_HEIGHT {
        for dx in -CROWN_RADIUS..=CROWN_RADIUS {
            for dz in -CROWN_RADIUS..=CROWN_RADIUS {
                let dist = dx.abs().max(dz.abs()).max(dy as i32 - 1);
                if dist > CROWN_RADIUS {
                    continue;
                }
                let cx = lx as i32 + dx;
                let cy = crown_center_ly as i32 + dy as i32;
                let cz = lz as i32 + dz;
                if cx < 0 || cy < 0 || cz < 0 {
                    continue;
                }
                let (cx, cy, cz) = (cx as usize, cy as usize, cz as usize);
                if cx < CHUNK_SIZE
                    && cy < CHUNK_SIZE
                    && cz < CHUNK_SIZE
                    && chunk.get(cx, cy, cz) == DefaultTerrainVoxel::Air
                {
                    chunk.set(cx, cy, cz, DefaultTerrainVoxel::Leaves);
                }
            }
        }
    }
}

impl ChunkGenerator<DefaultTerrainVoxel> for TerrainGenerator {
    fn generate(&self, coord: IVec3, chunk: &mut Chunk<DefaultTerrainVoxel>) {
        let cfg = &self.config;
        let chunk_world_y = coord.y * CHUNK_SIZE as i32;

        // Track per-column surface info for the tree pass.
        let mut surface_info: Vec<Option<(usize, bool)>> = vec![None; CHUNK_SIZE * CHUNK_SIZE];

        for lx in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                let wx = (coord.x * CHUNK_SIZE as i32 + lx as i32) as f32;
                let wz = (coord.z * CHUNK_SIZE as i32 + lz as i32) as f32;

                // Biome determines height multiplier and surface materials.
                let biome = if cfg.biomes_enabled {
                    sample_biome(wx, wz, cfg.seed)
                } else {
                    Biome::Plains
                };
                let effective_height = cfg.height_scale * biome.height_multiplier();

                // FBM output is in [-1, 1] — normalize to [0, 1] then scale.
                let noise = fbm(wx, 0.0, wz, cfg.seed, 4, cfg.frequency, 2.0, 0.5);
                let surface_y = (noise * 0.5 + 0.5) * effective_height;

                // Override surface voxels based on biome (sand_level still applies).
                let (top_voxel, sub_voxel) = if surface_y <= cfg.sand_level {
                    (DefaultTerrainVoxel::Sand, DefaultTerrainVoxel::Sand)
                } else {
                    biome.surface_voxels()
                };

                let can_have_trees = cfg.trees_enabled
                    && top_voxel == DefaultTerrainVoxel::Grass
                    && biome.tree_density().is_some();

                for ly in 0..CHUNK_SIZE {
                    let wy = (chunk_world_y + ly as i32) as f32;

                    let voxel = if wy >= surface_y {
                        // Above terrain surface — fill with water if below water level.
                        if wy < cfg.water_level {
                            DefaultTerrainVoxel::Water
                        } else {
                            DefaultTerrainVoxel::Air
                        }
                    } else if wy >= surface_y - 1.0 {
                        top_voxel
                    } else if wy >= surface_y - 5.0 {
                        sub_voxel
                    } else {
                        DefaultTerrainVoxel::Stone
                    };

                    chunk.set(lx, ly, lz, voxel);
                }

                // Record surface for tree pass.
                if can_have_trees {
                    let surface_world_ly = surface_y.ceil() as i32 - chunk_world_y;
                    if surface_world_ly >= 0 && surface_world_ly < CHUNK_SIZE as i32 {
                        surface_info[lz * CHUNK_SIZE + lx] =
                            Some((surface_world_ly as usize, true));
                    }
                }
            }
        }

        // Tree placement pass.
        if cfg.trees_enabled {
            for lx in 0..CHUNK_SIZE {
                for lz in 0..CHUNK_SIZE {
                    if let Some((trunk_base_ly, _)) = surface_info[lz * CHUNK_SIZE + lx] {
                        let wx_i = coord.x * CHUNK_SIZE as i32 + lx as i32;
                        let wz_i = coord.z * CHUNK_SIZE as i32 + lz as i32;
                        let wx = wx_i as f32;
                        let wz = wz_i as f32;
                        let biome = if cfg.biomes_enabled {
                            sample_biome(wx, wz, cfg.seed)
                        } else {
                            Biome::Plains
                        };
                        if let Some(density) = biome.tree_density() {
                            let h =
                                ((hash3(wx_i, 0, wz_i, cfg.seed) * 0.5 + 0.5) * 1_000_000.0) as u32;
                            if h % density == 0 {
                                place_tree(chunk, lx, lz, trunk_base_ly);
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::VoxelType;

    fn make_gen() -> TerrainGenerator {
        TerrainGenerator::new(TerrainConfig::default())
    }

    #[test]
    fn test_terrain_surface_chunk_has_grass() {
        let tgen = make_gen();
        // Chunk at y=1 should contain surface-level voxels (height_scale=48,
        // so surface is in [0, 48] ⊆ chunks y=0..y=3).
        let mut chunk = Chunk::new();
        tgen.generate(IVec3::new(0, 1, 0), &mut chunk);
        // There should be at least some non-air voxels.
        let solid_count = (0..CHUNK_SIZE)
            .flat_map(|x| {
                (0..CHUNK_SIZE).flat_map(move |y| (0..CHUNK_SIZE).map(move |z| (x, y, z)))
            })
            .filter(|&(x, y, z)| chunk.get(x, y, z).is_solid())
            .count();
        assert!(solid_count > 0, "surface chunk must have solid voxels");
    }

    #[test]
    fn test_terrain_deep_chunk_is_all_stone() {
        let tgen = make_gen();
        // Chunk at y=-10 is far below any surface (height_scale=48, min surface=0).
        let mut chunk = Chunk::new();
        tgen.generate(IVec3::new(0, -10, 0), &mut chunk);
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    assert_eq!(
                        chunk.get(x, y, z),
                        DefaultTerrainVoxel::Stone,
                        "deep chunk voxel ({x},{y},{z}) should be Stone"
                    );
                }
            }
        }
    }

    #[test]
    fn test_terrain_high_chunk_is_all_air() {
        let tgen = make_gen();
        // Chunk at y=10 is above max terrain height (height_scale=48, max=48 < 160).
        let mut chunk = Chunk::new();
        tgen.generate(IVec3::new(0, 10, 0), &mut chunk);
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    assert_eq!(
                        chunk.get(x, y, z),
                        DefaultTerrainVoxel::Air,
                        "high chunk voxel ({x},{y},{z}) should be Air"
                    );
                }
            }
        }
    }

    #[test]
    fn test_terrain_deterministic() {
        let tgen = make_gen();
        let mut a = Chunk::new();
        let mut b = Chunk::new();
        tgen.generate(IVec3::new(3, 1, -2), &mut a);
        tgen.generate(IVec3::new(3, 1, -2), &mut b);
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    assert_eq!(
                        a.get(x, y, z),
                        b.get(x, y, z),
                        "same coord must produce identical voxels"
                    );
                }
            }
        }
    }
}
