//! World manifest — serialised snapshot of all world configuration.
//!
//! Written to `{world_dir}/world.json` on first startup. The file is
//! self-describing: a reader can determine the voxel palette, item palette,
//! drop overrides, crafting recipes, audio, and key player parameters without
//! parsing any game code.

use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::Path;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::audio::SfxConfig;
use crate::crafting::CraftingRegistry;
use crate::items::drops::DropTable;
use crate::items::inventory::ItemType;
use crate::player::PlayerConfig;
use crate::voxel::VoxelType;
use crate::world::persistence::WorldDataDir;
use crate::world::config::WorldConfig;

// ─── Palette entry types ──────────────────────────────────────────────────────

/// One entry in the voxel palette section of the manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoxelPaletteEntry {
    pub byte: u8,
    pub name: String,
    pub color: [f32; 4],
    pub hardness: f32,
    pub mining_level: u8,
    pub alpha: f32,
    pub is_solid: bool,
}

/// One entry in the item palette section of the manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemEntry {
    pub byte: u8,
    pub name: String,
    pub color: [f32; 4],
    pub tool_level: u8,
    pub tool_speed: f32,
}

/// One crafting recipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeEntry {
    pub name: String,
    pub inputs: Vec<(u8, u32)>,  // (item_byte, count)
    pub outputs: Vec<(u8, u32)>, // (item_byte, count)
}

/// One explicit drop override (identity drops are implicit — not listed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropEntry {
    pub voxel_byte: u8,
    pub item_byte: u8,
    pub min_count: u32,
    pub max_count: u32,
}

// ─── WorldManifest ────────────────────────────────────────────────────────────

/// Root world manifest — written to `world.json` on first startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldManifest {
    pub schema_version: u32,
    pub world_name: String,
    pub seed: u64,
    pub voxel_type: String,
    pub voxel_palette: Vec<VoxelPaletteEntry>,
    pub items: Vec<ItemEntry>,
    pub drop_overrides: Vec<DropEntry>,
    pub crafting_recipes: Vec<RecipeEntry>,
    pub audio: HashMap<String, Vec<String>>,
    pub player_speed: f32,
    pub sun_illuminance: f32,
    pub ambient_brightness: f32,
}

impl WorldManifest {
    pub fn schema_version() -> u32 {
        1
    }

    /// Write manifest to `{world_dir}/world.json`. Creates dirs if needed.
    pub fn write(&self, world_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(world_dir)?;
        let path = world_dir.join("world.json");
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }

    /// Read manifest from `{world_dir}/world.json`.
    pub fn read(world_dir: &Path) -> std::io::Result<Self> {
        let path = world_dir.join("world.json");
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Plugin that writes `world.json` into the world data directory on startup.
///
/// The file is only written if it does not already exist, so existing worlds
/// are never overwritten. Generic over the voxel type `V` and item type `I`
/// so the manifest can enumerate their full palettes.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::world::manifest::WorldManifestPlugin;
/// use voxel_framework::voxel::DefaultVoxel;
/// use voxel_framework::items::DefaultItem;
///
/// App::new()
///     .add_plugins(WorldManifestPlugin::<DefaultVoxel, DefaultItem>::new("DefaultVoxel"));
/// ```
pub struct WorldManifestPlugin<V: VoxelType, I: ItemType> {
    pub voxel_type_name: &'static str,
    _v: PhantomData<V>,
    _i: PhantomData<I>,
}

impl<V: VoxelType, I: ItemType> WorldManifestPlugin<V, I> {
    pub fn new(voxel_type_name: &'static str) -> Self {
        Self {
            voxel_type_name,
            _v: PhantomData,
            _i: PhantomData,
        }
    }
}

impl<V: VoxelType, I: ItemType> Plugin for WorldManifestPlugin<V, I> {
    fn build(&self, app: &mut App) {
        let voxel_type_name = self.voxel_type_name;
        app.add_systems(
            Startup,
            move |world_config: Res<WorldConfig>,
                  world_data_dir: Res<WorldDataDir>,
                  drop_table: Option<Res<DropTable<I>>>,
                  crafting_registry: Option<Res<CraftingRegistry<I>>>,
                  sfx_config: Option<Res<SfxConfig>>,
                  player_config: Option<Res<PlayerConfig>>| {
                write_manifest_system::<V, I>(
                    voxel_type_name,
                    &world_config,
                    &world_data_dir,
                    drop_table.as_deref(),
                    crafting_registry.as_deref(),
                    sfx_config.as_deref(),
                    player_config.as_deref(),
                );
            },
        );
    }
}

// ─── Startup system ───────────────────────────────────────────────────────────

fn write_manifest_system<V: VoxelType, I: ItemType>(
    voxel_type_name: &'static str,
    world_config: &WorldConfig,
    world_data_dir: &WorldDataDir,
    drop_table: Option<&DropTable<I>>,
    crafting_registry: Option<&CraftingRegistry<I>>,
    sfx_config: Option<&SfxConfig>,
    player_config: Option<&PlayerConfig>,
) {
    let world_dir = std::path::Path::new(world_data_dir.as_str());
    let manifest_path = world_dir.join("world.json");

    // Skip if already exists.
    if manifest_path.exists() {
        return;
    }

    // ── Voxel palette ─────────────────────────────────────────────────────────
    // Air at byte 0, then all non-air entries for bytes 1..=255.
    let mut voxel_palette: Vec<VoxelPaletteEntry> = {
        let mut out = Vec::new();
        let air = V::air();
        // Always include air at byte 0.
        out.push(VoxelPaletteEntry {
            byte: 0,
            name: air.name().to_string(),
            color: air.color(),
            hardness: air.hardness(),
            mining_level: air.mining_level(),
            alpha: air.alpha(),
            is_solid: air.is_solid(),
        });
        for b in 1u8..=255 {
            let v = V::from_byte(b);
            if v == air {
                continue; // unknown byte — maps to air, skip
            }
            out.push(VoxelPaletteEntry {
                byte: b,
                name: v.name().to_string(),
                color: v.color(),
                hardness: v.hardness(),
                mining_level: v.mining_level(),
                alpha: v.alpha(),
                is_solid: v.is_solid(),
            });
        }
        out
    };
    // Deduplicate by byte (shouldn't be needed but defensive).
    voxel_palette.dedup_by_key(|e| e.byte);

    // ── Item palette ──────────────────────────────────────────────────────────
    let items: Vec<ItemEntry> = {
        let empty = I::default();
        let mut out = Vec::new();
        for b in 0u8..=255 {
            let item = I::from_byte(b);
            // Skip non-empty sentinels that map back to empty (unknown bytes).
            if item == empty && b != 0 {
                continue;
            }
            out.push(ItemEntry {
                byte: b,
                name: item.name().to_string(),
                color: item.color(),
                tool_level: item.tool_level(),
                tool_speed: item.tool_speed(),
            });
        }
        out
    };

    // ── Drop overrides ────────────────────────────────────────────────────────
    let drop_overrides: Vec<DropEntry> = match drop_table {
        Some(table) => table
            .iter_entries()
            .map(|(voxel_byte, item, min_count, max_count)| DropEntry {
                voxel_byte,
                item_byte: item.to_byte(),
                min_count,
                max_count,
            })
            .collect(),
        None => Vec::new(),
    };

    // ── Crafting recipes ──────────────────────────────────────────────────────
    let crafting_recipes: Vec<RecipeEntry> = match crafting_registry {
        Some(registry) => registry
            .recipes
            .iter()
            .map(|r| RecipeEntry {
                name: r.name.to_string(),
                inputs: r
                    .ingredients
                    .iter()
                    .map(|ing| (ing.item.to_byte(), ing.count))
                    .collect(),
                outputs: r
                    .outputs
                    .iter()
                    .map(|out| (out.item.to_byte(), out.count))
                    .collect(),
            })
            .collect(),
        None => Vec::new(),
    };

    // ── Audio ─────────────────────────────────────────────────────────────────
    let audio: HashMap<String, Vec<String>> = match sfx_config {
        Some(cfg) => cfg.sounds.clone(),
        None => HashMap::new(),
    };

    // ── Player speed ──────────────────────────────────────────────────────────
    let player_speed = player_config.map(|c| c.speed).unwrap_or(4.0);

    // ── Assemble manifest ─────────────────────────────────────────────────────
    let manifest = WorldManifest {
        schema_version: WorldManifest::schema_version(),
        world_name: world_config.world_name.clone(),
        seed: world_config.seed,
        voxel_type: voxel_type_name.to_string(),
        voxel_palette,
        items,
        drop_overrides,
        crafting_recipes,
        audio,
        player_speed,
        sun_illuminance: world_config.lighting.sun_illuminance,
        ambient_brightness: world_config.lighting.ambient_brightness,
    };

    match manifest.write(world_dir) {
        Ok(()) => eprintln!("voxel-framework: world manifest written to {:?}", manifest_path),
        Err(e) => eprintln!("voxel-framework: failed to write world manifest to {:?}: {e}", manifest_path),
    }
}
