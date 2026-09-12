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
//! - **[`voxel::VoxelType`]** — a trait that every voxel palette must implement.
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
//! ## Plugin Catalog Architecture
//!
//! `voxel-framework` exposes a flat catalog of Bevy plugins grouped by
//! responsibility. Game crates compose the plugins they need instead of
//! depending on a hidden monolithic game plugin:
//!
//! | Category | Purpose | Examples |
//! |----------|---------|----------|
//! | Engine | Foundation for streamable/renderable voxel worlds | [`world::WorldPlugin`], [`mesh::MeshPlugin`], [`render::RenderPlugin`] |
//! | World Data | Chunk sources, generation, persistence, lifecycle data | [`generator::GeneratorPlugin`], [`world::PersistencePlugin`] |
//! | Gameplay | Reusable rules and interactions | [`content::VoxelContentPlugin`], [`stats::VoxelStatsPlugin`], [`effects::VoxelEffectsPlugin`], [`world::mining::MiningPlugin`], [`world::placement::PlacementPlugin`], [`player::PlayerPlugin`] |
//! | Default Implementation | Replaceable UI/audio/visual defaults | [`ui::UiPlugin`], [`ui::VoxelHudPlugin`], [`skybox::SkyboxPlugin`], [`audio::DefaultAudioPlugin`] |
//! | Convenience | Compatibility bundles, input adapters, recipes, prototyping helpers | [`VoxelCorePlugin`], [`VoxelPlugin`] |
//!
//! These categories are also represented by marker traits in
//! [`plugin_category`]. They are documentation and API classification tools;
//! Bevy composition still uses the normal [`Plugin`] trait.
//!
//! [`VoxelCorePlugin`] and [`VoxelPlugin`] remain available as convenience
//! bundles for compatibility and quick starts, but they are not the primary
//! architecture. New domain behavior should normally be modeled as an explicit
//! Engine, World Data, Gameplay, or Default Implementation plugin.
//!
//! [`generator::GeneratorPlugin`] is always **opt-in** because it is generic
//! over `V: VoxelType` and requires the caller to supply a
//! [`generator::GeneratorResource`] holding a concrete generator. Add it
//! separately after either a bundle or explicit engine plugins.
//!
//! [`ui::VoxelHudPlugin`] is also explicit and remains outside [`ui::UiPlugin`]
//! and the convenience bundles. Authors register domain-neutral presentation
//! definitions, publish screen-ready values, and may schedule same-frame
//! publishers in [`ui::HudSystemSet::Publish`]. The retained renderer preserves
//! element identity and safely hides unresolved or unavailable asset inputs.
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
//! OcclusionPlugin — neighbor-solid occlusion
//!   Reacts to ChunkModified and hides chunks whose six face-adjacent
//!   neighbours are all loaded and fully solid.
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
//! Use explicit catalog plugins instead of [`VoxelPlugin`] when you want full
//! control over engine, world data, gameplay, and presentation defaults.
//!
//! ## Extending the Framework
//!
//! To use a custom voxel palette, implement [`voxel::VoxelType`] on your own type and
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

#![deny(rustdoc::broken_intra_doc_links)]

pub mod audio;
pub mod chunk;
pub mod content;
#[cfg(feature = "crafting")]
pub mod crafting;
pub mod effects;
pub mod fragments;
pub mod generator;
#[cfg(feature = "items")]
pub mod items;
pub mod mesh;
#[cfg(feature = "mobs")]
pub mod mobs;
pub mod noise;
pub mod player;
pub mod plugin_category;
pub mod prelude;
pub mod profile;
pub mod render;
pub mod settings;
pub mod skybox;
pub mod stats;
pub mod ui;
pub mod voxel;
pub mod world;

use bevy::prelude::*;

use audio::SfxPlugin;
use fragments::FragmentPlugin;
#[cfg(feature = "items")]
use items::hotbar::HotbarPlugin;
use mesh::MeshPlugin;
use render::RenderPlugin;
use skybox::SkyboxPlugin;
use ui::UiPlugin;
use voxel::DefaultVoxel;
use voxel::VoxelType;
use world::loading::LoadingPlugin;
use world::mining::MiningPlugin;
use world::occlusion::OcclusionPlugin;
use world::placement::PlacementPlugin;
use world::{WorldConfigPlugin, WorldPlugin};

/// Convenience bundle for the historical core voxel runtime.
///
/// `VoxelCorePlugin<V>` wires up the legacy core bundle — world streaming,
/// chunk lifecycle, meshing, PBR rendering, neighbor-solid occlusion, mining,
/// placement, and loading state — without the bundled presentation defaults
/// from [`VoxelPlugin`].
///
/// In the plugin catalog taxonomy this type is a [`plugin_category::ConveniencePlugin`]:
/// it composes stricter Engine and Gameplay plugins for compatibility and quick
/// starts. New architecture should describe those individual plugins by their
/// primary categories rather than treating this bundle as the whole model.
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
/// | `OcclusionPlugin<V>` | Neighbor-solid chunk occlusion |
/// | [`world::mining::MiningPlugin`] | Gameplay primitive for voxel removal |
/// | [`world::placement::PlacementPlugin`] | Gameplay primitive for voxel placement |
/// | [`world::loading::LoadingPlugin`] | World loading state machine |
///
/// # Not included (opt-in)
///
/// - [`generator::GeneratorPlugin`] — requires the caller to supply a
///   [`generator::GeneratorResource`].
#[cfg_attr(
    feature = "items",
    doc = "- [`FragmentPlugin`], [`SkyboxPlugin`], [`SfxPlugin`], [`HotbarPlugin`], and [`UiPlugin`] — presentation/gameplay defaults included by [`VoxelPlugin`]."
)]
#[cfg_attr(
    not(feature = "items"),
    doc = "- [`FragmentPlugin`], [`SkyboxPlugin`], [`SfxPlugin`], and [`UiPlugin`] — presentation defaults included by [`VoxelPlugin`]. `HotbarPlugin` additionally requires the `items` feature."
)]
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
/// - [`VoxelPlugin`] — broader quick-start convenience bundle.
pub struct VoxelCorePlugin<V: VoxelType = DefaultVoxel> {
    _m: std::marker::PhantomData<V>,
}

impl<V: VoxelType> VoxelCorePlugin<V> {
    /// Creates a new [`VoxelCorePlugin`] for voxel type `V`.
    pub fn new() -> Self {
        Self {
            _m: std::marker::PhantomData,
        }
    }
}

impl<V: VoxelType> Default for VoxelCorePlugin<V> {
    fn default() -> Self {
        Self::new()
    }
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

/// Quick-start convenience bundle for demos and prototypes.
///
/// `VoxelPlugin<V>` composes [`VoxelCorePlugin<V>`] with selected presentation
/// and gameplay defaults. It is useful for examples, demos, and early
/// prototyping, but it is not the architectural definition of the full
/// framework. Production games should treat it as a convenience bundle and
/// switch to explicit catalog plugins when they need finer control.
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
#[cfg_attr(
    feature = "items",
    doc = "| [`HotbarPlugin`] | Player hotbar, item switching, and held-item state |"
)]
#[cfg_attr(
    not(feature = "items"),
    doc = "| `HotbarPlugin` *(feature `items`)* | Player hotbar, item switching, and held-item state |"
)]
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
/// - [`VoxelCorePlugin`] — narrower compatibility/convenience bundle.
/// - [`prelude`] — commonly used re-exports for convenience imports.
pub struct VoxelPlugin<V: VoxelType = DefaultVoxel> {
    _m: std::marker::PhantomData<V>,
}

impl<V: VoxelType> VoxelPlugin<V> {
    /// Creates a new [`VoxelPlugin`] for voxel type `V`.
    pub fn new() -> Self {
        Self {
            _m: std::marker::PhantomData,
        }
    }
}

impl<V: VoxelType> Default for VoxelPlugin<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: VoxelType> Plugin for VoxelPlugin<V> {
    fn build(&self, app: &mut App) {
        app.add_plugins(VoxelCorePlugin::<V>::new());
        app.add_plugins(FragmentPlugin);
        app.add_plugins(SkyboxPlugin);
        app.add_plugins(SfxPlugin);
        #[cfg(feature = "items")]
        app.add_plugins(HotbarPlugin);
        app.add_plugins(UiPlugin);
    }
}
