//! Binary chunk I/O — save and load chunks to/from raw binary files.
//!
//! ## File format
//!
//! ```text
//! [CHUNK_VOLUME bytes]  — voxel palette bytes (VoxelType::to_byte)
//! [CHUNK_VOLUME bytes]  — state bytes (optional, present when file >= 2 * CHUNK_VOLUME)
//! ```
//!
//! Voxels are stored in flat array order: `x * CHUNK_SIZE² + y * CHUNK_SIZE + z`.
//! State bytes follow in the same order.  Files from older versions (exactly
//! `CHUNK_VOLUME` bytes) are loaded without error — state bytes default to `0`.
//!
//! File name convention: `chunk_X_Y_Z.bin` where X, Y, Z are the [`IVec3`] chunk
//! coordinates.

use std::fs;
use std::io;
use std::path::Path;

use bevy::math::IVec3;

use super::storage::Chunk;
use super::{CHUNK_SIZE, CHUNK_VOLUME};
use crate::voxel::VoxelType;

/// Saves a chunk to a binary file.
///
/// Writes `2 * CHUNK_VOLUME` bytes: first the voxel palette bytes (one per
/// voxel via [`VoxelType::to_byte`]), then the state bytes in the same flat
/// array order.
///
/// # Errors
///
/// Returns an [`io::Error`] if the file cannot be written (e.g., invalid path,
/// permission denied, disk full).
pub fn save_chunk<V: VoxelType>(chunk: &Chunk<V>, path: impl AsRef<Path>) -> io::Result<()> {
    let mut bytes = Vec::with_capacity(CHUNK_VOLUME * 2);
    for x in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                bytes.push(chunk.get(x, y, z).to_byte());
            }
        }
    }
    for x in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                bytes.push(chunk.get_state(x, y, z));
            }
        }
    }
    fs::write(path, &bytes)
}

/// Loads a chunk from a binary file.
///
/// Reads the voxel palette bytes and (if present) the state bytes.  Files that
/// contain only [`CHUNK_VOLUME`] bytes (old format) are loaded with all states
/// defaulting to `0`.
///
/// # Errors
///
/// Returns an [`io::Error`] if:
/// - The file cannot be read.
/// - The file is smaller than [`CHUNK_VOLUME`] bytes.
pub fn load_chunk<V: VoxelType>(path: impl AsRef<Path>) -> io::Result<Chunk<V>> {
    let bytes = fs::read(path)?;
    if bytes.len() < CHUNK_VOLUME {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!(
                "chunk file too small: expected at least {} bytes, got {}",
                CHUNK_VOLUME,
                bytes.len()
            ),
        ));
    }

    let mut chunk = Chunk::<V>::new();
    let mut i = 0;
    for x in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                chunk.set(x, y, z, V::from_byte(bytes[i]));
                i += 1;
            }
        }
    }

    // Load state bytes if the file contains them (new format).
    if bytes.len() >= CHUNK_VOLUME * 2 {
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    chunk.set_state(x, y, z, bytes[i]);
                    i += 1;
                }
            }
        }
    }

    Ok(chunk)
}

/// Generates a filename for a chunk at the given coordinate.
///
/// The returned string has the format `chunk_{x}_{y}_{z}.bin`, where `x`, `y`,
/// and `z` are the integer components of the coordinate vector.
///
/// # Examples
///
/// ```
/// use bevy::math::IVec3;
/// use voxel_framework::chunk::persistence::chunk_filename;
///
/// assert_eq!(chunk_filename(IVec3::new(1, -2, 3)), "chunk_1_-2_3.bin");
/// ```
pub fn chunk_filename(coord: IVec3) -> String {
    format!("chunk_{}_{}_{}.bin", coord.x, coord.y, coord.z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::DefaultVoxel;

    /// Helper: returns a temporary file path with the given name.
    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(name)
    }

    #[test]
    fn round_trip_save_load() {
        let path = temp_path("test_round_trip_chunk.bin");

        // Build a chunk with a variety of voxel types.
        let mut chunk = Chunk::<DefaultVoxel>::new();
        chunk.set(0, 0, 0, DefaultVoxel::Rock);
        chunk.set(1, 2, 3, DefaultVoxel::IronOre);
        chunk.set(15, 15, 15, DefaultVoxel::SiliconCrystal);
        chunk.set(8, 4, 12, DefaultVoxel::RareMineral);
        chunk.set(7, 7, 7, DefaultVoxel::Rock);

        // Save and reload.
        save_chunk(&chunk, &path).expect("save_chunk failed");
        let loaded = load_chunk::<DefaultVoxel>(&path).expect("load_chunk failed");

        // Verify every voxel matches.
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    assert_eq!(
                        chunk.get(x, y, z),
                        loaded.get(x, y, z),
                        "mismatch at ({x}, {y}, {z})"
                    );
                }
            }
        }

        // Clean up.
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn empty_chunk_round_trip() {
        let path = temp_path("test_empty_chunk.bin");

        let chunk = Chunk::<DefaultVoxel>::new();

        save_chunk(&chunk, &path).expect("save_chunk failed");
        let loaded = load_chunk::<DefaultVoxel>(&path).expect("load_chunk failed");

        // Every voxel should be Air.
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    assert_eq!(loaded.get(x, y, z), DefaultVoxel::Air);
                }
            }
        }
        assert!(loaded.is_empty());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn chunk_filename_formatting() {
        assert_eq!(chunk_filename(IVec3::new(1, 2, 3)), "chunk_1_2_3.bin");
        assert_eq!(chunk_filename(IVec3::new(-5, 0, 10)), "chunk_-5_0_10.bin");
        assert_eq!(chunk_filename(IVec3::new(0, 0, 0)), "chunk_0_0_0.bin");
        assert_eq!(
            chunk_filename(IVec3::new(-100, -200, -300)),
            "chunk_-100_-200_-300.bin"
        );
    }

    #[test]
    fn load_too_small_file_returns_error() {
        let path = temp_path("test_small_chunk.bin");
        // Write fewer bytes than CHUNK_VOLUME.
        fs::write(&path, &[0u8; 100]).expect("write failed");

        let result = load_chunk::<DefaultVoxel>(&path);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn load_nonexistent_file_returns_error() {
        let path = temp_path("test_nonexistent_chunk_xyz.bin");
        let result = load_chunk::<DefaultVoxel>(&path);
        assert!(result.is_err());
    }
}
