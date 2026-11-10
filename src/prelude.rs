//! Convenient re-exports of the most commonly used types.
//!
//! Add `use voxel_framework::prelude::*;` to a game crate to pull in all
//! public framework types without needing individual imports.

// Voxel types
pub use crate::voxel::{
    DefaultTerrainVoxel, DefaultVoxel, MovementModifier, VoxelPalette, VoxelType,
};

// Author-defined content
pub use crate::content::{
    BlockContentSnapshot, BlockDefinition, BlockEngineProperties, BlockRegistry, ContentId,
    ContentRegistryError, ContentValue, VoxelContentPlugin,
};
#[cfg(feature = "items")]
pub use crate::content::{
    BlockDropContentPlugin, BlockDropContentSnapshot, BlockDropDefinition, BlockDropRegistry,
    BlockDropRegistryError, DropCount, ItemContentPlugin, ItemContentSnapshot, ItemDefinition,
    ItemRegistry, ItemRegistryError,
};

// Author-defined stats
pub use crate::stats::{
    StatChanged, StatDefinition, StatDefinitionSnapshot, StatError, StatId, StatRegistry, StatSet,
    StatValueSnapshot, VoxelStatsPlugin, add_stat, drain_stat, fill_stat, set_stat,
};

// Author-defined effects
pub use crate::effects::{
    ActiveEffectSet, EffectApplied, EffectDefinition, EffectDefinitionSnapshot, EffectError,
    EffectExpired, EffectId, EffectInstance, EffectInstanceSnapshot, EffectRegistry,
    VoxelEffectsPlugin, apply_effect, apply_effect_instance, tick_effects,
};

// Crafting
#[cfg(feature = "crafting")]
pub use crate::crafting::{
    CraftingError, CraftingIngredient, CraftingMenuState, CraftingPlugin, CraftingRecipe,
    CraftingRegistry, CraftingUiPlugin, can_craft, try_craft,
};

// Chunk types and constants
pub use crate::chunk::{
    CHUNK_SIZE, CHUNK_VOLUME, Chunk, ChunkCoord, ChunkCoordComp, ChunkLod, VOXEL_SIZE,
    chunk_center, chunk_origin,
    persistence::{chunk_filename, load_chunk, save_chunk},
    world_to_chunk_coord, world_to_local_coord,
};

// Noise
pub use crate::noise::{fbm, hash3, value_noise};

// Mesh generation
pub use crate::mesh::{
    ATTRIBUTE_TEXTURE_LAYER, ChunkNeighbors, generate_chunk_mesh,
    generate_chunk_mesh_with_neighbors,
};

// Generator
pub use crate::generator::asteroid::{AsteroidConfig, AsteroidGenerator, MineralFrequencies};
pub use crate::generator::terrain::{TerrainConfig, TerrainGenerator};
pub use crate::generator::{
    ChunkGenerator, ChunkTask, GeneratorPlugin, GeneratorResource, MultiGenerator,
    MultiGeneratorPlugin, RegionGenerator,
};

// Mesh plugin
pub use crate::mesh::MeshPlugin;

// World streaming
pub use crate::world::{
    ChunkLoadRequest, ChunkManager, ChunkModified, ChunkUnloadRequest, DEFAULT_LOAD_DISTANCE,
    DEFAULT_UNLOAD_DISTANCE, DayCycleConfig, DayCyclePlugin, OcclusionPlugin, PersistenceConfig,
    PersistencePlugin, SunLight, ViewerPosition, WorldConfig, WorldPlugin, is_chunk_occluded,
};

// World interaction API
pub use crate::world::{
    RaycastHit, capsule_overlaps_voxels, get_state_at, get_voxel_at, raycast,
    resolve_capsule_collision, resolve_sphere_collision, set_state_at, set_voxel_at,
    sphere_overlaps_voxels,
};

// Chunk block-state helpers
pub use crate::chunk::face_to_state;

// Mining API
#[cfg(feature = "items")]
pub use crate::world::MiningInputPlugin;
pub use crate::world::{
    MiningConfig, MiningPlugin, MiningProgress, MiningState, VoxelBroken, mining_input_system,
    stop_mining, tick_mining,
};

// Placement API
pub use crate::world::{
    PlacementConfig, PlacementPlugin, PlacementState, VoxelPlaced, tick_placement, try_place_voxel,
};
#[cfg(feature = "items")]
pub use crate::world::{PlacementInputPlugin, placement_input_system};

// Loading API
pub use crate::world::{LoadingPlugin, LoadingProgress, WorldLoadingState, WorldReady};

// Render
pub use crate::render::material::{build_texture_array, chunk_aabb};
pub use crate::render::{
    ChunkMaterial, ChunkMaterialConfig, ChunkMaterialHandle, ChunkRendered, ChunkTextureArray,
    RenderPlugin,
};

// Fragment system
pub use crate::fragments::{Fragment, FragmentConfig, FragmentPlugin};

// Audio SFX system
pub use crate::audio::{DefaultAudioPlugin, SfxConfig, SfxEvent, SfxPlugin, SfxRequest};

// Skybox
pub use crate::skybox::{SkyboxConfig, SkyboxMode, SkyboxPlugin, StarfieldConfig};

// Items system
#[cfg(feature = "items")]
pub use crate::items::inventory::{
    Container, ContainerComponent, ContainerExchangeError, PrimaryContainer,
};
#[cfg(feature = "items")]
pub use crate::items::{
    DefaultItem, DropTable, DropTableEntry, DroppedItem, DropsPlugin, HotbarPlugin,
    HotbarSelectionChanged, HotbarState, Inventory, InventorySlot, ItemCollector, ItemPickedUp,
    ItemType, MagnetConfig, load_inventory, save_inventory,
};

// UI system
pub use crate::ui::hud::{
    HudAnchor, HudBar, HudBindingKind, HudDefinitionError, HudDiagnostic, HudDiagnosticKind,
    HudElementDefinition, HudElementId, HudElementSnapshot, HudIcon, HudIconSource, HudPlacement,
    HudPresentation, HudRegistry, HudRegistryError, HudScalar, HudScalarField, HudSegments,
    HudStackDirection, HudStyleOverride, HudSystemSet, HudText, HudTheme, HudValue,
    HudValueChanged, HudValueError, HudValueId, HudValueKind, HudValueSet, HudValueSnapshot,
    MAX_HUD_SEGMENTS, VoxelHudPlugin, remove_hud_value, set_hud_value,
};
pub use crate::ui::{
    CrosshairConfig, CrosshairPlugin, DebugOverlayPlugin, DebugOverlayState, LoadingScreenConfig,
    LoadingScreenPlugin, PauseMenuConfig, PauseMenuPlugin, PauseMenuState, SaveSettingsRequest,
    UiPlugin,
};
#[cfg(feature = "items")]
pub use crate::ui::{
    HotbarUiConfig, HotbarUiPlugin, PickupNotificationConfig, PickupNotificationPlugin,
};

// Player (6DOF jetpack / grounded)
pub use crate::player::effects::{
    ActiveEffect, ActiveEffects, AppliedEffectModifiers, EffectModifiers, StatusEffect,
    StatusEffectsPlugin,
};
pub use crate::player::persistence::{load_spawn_position, save_spawn_position};
pub use crate::player::{
    PLAYER_HALF_HEIGHT, PLAYER_RADIUS, Player, PlayerConfig, PlayerCrouching, PlayerDamaged,
    PlayerDied, PlayerHealth, PlayerMode, PlayerPlugin, RollMode, SPRINT_MULTIPLIER,
    SnapRollEasing, health_system, player_cursor_system, player_input_active, player_look,
    player_move, player_resolve_collision, respawn_system, spawn_player,
};

// Mobs
#[cfg(feature = "mobs")]
pub use crate::mobs::{
    MOB_HALF_HEIGHT, MOB_RADIUS, Mob, MobAiState, MobConfig, MobDied, MobPlugin, spawn_mob,
};

// World manifest (requires crafting because the manifest writer reads the recipe registry)
#[cfg(feature = "crafting")]
pub use crate::world::manifest::{VoxelPaletteEntry, WorldManifest, WorldManifestPlugin};

// Settings persistence
pub use crate::settings::PersistedSettings;

// Plugin catalog category marker traits
pub use crate::plugin_category::{
    ConveniencePlugin, DefaultImplementationPlugin, EnginePlugin, GameplayPlugin,
    VoxelFrameworkPlugin, WorldDataPlugin,
};

// Top-level plugins
pub use crate::{VoxelCorePlugin, VoxelPlugin};
