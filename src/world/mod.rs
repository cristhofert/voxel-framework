//! World streaming, chunk management, and lifecycle events.
//!
//! This module provides [`ChunkManager`], the central registry for tracking
//! which chunks are loaded, pending load, or pending unload. It also defines
//! the three lifecycle events that drive the streaming system:
//!
//! - [`ChunkLoadRequest`] — emitted when a chunk coord enters load range.
//! - [`ChunkUnloadRequest`] — emitted when a chunk coord exits unload range.
//! - [`ChunkModified`] — emitted when voxel data in a loaded chunk changes.
//!
//! Register [`WorldPlugin`] to get all resources and events registered on
//! the Bevy `App` automatically.

pub mod config;
pub mod persistence;
pub mod raycast;
pub mod systems;
pub mod voxel_ops;

pub use config::{WorldConfig, WorldConfigPlugin};
pub use persistence::{PersistenceConfig, PersistencePlugin, chunk_autosave_system, try_load_chunk};
pub use raycast::{RaycastHit, raycast};
pub use systems::ViewerPosition;
pub use voxel_ops::{get_voxel_at, set_voxel_at};

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use crate::chunk::ChunkCoord;

// ─── Constants ───────────────────────────────────────────────────────────────

/// Default radius (world units) within which chunks are loaded.
pub const DEFAULT_LOAD_DISTANCE: f32 = 150.0;

/// Default radius (world units) beyond which chunks are unloaded.
///
/// Must be greater than [`DEFAULT_LOAD_DISTANCE`] to create a hysteresis band
/// that prevents rapid load/unload cycling near the boundary.
pub const DEFAULT_UNLOAD_DISTANCE: f32 = 200.0;

// ─── Events ──────────────────────────────────────────────────────────────────

/// Emitted when a chunk coordinate enters the load range and should be spawned.
///
/// Consumers (e.g., a generation system) should respond by generating voxel
/// data and spawning the corresponding entity, then registering it with
/// [`ChunkManager::register`].
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkLoadRequest(pub ChunkCoord);

/// Emitted when a chunk coordinate exits the unload range and should be despawned.
///
/// Consumers should despawn the entity and remove it from [`ChunkManager`].
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkUnloadRequest(pub ChunkCoord);

/// Emitted when voxel data in a loaded chunk has been modified.
///
/// Consumers (e.g., mesh generation) should respond by regenerating the mesh
/// for this chunk and its affected neighbors.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkModified(pub ChunkCoord);

// ─── ChunkManager ─────────────────────────────────────────────────────────────

/// Central registry for the chunk streaming system.
///
/// Tracks which chunks are fully loaded (have an entity with mesh), which are
/// pending load (queued or in-flight), and the configured streaming distances.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::world::ChunkManager;
///
/// fn my_system(mut manager: ResMut<ChunkManager>) {
///     let coord = IVec3::new(0, 0, 0);
///     manager.mark_pending(coord);
/// }
/// ```
#[derive(Resource, Debug)]
pub struct ChunkManager {
    /// Fully loaded chunks: coord → Bevy entity with voxel mesh.
    loaded_chunks: HashMap<IVec3, Entity>,
    /// Chunk coords that have been requested but not yet fully loaded.
    pending_load: HashSet<IVec3>,
    /// Distance (world units) within which chunks are loaded.
    pub load_distance: f32,
    /// Distance (world units) beyond which chunks are unloaded.
    pub unload_distance: f32,
}

impl ChunkManager {
    /// Creates a new `ChunkManager` with the given streaming distances.
    pub fn new(load_distance: f32, unload_distance: f32) -> Self {
        Self {
            loaded_chunks: HashMap::new(),
            pending_load: HashSet::new(),
            load_distance,
            unload_distance,
        }
    }

    /// Returns `true` if the chunk at `coord` is fully loaded.
    #[inline]
    pub fn is_loaded(&self, coord: IVec3) -> bool {
        self.loaded_chunks.contains_key(&coord)
    }

    /// Returns `true` if the chunk at `coord` is queued or in-flight.
    #[inline]
    pub fn is_pending(&self, coord: IVec3) -> bool {
        self.pending_load.contains(&coord)
    }

    /// Returns the Bevy entity for a fully loaded chunk, if any.
    #[inline]
    pub fn get_entity(&self, coord: IVec3) -> Option<Entity> {
        self.loaded_chunks.get(&coord).copied()
    }

    /// Marks a chunk coord as pending load.
    ///
    /// No-op if the chunk is already loaded or already pending.
    pub fn mark_pending(&mut self, coord: IVec3) {
        if !self.is_loaded(coord) {
            self.pending_load.insert(coord);
        }
    }

    /// Registers a fully loaded chunk with its Bevy entity.
    ///
    /// Removes the coord from the pending set if it was there.
    pub fn register(&mut self, coord: IVec3, entity: Entity) {
        self.pending_load.remove(&coord);
        self.loaded_chunks.insert(coord, entity);
    }

    /// Unregisters a chunk, removing it from loaded and pending sets.
    ///
    /// Returns the entity if it was loaded, `None` otherwise.
    pub fn unregister(&mut self, coord: IVec3) -> Option<Entity> {
        self.pending_load.remove(&coord);
        self.loaded_chunks.remove(&coord)
    }

    /// Returns an iterator over all loaded chunk coordinates and their entities.
    pub fn iter_loaded(&self) -> impl Iterator<Item = (IVec3, Entity)> + '_ {
        self.loaded_chunks.iter().map(|(&coord, &entity)| (coord, entity))
    }

    /// Returns the number of fully loaded chunks.
    #[inline]
    pub fn loaded_count(&self) -> usize {
        self.loaded_chunks.len()
    }

    /// Returns the number of chunks queued for loading.
    #[inline]
    pub fn pending_count(&self) -> usize {
        self.pending_load.len()
    }
}

impl Default for ChunkManager {
    fn default() -> Self {
        Self::new(DEFAULT_LOAD_DISTANCE, DEFAULT_UNLOAD_DISTANCE)
    }
}

// ─── WorldPlugin ─────────────────────────────────────────────────────────────

/// Plugin that registers world streaming resources and events.
///
/// Adds [`ChunkManager`] as an [`init_resource`] and registers the three
/// lifecycle events: [`ChunkLoadRequest`], [`ChunkUnloadRequest`], and
/// [`ChunkModified`].
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::world::WorldPlugin;
///
/// App::new()
///     .add_plugins(WorldPlugin)
///     .run();
/// ```
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkManager>();
        app.init_resource::<ViewerPosition>();
        app.add_message::<ChunkLoadRequest>();
        app.add_message::<ChunkUnloadRequest>();
        app.add_message::<ChunkModified>();
        app.add_systems(Update, systems::streaming_system);
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> ChunkManager {
        ChunkManager::default()
    }

    #[test]
    fn new_manager_is_empty() {
        let m = manager();
        assert_eq!(m.loaded_count(), 0);
        assert_eq!(m.pending_count(), 0);
    }

    #[test]
    fn mark_pending_adds_to_pending_set() {
        let mut m = manager();
        let coord = IVec3::new(1, 0, 0);
        m.mark_pending(coord);
        assert!(m.is_pending(coord));
        assert!(!m.is_loaded(coord));
        assert_eq!(m.pending_count(), 1);
    }

    #[test]
    fn register_promotes_pending_to_loaded() {
        let mut m = manager();
        let coord = IVec3::new(2, 0, 0);
        m.mark_pending(coord);

        // Use bits to construct a fake Entity for testing.
        let entity = Entity::from_bits(1);
        m.register(coord, entity);

        assert!(m.is_loaded(coord));
        assert!(!m.is_pending(coord));
        assert_eq!(m.get_entity(coord), Some(entity));
        assert_eq!(m.loaded_count(), 1);
        assert_eq!(m.pending_count(), 0);
    }

    #[test]
    fn unregister_removes_loaded_chunk() {
        let mut m = manager();
        let coord = IVec3::new(3, 0, 0);
        let entity = Entity::from_bits(2);
        m.register(coord, entity);

        let removed = m.unregister(coord);
        assert_eq!(removed, Some(entity));
        assert!(!m.is_loaded(coord));
        assert_eq!(m.loaded_count(), 0);
    }

    #[test]
    fn mark_pending_no_op_if_already_loaded() {
        let mut m = manager();
        let coord = IVec3::new(4, 0, 0);
        let entity = Entity::from_bits(3);
        m.register(coord, entity);

        // Marking a loaded chunk pending should be a no-op.
        m.mark_pending(coord);
        assert!(!m.is_pending(coord));
        assert!(m.is_loaded(coord));
    }

    #[test]
    fn iter_loaded_yields_all_registered_chunks() {
        let mut m = manager();
        let coords: Vec<IVec3> = (0..5).map(|i| IVec3::new(i, 0, 0)).collect();

        for (i, &coord) in coords.iter().enumerate() {
            m.register(coord, Entity::from_bits(i as u64 + 1));
        }

        let loaded: Vec<IVec3> = m.iter_loaded().map(|(c, _)| c).collect();
        assert_eq!(loaded.len(), coords.len());
        for coord in &coords {
            assert!(loaded.contains(coord));
        }
    }

    #[test]
    fn default_distances_match_constants() {
        let m = manager();
        assert_eq!(m.load_distance, DEFAULT_LOAD_DISTANCE);
        assert_eq!(m.unload_distance, DEFAULT_UNLOAD_DISTANCE);
    }
}
