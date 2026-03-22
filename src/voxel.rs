//! Voxel type definitions, the `VoxelType` trait, and the default voxel palette.

use std::collections::HashMap;
use std::hash::Hash;

use bevy::prelude::Resource;

/// Core trait that all voxel types must implement.
///
/// Provides the minimal interface needed by the chunk storage, mesh generation,
/// and persistence systems. Implementors must be lightweight value types suitable
/// for storage in large flat arrays (one per voxel in a chunk).
pub trait VoxelType: Copy + Clone + Default + Send + Sync + PartialEq + Eq + Hash + 'static {
    /// Returns `true` if this voxel is considered solid (i.e., should produce geometry).
    fn is_solid(&self) -> bool;

    /// Returns the default RGBA color for this voxel type.
    fn color(&self) -> [f32; 4];

    /// Serializes this voxel to a single byte for binary persistence.
    fn to_byte(&self) -> u8;

    /// Deserializes a voxel from a single byte. Unknown bytes should map to `air()`.
    fn from_byte(byte: u8) -> Self;

    /// Returns the empty/air voxel. Default implementation returns `Self::default()`.
    fn air() -> Self {
        Self::default()
    }
}

/// The default set of voxel types used by the framework.
///
/// Matches the reference implementation: Air (empty), Rock, IronOre, SiliconCrystal,
/// and RareMineral. Colors are derived from the architecture specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DefaultVoxel {
    /// Empty space (not solid, default state).
    #[default]
    Air,
    /// Common rock — warm grey-brown.
    Rock,
    /// Iron ore deposit — rusty orange.
    IronOre,
    /// Silicon crystal vein — blue-violet.
    SiliconCrystal,
    /// Rare mineral node — vivid teal-green.
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

/// A resource that maps voxel types to their RGBA display colors.
///
/// Constructed at startup from configuration and inserted as a Bevy resource so
/// that systems (including async mesh generation tasks via cloning) can look up
/// voxel colors without accessing the world configuration directly.
#[derive(Resource, Clone, Debug)]
pub struct VoxelPalette<V: VoxelType> {
    colors: HashMap<V, [f32; 4]>,
}

impl<V: VoxelType> VoxelPalette<V> {
    /// Creates a new palette from a map of voxel types to RGBA colors.
    pub fn new(colors: HashMap<V, [f32; 4]>) -> Self {
        Self { colors }
    }

    /// Returns the RGBA color for the given voxel, falling back to the voxel's
    /// own default color if no palette override is present.
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
