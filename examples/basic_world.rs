//! Basic world example — minimal voxel-framework demo with texture array.
//!
//! Controls: WASD / Space / Shift / Q+E roll / mouse look.
//! Escape — pause menu  |  Left click — lock cursor.
//!
//! Textures (32×32 PNGs from assets/basic_world/textures/):
//!   Layer 0 — Air placeholder (grey, programmatically generated)
//!   Layer 1 — Rock         → stone.png
//!   Layer 2 — IronOre      → iron_ore.png
//!   Layer 3 — SiliconCrystal → amethyst_block.png
//!   Layer 4 — RareMineral  → emerald_ore.png

use std::sync::Arc;
use bevy::prelude::*;
use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::asset::RenderAssetUsages;
use voxel_framework::prelude::*;
use voxel_framework::audio::{SfxConfig, SfxEvent, SfxRequest};
use voxel_framework::player::PlayerPlugin;

/// Handles to the voxel textures pending assembly into a texture array.
#[derive(Resource)]
struct PendingTextures {
    handles: Vec<Handle<Image>>,
    assembled: bool,
}

fn main() {
    let world_config = WorldConfig::from_file("world.toml").unwrap_or_default();
    let generator    = world_config.build_asteroid_generator();
    let spawn_pos    = Vec3::from_array(world_config.spawn_position);

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
        .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())
        .add_plugins(PersistencePlugin::<DefaultVoxel>::new())
        .add_plugins(PauseMenuPlugin)
        .add_plugins(PlayerPlugin::<DefaultVoxel>::new())
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
            sfx.register(SfxEvent::ItemPickup,    vec!["basic_world/audio/rollover1.ogg"]);
            sfx.register(SfxEvent::HotbarScroll,  vec!["basic_world/audio/rollover1.ogg"]);
            sfx.register(SfxEvent::PauseOpen,     vec!["basic_world/audio/click1.ogg"]);
            sfx.register(SfxEvent::PauseClose,    vec!["basic_world/audio/click1.ogg"]);
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
        .add_systems(Startup, setup)
        .add_systems(Update, (play_sfx, assemble_texture_array))
        .run();
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

fn setup(
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

    // Layer 0: Air placeholder — a 32×32 mid-grey tile (not rendered in practice
    // because Air voxels produce no geometry, but the array slot must exist).
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

    // Layers 1–4: Load actual block textures.
    let stone   = asset_server.load("basic_world/textures/stone.png");
    let iron    = asset_server.load("basic_world/textures/iron_ore.png");
    let silicon = asset_server.load("basic_world/textures/amethyst_block.png");
    let rare    = asset_server.load("basic_world/textures/emerald_ore.png");

    commands.insert_resource(PendingTextures {
        handles: vec![placeholder_handle, stone, iron, silicon, rare],
        assembled: false,
    });
}

/// Waits until all voxel textures are loaded, then assembles them into a
/// `texture_2d_array` and inserts [`ChunkTextureArray`].
fn assemble_texture_array(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut pending: ResMut<PendingTextures>,
) {
    if pending.assembled {
        return;
    }

    // Wait until all image assets are available in the Assets<Image> store.
    // We don't use is_loaded_with_dependencies because it requires all
    // RecursiveDependencies to be Loaded, which can stall for certain PNG formats.
    let all_ready = pending.handles.iter().all(|h| images.contains(h));
    if !all_ready {
        return;
    }

    // Collect images and normalise them to Rgba8UnormSrgb 32×32.
    // Bevy may decode PNGs to different formats depending on the source;
    // we convert any grayscale / palette images to RGBA here.
    let mut layer_images: Vec<Image> = Vec::new();
    for handle in &pending.handles {
        let Some(img) = images.get(handle) else { return; };
        let normalised = normalise_to_rgba32(img);
        layer_images.push(normalised);
    }

    // Build the texture array and add it to the asset store.
    let array_image = build_texture_array(&layer_images);
    let array_handle = images.add(array_image);

    commands.insert_resource(ChunkTextureArray { handle: array_handle });
    pending.assembled = true;

    info!("voxel texture array assembled ({} layers)", layer_images.len());
}

/// Converts any [`Image`] to 32×32 `Rgba8UnormSrgb` format.
///
/// Bevy decodes PNGs into the format matching their colour type
/// (e.g. 8-bit grayscale → `R8Unorm`, 8-bit palette → `Rgba8UnormSrgb`).
/// The texture array requires all layers to share the same format,
/// so we normalise everything to `Rgba8UnormSrgb` here.
fn normalise_to_rgba32(img: &Image) -> Image {
    const TARGET_W: u32 = 32;
    const TARGET_H: u32 = 32;

    let src_data = img.data.as_deref().unwrap_or(&[]);
    let w = img.width();
    let h = img.height();
    let fmt = img.texture_descriptor.format;

    // Convert pixel data to RGBA8 depending on source format.
    let rgba: Vec<u8> = match fmt {
        TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm => {
            // Already RGBA8 — copy as-is (possibly resize below).
            src_data.to_vec()
        }
        TextureFormat::R8Unorm => {
            // Grayscale → RGBA (replicate R into G/B, set A=255).
            src_data.iter().flat_map(|&r| [r, r, r, 255u8]).collect()
        }
        TextureFormat::Rg8Unorm => {
            // RG → RGBA (R=R, G=G, B=0, A=255).
            src_data.chunks(2).flat_map(|p| [p[0], p[1], 0u8, 255u8]).collect()
        }
        _ => {
            // Fallback: fill with a bright magenta so missing textures are obvious.
            warn!("normalise_to_rgba32: unsupported format {:?}, filling with magenta", fmt);
            vec![255u8, 0, 255, 255].repeat((w * h) as usize)
        }
    };

    // Simple nearest-neighbour scale to 32×32 if needed.
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
