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

    /// Returns the empty/air voxel for this type.
    ///
    /// The default implementation delegates to `Self::default()`, which is
    /// correct as long as `Default` is derived with `#[default]` on the air
    /// variant. Override this method only if air is not the default variant.
    fn air() -> Self {
        Self::default()
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
