//! Convenient re-exports of the most commonly used types.
//!
//! Add `use voxel_framework::prelude::*;` to a game crate to pull in all
//! public framework types without needing individual imports.

// Voxel types
pub use crate::voxel::{DefaultVoxel, VoxelPalette, VoxelType};

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
};

// Generator
pub use crate::generator::{ChunkGenerator, ChunkTask, GeneratorPlugin, GeneratorResource};
pub use crate::generator::asteroid::{AsteroidConfig, AsteroidGenerator, MineralFrequencies};

// Mesh plugin
pub use crate::mesh::MeshPlugin;

// World streaming
pub use crate::world::{
    ChunkLoadRequest, ChunkManager, ChunkModified, ChunkUnloadRequest,
    DEFAULT_LOAD_DISTANCE, DEFAULT_UNLOAD_DISTANCE, ViewerPosition, WorldConfig, WorldPlugin,
    PersistenceConfig, PersistencePlugin,
    OcclusionPlugin, is_chunk_occluded,
};

// World interaction API
pub use crate::world::{
    get_voxel_at, set_voxel_at, raycast, RaycastHit,
    sphere_overlaps_voxels, resolve_sphere_collision,
    capsule_overlaps_voxels, resolve_capsule_collision,
};

// Mining API
pub use crate::world::{
    tick_mining, stop_mining, MiningConfig, MiningPlugin, MiningProgress, MiningState, VoxelBroken,
};

// Placement API
pub use crate::world::{
    try_place_voxel, tick_placement, PlacementConfig, PlacementPlugin, PlacementState, VoxelPlaced,
};

// Loading API
pub use crate::world::{LoadingPlugin, LoadingProgress, WorldLoadingState, WorldReady};

// Render
pub use crate::render::{ChunkMaterial, ChunkMaterialConfig, ChunkRendered, RenderPlugin};
pub use crate::render::material::chunk_aabb;

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

// UI system
pub use crate::ui::{
    CrosshairConfig, CrosshairPlugin,
    HotbarUiConfig, HotbarUiPlugin,
    LoadingScreenConfig, LoadingScreenPlugin,
    PickupNotificationConfig, PickupNotificationPlugin,
    PauseMenuConfig, PauseMenuPlugin, PauseMenuState,
    UiPlugin,
};

// Player (6DOF jetpack)
pub use crate::player::{
    Player, PlayerConfig, PlayerPlugin,
    spawn_player, player_look, player_move, player_resolve_collision, player_cursor_system,
    PLAYER_RADIUS, PLAYER_HALF_HEIGHT,
};

// Top-level plugins
pub use crate::{VoxelCorePlugin, VoxelPlugin};
