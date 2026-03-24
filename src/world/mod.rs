//! World streaming, chunk lifecycle, and runtime configuration.
//!
//! # Overview
//!
//! The world module owns three concerns:
//!
//! - **Chunk registry** — [`ChunkManager`] tracks which chunks are fully
//!   loaded, which are in-flight, and the configured streaming distances.
//! - **Viewer tracking** — [`ViewerPosition`] is a lightweight resource that
//!   your player/camera system updates every frame. The streaming system reads
//!   it to decide which chunks to load or unload.
//! - **Lifecycle events** — [`ChunkLoadRequest`], [`ChunkUnloadRequest`], and
//!   [`ChunkModified`] are the message bus between the streaming system and
//!   the rest of the engine (generation, meshing, persistence).
//!
//! Add [`WorldPlugin`] to your [`App`] to register all of the above
//! automatically.
//!
//! # The Streaming Pipeline
//!
//! Every frame the [`systems::streaming_system`] executes the following steps:
//!
//! ```text
//! ┌─────────────────────┐
//! │   ViewerPosition    │  updated by your camera/player system each frame
//! └────────┬────────────┘
//!          │ read by streaming_system
//!          ▼
//! ┌─────────────────────┐
//! │  Distance check     │  iterate cube of radius ceil(load_distance/CHUNK_SIZE)
//! │  around viewer      │  compute world-space center distance per coord
//! └────────┬────────────┘
//!          │
//!     ┌────┴─────┐
//!     │          │
//!     ▼          ▼
//! ChunkLoad   ChunkUnload
//! Request     Request
//!  (enter      (exit
//!  sphere)     sphere)
//!     │          │
//!     ▼          ▼
//! Generator  Despawn
//! system     system
//! (async)
//!     │
//!     ▼
//! ChunkModified  ←── voxel edits (mining, placement, etc.)
//!     │
//!     ▼
//! Mesh rebuild system
//! ```
//!
//! 1. [`ViewerPosition`] is updated by your player/camera system.
//! 2. [`systems::streaming_system`] converts it to chunk-grid space and
//!    iterates every coordinate within a bounding cube of radius
//!    `ceil(load_distance / CHUNK_SIZE)`.
//! 3. For each coordinate inside the load sphere that is neither loaded nor
//!    pending, the system emits [`ChunkLoadRequest`] and marks the coord
//!    pending in [`ChunkManager`].
//! 4. For each currently loaded chunk whose center has drifted beyond
//!    `unload_distance`, the system emits [`ChunkUnloadRequest`].
//! 5. An external generation system receives [`ChunkLoadRequest`], generates
//!    voxel data asynchronously, spawns the entity, and calls
//!    [`ChunkManager::register`].
//! 6. Whenever voxel data changes (mining, placement, etc.), the responsible
//!    system emits [`ChunkModified`], which triggers mesh regeneration for
//!    that chunk and its affected neighbors.
//!
//! # Events and Messages
//!
//! | Event | When it fires | Who consumes it |
//! |---|---|---|
//! | [`ChunkLoadRequest`] | Chunk coord enters `load_distance` sphere | Generation system |
//! | [`ChunkUnloadRequest`] | Loaded chunk center exits `unload_distance` sphere | Despawn system |
//! | [`ChunkModified`] | Voxel data written to a loaded chunk | Mesh rebuild system |
//!
//! # World Configuration
//!
//! [`config::WorldConfig`] is loaded from `world.toml` in the working
//! directory at startup by [`config::WorldConfigPlugin`]. It is inserted as a
//! Bevy [`Resource`] and its `load_distance` / `unload_distance` values are
//! applied directly to [`ChunkManager`]. If the file is absent or malformed,
//! [`config::WorldConfig::default`] is used silently.

pub mod collision;
pub mod config;
pub mod loading;
pub mod mining;
pub mod occlusion;
pub mod persistence;
pub mod placement;
pub mod raycast;
pub mod systems;
pub mod daycycle;
pub mod voxel_ops;

pub use daycycle::{DayCycleConfig, DayCyclePlugin, SunLight};
pub use collision::{
    capsule_overlaps_voxels, resolve_capsule_collision, resolve_sphere_collision,
    sphere_overlaps_voxels,
};
pub use config::{WorldConfig, WorldConfigPlugin};
pub use loading::{LoadingPlugin, LoadingProgress, WorldLoadingState, WorldReady};
pub use occlusion::{OcclusionPlugin, is_chunk_occluded};
pub use persistence::{PersistenceConfig, PersistencePlugin, chunk_autosave_system, try_load_chunk};
pub use raycast::{RaycastHit, raycast};
pub use mining::{
    tick_mining, stop_mining, mining_input_system,
    MiningConfig, MiningInputPlugin, MiningPlugin, MiningProgress, MiningState, VoxelBroken,
};
pub use placement::{
    try_place_voxel, tick_placement, placement_input_system,
    PlacementConfig, PlacementInputPlugin, PlacementPlugin, PlacementState, VoxelPlaced,
};
pub use systems::ViewerPosition;
pub use voxel_ops::{get_voxel_at, set_voxel_at};

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use crate::chunk::ChunkCoord;

// ─── Constants ───────────────────────────────────────────────────────────────

/// Default radius in world units within which chunks are loaded (150.0).
///
/// Used as the initial value for [`ChunkManager::load_distance`] when no
/// [`config::WorldConfig`] overrides it. Can be overridden per-instance via
/// [`ChunkManager::new`] or by loading a `world.toml` with
/// [`config::WorldConfigPlugin`].
pub const DEFAULT_LOAD_DISTANCE: f32 = 150.0;

/// Default radius in world units beyond which chunks are unloaded (200.0).
///
/// Used as the initial value for [`ChunkManager::unload_distance`]. The
/// 50-unit gap between this constant and [`DEFAULT_LOAD_DISTANCE`] creates a
/// **hysteresis band**: chunks that have been loaded are not immediately
/// unloaded when the viewer steps back past the load boundary. This prevents
/// rapid load/unload cycling ("thrashing") near the edge of the streaming
/// sphere.
pub const DEFAULT_UNLOAD_DISTANCE: f32 = 200.0;

// ─── Events ──────────────────────────────────────────────────────────────────

/// Fired by [`systems::streaming_system`] when a chunk coordinate enters the
/// load sphere and must be generated and spawned.
///
/// The wrapped [`ChunkCoord`] identifies the chunk-grid position that should
/// be populated. Upon receiving this event, a generation system should:
///
/// 1. Generate voxel data for the coordinate (typically on a thread pool).
/// 2. Spawn a Bevy entity with the resulting mesh and voxel components.
/// 3. Call [`ChunkManager::register`] with the coord and new entity so the
///    streaming system considers the chunk fully loaded.
///
/// The coord is guaranteed to be neither already loaded nor pending at the
/// moment the event is written.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkLoadRequest(pub ChunkCoord);

/// Fired by [`systems::streaming_system`] when a loaded chunk's world-space
/// center drifts beyond [`ChunkManager::unload_distance`].
///
/// The wrapped [`ChunkCoord`] identifies the chunk to remove. Upon receiving
/// this event, a despawn system should despawn the entity returned by
/// [`ChunkManager::get_entity`] and call [`ChunkManager::unregister`] so the
/// coord becomes eligible for reloading in the future.
///
/// Note that the 50-unit hysteresis gap between [`DEFAULT_LOAD_DISTANCE`] and
/// [`DEFAULT_UNLOAD_DISTANCE`] means this event fires noticeably later than
/// the chunk first became "out of load range".
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkUnloadRequest(pub ChunkCoord);

/// Fired whenever voxel data inside a loaded chunk is written.
///
/// The wrapped [`ChunkCoord`] identifies the modified chunk. Consumers
/// — typically the mesh rebuild system — should regenerate the mesh for this
/// chunk and re-evaluate the face visibility of its six face-adjacent
/// neighbors, because a voxel change at a chunk boundary affects the neighbor's
/// exposed faces as well.
///
/// This event is emitted by high-level operations such as [`mining`] and
/// [`placement`], and also by any system that calls [`voxel_ops::set_voxel_at`]
/// directly.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkModified(pub ChunkCoord);

// ─── ChunkManager ─────────────────────────────────────────────────────────────

/// Central registry that tracks the state of every chunk in the streaming system.
///
/// `ChunkManager` is inserted as a Bevy [`Resource`] by [`WorldPlugin`] and
/// updated by [`systems::streaming_system`] every frame. It maintains two
/// internal sets:
///
/// - **Loaded** — chunks that have a live Bevy entity with a built mesh. Query
///   membership with [`is_loaded`](ChunkManager::is_loaded) and retrieve the
///   entity with [`get_entity`](ChunkManager::get_entity).
/// - **Pending** — chunk coordinates for which a [`ChunkLoadRequest`] has been
///   emitted but whose generation is not yet complete. Query membership with
///   [`is_pending`](ChunkManager::is_pending).
///
/// The two public distance fields, [`load_distance`](ChunkManager::load_distance)
/// and [`unload_distance`](ChunkManager::unload_distance), are read every frame
/// by the streaming system. They default to [`DEFAULT_LOAD_DISTANCE`] and
/// [`DEFAULT_UNLOAD_DISTANCE`] respectively, and can be overridden at runtime
/// or by loading a [`config::WorldConfig`].
///
/// # Querying chunks
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::world::ChunkManager;
///
/// fn inspect_chunks(manager: Res<ChunkManager>) {
///     println!("loaded: {}, pending: {}", manager.loaded_count(), manager.pending_count());
///
///     for (coord, entity) in manager.iter_loaded() {
///         println!("chunk {:?} → entity {:?}", coord, entity);
///     }
/// }
/// ```
///
/// # Registering a generated chunk
///
/// After generating voxel data and spawning the entity, call
/// [`register`](ChunkManager::register) to promote the coord from pending to
/// loaded:
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::world::ChunkManager;
///
/// fn finish_generation(mut manager: ResMut<ChunkManager>, mut commands: Commands) {
///     let coord = IVec3::new(1, 0, 0);
///     let entity = commands.spawn_empty().id();
///     manager.register(coord, entity);
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

/// Plugin that wires up the world streaming system.
///
/// Registers the following on the Bevy [`App`]:
///
/// - [`ChunkManager`] resource (via `init_resource`, defaults to
///   [`DEFAULT_LOAD_DISTANCE`] / [`DEFAULT_UNLOAD_DISTANCE`]).
/// - [`ViewerPosition`] resource (via `init_resource`, defaults to the origin).
/// - [`ChunkLoadRequest`], [`ChunkUnloadRequest`], and [`ChunkModified`]
///   messages.
/// - [`systems::streaming_system`] scheduled in [`Update`].
///
/// For configuration loading, add [`config::WorldConfigPlugin`] as well. For
/// initial-load progress tracking, add [`loading::LoadingPlugin`].
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::world::{WorldPlugin, config::WorldConfigPlugin};
///
/// App::new()
///     .add_plugins((WorldPlugin, WorldConfigPlugin))
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
