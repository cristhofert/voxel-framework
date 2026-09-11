//! Official plugin category marker traits.
//!
//! `voxel-framework` exposes a flat catalog of Bevy plugins. These marker
//! traits document each plugin's primary architectural responsibility without
//! changing Bevy's normal [`Plugin`] composition model.
//!
//! A plugin should normally belong to exactly one category:
//!
//! - [`EnginePlugin`] — foundation for streamable/renderable voxel worlds.
//! - [`WorldDataPlugin`] — chunk sources, generation, persistence, and world
//!   lifecycle data.
//! - [`GameplayPlugin`] — reusable game rules and interactions.
//! - [`DefaultImplementationPlugin`] — replaceable UI/audio/visual defaults.
//! - [`ConveniencePlugin`] — compatibility bundles, input adapters, recipes,
//!   and prototyping helpers.
//!
//! New domain behavior should not start as [`ConveniencePlugin`]. Convenience
//! plugins should compose or adapt plugins from the stricter categories.

use bevy::app::Plugin;

use crate::audio::{DefaultAudioPlugin, SfxPlugin};
use crate::content::VoxelContentPlugin;
#[cfg(feature = "items")]
use crate::content::{BlockDropContentPlugin, ItemContentPlugin};
#[cfg(feature = "crafting")]
use crate::crafting::{CraftingPlugin, CraftingUiPlugin};
use crate::effects::VoxelEffectsPlugin;
use crate::fragments::FragmentPlugin;
use crate::generator::{GeneratorPlugin, MultiGeneratorPlugin};
#[cfg(feature = "items")]
use crate::items::{DropsPlugin, HotbarPlugin, ItemType};
use crate::mesh::MeshPlugin;
#[cfg(feature = "mobs")]
use crate::mobs::MobPlugin;
use crate::player::PlayerPlugin;
use crate::player::effects::{StatusEffect, StatusEffectsPlugin};
use crate::render::RenderPlugin;
use crate::skybox::SkyboxPlugin;
use crate::stats::VoxelStatsPlugin;
use crate::ui::{
    CrosshairPlugin, DebugOverlayPlugin, LoadingScreenPlugin, PauseMenuPlugin, UiPlugin,
    VoxelHudPlugin,
};
#[cfg(feature = "items")]
use crate::ui::{HotbarUiPlugin, PickupNotificationPlugin};
use crate::voxel::VoxelType;
#[cfg(feature = "crafting")]
use crate::world::manifest::WorldManifestPlugin;
use crate::world::mining::MiningPlugin;
use crate::world::{
    DayCyclePlugin, LoadingPlugin, OcclusionPlugin, PersistencePlugin, PlacementPlugin,
    WorldConfigPlugin, WorldPlugin,
};
#[cfg(feature = "items")]
use crate::world::{MiningInputPlugin, PlacementInputPlugin};

/// Marker for plugins that are part of the `voxel-framework` plugin catalog.
pub trait VoxelFrameworkPlugin: Plugin {}

/// Foundation/infrastructure plugins for streamable, renderable voxel worlds.
pub trait EnginePlugin: VoxelFrameworkPlugin {}

/// Chunk source, generation, persistence, and world lifecycle data plugins.
pub trait WorldDataPlugin: VoxelFrameworkPlugin {}

/// Reusable game rules and interaction plugins.
pub trait GameplayPlugin: VoxelFrameworkPlugin {}

/// Replaceable UI, audio, visual, and presentation default plugins.
pub trait DefaultImplementationPlugin: VoxelFrameworkPlugin {}

/// Exceptional compatibility, bundle, input-adapter, recipe, or prototype helper plugins.
pub trait ConveniencePlugin: VoxelFrameworkPlugin {}

impl VoxelFrameworkPlugin for WorldPlugin {}
impl EnginePlugin for WorldPlugin {}

impl VoxelFrameworkPlugin for WorldConfigPlugin {}
impl EnginePlugin for WorldConfigPlugin {}

impl<V: VoxelType> VoxelFrameworkPlugin for RenderPlugin<V> {}
impl<V: VoxelType> EnginePlugin for RenderPlugin<V> {}

impl<V: VoxelType> VoxelFrameworkPlugin for MeshPlugin<V> {}
impl<V: VoxelType> EnginePlugin for MeshPlugin<V> {}

impl<V: VoxelType> VoxelFrameworkPlugin for OcclusionPlugin<V> {}
impl<V: VoxelType> EnginePlugin for OcclusionPlugin<V> {}

impl VoxelFrameworkPlugin for LoadingPlugin {}
impl EnginePlugin for LoadingPlugin {}

impl<V: VoxelType> VoxelFrameworkPlugin for GeneratorPlugin<V> {}
impl<V: VoxelType> WorldDataPlugin for GeneratorPlugin<V> {}

impl<V: VoxelType> VoxelFrameworkPlugin for MultiGeneratorPlugin<V> {}
impl<V: VoxelType> WorldDataPlugin for MultiGeneratorPlugin<V> {}

impl<V: VoxelType> VoxelFrameworkPlugin for PersistencePlugin<V> {}
impl<V: VoxelType> WorldDataPlugin for PersistencePlugin<V> {}

impl<V: VoxelType> VoxelFrameworkPlugin for VoxelContentPlugin<V> {}
impl<V: VoxelType> GameplayPlugin for VoxelContentPlugin<V> {}

impl VoxelFrameworkPlugin for VoxelStatsPlugin {}
impl GameplayPlugin for VoxelStatsPlugin {}

impl VoxelFrameworkPlugin for VoxelEffectsPlugin {}
impl GameplayPlugin for VoxelEffectsPlugin {}

#[cfg(feature = "items")]
impl<I: ItemType> VoxelFrameworkPlugin for ItemContentPlugin<I> {}
#[cfg(feature = "items")]
impl<I: ItemType> GameplayPlugin for ItemContentPlugin<I> {}

#[cfg(feature = "items")]
impl<I: ItemType> VoxelFrameworkPlugin for BlockDropContentPlugin<I> {}
#[cfg(feature = "items")]
impl<I: ItemType> GameplayPlugin for BlockDropContentPlugin<I> {}

#[cfg(feature = "crafting")]
impl<V: VoxelType, I: ItemType> VoxelFrameworkPlugin for WorldManifestPlugin<V, I> {}
#[cfg(feature = "crafting")]
impl<V: VoxelType, I: ItemType> WorldDataPlugin for WorldManifestPlugin<V, I> {}

impl VoxelFrameworkPlugin for MiningPlugin {}
impl GameplayPlugin for MiningPlugin {}

impl VoxelFrameworkPlugin for PlacementPlugin {}
impl GameplayPlugin for PlacementPlugin {}

impl<V: VoxelType> VoxelFrameworkPlugin for PlayerPlugin<V> {}
impl<V: VoxelType> GameplayPlugin for PlayerPlugin<V> {}

#[cfg(feature = "items")]
impl VoxelFrameworkPlugin for HotbarPlugin {}
#[cfg(feature = "items")]
impl GameplayPlugin for HotbarPlugin {}

#[cfg(feature = "items")]
impl<I: ItemType> VoxelFrameworkPlugin for DropsPlugin<I> {}
#[cfg(feature = "items")]
impl<I: ItemType> GameplayPlugin for DropsPlugin<I> {}

#[cfg(feature = "crafting")]
impl<I: ItemType> VoxelFrameworkPlugin for CraftingPlugin<I> {}
#[cfg(feature = "crafting")]
impl<I: ItemType> GameplayPlugin for CraftingPlugin<I> {}

#[cfg(feature = "mobs")]
impl<V: VoxelType> VoxelFrameworkPlugin for MobPlugin<V> {}
#[cfg(feature = "mobs")]
impl<V: VoxelType> GameplayPlugin for MobPlugin<V> {}

impl<E: StatusEffect> VoxelFrameworkPlugin for StatusEffectsPlugin<E> {}
impl<E: StatusEffect> GameplayPlugin for StatusEffectsPlugin<E> {}

impl VoxelFrameworkPlugin for FragmentPlugin {}
impl DefaultImplementationPlugin for FragmentPlugin {}

impl VoxelFrameworkPlugin for SkyboxPlugin {}
impl DefaultImplementationPlugin for SkyboxPlugin {}

impl VoxelFrameworkPlugin for SfxPlugin {}
impl DefaultImplementationPlugin for SfxPlugin {}

impl VoxelFrameworkPlugin for DefaultAudioPlugin {}
impl DefaultImplementationPlugin for DefaultAudioPlugin {}

impl VoxelFrameworkPlugin for UiPlugin {}
impl DefaultImplementationPlugin for UiPlugin {}

impl VoxelFrameworkPlugin for VoxelHudPlugin {}
impl DefaultImplementationPlugin for VoxelHudPlugin {}

impl VoxelFrameworkPlugin for CrosshairPlugin {}
impl DefaultImplementationPlugin for CrosshairPlugin {}

impl VoxelFrameworkPlugin for LoadingScreenPlugin {}
impl DefaultImplementationPlugin for LoadingScreenPlugin {}

#[cfg(feature = "items")]
impl VoxelFrameworkPlugin for HotbarUiPlugin {}
#[cfg(feature = "items")]
impl DefaultImplementationPlugin for HotbarUiPlugin {}

#[cfg(feature = "items")]
impl<I: ItemType> VoxelFrameworkPlugin for PickupNotificationPlugin<I> {}
#[cfg(feature = "items")]
impl<I: ItemType> DefaultImplementationPlugin for PickupNotificationPlugin<I> {}

impl VoxelFrameworkPlugin for PauseMenuPlugin {}
impl DefaultImplementationPlugin for PauseMenuPlugin {}

impl VoxelFrameworkPlugin for DebugOverlayPlugin {}
impl DefaultImplementationPlugin for DebugOverlayPlugin {}

impl VoxelFrameworkPlugin for DayCyclePlugin {}
impl DefaultImplementationPlugin for DayCyclePlugin {}

impl<V: VoxelType> VoxelFrameworkPlugin for crate::VoxelCorePlugin<V> {}
impl<V: VoxelType> ConveniencePlugin for crate::VoxelCorePlugin<V> {}

impl<V: VoxelType> VoxelFrameworkPlugin for crate::VoxelPlugin<V> {}
impl<V: VoxelType> ConveniencePlugin for crate::VoxelPlugin<V> {}

#[cfg(feature = "items")]
impl<V: VoxelType, I: ItemType> VoxelFrameworkPlugin for MiningInputPlugin<V, I> {}
#[cfg(feature = "items")]
impl<V: VoxelType, I: ItemType> ConveniencePlugin for MiningInputPlugin<V, I> {}

#[cfg(feature = "items")]
impl<V: VoxelType> VoxelFrameworkPlugin for PlacementInputPlugin<V> {}
#[cfg(feature = "items")]
impl<V: VoxelType> ConveniencePlugin for PlacementInputPlugin<V> {}

#[cfg(feature = "crafting")]
impl<I: ItemType> VoxelFrameworkPlugin for CraftingUiPlugin<I> {}
#[cfg(feature = "crafting")]
impl<I: ItemType> ConveniencePlugin for CraftingUiPlugin<I> {}
