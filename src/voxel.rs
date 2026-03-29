//! Voxel type trait, the built-in [`DefaultVoxel`] palette, and runtime color lookup.
//!
//! ## Overview
//!
//! Every material in the voxel world is represented by a value type that
//! implements [`VoxelType`]. The trait is intentionally minimal: it describes
//! whether a voxel occupies space, what color it renders as, and how it maps
//! to a single byte for binary persistence. Keeping the interface small lets
//! implementors choose any internal representation — a simple enum, a newtype
//! around a `u8`, or a richer struct.
//!
//! ## The `DefaultVoxel` palette
//!
//! [`DefaultVoxel`] is the built-in five-material palette used by the reference
//! implementation and all bundled generators. It is the `V` parameter used by
//! [`crate::VoxelCorePlugin`] and [`crate::VoxelPlugin`]. You can swap it for a
//! custom type wherever the framework is generic over `V: VoxelType`.
//!
//! ## Serialization contract
//!
//! The [`VoxelType::to_byte`] / [`VoxelType::from_byte`] pair forms a stable
//! binary serialization contract used by [`crate::chunk::persistence`]. Two
//! invariants must hold:
//!
//! 1. `from_byte(v.to_byte()) == v` for every non-air voxel.
//! 2. Unknown bytes must round-trip to [`VoxelType::air()`] — never panic.
//!
//! Changing the byte mapping of an existing palette breaks saved worlds.

use std::collections::HashMap;
use std::hash::Hash;

use bevy::prelude::Resource;

/// The interface every voxel material type must satisfy.
///
/// `VoxelType` is the central abstraction of the framework. It is the `V`
/// parameter on [`crate::chunk::Chunk<V>`], [`crate::generator::GeneratorPlugin`],
/// [`crate::mesh::MeshPlugin`], and [`crate::render::RenderPlugin`]. Any type
/// that implements this trait can drive the full chunk pipeline.
///
/// Implementors must be **lightweight value types** because one instance is
/// stored per voxel in a [`crate::chunk::Chunk`]
/// (4 096 voxels per 16×16×16 chunk). Heap allocations inside a `VoxelType`
/// value are strongly discouraged.
///
/// The required bounds (`Copy`, `Clone`, `Default`, `Send`, `Sync`,
/// `PartialEq`, `Eq`, `Hash`, `'static`) exist because:
/// - `Copy` / `Clone` — voxel values are read from and written to flat arrays
///   by value without reference counting.
/// - `Default` — used to initialize freshly allocated chunks and by [`air`].
/// - `Send + Sync + 'static` — chunks are generated on a background thread via
///   Bevy's `AsyncComputeTaskPool`.
/// - `Eq + Hash` — required by [`VoxelPalette`]'s `HashMap<V, [f32; 4]>`.
///
/// # Examples
///
/// Implementing `VoxelType` for a two-material palette:
///
/// ```rust,no_run
/// use voxel_framework::voxel::VoxelType;
///
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
/// pub enum MyVoxel { #[default] Air, Stone }
///
/// impl VoxelType for MyVoxel {
///     fn is_solid(&self) -> bool { !matches!(self, MyVoxel::Air) }
///     fn color(&self) -> [f32; 4] {
///         match self {
///             MyVoxel::Air   => [0.0, 0.0, 0.0, 0.0],
///             MyVoxel::Stone => [0.5, 0.5, 0.5, 1.0],
///         }
///     }
///     fn to_byte(&self) -> u8 { match self { MyVoxel::Air => 0, MyVoxel::Stone => 1 } }
///     fn from_byte(b: u8) -> Self { match b { 1 => MyVoxel::Stone, _ => MyVoxel::Air } }
/// }
/// ```
///
/// # See also
///
/// - [`DefaultVoxel`] — built-in five-material implementation.
/// - [`VoxelPalette`] — runtime RGBA overrides keyed by `VoxelType`.
///
/// [`air`]: VoxelType::air
pub trait VoxelType: Copy + Clone + Default + Send + Sync + PartialEq + Eq + Hash + 'static {
    /// Returns `true` if this voxel occupies space and should produce mesh geometry.
    ///
    /// The meshing system skips faces between two adjacent solid voxels, so
    /// this predicate directly controls which faces are emitted. Returning
    /// `false` for transparent or non-solid materials (e.g. glass, water) is
    /// valid and causes those faces to be emitted on neighbouring solid voxels.
    ///
    /// The default contract is: [`air()`] returns `false`, all other variants
    /// return `true`.
    ///
    /// [`air()`]: VoxelType::air
    fn is_solid(&self) -> bool;

    /// Returns the RGBA color of this voxel as linear floating-point values
    /// in the range `[0.0, 1.0]` per channel.
    ///
    /// The returned value is used both by [`VoxelPalette`] as the fallback
    /// when no palette override exists, and directly by the greedy meshing
    /// algorithm to assign per-face vertex colors.
    ///
    /// [`air()`] should return `[0.0, 0.0, 0.0, 0.0]` (transparent black).
    ///
    /// [`air()`]: VoxelType::air
    fn color(&self) -> [f32; 4];

    /// Serializes this voxel to a single byte for binary chunk persistence.
    ///
    /// The mapping must be **stable across releases**: changing an existing
    /// variant's byte value breaks saved chunk data. New variants should use
    /// previously unused byte values.
    ///
    /// The invariant `from_byte(v.to_byte()) == v` must hold for every valid
    /// voxel value. [`air()`] must serialize to `0`.
    ///
    /// [`air()`]: VoxelType::air
    fn to_byte(&self) -> u8;

    /// Deserializes a voxel from a single byte read out of a chunk file.
    ///
    /// Unknown byte values (those not in the current palette) **must** map to
    /// [`air()`] rather than panicking, so that worlds saved with a newer
    /// palette version can still be read by an older build.
    ///
    /// [`air()`]: VoxelType::air
    fn from_byte(byte: u8) -> Self;

    /// Returns the hardness multiplier applied to the base mining break time.
    ///
    /// The effective break time is `config.break_time * voxel.hardness()`.
    /// `0.0` prevents mining (the voxel is unbreakable).
    ///
    /// Default: `1.0` — inherits the global [`crate::world::mining::MiningConfig::break_time`]
    /// unchanged.
    fn hardness(&self) -> f32 {
        1.0
    }

    /// Minimum tool level required to break this voxel.
    ///
    /// | Level | Example tool |
    /// |-------|-------------|
    /// | `0`   | Bare hands / any tool |
    /// | `1`   | Stone pickaxe or better |
    /// | `2`   | Iron pickaxe or better |
    /// | `3`   | Diamond pickaxe |
    ///
    /// If the player's active tool level is below this value, mining makes
    /// no progress (effective break time is treated as infinite).
    ///
    /// Default: `0` — breakable with bare hands.
    fn mining_level(&self) -> u8 {
        0
    }

    /// Returns the texture array layer index for this voxel on the given face.
    ///
    /// Face index convention (matches `FACE_DIRS` in the greedy mesher):
    /// - `0` = +Y (top)
    /// - `1` = -Y (bottom)
    /// - `2` = +X
    /// - `3` = -X
    /// - `4` = +Z
    /// - `5` = -Z
    ///
    /// The returned value selects which layer of the GPU texture array is
    /// sampled by the `ChunkMaterial` shader. Layer indices correspond to the
    /// order in which textures are assembled into the array by the game.
    ///
    /// Default: all faces use layer `0`.
    fn texture_layer(&self, _face_index: usize) -> u32 {
        0
    }

    /// Returns the texture array layer index for this voxel on the given face,
    /// taking into account the block's placement state.
    ///
    /// `face_index` follows the same convention as [`texture_layer`]:
    /// - `0` = +Y (top), `1` = -Y (bottom), `2` = +X, `3` = -X, `4` = +Z, `5` = -Z
    ///
    /// `state` is the raw byte stored in [`crate::chunk::Chunk::get_state`].
    /// Bits 0–2 hold the placement face (see [`crate::chunk::face_to_state`]);
    /// bits 3–7 are available for game-specific use.
    ///
    /// Override this method to make a block render differently depending on
    /// which face it was placed against (e.g. a log block that shows end-grain
    /// on the faces perpendicular to its axis).
    ///
    /// Default: delegates to [`texture_layer`], ignoring `state`.
    ///
    /// [`texture_layer`]: VoxelType::texture_layer
    #[inline]
    fn texture_layer_for_state(&self, face_index: usize, _state: u8) -> u32 {
        self.texture_layer(face_index)
    }

    /// Returns the empty/air voxel for this type.
    ///
    /// The default implementation delegates to `Self::default()`, which is
    /// correct as long as `Default` is derived with `#[default]` on the air
    /// variant. Override this method only if air is not the default variant.
    fn air() -> Self {
        Self::default()
    }

    /// Returns the movement modifier applied to the player while standing on
    /// this voxel type.
    ///
    /// The modifier scales speed, jump height, gravity, and friction
    /// independently. All fields default to `1.0` (identity — no change).
    /// Override for voxel types that should slow, accelerate, or alter
    /// jump/friction behaviour.
    fn movement_modifier(&self) -> MovementModifier {
        MovementModifier::default()
    }

    /// Returns the opacity of this voxel (1.0 = fully opaque, 0.0 = invisible).
    /// Used by the mesh generator to set vertex color alpha.
    #[inline]
    fn alpha(&self) -> f32 {
        1.0
    }
}

/// Per-surface movement modifiers applied by [`VoxelType::movement_modifier`].
///
/// Each field is a multiplier applied to the corresponding movement parameter
/// while the player stands on the voxel. A value of `1.0` is the identity
/// (no change). Values below `1.0` reduce; values above `1.0` amplify.
///
/// | Field | Applied to |
/// |-------|-----------|
/// | `speed_mult`   | Horizontal target speed |
/// | `jump_mult`    | Jump impulse |
/// | `gravity_mult` | Per-frame gravity accumulation |
/// | `friction_mult`| Deceleration lerp factor (`DECEL * friction_mult`) |
///
/// # Convention
/// - `friction_mult < 1.0` → slippery (less decel, e.g. Ice)
/// - `speed_mult < 1.0` → movement slowed (e.g. Water, Snow)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementModifier {
    pub speed_mult: f32,
    pub jump_mult: f32,
    pub gravity_mult: f32,
    pub friction_mult: f32,
}

impl Default for MovementModifier {
    fn default() -> Self {
        Self { speed_mult: 1.0, jump_mult: 1.0, gravity_mult: 1.0, friction_mult: 1.0 }
    }
}

/// The built-in five-material voxel palette used by all bundled systems.
///
/// `DefaultVoxel` is the concrete `VoxelType` implementation that the bundled
/// plugins and generators operate on. It covers the five materials defined in
/// the framework's reference specification and is ready to use without any
/// additional configuration.
///
/// When you want a custom material set, implement [`VoxelType`] on your own
/// enum and substitute it wherever the framework is generic over `V: VoxelType`.
///
/// # Byte mapping (persistence contract)
///
/// | Variant | `to_byte` |
/// |---------|-----------|
/// | `Air` | `0` |
/// | `Rock` | `1` |
/// | `IronOre` | `2` |
/// | `SiliconCrystal` | `3` |
/// | `RareMineral` | `4` |
///
/// Do not change these values; doing so breaks any saved chunk data.
///
/// # See also
///
/// - [`VoxelType`] — the trait this enum implements.
/// - [`VoxelPalette`] — runtime RGBA overrides for individual variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DefaultVoxel {
    /// Empty space — not solid, produces no geometry.
    ///
    /// All voxels in a freshly allocated [`crate::chunk::Chunk`] start as
    /// `Air`. Serializes to byte `0`.
    #[default]
    Air,
    /// Common bulk rock — warm grey-brown (`#5966648`).
    ///
    /// The most abundant material in asteroid and terrain generators.
    /// Linear RGBA: `[0.35, 0.30, 0.28, 1.0]`.
    Rock,
    /// Iron ore deposit — rusty orange.
    ///
    /// Appears as scattered veins inside rock masses.
    /// Linear RGBA: `[0.55, 0.35, 0.20, 1.0]`.
    IronOre,
    /// Silicon crystal vein — cool blue-violet.
    ///
    /// Typically rarer than iron ore; used as a crafting input.
    /// Linear RGBA: `[0.40, 0.50, 0.80, 1.0]`.
    SiliconCrystal,
    /// Rare mineral node — vivid teal-green.
    ///
    /// The rarest material; intended for high-value resource mechanics.
    /// Linear RGBA: `[0.20, 0.85, 0.60, 1.0]`.
    RareMineral,
}

impl VoxelType for DefaultVoxel {
    #[inline]
    fn is_solid(&self) -> bool {
        !matches!(self, DefaultVoxel::Air)
    }

    #[inline]
    fn color(&self) -> [f32; 4] {
        match self {
            DefaultVoxel::Air => [0.0, 0.0, 0.0, 0.0],
            DefaultVoxel::Rock => [0.35, 0.30, 0.28, 1.0],
            DefaultVoxel::IronOre => [0.55, 0.35, 0.20, 1.0],
            DefaultVoxel::SiliconCrystal => [0.40, 0.50, 0.80, 1.0],
            DefaultVoxel::RareMineral => [0.20, 0.85, 0.60, 1.0],
        }
    }

    #[inline]
    fn to_byte(&self) -> u8 {
        match self {
            DefaultVoxel::Air => 0,
            DefaultVoxel::Rock => 1,
            DefaultVoxel::IronOre => 2,
            DefaultVoxel::SiliconCrystal => 3,
            DefaultVoxel::RareMineral => 4,
        }
    }

    #[inline]
    fn from_byte(byte: u8) -> Self {
        match byte {
            0 => DefaultVoxel::Air,
            1 => DefaultVoxel::Rock,
            2 => DefaultVoxel::IronOre,
            3 => DefaultVoxel::SiliconCrystal,
            4 => DefaultVoxel::RareMineral,
            _ => DefaultVoxel::Air,
        }
    }

    #[inline]
    fn hardness(&self) -> f32 {
        match self {
            DefaultVoxel::Air            => 0.0,
            DefaultVoxel::Rock           => 3.0,
            DefaultVoxel::IronOre        => 4.0,
            DefaultVoxel::SiliconCrystal => 2.5,
            DefaultVoxel::RareMineral    => 5.0,
        }
    }

    #[inline]
    fn mining_level(&self) -> u8 {
        match self {
            DefaultVoxel::Air            => 0,
            DefaultVoxel::Rock           => 1,
            DefaultVoxel::IronOre        => 2,
            DefaultVoxel::SiliconCrystal => 1,
            DefaultVoxel::RareMineral    => 2,
        }
    }

    /// Returns the texture array layer index for this voxel.
    ///
    /// Layers align with the loading order in `basic_world`:
    /// - `0` = Air placeholder (grey)
    /// - `1` = Rock (stone.png)
    /// - `2` = IronOre (iron_ore.png)
    /// - `3` = SiliconCrystal (amethyst_block.png)
    /// - `4` = RareMineral (emerald_ore.png)
    #[inline]
    fn texture_layer(&self, _face_index: usize) -> u32 {
        match self {
            DefaultVoxel::Air => 0,
            DefaultVoxel::Rock => 1,
            DefaultVoxel::IronOre => 2,
            DefaultVoxel::SiliconCrystal => 3,
            DefaultVoxel::RareMineral => 4,
        }
    }
}

/// Built-in terrain voxel palette for Minetest/Minecraft-style worlds.
///
/// `DefaultTerrainVoxel` is a ten-material palette tuned for procedural
/// heightmap terrain. Use it with [`crate::generator::terrain::TerrainGenerator`]
/// and [`crate::VoxelPlugin::<DefaultTerrainVoxel>::new()`].
///
/// # Byte mapping (persistence contract)
///
/// | Variant | `to_byte` |
/// |---------|-----------|
/// | `Air`    | `0` |
/// | `Grass`  | `1` |
/// | `Dirt`   | `2` |
/// | `Stone`  | `3` |
/// | `Sand`   | `4` |
/// | `Wood`   | `5` |
/// | `Leaves` | `6` |
/// | `Gravel` | `7` |
/// | `Snow`        | `8`  |
/// | `Water`       | `9`  |
/// | `CoalOre`     | `10` |
/// | `IronOre`     | `11` |
/// | `Cobblestone` | `12` |
/// | `Sandstone`   | `13` |
/// | `Planks`      | `14` |
/// | `Brick`       | `15` |
///
/// Do not change these values; doing so breaks any saved chunk data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DefaultTerrainVoxel {
    /// Empty space — not solid, produces no geometry.
    #[default]
    Air,
    /// Grass-covered topsoil — vivid green.
    ///
    /// Top layer of terrain surface. Linear RGBA: `[0.30, 0.60, 0.20, 1.0]`.
    Grass,
    /// Dirt layer — warm brown.
    ///
    /// Found just below the grass layer. Linear RGBA: `[0.50, 0.35, 0.20, 1.0]`.
    Dirt,
    /// Stone — cool grey.
    ///
    /// Bulk underground material. Linear RGBA: `[0.50, 0.50, 0.50, 1.0]`.
    Stone,
    /// Sand — pale yellow.
    ///
    /// Replaces grass/dirt at very low elevations (beaches). Linear RGBA: `[0.85, 0.80, 0.50, 1.0]`.
    Sand,
    /// Tree trunk wood — brown.
    ///
    /// Used for procedurally generated tree trunks. Linear RGBA: `[0.45, 0.30, 0.18, 1.0]`.
    Wood,
    /// Tree foliage — green.
    ///
    /// Used for procedurally generated tree crowns. Linear RGBA: `[0.20, 0.55, 0.15, 1.0]`.
    Leaves,
    /// Gravel — mid grey.
    ///
    /// Loose stone aggregate. Linear RGBA: `[0.55, 0.52, 0.50, 1.0]`.
    Gravel,
    /// Snow — near-white.
    ///
    /// Surface cover at high elevations. Linear RGBA: `[0.92, 0.95, 0.98, 1.0]`.
    Snow,
    /// Water — blue, non-solid.
    ///
    /// Fills low-elevation air voxels up to the configured water level.
    /// Not solid — adjacent solid faces are still meshed. Linear RGBA: `[0.15, 0.40, 0.75, 1.0]`.
    Water,
    /// Coal ore — black speckled stone.
    ///
    /// Common underground fuel ore. Linear RGBA: `[0.30, 0.30, 0.30, 1.0]`.
    CoalOre,
    /// Iron ore — rusty brown stone.
    ///
    /// Iron-bearing rock formation. Linear RGBA: `[0.60, 0.45, 0.35, 1.0]`.
    IronOre,
    /// Cobblestone — rough gray stone.
    ///
    /// Processed stone aggregate. Linear RGBA: `[0.40, 0.40, 0.40, 1.0]`.
    Cobblestone,
    /// Sandstone — beige compacted stone.
    ///
    /// Compressed sand layers. Linear RGBA: `[0.80, 0.72, 0.45, 1.0]`.
    Sandstone,
    /// Planks — warm tan wood.
    ///
    /// Processed wood boards. Linear RGBA: `[0.65, 0.48, 0.28, 1.0]`.
    Planks,
    /// Brick — deep red fired clay.
    ///
    /// Kiln-fired clay building block. Linear RGBA: `[0.65, 0.28, 0.20, 1.0]`.
    Brick,
}

impl VoxelType for DefaultTerrainVoxel {
    #[inline]
    fn is_solid(&self) -> bool {
        !matches!(self, DefaultTerrainVoxel::Air | DefaultTerrainVoxel::Water)
    }

    #[inline]
    fn color(&self) -> [f32; 4] {
        match self {
            DefaultTerrainVoxel::Air    => [0.0,  0.0,  0.0,  0.0],
            DefaultTerrainVoxel::Grass  => [0.30, 0.60, 0.20, 1.0],
            DefaultTerrainVoxel::Dirt   => [0.50, 0.35, 0.20, 1.0],
            DefaultTerrainVoxel::Stone  => [0.50, 0.50, 0.50, 1.0],
            DefaultTerrainVoxel::Sand   => [0.85, 0.80, 0.50, 1.0],
            DefaultTerrainVoxel::Wood   => [0.45, 0.30, 0.18, 1.0],
            DefaultTerrainVoxel::Leaves => [0.20, 0.55, 0.15, 1.0],
            DefaultTerrainVoxel::Gravel => [0.55, 0.52, 0.50, 1.0],
            DefaultTerrainVoxel::Snow   => [0.92, 0.95, 0.98, 1.0],
            DefaultTerrainVoxel::Water       => [0.15, 0.40, 0.75, 0.7],
            DefaultTerrainVoxel::CoalOre     => [0.30, 0.30, 0.30, 1.0],
            DefaultTerrainVoxel::IronOre     => [0.60, 0.45, 0.35, 1.0],
            DefaultTerrainVoxel::Cobblestone => [0.40, 0.40, 0.40, 1.0],
            DefaultTerrainVoxel::Sandstone   => [0.80, 0.72, 0.45, 1.0],
            DefaultTerrainVoxel::Planks      => [0.65, 0.48, 0.28, 1.0],
            DefaultTerrainVoxel::Brick       => [0.65, 0.28, 0.20, 1.0],
        }
    }

    #[inline]
    fn to_byte(&self) -> u8 {
        match self {
            DefaultTerrainVoxel::Air    => 0,
            DefaultTerrainVoxel::Grass  => 1,
            DefaultTerrainVoxel::Dirt   => 2,
            DefaultTerrainVoxel::Stone  => 3,
            DefaultTerrainVoxel::Sand   => 4,
            DefaultTerrainVoxel::Wood   => 5,
            DefaultTerrainVoxel::Leaves => 6,
            DefaultTerrainVoxel::Gravel => 7,
            DefaultTerrainVoxel::Snow   => 8,
            DefaultTerrainVoxel::Water       => 9,
            DefaultTerrainVoxel::CoalOre     => 10,
            DefaultTerrainVoxel::IronOre     => 11,
            DefaultTerrainVoxel::Cobblestone => 12,
            DefaultTerrainVoxel::Sandstone   => 13,
            DefaultTerrainVoxel::Planks      => 14,
            DefaultTerrainVoxel::Brick       => 15,
        }
    }

    #[inline]
    fn from_byte(byte: u8) -> Self {
        match byte {
            1 => DefaultTerrainVoxel::Grass,
            2 => DefaultTerrainVoxel::Dirt,
            3 => DefaultTerrainVoxel::Stone,
            4 => DefaultTerrainVoxel::Sand,
            5 => DefaultTerrainVoxel::Wood,
            6 => DefaultTerrainVoxel::Leaves,
            7 => DefaultTerrainVoxel::Gravel,
            8 => DefaultTerrainVoxel::Snow,
            9  => DefaultTerrainVoxel::Water,
            10 => DefaultTerrainVoxel::CoalOre,
            11 => DefaultTerrainVoxel::IronOre,
            12 => DefaultTerrainVoxel::Cobblestone,
            13 => DefaultTerrainVoxel::Sandstone,
            14 => DefaultTerrainVoxel::Planks,
            15 => DefaultTerrainVoxel::Brick,
            _  => DefaultTerrainVoxel::Air,
        }
    }

    #[inline]
    fn hardness(&self) -> f32 {
        match self {
            DefaultTerrainVoxel::Air    => 0.0, // unbreakable
            DefaultTerrainVoxel::Grass  => 0.5,
            DefaultTerrainVoxel::Dirt   => 0.5,
            DefaultTerrainVoxel::Stone  => 3.0,
            DefaultTerrainVoxel::Sand   => 0.5,
            DefaultTerrainVoxel::Wood   => 1.5,
            DefaultTerrainVoxel::Leaves => 0.3,
            DefaultTerrainVoxel::Gravel => 0.75,
            DefaultTerrainVoxel::Snow   => 0.3,
            DefaultTerrainVoxel::Water       => 0.0, // unbreakable
            DefaultTerrainVoxel::CoalOre     => 3.0,
            DefaultTerrainVoxel::IronOre     => 4.0,
            DefaultTerrainVoxel::Cobblestone => 2.5,
            DefaultTerrainVoxel::Sandstone   => 1.5,
            DefaultTerrainVoxel::Planks      => 1.0,
            DefaultTerrainVoxel::Brick       => 2.0,
        }
    }

    #[inline]
    fn mining_level(&self) -> u8 {
        match self {
            DefaultTerrainVoxel::Stone       => 1,
            DefaultTerrainVoxel::Gravel      => 1,
            DefaultTerrainVoxel::CoalOre     => 1,
            DefaultTerrainVoxel::IronOre     => 2,
            DefaultTerrainVoxel::Cobblestone => 1,
            DefaultTerrainVoxel::Sandstone   => 1,
            DefaultTerrainVoxel::Brick       => 1,
            _ => 0,
        }
    }

    #[inline]
    fn texture_layer(&self, _face_index: usize) -> u32 {
        match self {
            DefaultTerrainVoxel::Air         => 0,
            DefaultTerrainVoxel::Grass       => 1,
            DefaultTerrainVoxel::Dirt        => 2,
            DefaultTerrainVoxel::Stone       => 3,
            DefaultTerrainVoxel::Sand        => 4,
            DefaultTerrainVoxel::Wood        => 5,
            DefaultTerrainVoxel::Leaves      => 6,
            DefaultTerrainVoxel::Gravel      => 7,
            DefaultTerrainVoxel::Snow        => 8,
            DefaultTerrainVoxel::Water       => 9,
            DefaultTerrainVoxel::CoalOre     => 10,
            DefaultTerrainVoxel::IronOre     => 11,
            DefaultTerrainVoxel::Cobblestone => 12,
            DefaultTerrainVoxel::Sandstone   => 13,
            DefaultTerrainVoxel::Planks      => 14,
            DefaultTerrainVoxel::Brick       => 15,
        }
    }

    #[inline]
    fn movement_modifier(&self) -> MovementModifier {
        match self {
            DefaultTerrainVoxel::Water => MovementModifier {
                speed_mult: 0.4,
                jump_mult: 0.5,
                gravity_mult: 1.0,
                friction_mult: 1.0,
            },
            DefaultTerrainVoxel::Snow => MovementModifier {
                speed_mult: 0.7,
                ..MovementModifier::default()
            },
            _ => MovementModifier::default(),
        }
    }

    #[inline]
    fn alpha(&self) -> f32 {
        match self {
            Self::Water => 0.7,
            _ => 1.0,
        }
    }
}

/// A Bevy [`Resource`] that maps voxel types to runtime RGBA color overrides.
///
/// `VoxelPalette` lets you override the colors returned by
/// [`VoxelType::color`] without changing the `VoxelType` implementation. This
/// is useful when colors come from a data file or when you want to support
/// texture packs / themes at runtime.
///
/// The palette is inserted into the Bevy world at startup and cloned into
/// async mesh generation tasks, so it must be `Clone`. When no override is
/// registered for a voxel, [`color_of`] falls back to `VoxelType::color`.
///
/// # Example
///
/// ```rust,no_run
/// use std::collections::HashMap;
/// use voxel_framework::voxel::{DefaultVoxel, VoxelPalette};
///
/// let mut colors = HashMap::new();
/// // Override Rock to a brighter shade for testing.
/// colors.insert(DefaultVoxel::Rock, [0.6, 0.6, 0.6, 1.0]);
/// let palette = VoxelPalette::new(colors);
///
/// // Unknown voxels fall back to their own color() impl.
/// let iron_color = palette.color_of(&DefaultVoxel::IronOre);
/// assert_eq!(iron_color, [0.55, 0.35, 0.20, 1.0]);
/// ```
///
/// # See also
///
/// - [`VoxelType::color`] — the per-variant color fallback.
///
/// [`color_of`]: VoxelPalette::color_of
#[derive(Resource, Clone, Debug)]
pub struct VoxelPalette<V: VoxelType> {
    colors: HashMap<V, [f32; 4]>,
}

impl<V: VoxelType> VoxelPalette<V> {
    /// Creates a new palette from a pre-built map of voxel types to RGBA colors.
    ///
    /// Only the variants present in `colors` are overridden; all others
    /// continue to use their [`VoxelType::color`] value.
    pub fn new(colors: HashMap<V, [f32; 4]>) -> Self {
        Self { colors }
    }

    /// Returns the RGBA color for `voxel`.
    ///
    /// Checks the override map first. If no entry is present, falls back to
    /// `voxel.color()`. The returned array is in linear floating-point RGBA
    /// with each channel in `[0.0, 1.0]`.
    #[inline]
    pub fn color_of(&self, voxel: &V) -> [f32; 4] {
        self.colors.get(voxel).copied().unwrap_or_else(|| voxel.color())
    }
}

impl<V: VoxelType> Default for VoxelPalette<V> {
    fn default() -> Self {
        Self {
            colors: HashMap::new(),
        }
    }
}
