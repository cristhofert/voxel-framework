//! Unified voxel launcher — load any world by name.
//!
//! Usage:
//!   cargo run --example voxel -- --world basic_world
//!   cargo run --example voxel -- --world minetest
//!   cargo run --example voxel               # uses basic_world by default
//!
//! The `--world <name>` flag loads `examples/<name>.toml`.
//! The `example` field inside the TOML decides which game mode runs.

use std::sync::Arc;
use bevy::prelude::*;
use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::asset::RenderAssetUsages;
use voxel_framework::prelude::*;
use voxel_framework::audio::{DefaultAudioPlugin, SfxConfig, SfxEvent, SfxRequest};
use voxel_framework::items::{DefaultItem, ItemCollector};
use voxel_framework::items::inventory::{ContainerComponent, PrimaryContainer};
use voxel_framework::mobs::{MobConfig, MobPlugin, spawn_mob};
use voxel_framework::player::PlayerPlugin;

// ─── Shared types ─────────────────────────────────────────────────────────────

/// Handles to voxel textures pending assembly into a texture array.
/// `target_size` is the pixel dimension each layer is scaled to (32 for
/// basic_world, 16 for minetest).
#[derive(Resource)]
struct PendingTextures {
    handles: Vec<Handle<Image>>,
    assembled: bool,
    target_size: u32,
}

// ─── Block-info overlay (minetest only) ───────────────────────────────────────

#[derive(Resource, Default)]
struct LookedAtBlock {
    voxel: Option<(DefaultTerrainVoxel, IVec3)>,
}

#[derive(Component)]
struct BlockInfoText;

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

fn spawn_block_info_ui(mut commands: Commands) {
    commands.spawn((
        BlockInfoText,
        Text::new(""),
        TextFont { font_size: 14.0, ..default() },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            right: Val::Px(8.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        ZIndex(199),
    ));
}

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

// ─── Entry point ──────────────────────────────────────────────────────────────

fn load_config() -> WorldConfig {
    let args: Vec<String> = std::env::args().collect();
    let name = args.windows(2)
        .find(|w| w[0] == "--world")
        .map(|w| w[1].clone())
        .unwrap_or_else(|| "basic_world".to_string());
    let path = format!("examples/{}.toml", name);
    WorldConfig::from_file(&path)
        .inspect_err(|e| eprintln!("No se pudo cargar '{path}': {e}. Usando defaults."))
        .unwrap_or_else(|_| WorldConfig { world_name: name, ..WorldConfig::default() })
}

fn main() {
    let cfg = load_config();
    match cfg.example.as_str() {
        "minetest" => run_minetest(cfg),
        _          => run_basic_world(cfg),
    }
}

// ─── basic_world ──────────────────────────────────────────────────────────────

fn run_basic_world(world_config: WorldConfig) {
    let generator = world_config.build_asteroid_generator();
    let spawn_pos = Vec3::from_array(world_config.spawn_position);

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
        .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())
        .add_plugins(PersistencePlugin::<DefaultVoxel>::with_world(&world_config.world_name))
        .add_plugins(PauseMenuPlugin)
        .add_plugins(PlayerPlugin::<DefaultVoxel>::with_world(&world_config.world_name))
        .insert_resource({
            let mut sfx = SfxConfig::new();
            sfx.register(SfxEvent::Mining, vec![
                "basic_world/audio/impactMining_000.ogg",
                "basic_world/audio/impactMining_001.ogg",
                "basic_world/audio/impactMining_002.ogg",
                "basic_world/audio/impactMining_003.ogg",
                "basic_world/audio/impactMining_004.ogg",
            ]);
            sfx.register(SfxEvent::MiningComplete, vec![
                "basic_world/audio/impactGeneric_light_000.ogg",
                "basic_world/audio/impactGeneric_light_001.ogg",
                "basic_world/audio/impactGeneric_light_002.ogg",
            ]);
            sfx.register(SfxEvent::BlockPlace, vec![
                "basic_world/audio/impactTin_medium_002.ogg",
            ]);
            sfx.register(SfxEvent::ItemPickup,   vec!["basic_world/audio/rollover1.ogg"]);
            sfx.register(SfxEvent::HotbarScroll, vec!["basic_world/audio/rollover1.ogg"]);
            sfx.register(SfxEvent::PauseOpen,    vec!["basic_world/audio/click1.ogg"]);
            sfx.register(SfxEvent::PauseClose,   vec!["basic_world/audio/click1.ogg"]);
            sfx
        })
        .insert_resource(GeneratorResource::<DefaultVoxel> {
            generator: Arc::new(generator),
        })
        .insert_resource(world_config)
        .insert_resource(PlayerConfig {
            spawn_position: spawn_pos,
            speed: 16.0,
            ..default()
        })
        .add_systems(Startup, setup_basic_world)
        .add_systems(Update, (play_sfx, assemble_texture_array))
        .run();
}

fn setup_basic_world(
    mut commands: Commands,
    mut viewer: ResMut<ViewerPosition>,
    config: Res<PlayerConfig>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
) {
    viewer.position = config.spawn_position;

    commands.spawn((
        DirectionalLight {
            illuminance: 15_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(100.0, 200.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.insert_resource(PauseMenuConfig {
        font: asset_server.load("basic_world/fonts/NotoSans-Regular.ttf"),
    });

    let grey_pixel: Vec<u8> = (0..32 * 32)
        .flat_map(|_| [128u8, 128, 128, 255])
        .collect();
    let placeholder = Image::new(
        Extent3d { width: 32, height: 32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        grey_pixel,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    let placeholder_handle = images.add(placeholder);

    let stone   = asset_server.load("basic_world/textures/stone.png");
    let iron    = asset_server.load("basic_world/textures/iron_ore.png");
    let silicon = asset_server.load("basic_world/textures/amethyst_block.png");
    let rare    = asset_server.load("basic_world/textures/emerald_ore.png");

    commands.insert_resource(PendingTextures {
        handles: vec![placeholder_handle, stone, iron, silicon, rare],
        assembled: false,
        target_size: 32,
    });
}

fn play_sfx(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    sfx_config: Res<SfxConfig>,
    mut sfx_reader: bevy::ecs::message::MessageReader<SfxRequest>,
    mut seed: Local<u64>,
) {
    for request in sfx_reader.read() {
        *seed = seed.wrapping_add(1);
        if let Some(path) = sfx_config.pick_path(&request.event, *seed) {
            let handle = asset_server.load::<AudioSource>(path.to_string());
            commands.spawn((AudioPlayer(handle), PlaybackSettings::ONCE));
        }
    }
}

// ─── minetest ─────────────────────────────────────────────────────────────────

fn run_minetest(world_config: WorldConfig) {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(VoxelPlugin::<DefaultTerrainVoxel>::new())
        .add_plugins(GeneratorPlugin::<DefaultTerrainVoxel>::new())
        .add_plugins(PersistencePlugin::<DefaultTerrainVoxel>::with_world(&world_config.world_name))
        .add_plugins(PlayerPlugin::<DefaultTerrainVoxel>::with_world(&world_config.world_name))
        .add_plugins(MiningInputPlugin::<DefaultTerrainVoxel, DefaultItem>::new())
        .add_plugins(PlacementInputPlugin::<DefaultTerrainVoxel>::new())
        .add_plugins(CraftingPlugin::<DefaultItem>::new())
        .add_plugins(CraftingUiPlugin::<DefaultItem>::new())
        .add_plugins(DayCyclePlugin)
        .add_plugins(DebugOverlayPlugin)
        .add_plugins(MobPlugin::<DefaultTerrainVoxel>::new())
        .add_plugins(DefaultAudioPlugin)
        .add_plugins(PauseMenuPlugin)
        .insert_resource(GeneratorResource::<DefaultTerrainVoxel> {
            generator: Arc::new(TerrainGenerator::new(TerrainConfig::default())),
        })
        .insert_resource(PlayerConfig {
            spawn_position: Vec3::from_array(world_config.spawn_position),
            speed: 6.0,
            mode: PlayerMode::Grounded { gravity: 20.0, jump_speed: 8.0 },
            ..default()
        })
        .insert_resource(DayCycleConfig {
            day_length_secs: 300.0,
            time_of_day: 0.35,
            ..default()
        })
        .insert_resource(world_config)
        .init_resource::<LookedAtBlock>()
        .add_systems(Startup, (
            spawn_minetest_container,
            setup_minetest_crafting,
            setup_minetest_audio,
            setup_minetest_mobs,
            spawn_block_info_ui,
            setup_minetest_textures,
        ))
        .add_systems(Update, (update_block_info, assemble_texture_array))
        .run();
}

fn spawn_minetest_container(mut commands: Commands) {
    commands.spawn((
        ContainerComponent::<DefaultItem>::new(36),
        PrimaryContainer,
        ItemCollector,
    ));
}

fn setup_minetest_crafting(
    mut registry: ResMut<CraftingRegistry<DefaultItem>>,
    mut container_query: Query<&mut ContainerComponent<DefaultItem>, With<PrimaryContainer>>,
) {
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

    if let Ok(mut container) = container_query.single_mut() {
        container.0.add(DefaultItem::IronOre, 12);
        container.0.add(DefaultItem::RockFragment, 5);
    }
}

fn setup_minetest_audio(mut cfg: ResMut<SfxConfig>) {
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
    cfg.register(SfxEvent::Footstep(0), vec![
        "minetest/audio/impactGeneric_light_000.ogg",
        "minetest/audio/impactGeneric_light_001.ogg",
        "minetest/audio/impactGeneric_light_002.ogg",
        "minetest/audio/impactTin_medium_002.ogg",
    ]);
}

fn setup_minetest_mobs(
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
    for (dx, dz) in [(16, 0), (-16, 0), (0, 16), (8, 12), (-12, 8)] {
        spawn_mob(
            Vec3::new(8.0 + dx as f32, 65.0, 8.0 + dz as f32),
            config.clone(),
            &mut commands, &mut meshes, &mut materials,
        );
    }
}

fn setup_minetest_textures(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
) {
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
        target_size: 16,
    });
}

// ─── Shared texture assembly ───────────────────────────────────────────────────

fn assemble_texture_array(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut pending: ResMut<PendingTextures>,
) {
    if pending.assembled {
        return;
    }

    let all_ready = pending.handles.iter().all(|h| images.contains(h));
    if !all_ready {
        return;
    }

    let target_size = pending.target_size;
    let mut layer_images: Vec<Image> = Vec::new();
    for handle in &pending.handles {
        let Some(img) = images.get(handle) else { return; };
        let normalised = normalise_to_rgba32(img, target_size);
        layer_images.push(normalised);
    }

    let array_image = build_texture_array(&layer_images);
    let array_handle = images.add(array_image);

    commands.insert_resource(ChunkTextureArray { handle: array_handle });
    pending.assembled = true;

    info!("voxel texture array assembled ({} layers, {}px)", layer_images.len(), target_size);
}

/// Converts any [`Image`] to `target_size × target_size` `Rgba8UnormSrgb`.
fn normalise_to_rgba32(img: &Image, target_size: u32) -> Image {
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

    let scaled = if w == target_size && h == target_size {
        rgba
    } else {
        let mut out = Vec::with_capacity((target_size * target_size * 4) as usize);
        for ty in 0..target_size {
            for tx in 0..target_size {
                let sx = (tx * w / target_size) as usize;
                let sy = (ty * h / target_size) as usize;
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
        Extent3d { width: target_size, height: target_size, depth_or_array_layers: 1 },
        TextureDimension::D2,
        scaled,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}
