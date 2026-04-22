//! Convenient re-exports of the most commonly used types.
//!
//! Add `use voxel_framework::prelude::*;` to a game crate to pull in all
//! public framework types without needing individual imports.

// Voxel types
pub use crate::voxel::{DefaultVoxel, DefaultTerrainVoxel, MovementModifier, VoxelPalette, VoxelType};

// Crafting
pub use crate::crafting::{
    CraftingIngredient, CraftingPlugin, CraftingUiPlugin, CraftingRecipe, CraftingRegistry,
    CraftingMenuState, CraftingError, can_craft, try_craft,
};

// Chunk types and constants
pub use crate::chunk::{
    persistence::{chunk_filename, load_chunk, save_chunk},
    chunk_center, chunk_origin, world_to_chunk_coord, world_to_local_coord,
    Chunk, ChunkCoord, ChunkCoordComp, ChunkLod, CHUNK_SIZE, CHUNK_VOLUME, VOXEL_SIZE,
};

// Noise
pub use crate::noise::{fbm, hash3, value_noise};

// Mesh generation
pub use crate::mesh::{
    generate_chunk_mesh, generate_chunk_mesh_with_neighbors, ChunkNeighbors,
    ATTRIBUTE_TEXTURE_LAYER,
};

// Generator
pub use crate::generator::{ChunkGenerator, ChunkTask, GeneratorPlugin, GeneratorResource};
pub use crate::generator::asteroid::{AsteroidConfig, AsteroidGenerator, MineralFrequencies};
pub use crate::generator::terrain::{TerrainConfig, TerrainGenerator};

// Mesh plugin
pub use crate::mesh::MeshPlugin;

// World streaming
pub use crate::world::{
    ChunkLoadRequest, ChunkManager, ChunkModified, ChunkUnloadRequest,
    DEFAULT_LOAD_DISTANCE, DEFAULT_UNLOAD_DISTANCE, ViewerPosition, WorldConfig, WorldPlugin,
    PersistenceConfig, PersistencePlugin,
    OcclusionPlugin, is_chunk_occluded,
    DayCycleConfig, DayCyclePlugin, SunLight,
};

// World interaction API
pub use crate::world::{
    get_voxel_at, set_voxel_at, get_state_at, set_state_at,
    raycast, RaycastHit,
    sphere_overlaps_voxels, resolve_sphere_collision,
    capsule_overlaps_voxels, resolve_capsule_collision,
};

// Chunk block-state helpers
pub use crate::chunk::face_to_state;

// Mining API
pub use crate::world::{
    tick_mining, stop_mining, mining_input_system,
    MiningConfig, MiningInputPlugin, MiningPlugin, MiningProgress, MiningState, VoxelBroken,
};

// Placement API
pub use crate::world::{
    try_place_voxel, tick_placement, placement_input_system,
    PlacementConfig, PlacementInputPlugin, PlacementPlugin, PlacementState, VoxelPlaced,
};

// Loading API
pub use crate::world::{LoadingPlugin, LoadingProgress, WorldLoadingState, WorldReady};

// Render
pub use crate::render::{
    ChunkMaterial, ChunkMaterialConfig, ChunkMaterialHandle, ChunkRendered,
    ChunkTextureArray, RenderPlugin,
};
pub use crate::render::material::{build_texture_array, chunk_aabb};

// Fragment system
pub use crate::fragments::{Fragment, FragmentConfig, FragmentPlugin};

// Audio SFX system
pub use crate::audio::{DefaultAudioPlugin, SfxConfig, SfxEvent, SfxPlugin, SfxRequest};

// Skybox
pub use crate::skybox::{SkyboxConfig, SkyboxMode, SkyboxPlugin, StarfieldConfig};

// Items system
pub use crate::items::{
    DefaultItem, DroppedItem, DropsPlugin, DropTable, HotbarPlugin, HotbarSelectionChanged,
    HotbarState, Inventory, InventorySlot, ItemCollector, ItemPickedUp, ItemType, MagnetConfig,
    load_inventory, save_inventory,
};
pub use crate::items::inventory::{Container, ContainerComponent, PrimaryContainer};

// UI system
pub use crate::ui::{
    CrosshairConfig, CrosshairPlugin,
    HotbarUiConfig, HotbarUiPlugin,
    LoadingScreenConfig, LoadingScreenPlugin,
    PickupNotificationConfig, PickupNotificationPlugin,
    PauseMenuConfig, PauseMenuPlugin, PauseMenuState, SaveSettingsRequest,
    DebugOverlayPlugin, DebugOverlayState,
    UiPlugin,
};

// Player (6DOF jetpack / grounded)
pub use crate::player::{
    Player, PlayerConfig, PlayerMode, PlayerPlugin,
    PlayerHealth, PlayerDamaged, PlayerDied,
    PlayerCrouching,
    spawn_player, player_look, player_move, player_resolve_collision, player_cursor_system,
    health_system, respawn_system,
    PLAYER_RADIUS, PLAYER_HALF_HEIGHT, SPRINT_MULTIPLIER,
};
pub use crate::player::persistence::{save_spawn_position, load_spawn_position};
pub use crate::player::effects::{
    ActiveEffects, ActiveEffect, AppliedEffectModifiers,
    EffectModifiers, StatusEffect, StatusEffectsPlugin,
};

// Mobs
pub use crate::mobs::{
    Mob, MobAiState, MobConfig, MobDied, MobPlugin,
    spawn_mob, MOB_HALF_HEIGHT, MOB_RADIUS,
};

// World manifest
pub use crate::world::manifest::{WorldManifest, WorldManifestPlugin, VoxelPaletteEntry};

// Settings persistence
pub use crate::settings::PersistedSettings;

// Top-level plugins
pub use crate::{VoxelCorePlugin, VoxelPlugin};
