//! Procedural chunk generation trait, async pipeline, and Bevy plugin.
//!
//! # Architecture
//!
//! World generation is split across two systems that run every frame inside
//! [`GeneratorSystems`]:
//!
//! 1. **[`chunk_request_system`]** — receives [`crate::world::ChunkLoadRequest`]
//!    messages.  It first tries to restore the chunk from disk (when
//!    [`crate::world::PersistenceConfig`] is present); on a cache miss it
//!    clones the [`Arc`]-wrapped generator and offloads work to Bevy's
//!    [`bevy::tasks::AsyncComputeTaskPool`], storing the returned [`Task`] as a
//!    [`ChunkTask<V>`] component on a new entity.
//!
//! 2. **[`chunk_task_poll_system`]** — polls every in-flight [`ChunkTask<V>`]
//!    with a single non-blocking `poll_once`.  When a task resolves it removes
//!    the task component, inserts the completed [`crate::chunk::Chunk<V>`], and
//!    emits [`crate::world::ChunkModified`] so that the mesh system in
//!    [`crate::mesh`] can build geometry on the same frame.
//!
//! # The `ChunkGenerator` contract
//!
//! Every implementation of [`ChunkGenerator<V>`] runs on a worker thread from
//! `AsyncComputeTaskPool`.  It **must not** access the Bevy [`bevy::ecs::world::World`]
//! or any `Res<T>` — the generator `Arc` is the only shared state it may touch.
//! The trait bounds `Send + Sync + 'static` enforce this at compile time.
//!
//! # Wiring into a Bevy app
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use bevy::prelude::*;
//! use voxel_framework::{
//!     generator::{GeneratorPlugin, GeneratorResource},
//!     mesh::MeshPlugin,
//!     voxel::DefaultVoxel,
//!     VoxelPlugin,
//! };
//! use voxel_framework::generator::asteroid::{AsteroidConfig, AsteroidGenerator};
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
//!     .insert_resource(GeneratorResource::<DefaultVoxel> {
//!         generator: Arc::new(AsteroidGenerator::new(AsteroidConfig::default())),
//!     })
//!     .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())
//!     .add_plugins(MeshPlugin::<DefaultVoxel>::new())
//!     .run();
//! ```
//!
//! [`Arc`]: std::sync::Arc
//! [`Task`]: bevy::tasks::Task

pub mod asteroid;
pub mod region;
pub mod systems;
pub mod terrain;

pub use region::{MultiGenerator, MultiGeneratorPlugin, RegionGenerator};
pub use systems::{ChunkTask, chunk_request_system, chunk_task_poll_system};

use std::sync::Arc;

use bevy::prelude::*;

use crate::chunk::Chunk;
use crate::voxel::VoxelType;

/// System set label for the generator pipeline.
///
/// `chunk_mesh_system` uses `.after(GeneratorSystems)` to guarantee it reads
/// `ChunkModified` messages emitted in the same frame by `chunk_task_poll_system`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct GeneratorSystems;

/// Contract for procedural chunk generation strategies.
///
/// An implementation receives an empty [`Chunk<V>`] and a chunk-grid coordinate,
/// and fills the chunk with whatever voxel data the strategy produces.  The
/// framework stores implementations behind an [`std::sync::Arc`] and calls
/// [`ChunkGenerator::generate`] on worker threads from Bevy's
/// [`bevy::tasks::AsyncComputeTaskPool`].
///
/// # Thread safety contract
///
/// `generate` is called from a **background thread** — never from the Bevy main
/// thread.  Implementations **must not**:
///
/// - access any [`bevy::ecs::world::World`] data (queries, resources, etc.),
/// - hold non-`Send` types, or
/// - perform blocking I/O on the calling thread.
///
/// The `Send + Sync + 'static` bounds on the trait enforce these constraints at
/// compile time.  Internal state may be freely read (shared references via
/// `Arc`), but must not be mutated without interior synchronization.
///
/// # Coordinate space
///
/// `coord` is in **chunk-grid space**: each axis unit corresponds to
/// `CHUNK_SIZE` world units.  Convert to world space with
/// `coord.as_vec3() * CHUNK_SIZE as f32`.
///
/// # Examples
///
/// ```rust,no_run
/// use bevy::math::IVec3;
/// use voxel_framework::{
///     chunk::Chunk,
///     generator::ChunkGenerator,
///     voxel::DefaultVoxel,
/// };
///
/// struct FlatWorldGenerator {
///     ground_level: i32,
/// }
///
/// impl ChunkGenerator<DefaultVoxel> for FlatWorldGenerator {
///     fn generate(&self, coord: IVec3, chunk: &mut Chunk<DefaultVoxel>) {
///         // Fill voxels below ground_level with rock, leave the rest as air.
///         if coord.y < self.ground_level {
///             for x in 0..voxel_framework::chunk::CHUNK_SIZE {
///                 for y in 0..voxel_framework::chunk::CHUNK_SIZE {
///                     for z in 0..voxel_framework::chunk::CHUNK_SIZE {
///                         chunk.set(x, y, z, DefaultVoxel::Rock);
///                     }
///                 }
///             }
///         }
///     }
/// }
/// ```
pub trait ChunkGenerator<V: VoxelType>: Send + Sync + 'static {
    /// Fills `chunk` with voxel data for the chunk at `coord`.
    ///
    /// `coord` is in chunk-grid space; each unit equals `CHUNK_SIZE` world units.
    ///
    /// **Called on a background thread. Must not access the Bevy `World`.**
    fn generate(&self, coord: IVec3, chunk: &mut Chunk<V>);
}

/// Bevy [`Resource`] that holds the active [`ChunkGenerator<V>`].
///
/// Insert this resource before adding [`GeneratorPlugin<V>`] to provide the
/// generation strategy.  The inner [`std::sync::Arc`] is cloned cheaply into
/// each async task so that the generator is shared without blocking or copying.
///
/// ```rust,no_run
/// use std::sync::Arc;
/// use bevy::prelude::*;
/// use voxel_framework::{
///     generator::GeneratorResource,
///     voxel::DefaultVoxel,
/// };
/// use voxel_framework::generator::asteroid::{AsteroidConfig, AsteroidGenerator};
///
/// fn main() {
///     App::new()
///         .insert_resource(GeneratorResource::<DefaultVoxel> {
///             generator: Arc::new(AsteroidGenerator::new(AsteroidConfig::default())),
///         })
///         // ... add other plugins ...
///         .run();
/// }
/// ```
#[derive(Resource)]
pub struct GeneratorResource<V: VoxelType> {
    /// The active chunk generator, shared across worker threads via [`std::sync::Arc`].
    pub generator: Arc<dyn ChunkGenerator<V>>,
}

/// Bevy [`Plugin`] that registers the async chunk generation pipeline for
/// voxel type `V`.
///
/// Adds two systems to the [`Update`] schedule inside [`GeneratorSystems`],
/// chained so that poll results are available to downstream systems in the same
/// frame:
///
/// 1. [`chunk_request_system`] — converts [`crate::world::ChunkLoadRequest`]
///    messages into async tasks (or immediate disk loads).
/// 2. [`chunk_task_poll_system`] — harvests completed tasks and attaches
///    [`crate::chunk::Chunk<V>`] components.
///
/// [`GeneratorResource<V>`] must be inserted separately; the plugin does not
/// require it at registration time (the system accepts it as `Option<Res<_>>`).
pub struct GeneratorPlugin<V: VoxelType> {
    _marker: std::marker::PhantomData<V>,
}

impl<V: VoxelType> GeneratorPlugin<V> {
    /// Creates a new [`GeneratorPlugin`] for voxel type `V`.
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<V: VoxelType> Default for GeneratorPlugin<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: VoxelType> Plugin for GeneratorPlugin<V> {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                systems::chunk_request_system::<V>,
                systems::chunk_task_poll_system::<V>,
            )
                .chain()
                .in_set(GeneratorSystems),
        );
    }
}
