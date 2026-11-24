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
/// [`PlayerPlugin::with_world`](crate::player::PlayerPlugin::with_world).  All world-specific files (chunks,
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
    pub fn as_str(&self) -> &str {
        &self.0
    }
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
        let name = if self.world_name.is_empty() {
            "world"
        } else {
            &self.world_name
        };
        let save_dir = format!("./worlds/{}/chunks", name);
        app.insert_resource(PersistenceConfig {
            save_dir,
            auto_save: true,
        });
        app.add_systems(Update, chunk_autosave_system::<V>);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::persistence::{chunk_filename, save_chunk};
    use crate::voxel::DefaultVoxel;

    fn chunk_with_marker() -> Chunk<DefaultVoxel> {
        let mut chunk = Chunk::new();
        chunk.set(3, 4, 5, DefaultVoxel::IronOre);
        chunk.set_state(3, 4, 5, 7);
        chunk
    }

    /// App with only the autosave system, one loaded chunk at `coord`, and
    /// chunk files written under `save_dir`.
    fn autosave_app(coord: IVec3, save_dir: &std::path::Path, auto_save: bool) -> App {
        let mut app = App::new();
        app.add_message::<ChunkModified>();
        app.insert_resource(PersistenceConfig {
            save_dir: save_dir.to_string_lossy().into_owned(),
            auto_save,
        });
        let entity = app.world_mut().spawn(chunk_with_marker()).id();
        let mut manager = ChunkManager::default();
        manager.register(coord, entity);
        app.insert_resource(manager);
        app.add_systems(Update, chunk_autosave_system::<DefaultVoxel>);
        app
    }

    #[test]
    fn try_load_chunk_returns_none_when_file_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = try_load_chunk::<DefaultVoxel>(IVec3::ZERO, dir.path().to_str().unwrap());
        assert!(loaded.is_none());
    }

    #[test]
    fn try_load_chunk_reads_saved_chunk_with_negative_coord() {
        let dir = tempfile::tempdir().unwrap();
        let coord = IVec3::new(-3, 0, 12);
        save_chunk(&chunk_with_marker(), dir.path().join(chunk_filename(coord))).unwrap();

        let loaded = try_load_chunk::<DefaultVoxel>(coord, dir.path().to_str().unwrap())
            .expect("saved chunk should load");
        assert_eq!(loaded.get(3, 4, 5), DefaultVoxel::IronOre);
        assert_eq!(loaded.get_state(3, 4, 5), 7);
    }

    #[test]
    fn autosave_writes_modified_chunk_and_creates_missing_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let save_dir = dir.path().join("nested").join("chunks");
        let coord = IVec3::new(1, -2, 3);
        let mut app = autosave_app(coord, &save_dir, true);

        app.world_mut().write_message(ChunkModified(coord));
        app.update();

        let loaded = try_load_chunk::<DefaultVoxel>(coord, save_dir.to_str().unwrap())
            .expect("autosave should have written the chunk");
        assert_eq!(loaded.get(3, 4, 5), DefaultVoxel::IronOre);
        assert_eq!(loaded.get_state(3, 4, 5), 7);
    }

    #[test]
    fn autosave_disabled_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = autosave_app(IVec3::ZERO, dir.path(), false);

        app.world_mut().write_message(ChunkModified(IVec3::ZERO));
        app.update();

        assert!(!dir.path().join(chunk_filename(IVec3::ZERO)).exists());
    }

    #[test]
    fn autosave_skips_chunks_that_are_not_loaded() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = autosave_app(IVec3::ZERO, dir.path(), true);
        let unloaded = IVec3::new(9, 9, 9);

        app.world_mut().write_message(ChunkModified(unloaded));
        app.update();

        assert!(!dir.path().join(chunk_filename(unloaded)).exists());
    }

    #[test]
    fn plugin_save_dir_follows_world_name() {
        let mut app = App::new();
        app.add_plugins(PersistencePlugin::<DefaultVoxel>::with_world("survival"));
        let config = app.world().resource::<PersistenceConfig>();
        assert_eq!(config.save_dir, "./worlds/survival/chunks");
        assert!(config.auto_save);

        let mut app = App::new();
        app.add_plugins(PersistencePlugin::<DefaultVoxel>::new());
        assert_eq!(
            app.world().resource::<PersistenceConfig>().save_dir,
            PersistenceConfig::default().save_dir
        );
    }

    #[test]
    fn world_data_dir_for_world() {
        assert_eq!(WorldDataDir::for_world("alpha").as_str(), "./worlds/alpha");
        assert_eq!(WorldDataDir::default().as_str(), "./worlds/world");
    }
}
