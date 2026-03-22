//! Procedural chunk generation traits and plugin.
//!
//! This module defines the [`ChunkGenerator`] trait for implementing procedural
//! world generation strategies, and provides the [`GeneratorPlugin`] for Bevy
//! integration. The [`AsteroidGenerator`] is the default implementation that
//! creates asteroid-shaped voxel formations.

pub mod asteroid;
pub mod systems;

pub use systems::{ChunkTask, chunk_request_system, chunk_task_poll_system};

use std::sync::Arc;

use bevy::prelude::*;

use crate::chunk::Chunk;
use crate::voxel::VoxelType;

/// Trait for procedural chunk generation strategies.
///
/// Implementors produce voxel data for a given chunk coordinate.
/// The framework calls `generate` on background threads via `AsyncComputeTaskPool`,
/// so implementations must be `Send + Sync` and must not access the Bevy `World`.
pub trait ChunkGenerator<V: VoxelType>: Send + Sync + 'static {
    /// Fill the chunk at the given coordinate with voxel data.
    ///
    /// Called on a background thread. Must not access the Bevy World.
    /// The `coord` is in chunk-grid space (each unit = `CHUNK_SIZE` world units).
    fn generate(&self, coord: IVec3, chunk: &mut Chunk<V>);
}

/// Resource wrapping the active chunk generator.
///
/// Users insert this resource into the Bevy app to provide the generation
/// strategy. It holds an `Arc<dyn ChunkGenerator<V>>` so it can be cheaply
/// cloned into async tasks.
#[derive(Resource)]
pub struct GeneratorResource<V: VoxelType> {
    /// The active chunk generator, wrapped in an `Arc` for thread-safe sharing.
    pub generator: Arc<dyn ChunkGenerator<V>>,
}

/// Plugin that registers the generator system.
///
/// Users must insert [`GeneratorResource<V>`] into the app separately,
/// either before or after adding this plugin.
pub struct GeneratorPlugin<V: VoxelType> {
    _marker: std::marker::PhantomData<V>,
}

impl<V: VoxelType> GeneratorPlugin<V> {
    /// Creates a new `GeneratorPlugin`.
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
            ),
        );
    }
}
