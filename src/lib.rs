//! A reusable voxel engine framework built on Bevy.
//!
//! `voxel-framework` provides the core building blocks for voxel-based games:
//! chunk storage, procedural noise generation, voxel type abstractions,
//! greedy meshing, procedural generation, world streaming, and PBR rendering.
//!
//! # Quick start
//!
//! Add [`VoxelPlugin`] to your Bevy app to register all framework resources
//! and events in one step:
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use voxel_framework::VoxelPlugin;
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(VoxelPlugin)
//!     .run();
//! ```
//!
//! For a fully functional voxel world with async generation and mesh output,
//! also add the generic plugins:
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use voxel_framework::{
//!     VoxelPlugin,
//!     generator::{GeneratorPlugin, GeneratorResource},
//!     generator::asteroid::{AsteroidConfig, AsteroidGenerator},
//!     mesh::MeshPlugin,
//!     voxel::DefaultVoxel,
//! };
//! use std::sync::Arc;
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(VoxelPlugin)
//!     .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())
//!     .add_plugins(MeshPlugin::<DefaultVoxel>::new())
//!     .insert_resource(GeneratorResource::<DefaultVoxel> {
//!         generator: Arc::new(AsteroidGenerator::new(AsteroidConfig::default())),
//!     })
//!     .run();
//! ```
//!
//! See [`prelude`] for the most commonly used re-exports.

pub mod audio;
pub mod chunk;
pub mod fragments;
pub mod generator;
pub mod items;
pub mod mesh;
pub mod noise;
pub mod player;
pub mod prelude;
pub mod render;
pub mod skybox;
pub mod ui;
pub mod voxel;
pub mod world;

use bevy::prelude::*;

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

/// Core voxel engine plugin — pure engine systems with no game-specific content.
///
/// Registers the following sub-plugins:
///
/// - [`WorldPlugin`] — chunk manager, viewer position, lifecycle events and streaming.
/// - [`WorldConfigPlugin`] — world configuration resource.
/// - [`RenderPlugin<DefaultVoxel>`] — shared chunk material and startup system.
/// - [`MeshPlugin<DefaultVoxel>`] — mesh generation and chunk despawn systems.
/// - [`OcclusionPlugin<DefaultVoxel>`] — per-chunk occlusion culling.
/// - [`MiningPlugin`] — voxel mining interactions.
/// - [`PlacementPlugin`] — voxel placement interactions.
/// - [`LoadingPlugin`] — world loading state machine.
///
/// **Not included** (opt-in):
/// - [`generator::GeneratorPlugin`] — requires the caller to supply a
///   [`generator::GeneratorResource`]; add it separately.
/// - Game-specific plugins ([`FragmentPlugin`], [`SkyboxPlugin`], [`SfxPlugin`],
///   [`HotbarPlugin`], [`UiPlugin`]) — use [`VoxelPlugin`] for those.
pub struct VoxelCorePlugin;

impl Plugin for VoxelCorePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(WorldPlugin);
        app.add_plugins(WorldConfigPlugin);
        app.add_plugins(RenderPlugin::<DefaultVoxel>::new());
        app.add_plugins(MeshPlugin::<DefaultVoxel>::new());
        app.add_plugins(OcclusionPlugin::<DefaultVoxel>::new());
        app.add_plugins(MiningPlugin);
        app.add_plugins(PlacementPlugin);
        app.add_plugins(LoadingPlugin);
    }
}

/// Batteries-included plugin that registers all voxel framework systems,
/// including game-specific content (UI, skybox, audio, items).
///
/// Internally uses [`VoxelCorePlugin`] for the engine layer and adds:
///
/// - [`FragmentPlugin`] — floating voxel fragment entities.
/// - [`SkyboxPlugin`] — skybox and starfield rendering.
/// - [`SfxPlugin`] — spatial audio effects.
/// - [`HotbarPlugin`] — hotbar and item system.
/// - [`UiPlugin`] — crosshair, loading screen, and HUD.
///
/// [`generator::GeneratorPlugin`] is intentionally excluded because it requires
/// the user to supply a [`generator::GeneratorResource`]. Add it separately:
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::{VoxelPlugin, generator::{GeneratorPlugin, GeneratorResource}};
/// use voxel_framework::generator::asteroid::{AsteroidConfig, AsteroidGenerator};
/// use voxel_framework::voxel::DefaultVoxel;
/// use std::sync::Arc;
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(VoxelPlugin)
///     .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())
///     .insert_resource(GeneratorResource::<DefaultVoxel> {
///         generator: Arc::new(AsteroidGenerator::new(AsteroidConfig::default())),
///     })
///     .run();
/// ```
pub struct VoxelPlugin;

impl Plugin for VoxelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(VoxelCorePlugin);
        app.add_plugins(FragmentPlugin);
        app.add_plugins(SkyboxPlugin);
        app.add_plugins(SfxPlugin);
        app.add_plugins(HotbarPlugin);
        app.add_plugins(UiPlugin);
    }
}
