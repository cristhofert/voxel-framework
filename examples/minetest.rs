//! Minetest-style example — grounded terrain world with gravity, jump, mining.
//!
//! Controls:
//!   WASD — walk  |  Space — jump  |  Mouse — look
//!   Shift — sprint  |  Ctrl — crouch
//!   Left click — mine block  |  Right click — place block from hotbar
//!   Scroll wheel — switch hotbar slot
//!   E — inventory  |  C — crafting menu
//!   Escape — pause menu  |  F3 — debug overlay
//!
//! Debug effects (for demo/testing):
//!   F5 — Speed effect  |  F6 — Regeneration effect
//!   F7 — Poison effect  |  F8 — Jump Boost effect
//!   P — Haste effect

use std::sync::Arc;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::asset::RenderAssetUsages;
use voxel_framework::prelude::*;
use voxel_framework::audio::{DefaultAudioPlugin, SfxConfig, SfxEvent};
use voxel_framework::items::{DefaultItem, ItemCollector};
use voxel_framework::items::inventory::{ContainerComponent, PrimaryContainer};
use voxel_framework::mobs::{MobConfig, MobPlugin, spawn_mob};

// ─── Status effects ──────────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Clone, PartialEq)]
enum MinetestEffect {
    Speed,
    Slowness,
    JumpBoost,
    Haste,
    MiningFatigue,
    Regeneration,
    Poison,
    Resistance,
}

impl StatusEffect for MinetestEffect {
    fn name(&self) -> &'static str {
        match self {
            Self::Speed         => "Speed",
            Self::Slowness      => "Slowness",
            Self::JumpBoost     => "Jump Boost",
            Self::Haste         => "Haste",
            Self::MiningFatigue => "Mining Fatigue",
            Self::Regeneration  => "Regeneration",
            Self::Poison        => "Poison",
            Self::Resistance    => "Resistance",
        }
    }

    fn modifiers(&self, amplifier: u8) -> EffectModifiers {
        let lvl = (amplifier + 1) as f32;
        match self {
            Self::Speed        => EffectModifiers { speed_mult: 1.0 + 0.2 * lvl, ..Default::default() },
            Self::Slowness     => EffectModifiers { speed_mult: 1.0 / (1.0 + 0.15 * lvl), ..Default::default() },
            Self::JumpBoost    => EffectModifiers { jump_mult: 1.0 + 0.2 * lvl, ..Default::default() },
            Self::Haste        => EffectModifiers { mining_speed_mult: 1.0 + 0.2 * lvl, ..Default::default() },
            Self::MiningFatigue => EffectModifiers { mining_speed_mult: 1.0 / (1.0 + 0.3 * lvl), ..Default::default() },
            Self::Regeneration => EffectModifiers { regen_per_sec: 1.0 * lvl, ..Default::default() },
            Self::Poison       => EffectModifiers { damage_per_sec: 1.0 * lvl, ..Default::default() },
            Self::Resistance   => EffectModifiers { damage_resistance: 0.2 * lvl, ..Default::default() },
        }
    }

    fn is_beneficial(&self) -> bool {
        !matches!(self, Self::Slowness | Self::MiningFatigue | Self::Poison)
    }

    fn color(&self) -> [f32; 4] {
        match self {
            Self::Speed         => [0.4, 0.8, 1.0, 1.0],
            Self::Slowness      => [0.5, 0.5, 0.7, 1.0],
            Self::JumpBoost     => [0.5, 1.0, 0.5, 1.0],
            Self::Haste         => [0.9, 0.7, 0.2, 1.0],
            Self::MiningFatigue => [0.3, 0.2, 0.5, 1.0],
            Self::Regeneration  => [0.6, 1.0, 0.6, 1.0],
            Self::Poison        => [0.4, 0.6, 0.1, 1.0],
            Self::Resistance    => [0.6, 0.4, 0.8, 1.0],
        }
    }
}

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
            v.name(),
            v.hardness(),
            v.mining_level(),
            pos.x, pos.y, pos.z,
        )),
        None => Text::new("Mirando: —"),
    };
}

/// Reads `--world <name>` from CLI args.  Falls back to `default_name`.
///
/// Load order:
/// 1. `worlds/<name>/world.json` — if the world has been run before, load from
///    the self-contained manifest written by [`WorldManifestPlugin`].
/// 2. `examples/<name>.toml` — template config for the first run.
/// 3. [`WorldConfig::default`] with `world_name` set — if neither file exists.
fn load_world_config(default_name: &str) -> WorldConfig {
    let args: Vec<String> = std::env::args().collect();
    let name = args.windows(2)
        .find(|w| w[0] == "--world")
        .map(|w| w[1].clone())
        .unwrap_or_else(|| default_name.to_string());

    // Try the persisted world manifest first.
    let manifest_path = format!("worlds/{}/world.json", name);
    if std::path::Path::new(&manifest_path).exists() {
        return WorldConfig::from_world_dir(&name);
    }

    // Fall back to the TOML template for the first run.
    let toml_path = format!("examples/{}.toml", name);
    WorldConfig::from_file(&toml_path)
        .inspect_err(|e| eprintln!("No se pudo cargar '{toml_path}': {e}. Usando defaults."))
        .unwrap_or_else(|_| WorldConfig { world_name: name, ..WorldConfig::default() })
}

fn main() {
    let mut world_config = load_world_config("minetest");

    let mut player_config = PlayerConfig {
        spawn_position: Vec3::new(8.0, 60.0, 8.0),
        speed: 6.0,
        mode: PlayerMode::Grounded { gravity: 20.0, jump_speed: 8.0 },
        ..default()
    };

    let mut sfx_config = SfxConfig::new();

    // Apply persisted settings if available
    if let Some(saved) = PersistedSettings::load(&world_config.world_name) {
        player_config.fov_degrees = saved.fov_degrees;
        player_config.sensitivity = (saved.sensitivity_pct.clamp(10, 100) as f32) / 100.0;
        world_config.load_distance = (saved.render_distance_chunks as f32) * 16.0;
        world_config.unload_distance = world_config.load_distance + 32.0;
        sfx_config.volume = saved.sfx_volume;
    }

    App::new()
        .add_plugins(DefaultPlugins)
        // ── Engine (terrain voxel type) ──────────────────────────────────────
        .add_plugins(VoxelPlugin::<DefaultTerrainVoxel>::new())
        .add_plugins(GeneratorPlugin::<DefaultTerrainVoxel>::new())
        .add_plugins(PersistencePlugin::<DefaultTerrainVoxel>::with_world(&world_config.world_name))
        // ── Player (grounded with gravity) ───────────────────────────────────
        .add_plugins(PlayerPlugin::<DefaultTerrainVoxel>::with_world(&world_config.world_name))
        // ── Mining & placement input (batteries-included) ────────────────────
        .add_plugins(MiningInputPlugin::<DefaultTerrainVoxel, DefaultItem>::new())
        .add_plugins(DropsPlugin::<DefaultItem>::new())
        // PlacementInputPlugin maps hotbar slot N to voxel byte N+1:
        // slot 0 → Grass, slot 1 → Dirt, slot 2 → Stone, etc.
        .add_plugins(PlacementInputPlugin::<DefaultTerrainVoxel>::new())
        // ── Crafting (C to open) ─────────────────────────────────────────────
        .add_plugins(CraftingPlugin::<DefaultItem>::new())
        .add_plugins(CraftingUiPlugin::<DefaultItem>::new())
        // ── Day/night cycle (manages sun + sky color) ────────────────────────
        .add_plugins(DayCyclePlugin)
        // ── Debug overlay (F3 toggle) ────────────────────────────────────────
        .add_plugins(DebugOverlayPlugin)
        // ── Mobs (patrol + chase + attack) ──────────────────────────────────
        .add_plugins(MobPlugin::<DefaultTerrainVoxel>::new())
        // ── Audio (real bevy_audio playback) ────────────────────────────────
        .add_plugins(DefaultAudioPlugin)
        // ── World manifest (writes world.json on first run) ──────────────────
        .add_plugins(WorldManifestPlugin::<DefaultTerrainVoxel, DefaultItem>::new("DefaultTerrainVoxel"))
        // ── Status effects ────────────────────────────────────────────────────
        .add_plugins(StatusEffectsPlugin::<MinetestEffect>::new())
        // ── Pause menu ───────────────────────────────────────────────────────
        .add_plugins(PauseMenuPlugin)
        // ── Resources ────────────────────────────────────────────────────────
        .insert_resource(sfx_config)
        .insert_resource(GeneratorResource::<DefaultTerrainVoxel> {
            generator: Arc::new(TerrainGenerator::new(TerrainConfig::default())),
        })
        .insert_resource(player_config)
        .insert_resource(DayCycleConfig {
            day_length_secs: 300.0, // 5-minute days
            time_of_day: 0.35,      // start mid-morning — sun visible
            ..default()
        })
        .insert_resource(world_config)
        // ── Block-info overlay (looked-at block) ─────────────────────────────
        .init_resource::<LookedAtBlock>()
        .add_systems(Startup, (spawn_player_container, setup_crafting, setup_drops, setup_audio, setup_mobs, spawn_block_info_ui, setup_textures, spawn_inventory_ui, spawn_effects_hud))
        .add_systems(Update, (
            update_block_info,
            assemble_texture_array,
            toggle_inventory_ui,
            update_inventory_ui,
            debug_apply_effects,
        ).run_if(in_state(PauseMenuState::Playing)))
        .add_systems(Update, (update_effects_hud, handle_save_settings))
        .run();
}

/// Persists current in-memory settings (FOV, volume, render distance, sensitivity)
/// to `worlds/{world_name}/settings.json` whenever the pause-menu Settings submenu
/// is closed via the Back button.
fn handle_save_settings(
    mut reader: MessageReader<SaveSettingsRequest>,
    player_config: Res<PlayerConfig>,
    sfx_config: Res<SfxConfig>,
    world_config: Res<WorldConfig>,
) {
    for _ in reader.read() {
        let settings = PersistedSettings {
            fov_degrees: player_config.fov_degrees,
            sfx_volume: sfx_config.volume,
            render_distance_chunks: (world_config.load_distance / 16.0).round() as u32,
            sensitivity_pct: (player_config.sensitivity * 100.0).round() as u32,
        };
        settings.save(&world_config.world_name);
    }
}

/// Spawns the player's item container entity with `ContainerComponent`,
/// `PrimaryContainer`, and `ItemCollector` so the magnet, hotbar, and crafting
/// systems can locate it via ECS queries.
fn spawn_player_container(mut commands: Commands) {
    commands.spawn((
        ContainerComponent::<DefaultItem>::new(36),
        PrimaryContainer,
        ItemCollector,
    ));
}

/// Seeds the crafting registry with example recipes and the inventory with
/// starting materials so the player can try crafting immediately.
fn setup_crafting(
    mut registry: ResMut<CraftingRegistry<DefaultItem>>,
    mut container_query: Query<&mut ContainerComponent<DefaultItem>, With<PrimaryContainer>>,
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

    // Seed starting tools and a small rock stash so the player can mine and
    // try crafting immediately.  Resources like IronOre must be earned by mining.
    if let Ok(mut container) = container_query.single_mut() {
        container.0.add(DefaultItem::WoodenPickaxe, 1);
        container.0.add(DefaultItem::StonePickaxe, 1);
        container.0.add(DefaultItem::RockFragment, 3);
    }
}

/// Populates the [`DropTable`] with item drops for each terrain voxel.
///
/// Byte mapping (matches `DefaultTerrainVoxel::to_byte`):
///   1 = Grass, 3 = Stone, 5 = Wood, 8 = Snow, 10 = CoalOre
///
/// Identity drops (voxel byte == item byte) are omitted — the framework
/// handles those automatically via the shared byte key:
///   Dirt (2), Sand (4), Gravel (7), Cobblestone (12), Sandstone (13),
///   Planks (14), Brick (15).
///
/// Leaves (6) and Water (9) drop nothing and are not registered.
fn setup_drops(mut drop_table: ResMut<DropTable<DefaultItem>>) {
    // Grass → Dirt (grass block drops dirt, no placeable Grass item)
    drop_table.register(1, DefaultItem::Dirt, 1, 1);
    // Stone → Cobblestone (mining_level 1, requires pickaxe)
    drop_table.register(3, DefaultItem::Cobblestone, 1, 1);
    // Wood → Log (wood voxel drops a log item, not a wood block)
    drop_table.register(5, DefaultItem::Log, 1, 1);
    // Snow → Snowball (snow voxel drops a snowball, not a snow block)
    drop_table.register(8, DefaultItem::Snowball, 1, 2);
    // CoalOre → Coal (ore drops processed coal lump)
    drop_table.register(10, DefaultItem::Coal, 1, 1);
    // Leaves and Water drop nothing
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
    // Pause menu font: the framework embeds NotoSans via `embedded_asset!`
    // and wires it up automatically in `PauseMenuPlugin`, so no explicit
    // `PauseMenuConfig` insert is needed here.

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

// ─── Status effects HUD ───────────────────────────────────────────────────────

/// Marker for the status-effects HUD root node (top-right panel).
#[derive(Component)]
struct EffectsHudRoot;

/// Marker for a single effect row.
#[derive(Component)]
struct EffectHudRow;

/// Spawns the effects HUD root node — initially hidden.
fn spawn_effects_hud(mut commands: Commands) {
    commands.spawn((
        EffectsHudRoot,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            right: Val::Px(12.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}

/// Rebuilds the effects HUD every frame from `ActiveEffects<MinetestEffect>`.
fn update_effects_hud(
    effects: Res<ActiveEffects<MinetestEffect>>,
    mut root_q: Query<(Entity, &mut Visibility), With<EffectsHudRoot>>,
    row_q: Query<Entity, With<EffectHudRow>>,
    mut commands: Commands,
) {
    // 1. Despawn all existing rows.
    for entity in &row_q {
        commands.entity(entity).despawn();
    }

    let Ok((root_entity, mut root_vis)) = root_q.single_mut() else { return };

    // 2. Hide when no active effects.
    if effects.effects.is_empty() {
        *root_vis = Visibility::Hidden;
        return;
    }

    // 3. Show panel.
    *root_vis = Visibility::Visible;

    // 4. Spawn one row per active effect.
    for active in effects.effects.iter() {
        let c = active.effect.color();
        let first_char = active.effect.name().chars().next().unwrap_or('?').to_string();
        let secs_text = format!("{:.0}s", active.remaining_secs);

        // Icon square (28×28) with first letter centred inside.
        let icon = commands.spawn((
            Node {
                width: Val::Px(28.0),
                height: Val::Px(28.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(c[0], c[1], c[2], c[3])),
        ))
        .with_children(|icon_root| {
            icon_root.spawn((
                Text::new(first_char),
                TextFont { font_size: 16.0, ..default() },
                TextColor(Color::WHITE),
            ));
        })
        .id();

        // Duration label.
        let label = commands.spawn((
            Text::new(secs_text),
            TextFont { font_size: 14.0, ..default() },
            TextColor(Color::WHITE),
            Node {
                margin: UiRect::left(Val::Px(6.0)),
                align_self: AlignSelf::Center,
                ..default()
            },
        ))
        .id();

        // Row (horizontal flex).
        let row = commands.spawn((
            EffectHudRow,
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .add_children(&[icon, label])
        .id();

        commands.entity(root_entity).add_child(row);
    }
}

fn debug_apply_effects(
    keys: Res<ButtonInput<KeyCode>>,
    mut effects: ResMut<ActiveEffects<MinetestEffect>>,
) {
    if keys.just_pressed(KeyCode::F5) {
        effects.apply(MinetestEffect::Speed, 1, 10.0);
    }
    if keys.just_pressed(KeyCode::F6) {
        effects.apply(MinetestEffect::Regeneration, 1, 15.0);
    }
    if keys.just_pressed(KeyCode::F7) {
        effects.apply(MinetestEffect::Poison, 1, 8.0);
    }
    if keys.just_pressed(KeyCode::F8) {
        effects.apply(MinetestEffect::JumpBoost, 2, 20.0);
    }
    if keys.just_pressed(KeyCode::KeyP) {
        effects.apply(MinetestEffect::Haste, 1, 30.0);
    }
}

// ─── Inventory UI ─────────────────────────────────────────────────────────────

/// Marker for the root inventory overlay node.
#[derive(Component)]
struct InventoryUiRoot;

/// Marker for individual inventory slot nodes (index stored as component).
#[derive(Component)]
struct InventorySlotNode(usize);

/// Spawns a 4×9 inventory grid overlay (hidden by default, toggle with E).
fn spawn_inventory_ui(mut commands: Commands) {
    const COLS: usize = 9;
    const ROWS: usize = 4;
    const SLOT_SIZE: f32 = 44.0;
    const GAP: f32 = 4.0;

    commands
        .spawn((
            InventoryUiRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.65)),
            Visibility::Hidden,
            ZIndex(200),
        ))
        .with_children(|root| {
            // Title
            root.spawn((
                Text::new("Inventario"),
                TextFont { font_size: 20.0, ..default() },
                TextColor(Color::WHITE),
                Node { margin: UiRect::bottom(Val::Px(12.0)), ..default() },
            ));

            // Grid container
            root.spawn(Node {
                display: Display::Grid,
                grid_template_columns: vec![RepeatedGridTrack::px(GridTrackRepetition::Count(COLS as u16), SLOT_SIZE)],
                column_gap: Val::Px(GAP),
                row_gap: Val::Px(GAP),
                ..default()
            })
            .with_children(|grid| {
                for i in 0..(COLS * ROWS) {
                    grid.spawn((
                        InventorySlotNode(i),
                        Node {
                            width: Val::Px(SLOT_SIZE),
                            height: Val::Px(SLOT_SIZE),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.9)),
                    ))
                    .with_children(|slot| {
                        slot.spawn((
                            Text::new(""),
                            TextFont { font_size: 11.0, ..default() },
                            TextColor(Color::WHITE),
                            Node {
                                position_type: PositionType::Absolute,
                                bottom: Val::Px(2.0),
                                right: Val::Px(4.0),
                                ..default()
                            },
                        ));
                    });
                }
            });
        });
}

/// Toggles the inventory overlay on E key.
fn toggle_inventory_ui(
    keys: Res<ButtonInput<KeyCode>>,
    mut root_q: Query<&mut Visibility, With<InventoryUiRoot>>,
) {
    if keys.just_pressed(KeyCode::KeyE) {
        if let Ok(mut vis) = root_q.single_mut() {
            *vis = if *vis == Visibility::Hidden {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

/// Updates each slot's color and count text from the PrimaryContainer.
fn update_inventory_ui(
    container_q: Query<&ContainerComponent<DefaultItem>, With<PrimaryContainer>>,
    mut slot_q: Query<(&InventorySlotNode, &mut BackgroundColor, &Children)>,
    mut text_q: Query<&mut Text>,
) {
    let Ok(container) = container_q.single() else { return };

    for (slot_node, mut bg, children) in &mut slot_q {
        let item_slot = container.0.slots.get(slot_node.0).and_then(|s| s.as_ref());

        if let Some(slot) = item_slot {
            let c = slot.item.color();
            *bg = BackgroundColor(Color::srgba(c[0], c[1], c[2], c[3]));
            if let Some(&child) = children.first() {
                if let Ok(mut text) = text_q.get_mut(child) {
                    *text = Text::new(slot.count.to_string());
                }
            }
        } else {
            *bg = BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.9));
            if let Some(&child) = children.first() {
                if let Ok(mut text) = text_q.get_mut(child) {
                    *text = Text::new("");
                }
            }
        }
    }
}
