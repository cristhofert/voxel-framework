//! # voxel-framework
//!
//! A reusable voxel engine library built on [Bevy](https://bevyengine.org/),
//! providing all the infrastructure needed to build voxel-based games and
//! simulations: chunk storage, procedural generation, greedy meshing, world
//! streaming, PBR rendering, and player interaction.
//!
//! ## Overview
//!
//! The framework is built around three core abstractions:
//!
//! - **[`VoxelType`](crate::voxel::VoxelType)** — a trait that every voxel palette must implement.
//!   It describes whether a voxel is solid, what color it renders as, and how
//!   it serializes to and from a single byte for persistence. The built-in
//!   [`voxel::DefaultVoxel`] palette covers five material types (Air, Rock,
//!   IronOre, SiliconCrystal, RareMineral) and is ready to use out of the box.
//!
//! - **[`chunk::Chunk<V>`]** — the fundamental storage unit. A 16×16×16 flat
//!   array of voxels stored on the heap. All generation, meshing, and I/O
//!   operate on chunks identified by a [`chunk::ChunkCoord`] (an `IVec3`
//!   where one unit equals `CHUNK_SIZE` world-space units).
//!
//! - **[`generator::ChunkGenerator`]** — an async-safe trait implemented by
//!   procedural generators. Generators run on a Bevy `AsyncComputeTaskPool`
//!   thread and produce fully populated [`chunk::Chunk<V>`] values, which are
//!   then handed back to the main world for meshing and insertion.
//!
//! ## Plugin Architecture
//!
//! Two plugins cover most use cases:
//!
//! | Plugin | Purpose |
//! |--------|---------|
//! | [`VoxelCorePlugin`] | Engine systems only — chunk management, streaming, meshing, rendering, mining, placement. No game-specific content. |
//! | [`VoxelPlugin`] | Everything in [`VoxelCorePlugin`] plus game content: skybox, audio, hotbar, UI, fragment entities. |
//!
//! [`generator::GeneratorPlugin`] is always **opt-in** because it is generic
//! over `V: VoxelType` and requires the caller to supply a
//! [`generator::GeneratorResource`] holding a concrete generator. Add it
//! separately after either core plugin.
//!
//! ## The Chunk Pipeline
//!
//! The full data flow from viewer movement to rendered geometry:
//!
//! ```text
//! ViewerPosition update
//!        │
//!        ▼
//! WorldPlugin — streaming system
//!   Computes which chunks are within the configured render distance.
//!   Emits ChunkRequested events for chunks not yet loaded.
//!        │
//!        ▼
//! GeneratorPlugin — async generation (AsyncComputeTaskPool)
//!   Spawns a task per requested chunk. Each task calls
//!   ChunkGenerator::generate(coord) → Chunk<V>.
//!   When the task completes, inserts the Chunk<V> component and emits
//!   ChunkModified so downstream systems are notified.
//!        │
//!        ▼
//! MeshPlugin — greedy mesh generation
//!   Reacts to ChunkModified. Iterates the chunk with a configurable LOD
//!   step, runs the greedy meshing algorithm, and uploads a Bevy Mesh asset.
//!        │
//!        ▼
//! RenderPlugin — PBR material + entity spawn
//!   Attaches a shared chunk PBR material and a MeshBundle to the chunk
//!   entity so Bevy's renderer picks it up on the next frame.
//!        │
//!        ▼
//! OcclusionPlugin — visibility culling
//!   Each frame, tests chunk entities against the camera frustum and sets
//!   Visibility accordingly to skip GPU draw calls for off-screen chunks.
//! ```
//!
//! ## Quick Start
//!
//! A minimal working world using the built-in asteroid generator:
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use voxel_framework::{
//!     VoxelPlugin,
//!     generator::{GeneratorPlugin, GeneratorResource},
//!     generator::asteroid::{AsteroidConfig, AsteroidGenerator},
//!     voxel::DefaultVoxel,
//! };
//! use std::sync::Arc;
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
//!     .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())
//!     .insert_resource(GeneratorResource::<DefaultVoxel> {
//!         generator: Arc::new(AsteroidGenerator::new(AsteroidConfig::default())),
//!     })
//!     .run();
//! ```
//!
//! Use [`VoxelCorePlugin`] instead of [`VoxelPlugin`] when you want the engine
//! layer without the bundled game content (skybox, audio, hotbar, UI).
//!
//! ## Extending the Framework
//!
//! To use a custom voxel palette, implement [`VoxelType`](crate::voxel::VoxelType) on your own type and
//! substitute it wherever the framework is generic over `V: VoxelType`:
//!
//! - [`generator::GeneratorPlugin::<YourVoxel>::new()`][`generator::GeneratorPlugin`]
//! - [`mesh::MeshPlugin::<YourVoxel>::new()`][`mesh::MeshPlugin`]
//! - [`render::RenderPlugin::<YourVoxel>::new()`][`render::RenderPlugin`]
//!
//! To drive generation, implement [`generator::ChunkGenerator`] and wrap it in
//! a [`generator::GeneratorResource`].
//!
//! See [`prelude`] for the most commonly used re-exports.

pub mod audio;
pub mod chunk;
pub mod crafting;
pub mod fragments;
pub mod generator;
pub mod items;
pub mod mesh;
pub mod mobs;
pub mod noise;
pub mod player;
pub mod prelude;
pub mod profile;
pub mod render;
pub mod settings;
pub mod skybox;
pub mod ui;
pub mod voxel;
pub mod world;

use bevy::prelude::*;

use voxel::VoxelType;
use audio::SfxPlugin;
use fragments::FragmentPlugin;
use items::hotbar::HotbarPlugin;
use mesh::MeshPlugin;
use render::RenderPlugin;
use skybox::SkyboxPlugin;
use ui::UiPlugin;
use voxel::DefaultVoxel;
use world::loading::LoadingPlugin;
use world::mining::MiningPlugin;
use world::placement::PlacementPlugin;
use world::occlusion::OcclusionPlugin;
use world::{WorldConfigPlugin, WorldPlugin};

/// Engine-only plugin with no game-specific content.
///
/// `VoxelCorePlugin<V>` wires up the complete voxel engine layer — world
/// streaming, chunk lifecycle, meshing, PBR rendering, occlusion culling, and
/// player interactions — without any game-specific subsystems (skybox, audio,
/// hotbar, UI). It is the right choice when you are building your own game
/// layer on top of the framework and want full control over what gets added.
///
/// The generic parameter `V` selects the voxel type used by the rendering,
/// meshing, and occlusion systems. Defaults to [`DefaultVoxel`] for backwards
/// compatibility: `VoxelCorePlugin::default()` and
/// `VoxelCorePlugin::<DefaultVoxel>::new()` are equivalent.
///
/// # Included systems
///
/// | Plugin | Responsibility |
/// |--------|----------------|
/// | [`WorldPlugin`] | [`world::ChunkManager`], [`world::systems::ViewerPosition`], chunk lifecycle events, and streaming |
/// | [`WorldConfigPlugin`] | Inserts the [`world::WorldConfig`] resource |
/// | `RenderPlugin<V>` | Shared chunk PBR material and entity spawn |
/// | `MeshPlugin<V>` | Greedy mesh generation and chunk despawn |
/// | `OcclusionPlugin<V>` | Per-chunk frustum culling |
/// | [`world::mining::MiningPlugin`] | Raycast voxel removal |
/// | [`world::placement::PlacementPlugin`] | Raycast voxel placement |
/// | [`world::loading::LoadingPlugin`] | World loading state machine |
///
/// # Not included (opt-in)
///
/// - [`generator::GeneratorPlugin`] — requires the caller to supply a
///   [`generator::GeneratorResource`].
/// - [`FragmentPlugin`], [`SkyboxPlugin`], [`SfxPlugin`], [`HotbarPlugin`],
///   [`UiPlugin`] — use [`VoxelPlugin`] to get all of these at once.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::{VoxelCorePlugin, generator::{GeneratorPlugin, GeneratorResource}};
/// use voxel_framework::generator::asteroid::{AsteroidConfig, AsteroidGenerator};
/// use voxel_framework::voxel::DefaultVoxel;
/// use std::sync::Arc;
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(VoxelCorePlugin::<DefaultVoxel>::new())
///     .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())
///     .insert_resource(GeneratorResource::<DefaultVoxel> {
///         generator: Arc::new(AsteroidGenerator::new(AsteroidConfig::default())),
///     })
///     .run();
/// ```
///
/// # See also
///
/// - [`VoxelPlugin`] — batteries-included variant that also adds game content.
pub struct VoxelCorePlugin<V: VoxelType = DefaultVoxel> {
    _m: std::marker::PhantomData<V>,
}

impl<V: VoxelType> VoxelCorePlugin<V> {
    /// Creates a new [`VoxelCorePlugin`] for voxel type `V`.
    pub fn new() -> Self {
        Self { _m: std::marker::PhantomData }
    }
}

impl<V: VoxelType> Default for VoxelCorePlugin<V> {
    fn default() -> Self { Self::new() }
}

impl<V: VoxelType> Plugin for VoxelCorePlugin<V> {
    fn build(&self, app: &mut App) {
        app.add_plugins(WorldPlugin);
        app.add_plugins(WorldConfigPlugin);
        app.add_plugins(RenderPlugin::<V>::new());
        app.add_plugins(MeshPlugin::<V>::new());
        app.add_plugins(OcclusionPlugin::<V>::new());
        app.add_plugins(MiningPlugin);
        app.add_plugins(PlacementPlugin);
        app.add_plugins(LoadingPlugin);
    }
}

/// Batteries-included plugin that registers the full voxel framework.
///
/// `VoxelPlugin<V>` is the quickest way to add a complete voxel world to a
/// Bevy application. It composes [`VoxelCorePlugin<V>`] (engine systems) with
/// the game-facing subsystems listed below. Prefer [`VoxelCorePlugin`] when
/// you need fine-grained control over which subsystems are active.
///
/// The generic parameter `V` selects the voxel type. Defaults to
/// [`DefaultVoxel`] — `VoxelPlugin::default()` produces an asteroid-palette
/// world, `VoxelPlugin::<DefaultTerrainVoxel>::new()` produces a terrain world.
///
/// # Included beyond [`VoxelCorePlugin`]
///
/// | Plugin | Responsibility |
/// |--------|----------------|
/// | [`FragmentPlugin`] | Floating voxel debris entities ejected on mining |
/// | [`SkyboxPlugin`] | Procedural starfield and skybox rendering |
/// | [`SfxPlugin`] | Positional audio effects for mining and placement |
/// | [`HotbarPlugin`] | Player hotbar, item switching, and held-item state |
/// | [`UiPlugin`] | Crosshair, loading screen, and HUD overlays |
///
/// # Generator not included
///
/// [`generator::GeneratorPlugin`] is intentionally excluded because it requires
/// the caller to supply a concrete [`generator::GeneratorResource`]. Always add
/// it separately:
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::{VoxelPlugin, generator::{GeneratorPlugin, GeneratorResource}};
/// use voxel_framework::generator::terrain::{TerrainConfig, TerrainGenerator};
/// use voxel_framework::voxel::DefaultTerrainVoxel;
/// use std::sync::Arc;
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(VoxelPlugin::<DefaultTerrainVoxel>::new())
///     .add_plugins(GeneratorPlugin::<DefaultTerrainVoxel>::new())
///     .insert_resource(GeneratorResource::<DefaultTerrainVoxel> {
///         generator: Arc::new(TerrainGenerator::new(TerrainConfig::default())),
///     })
///     .run();
/// ```
///
/// # See also
///
/// - [`VoxelCorePlugin`] — engine-only variant without game content.
/// - [`prelude`] — commonly used re-exports for convenience imports.
pub struct VoxelPlugin<V: VoxelType = DefaultVoxel> {
    _m: std::marker::PhantomData<V>,
}

impl<V: VoxelType> VoxelPlugin<V> {
    /// Creates a new [`VoxelPlugin`] for voxel type `V`.
    pub fn new() -> Self {
        Self { _m: std::marker::PhantomData }
    }
}

impl<V: VoxelType> Default for VoxelPlugin<V> {
    fn default() -> Self { Self::new() }
}

impl<V: VoxelType> Plugin for VoxelPlugin<V> {
    fn build(&self, app: &mut App) {
        app.add_plugins(VoxelCorePlugin::<V>::new());
        app.add_plugins(FragmentPlugin);
        app.add_plugins(SkyboxPlugin);
        app.add_plugins(SfxPlugin);
        app.add_plugins(HotbarPlugin);
        app.add_plugins(UiPlugin);
    }
}
