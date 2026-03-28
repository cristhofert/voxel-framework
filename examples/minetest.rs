//! Minetest-style example — grounded terrain world with gravity, jump, mining.
//!
//! Controls:
//!   WASD — walk  |  Space — jump  |  Mouse — look
//!   Shift — sprint  |  Ctrl — crouch
//!   Left click — mine block  |  Right click — place block from hotbar
//!   Scroll wheel — switch hotbar slot
//!   Escape — pause menu  |  F3 — debug overlay  |  C — crafting menu

use std::sync::Arc;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::asset::RenderAssetUsages;
use voxel_framework::prelude::*;
use voxel_framework::audio::{DefaultAudioPlugin, SfxConfig, SfxEvent};
use voxel_framework::items::{DefaultItem, Inventory};
use voxel_framework::mobs::{MobConfig, MobPlugin, spawn_mob};

/// Handles to the voxel textures pending assembly into a texture array.
///
/// Layers (match `DefaultTerrainVoxel::texture_layer`):
///   0 — Air placeholder (grey)
///   1 — Grass  → grass_block_top.png
///   2 — Dirt   → dirt.png
///   3 — Stone  → stone.png
///   4 — Sand   → sand.png
///   5 — Wood   → oak_log.png
///   6 — Leaves → oak_leaves.png
///   7 — Gravel → gravel.png
///   8 — Snow   → snow.png
///   9 — Water  → water_still.png
#[derive(Resource)]
struct PendingTextures {
    handles: Vec<Handle<Image>>,
    assembled: bool,
}

// ─── Block-info overlay ──────────────────────────────────────────────────────

/// Cached result of the per-frame "looked-at" raycast.
#[derive(Resource, Default)]
struct LookedAtBlock {
    /// Voxel the camera crosshair is pointing at, or `None` if the ray missed.
    voxel: Option<(DefaultTerrainVoxel, IVec3)>, // (type, world grid position)
}

/// Marker component for the block-info HUD text node.
#[derive(Component)]
struct BlockInfoText;

/// Human-readable name for each terrain voxel type.
fn voxel_name(v: DefaultTerrainVoxel) -> &'static str {
    match v {
        DefaultTerrainVoxel::Air    => "Air",
        DefaultTerrainVoxel::Grass  => "Grass",
        DefaultTerrainVoxel::Dirt   => "Dirt",
        DefaultTerrainVoxel::Stone  => "Stone",
        DefaultTerrainVoxel::Sand   => "Sand",
        DefaultTerrainVoxel::Wood   => "Wood",
        DefaultTerrainVoxel::Leaves => "Leaves",
        DefaultTerrainVoxel::Gravel => "Gravel",
        DefaultTerrainVoxel::Snow   => "Snow",
        DefaultTerrainVoxel::Water  => "Water",
    }
}

/// Spawns the block-info HUD text — always visible, top-left below the F3 overlay.
fn spawn_block_info_ui(mut commands: Commands) {
    commands.spawn((
        BlockInfoText,
        Text::new(""),
        TextFont { font_size: 14.0, ..default() },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            // Placed to the right so it doesn't overlap the F3 overlay (left side).
            right: Val::Px(8.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        ZIndex(199),
    ));
}

/// Raycasts from the camera every frame and updates `LookedAtBlock` + the HUD.
fn update_block_info(
    camera_q: Query<&GlobalTransform, With<Camera3d>>,
    manager: Res<ChunkManager>,
    chunks: Query<&Chunk<DefaultTerrainVoxel>>,
    mut looked_at: ResMut<LookedAtBlock>,
    mut text_q: Query<&mut Text, With<BlockInfoText>>,
) {
    let Ok(cam_tf) = camera_q.single() else { return };
    let origin    = cam_tf.translation();
    let direction = cam_tf.forward().as_vec3();

    let hit = raycast(origin, direction, 8.0, &manager, &chunks);

    looked_at.voxel = hit.as_ref().and_then(|h| {
        let entity = manager.get_entity(h.chunk_coord)?;
        let chunk  = chunks.get(entity).ok()?;
        let (x, y, z) = h.local;
        let v = chunk.get(x, y, z);
        // World-grid position of the hit voxel
        let world_grid = h.chunk_coord * CHUNK_SIZE as i32
            + IVec3::new(x as i32, y as i32, z as i32);
        Some((v, world_grid))
    });

    let Ok(mut text) = text_q.single_mut() else { return };

    *text = match &looked_at.voxel {
        Some((v, pos)) => Text::new(format!(
            "Mirando: {} (hardness: {:.1}, lvl: {})\nPos: ({}, {}, {})",
            voxel_name(*v),
            v.hardness(),
            v.mining_level(),
            pos.x, pos.y, pos.z,
        )),
        None => Text::new("Mirando: —"),
    };
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        // ── Engine (terrain voxel type) ──────────────────────────────────────
        .add_plugins(VoxelPlugin::<DefaultTerrainVoxel>::new())
        .add_plugins(GeneratorPlugin::<DefaultTerrainVoxel>::new())
        // ── Player (grounded with gravity) ───────────────────────────────────
        .add_plugins(PlayerPlugin::<DefaultTerrainVoxel>::new())
        // ── Mining & placement input (batteries-included) ────────────────────
        .add_plugins(MiningInputPlugin::<DefaultTerrainVoxel, DefaultItem>::new())
        .add_plugins(PlacementInputPlugin::<DefaultTerrainVoxel>::new())
        // ── Inventory UI (E to open) ─────────────────────────────────────────
        .add_plugins(InventoryUiPlugin::<DefaultItem>::new())
        // ── Crafting (C to open) ─────────────────────────────────────────────
        .add_plugins(CraftingPlugin::<DefaultItem>::new())
        // ── Day/night cycle (manages sun + sky color) ────────────────────────
        .add_plugins(DayCyclePlugin)
        // ── Debug overlay (F3 toggle) ────────────────────────────────────────
        .add_plugins(DebugOverlayPlugin)
        // ── Mobs (patrol + chase + attack) ──────────────────────────────────
        .add_plugins(MobPlugin::<DefaultTerrainVoxel>::new())
        // ── Audio (real bevy_audio playback) ────────────────────────────────
        .add_plugins(DefaultAudioPlugin)
        // ── Pause menu ───────────────────────────────────────────────────────
        .add_plugins(PauseMenuPlugin)
        // ── Resources ────────────────────────────────────────────────────────
        .insert_resource(GeneratorResource::<DefaultTerrainVoxel> {
            generator: Arc::new(TerrainGenerator::new(TerrainConfig::default())),
        })
        .insert_resource(PlayerConfig {
            spawn_position: Vec3::new(8.0, 60.0, 8.0),
            speed: 6.0,
            mode: PlayerMode::Grounded { gravity: 20.0, jump_speed: 8.0 },
            ..default()
        })
        .insert_resource(DayCycleConfig {
            day_length_secs: 300.0, // 5-minute days
            time_of_day: 0.35,      // start mid-morning — sun visible
            ..default()
        })
        .insert_resource(Inventory::<DefaultItem>::default())
        // ── Block-info overlay (looked-at block) ─────────────────────────────
        .init_resource::<LookedAtBlock>()
        .add_systems(Startup, (setup_crafting, setup_audio, setup_mobs, spawn_block_info_ui, setup_textures))
        .add_systems(Update, (update_block_info, assemble_texture_array))
        .run();
}

/// Seeds the crafting registry with example recipes and the inventory with
/// starting materials so the player can try crafting immediately.
fn setup_crafting(
    mut registry: ResMut<CraftingRegistry<DefaultItem>>,
    mut inventory: ResMut<Inventory<DefaultItem>>,
) {
    // Sample recipes.
    registry.add(CraftingRecipe {
        name: "Crystal Shard",
        ingredients: vec![CraftingIngredient { item: DefaultItem::IronOre, count: 4 }],
        outputs: vec![CraftingIngredient { item: DefaultItem::Crystal, count: 1 }],
    });
    registry.add(CraftingRecipe {
        name: "Rare Mineral",
        ingredients: vec![
            CraftingIngredient { item: DefaultItem::IronOre, count: 2 },
            CraftingIngredient { item: DefaultItem::Crystal, count: 1 },
        ],
        outputs: vec![CraftingIngredient { item: DefaultItem::RareMineral, count: 1 }],
    });
    registry.add(CraftingRecipe {
        name: "Rock Fragments",
        ingredients: vec![CraftingIngredient { item: DefaultItem::RareMineral, count: 1 }],
        outputs: vec![CraftingIngredient { item: DefaultItem::RockFragment, count: 3 }],
    });

    // Seed starting items so the player can try crafting right away.
    inventory.add(DefaultItem::IronOre, 12);
    inventory.add(DefaultItem::RockFragment, 5);
}

/// Registers sound effect asset paths.
///
/// Place `.ogg` files under `assets/minetest/audio/` to hear in-game audio.
/// Missing files are silently ignored by Bevy's asset server.
fn setup_audio(mut cfg: ResMut<SfxConfig>) {
    cfg.register(SfxEvent::MiningComplete, vec![
        "minetest/audio/impactMining_000.ogg",
        "minetest/audio/impactMining_001.ogg",
        "minetest/audio/impactMining_002.ogg",
        "minetest/audio/impactMining_003.ogg",
        "minetest/audio/impactMining_004.ogg",
    ]);
    cfg.register(SfxEvent::BlockPlace, vec![
        "minetest/audio/impactGeneric_light_000.ogg",
        "minetest/audio/impactGeneric_light_001.ogg",
        "minetest/audio/impactGeneric_light_002.ogg",
    ]);
    cfg.register(SfxEvent::ItemPickup, vec!["minetest/audio/click1.ogg"]);
    cfg.register(SfxEvent::Footstep(0), vec![  // generic / stone
        "minetest/audio/impactGeneric_light_000.ogg",
        "minetest/audio/impactGeneric_light_001.ogg",
        "minetest/audio/impactGeneric_light_002.ogg",
        "minetest/audio/impactTin_medium_002.ogg",
    ]);
}

/// Spawns a few mob enemies near the player spawn point.
fn setup_mobs(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let config = MobConfig {
        max_health: 20.0,
        chase_speed: 3.0,
        detection_radius: 14.0,
        color: Color::srgb(0.7, 0.15, 0.15),
        ..MobConfig::default()
    };
    // Spread mobs around the spawn point so the player encounters them quickly.
    for (dx, dz) in [(16, 0), (-16, 0), (0, 16), (8, 12), (-12, 8)] {
        spawn_mob(
            Vec3::new(8.0 + dx as f32, 65.0, 8.0 + dz as f32),
            config.clone(),
            &mut commands, &mut meshes, &mut materials,
        );
    }
}

/// Queues all voxel texture handles for deferred array assembly.
fn setup_textures(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
) {
    // Layer 0: Air placeholder — mid-grey 16×16 tile.
    let grey_pixel: Vec<u8> = (0..16 * 16)
        .flat_map(|_| [128u8, 128, 128, 255])
        .collect();
    let placeholder = Image::new(
        Extent3d { width: 16, height: 16, depth_or_array_layers: 1 },
        TextureDimension::D2,
        grey_pixel,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    let placeholder_handle = images.add(placeholder);

    // Layers 1–9: actual block textures.
    let grass  = asset_server.load("minetest/textures/grass_block_top.png");
    let dirt   = asset_server.load("minetest/textures/dirt.png");
    let stone  = asset_server.load("minetest/textures/stone.png");
    let sand   = asset_server.load("minetest/textures/sand.png");
    let wood   = asset_server.load("minetest/textures/oak_log.png");
    let leaves = asset_server.load("minetest/textures/oak_leaves.png");
    let gravel = asset_server.load("minetest/textures/gravel.png");
    let snow   = asset_server.load("minetest/textures/snow.png");
    let water  = asset_server.load("minetest/textures/water_still.png");

    commands.insert_resource(PendingTextures {
        handles: vec![
            placeholder_handle,
            grass, dirt, stone, sand, wood, leaves, gravel, snow, water,
        ],
        assembled: false,
    });
}

/// Waits until all textures are loaded, then assembles them into a
/// `texture_2d_array` and inserts [`ChunkTextureArray`].
fn assemble_texture_array(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut pending: ResMut<PendingTextures>,
) {
    if pending.assembled {
        return;
    }

    let all_loaded = pending.handles.iter().all(|h| {
        asset_server.is_loaded_with_dependencies(h)
    });
    if !all_loaded {
        return;
    }

    let mut layer_images: Vec<Image> = Vec::new();
    for handle in &pending.handles {
        let Some(img) = images.get(handle) else { return; };
        let normalised = normalise_to_rgba32(img);
        layer_images.push(normalised);
    }

    let array_image = build_texture_array(&layer_images);
    let array_handle = images.add(array_image);

    commands.insert_resource(ChunkTextureArray { handle: array_handle });
    pending.assembled = true;

    info!("voxel texture array assembled ({} layers)", layer_images.len());
}

/// Converts any [`Image`] to 16×16 `Rgba8UnormSrgb` format.
fn normalise_to_rgba32(img: &Image) -> Image {
    const TARGET_W: u32 = 16;
    const TARGET_H: u32 = 16;

    let src_data = img.data.as_deref().unwrap_or(&[]);
    let w = img.width();
    let h = img.height();
    let fmt = img.texture_descriptor.format;

    let rgba: Vec<u8> = match fmt {
        TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm => src_data.to_vec(),
        TextureFormat::R8Unorm => {
            src_data.iter().flat_map(|&r| [r, r, r, 255u8]).collect()
        }
        TextureFormat::Rg8Unorm => {
            src_data.chunks(2).flat_map(|p| [p[0], p[1], 0u8, 255u8]).collect()
        }
        _ => {
            warn!("normalise_to_rgba32: unsupported format {:?}, filling with magenta", fmt);
            vec![255u8, 0, 255, 255].repeat((w * h) as usize)
        }
    };

    let scaled = if w == TARGET_W && h == TARGET_H {
        rgba
    } else {
        let mut out = Vec::with_capacity((TARGET_W * TARGET_H * 4) as usize);
        for ty in 0..TARGET_H {
            for tx in 0..TARGET_W {
                let sx = (tx * w / TARGET_W) as usize;
                let sy = (ty * h / TARGET_H) as usize;
                let idx = (sy * w as usize + sx) * 4;
                if idx + 3 < rgba.len() {
                    out.extend_from_slice(&rgba[idx..idx + 4]);
                } else {
                    out.extend_from_slice(&[255u8, 0, 255, 255]);
                }
            }
        }
        out
    };

    Image::new(
        Extent3d { width: TARGET_W, height: TARGET_H, depth_or_array_layers: 1 },
        TextureDimension::D2,
        scaled,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}
