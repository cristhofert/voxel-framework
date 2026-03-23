//! The [`Chunk<V>`] data structure — a fixed-size, flat-array 3D voxel grid.

use bevy::prelude::Component;

use super::{CHUNK_SIZE, CHUNK_VOLUME};
use crate::voxel::VoxelType;

/// A 16×16×16 block of voxels — the fundamental unit of world storage.
///
/// `Chunk<V>` is a Bevy [`Component`] that holds all voxel data for one
/// 16-voxel-per-axis region of the world. It is generic over any type
/// implementing [`VoxelType`], so it works with [`crate::voxel::DefaultVoxel`]
/// out of the box and with custom palettes without any changes to the framework.
///
/// ## Memory layout
///
/// Voxels are stored in a single heap-allocated array of length
/// [`CHUNK_VOLUME`] (`4 096`), indexed in **X-major order**:
///
/// ```text
/// flat_index = x * CHUNK_SIZE² + y * CHUNK_SIZE + z
/// ```
///
/// This means a full row of Z-adjacent voxels is contiguous in memory —
/// the layout the greedy meshing algorithm iterates most frequently.
/// The array is wrapped in a `Box` to keep the struct's stack size to a single
/// pointer (`8 bytes`) regardless of `V`'s size.
///
/// ## Coordinate bounds
///
/// Every method that takes `(x, y, z)` expects each coordinate in
/// `[0, CHUNK_SIZE)` (i.e. `0..16`). Out-of-bounds access panics in debug
/// builds via [`debug_assert`] inside [`Chunk::get_index`]. In release builds,
/// out-of-bounds indices silently alias other voxels — callers must ensure
/// correct inputs.
///
/// Use [`crate::chunk::world_to_local_coord`] to convert a world-space
/// position to a valid `(x, y, z)` triple before calling `get` or `set`.
///
/// # See also
///
/// - [`crate::chunk`] — coordinate systems and conversion helpers.
/// - [`crate::voxel::VoxelType`] — the trait `V` must implement.
#[derive(Component, Clone, Debug)]
pub struct Chunk<V: VoxelType> {
    voxels: Box<[V; CHUNK_VOLUME]>,
}

impl<V: VoxelType> Chunk<V> {
    /// Creates a new chunk with every voxel initialized to [`VoxelType::air`].
    ///
    /// Allocates a [`CHUNK_VOLUME`]-element array on the heap. Equivalent to
    /// calling `Default::default()`.
    pub fn new() -> Self {
        Self {
            voxels: Box::new([V::air(); CHUNK_VOLUME]),
        }
    }

    /// Computes the flat array index for the voxel at local coordinate `(x, y, z)`.
    ///
    /// The formula is `x * CHUNK_SIZE² + y * CHUNK_SIZE + z`, giving X-major
    /// (row-major with X as the outermost dimension) storage order.
    ///
    /// Prefer calling [`Chunk::get`] or [`Chunk::set`] directly; this method is
    /// public for use by the meshing and persistence systems that need to
    /// iterate the flat array efficiently.
    ///
    /// # Panics
    ///
    /// Panics in **debug builds** if any coordinate is `>= CHUNK_SIZE`. In
    /// release builds, the assertion is omitted for performance.
    #[inline]
    pub fn get_index(x: usize, y: usize, z: usize) -> usize {
        debug_assert!(x < CHUNK_SIZE, "x={x} out of range");
        debug_assert!(y < CHUNK_SIZE, "y={y} out of range");
        debug_assert!(z < CHUNK_SIZE, "z={z} out of range");
        x * CHUNK_SIZE * CHUNK_SIZE + y * CHUNK_SIZE + z
    }

    /// Returns a copy of the voxel at local coordinate `(x, y, z)`.
    ///
    /// Each coordinate must be in `[0, CHUNK_SIZE)`. The value is returned by
    /// copy because [`VoxelType`] requires `Copy`.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if any coordinate is out of bounds (see
    /// [`Chunk::get_index`]).
    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> V {
        self.voxels[Self::get_index(x, y, z)]
    }

    /// Overwrites the voxel at local coordinate `(x, y, z)` with `voxel`.
    ///
    /// Each coordinate must be in `[0, CHUNK_SIZE)`. Use
    /// [`VoxelType::air`] to clear a voxel back to empty.
    ///
    /// After calling this method on an already-spawned chunk entity, emit a
    /// [`crate::world::ChunkModified`] event so that the mesh system
    /// regenerates the chunk's geometry.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if any coordinate is out of bounds (see
    /// [`Chunk::get_index`]).
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, voxel: V) {
        self.voxels[Self::get_index(x, y, z)] = voxel;
    }

    /// Returns `true` if the voxel at local coordinate `(x, y, z)` is solid.
    ///
    /// Equivalent to `self.get(x, y, z).is_solid()`. Used by the meshing and
    /// occlusion systems to test individual voxels without materializing the
    /// full voxel value.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if any coordinate is out of bounds.
    #[inline]
    pub fn is_solid_at(&self, x: usize, y: usize, z: usize) -> bool {
        self.get(x, y, z).is_solid()
    }

    /// Returns `true` if every voxel in the chunk is solid.
    ///
    /// A fully solid chunk is opaque from all six faces and requires no
    /// interior face geometry. The occlusion system uses this to skip mesh
    /// generation for completely buried chunks.
    ///
    /// Runs in O([`CHUNK_VOLUME`]) time by scanning the flat array.
    pub fn is_fully_solid(&self) -> bool {
        self.voxels.iter().all(|v| v.is_solid())
    }

    /// Returns `true` if every voxel in the chunk is air (non-solid).
    ///
    /// An empty chunk produces no geometry. The streaming system uses this
    /// to avoid spawning mesh entities for chunks that contain only air,
    /// such as the open space above a terrain surface or inside a cave ceiling.
    ///
    /// Runs in O([`CHUNK_VOLUME`]) time by scanning the flat array.
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
