//! Chunk data structure — a fixed-size 3D grid of voxels.

use bevy::prelude::Component;

use super::{CHUNK_SIZE, CHUNK_VOLUME};
use crate::voxel::VoxelType;

/// A 16x16x16 block of voxels, the fundamental unit of world storage.
///
/// Voxels are stored in a flat heap-allocated array indexed as
/// `x * CHUNK_SIZE * CHUNK_SIZE + y * CHUNK_SIZE + z`. The chunk is generic
/// over any type implementing [`VoxelType`].
#[derive(Component, Clone, Debug)]
pub struct Chunk<V: VoxelType> {
    voxels: Box<[V; CHUNK_VOLUME]>,
}

impl<V: VoxelType> Chunk<V> {
    /// Creates a new chunk filled entirely with air voxels.
    pub fn new() -> Self {
        Self {
            voxels: Box::new([V::air(); CHUNK_VOLUME]),
        }
    }

    /// Computes the flat index for the voxel at `(x, y, z)`.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if any coordinate is out of range (`>= CHUNK_SIZE`).
    #[inline]
    pub fn get_index(x: usize, y: usize, z: usize) -> usize {
        debug_assert!(x < CHUNK_SIZE, "x={x} out of range");
        debug_assert!(y < CHUNK_SIZE, "y={y} out of range");
        debug_assert!(z < CHUNK_SIZE, "z={z} out of range");
        x * CHUNK_SIZE * CHUNK_SIZE + y * CHUNK_SIZE + z
    }

    /// Returns the voxel at `(x, y, z)`.
    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> V {
        self.voxels[Self::get_index(x, y, z)]
    }

    /// Sets the voxel at `(x, y, z)`.
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, voxel: V) {
        self.voxels[Self::get_index(x, y, z)] = voxel;
    }

    /// Returns `true` if the voxel at `(x, y, z)` is solid.
    #[inline]
    pub fn is_solid_at(&self, x: usize, y: usize, z: usize) -> bool {
        self.get(x, y, z).is_solid()
    }

    /// Returns `true` if every voxel in the chunk is solid.
    pub fn is_fully_solid(&self) -> bool {
        self.voxels.iter().all(|v| v.is_solid())
    }

    /// Returns `true` if every voxel in the chunk is air (empty).
    pub fn is_empty(&self) -> bool {
        self.voxels.iter().all(|v| !v.is_solid())
    }

    /// Extracts a 16x16 border slice for one of the six chunk faces.
    ///
    /// The returned array contains the voxels on the outermost layer in the
    /// direction indicated by `face`:
    ///
    /// | Face | Direction | Fixed axis value |
    /// |------|-----------|-----------------|
    /// | 0    | +Y        | y = 15          |
    /// | 1    | -Y        | y = 0           |
    /// | 2    | +X        | x = 15          |
    /// | 3    | -X        | x = 0           |
    /// | 4    | +Z        | z = 15          |
    /// | 5    | -Z        | z = 0           |
    ///
    /// The slice is packed in row-major order for the two varying axes.
    ///
    /// # Panics
    ///
    /// Panics if `face >= 6`.
    pub fn border_slice(&self, face: usize) -> [V; CHUNK_SIZE * CHUNK_SIZE] {
        let mut slice = [V::air(); CHUNK_SIZE * CHUNK_SIZE];
        let last = CHUNK_SIZE - 1;

        match face {
            // +Y (y = 15): iterate x, z
            0 => {
                for x in 0..CHUNK_SIZE {
                    for z in 0..CHUNK_SIZE {
                        slice[x * CHUNK_SIZE + z] = self.get(x, last, z);
                    }
                }
            }
            // -Y (y = 0): iterate x, z
            1 => {
                for x in 0..CHUNK_SIZE {
                    for z in 0..CHUNK_SIZE {
                        slice[x * CHUNK_SIZE + z] = self.get(x, 0, z);
                    }
                }
            }
            // +X (x = 15): iterate y, z
            2 => {
                for y in 0..CHUNK_SIZE {
                    for z in 0..CHUNK_SIZE {
                        slice[y * CHUNK_SIZE + z] = self.get(last, y, z);
                    }
                }
            }
            // -X (x = 0): iterate y, z
            3 => {
                for y in 0..CHUNK_SIZE {
                    for z in 0..CHUNK_SIZE {
                        slice[y * CHUNK_SIZE + z] = self.get(0, y, z);
                    }
                }
            }
            // +Z (z = 15): iterate x, y
            4 => {
                for x in 0..CHUNK_SIZE {
                    for y in 0..CHUNK_SIZE {
                        slice[x * CHUNK_SIZE + y] = self.get(x, y, last);
                    }
                }
            }
            // -Z (z = 0): iterate x, y
            5 => {
                for x in 0..CHUNK_SIZE {
                    for y in 0..CHUNK_SIZE {
                        slice[x * CHUNK_SIZE + y] = self.get(x, y, 0);
                    }
                }
            }
            _ => panic!("invalid face index: {face} (must be 0..6)"),
        }

        slice
    }
}

impl<V: VoxelType> Default for Chunk<V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::DefaultVoxel;

    #[test]
    fn new_chunk_is_all_air() {
        let chunk = Chunk::<DefaultVoxel>::new();
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    assert_eq!(chunk.get(x, y, z), DefaultVoxel::Air);
                }
            }
        }
    }

    #[test]
    fn get_set_round_trip() {
        let mut chunk = Chunk::<DefaultVoxel>::new();
        chunk.set(3, 7, 11, DefaultVoxel::IronOre);
        assert_eq!(chunk.get(3, 7, 11), DefaultVoxel::IronOre);

        chunk.set(0, 0, 0, DefaultVoxel::RareMineral);
        assert_eq!(chunk.get(0, 0, 0), DefaultVoxel::RareMineral);

        chunk.set(15, 15, 15, DefaultVoxel::SiliconCrystal);
        assert_eq!(chunk.get(15, 15, 15), DefaultVoxel::SiliconCrystal);
    }

    #[test]
    fn is_fully_solid_on_filled_chunk() {
        let mut chunk = Chunk::<DefaultVoxel>::new();
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    chunk.set(x, y, z, DefaultVoxel::Rock);
                }
            }
        }
        assert!(chunk.is_fully_solid());
        assert!(!chunk.is_empty());
    }

    #[test]
    fn is_empty_on_new_chunk() {
        let chunk = Chunk::<DefaultVoxel>::new();
        assert!(chunk.is_empty());
        assert!(!chunk.is_fully_solid());
    }

    #[test]
    fn border_slice_plus_y() {
        let mut chunk = Chunk::<DefaultVoxel>::new();
        // Fill the top face (y=15) with Rock
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                chunk.set(x, 15, z, DefaultVoxel::Rock);
            }
        }
        let slice = chunk.border_slice(0); // +Y face
        for v in &slice {
            assert_eq!(*v, DefaultVoxel::Rock);
        }
        // -Y face should still be air
        let bottom = chunk.border_slice(1);
        for v in &bottom {
            assert_eq!(*v, DefaultVoxel::Air);
        }
    }

    #[test]
    fn border_slice_specific_voxel() {
        let mut chunk = Chunk::<DefaultVoxel>::new();
        // Place a single voxel on the +X face at (15, 3, 7)
        chunk.set(15, 3, 7, DefaultVoxel::IronOre);
        let slice = chunk.border_slice(2); // +X face: iterate y, z
        assert_eq!(slice[3 * CHUNK_SIZE + 7], DefaultVoxel::IronOre);
        // All other entries on this face should be air
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                if y == 3 && z == 7 {
                    continue;
                }
                assert_eq!(slice[y * CHUNK_SIZE + z], DefaultVoxel::Air);
            }
        }
    }
}
