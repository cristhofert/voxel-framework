//! Chunk persistence integration — auto-save and disk-backed load.
//!
//! This module wires the low-level [`chunk::persistence`](crate::chunk::persistence) I/O into the Bevy
//! game loop via two pieces:
//!
//! - [`PersistenceConfig`] — a [`Resource`] that controls where chunk files
//!   live on disk and whether auto-save is enabled.
//! - [`chunk_autosave_system`] — listens for [`ChunkModified`] messages and
//!   saves the affected chunk to disk when [`PersistenceConfig::auto_save`] is
//!   `true`.
//! - [`try_load_chunk`] — a free function (not a system) that attempts to load
//!   a chunk from disk; called by the generator pipeline before spawning an
//!   async generation task.

use std::path::PathBuf;

use bevy::prelude::*;

use crate::chunk::{Chunk, ChunkCoord, persistence as chunk_persistence};
use crate::voxel::VoxelType;
use crate::world::{ChunkManager, ChunkModified};

// ─── WorldDataDir ─────────────────────────────────────────────────────────────

/// Resource that holds the base data directory for the active world.
///
/// Set automatically by [`PersistencePlugin::with_world`] and
/// [`PlayerPlugin::with_world`].  All world-specific files (chunks,
/// `player_data.json`) are stored under this path:
///
/// ```text
/// ./worlds/{world_name}/
///   ├── chunks/
///   │   └── chunk_X_Y_Z.bin
///   └── player_data.json
/// ```
///
/// The default value (`"./worlds/world"`) matches the default world name.
#[derive(Resource, Clone, Debug)]
pub struct WorldDataDir(pub String);

impl Default for WorldDataDir {
    fn default() -> Self {
        Self("./worlds/world".to_string())
    }
}

impl WorldDataDir {
    /// Creates a `WorldDataDir` for the given world name.
    pub fn for_world(world_name: &str) -> Self {
        Self(format!("./worlds/{}", world_name))
    }
    /// Returns the directory path as a string slice.
    pub fn as_str(&self) -> &str { &self.0 }
}

// ─── PersistenceConfig ────────────────────────────────────────────────────────

/// Configuration resource that controls chunk persistence behaviour.
///
/// Insert this resource before or after adding [`PersistencePlugin`] to
/// override the defaults.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::world::PersistenceConfig;
///
/// App::new()
///     .insert_resource(PersistenceConfig {
///         save_dir: "./saves/chunks".to_string(),
///         auto_save: true,
///     });
/// ```
#[derive(Resource, Debug, Clone)]
pub struct PersistenceConfig {
    /// Directory where chunk binary files are stored.
    ///
    /// The directory is created automatically (including all parent
    /// directories) the first time a chunk is saved.
    ///
    /// Default: `"./chunks"`.
    pub save_dir: String,

    /// When `true`, every [`ChunkModified`] event triggers a disk write for
    /// the affected chunk.
    ///
    /// Default: `true`.
    pub auto_save: bool,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            save_dir: "./worlds/world/chunks".to_string(),
            auto_save: true,
        }
    }
}

// ─── try_load_chunk ───────────────────────────────────────────────────────────

/// Attempts to load a chunk from disk.
///
/// Constructs the expected file path from `save_dir` and `coord`, then tries
/// to read and deserialise it via [`load_chunk`](crate::chunk::persistence::load_chunk).
///
/// Returns `Some(chunk)` on success, `None` if the file does not exist or
/// cannot be read.
pub fn try_load_chunk<V: VoxelType>(coord: ChunkCoord, save_dir: &str) -> Option<Chunk<V>> {
    let filename = chunk_persistence::chunk_filename(coord);
    let path: PathBuf = [save_dir, &filename].iter().collect();

    chunk_persistence::load_chunk::<V>(&path).ok()
}

// ─── chunk_autosave_system ────────────────────────────────────────────────────

/// Saves modified chunks to disk when [`PersistenceConfig::auto_save`] is enabled.
///
/// For each [`ChunkModified`] message this system:
/// 1. Resolves the chunk entity from [`ChunkManager`].
/// 2. Queries the [`Chunk<V>`] component on that entity.
/// 3. Constructs the output path from [`PersistenceConfig::save_dir`] and the
///    chunk coordinate.
/// 4. Ensures the save directory exists (`std::fs::create_dir_all`).
/// 5. Writes the chunk data via [`save_chunk`](crate::chunk::persistence::save_chunk).
///
/// Errors are reported with `eprintln!` and do not panic.
pub fn chunk_autosave_system<V: VoxelType>(
    mut modified_reader: MessageReader<ChunkModified>,
    config: Res<PersistenceConfig>,
    manager: Res<ChunkManager>,
    chunks: Query<&Chunk<V>>,
) {
    if !config.auto_save {
        return;
    }

    for &ChunkModified(coord) in modified_reader.read() {
        let Some(entity) = manager.get_entity(coord) else {
            continue;
        };

        let Ok(chunk) = chunks.get(entity) else {
            continue;
        };

        let filename = chunk_persistence::chunk_filename(coord);
        let path: PathBuf = [config.save_dir.as_str(), &filename].iter().collect();

        // Ensure the save directory exists.
        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            eprintln!(
                "voxel-framework: failed to create chunk save directory '{}': {e}",
                parent.display()
            );
            continue;
        }

        if let Err(e) = chunk_persistence::save_chunk(chunk, &path) {
            eprintln!(
                "voxel-framework: failed to save chunk {coord} to '{}': {e}",
                path.display()
            );
        }
    }
}

// ─── PersistencePlugin ────────────────────────────────────────────────────────

/// Plugin that registers [`PersistenceConfig`] and the auto-save system.
///
/// The plugin is generic over the voxel type `V` so it can operate on any
/// [`Chunk<V>`] component.
///
/// Users who want the default [`DefaultVoxel`] pipeline can add
/// [`PersistencePlugin::<DefaultVoxel>::new()`].  Users with a custom voxel
/// type add `PersistencePlugin::<MyVoxel>::new()`.
///
/// [`DefaultVoxel`]: crate::voxel::DefaultVoxel
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::world::{PersistenceConfig, PersistencePlugin};
/// use voxel_framework::voxel::DefaultVoxel;
///
/// App::new()
///     .add_plugins(PersistencePlugin::<DefaultVoxel>::new());
/// ```
pub struct PersistencePlugin<V: VoxelType> {
    _marker: std::marker::PhantomData<V>,
    world_name: String,
}

impl<V: VoxelType> PersistencePlugin<V> {
    /// Creates a `PersistencePlugin` using the default world name (`"world"`).
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
            world_name: String::new(),
        }
    }

    /// Creates a `PersistencePlugin` that saves chunks under
    /// `./worlds/{world_name}/chunks/`.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bevy::prelude::*;
    /// use voxel_framework::world::PersistencePlugin;
    /// use voxel_framework::voxel::DefaultVoxel;
    ///
    /// App::new().add_plugins(PersistencePlugin::<DefaultVoxel>::with_world("survival"));
    /// ```
    pub fn with_world(world_name: impl Into<String>) -> Self {
        Self {
            _marker: std::marker::PhantomData,
            world_name: world_name.into(),
        }
    }
}

impl<V: VoxelType> Default for PersistencePlugin<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: VoxelType> Plugin for PersistencePlugin<V> {
    fn build(&self, app: &mut App) {
        let name = if self.world_name.is_empty() { "world" } else { &self.world_name };
        let save_dir = format!("./worlds/{}/chunks", name);
        app.insert_resource(PersistenceConfig { save_dir, auto_save: true });
        app.add_systems(Update, chunk_autosave_system::<V>);
    }
}
