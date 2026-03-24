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
use voxel_framework::prelude::*;
use voxel_framework::items::{DefaultItem, Inventory};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        // ── Engine (terrain voxel type) ──────────────────────────────────────
        .add_plugins(VoxelPlugin::<DefaultTerrainVoxel>::new())
        .add_plugins(GeneratorPlugin::<DefaultTerrainVoxel>::new())
        // ── Player (grounded with gravity) ───────────────────────────────────
        .add_plugins(PlayerPlugin::<DefaultTerrainVoxel>::new())
        // ── Mining & placement input (batteries-included) ────────────────────
        .add_plugins(MiningInputPlugin::<DefaultTerrainVoxel>::new())
        .add_plugins(PlacementInputPlugin::<DefaultTerrainVoxel>::new())
        // ── Crafting (C to open) ─────────────────────────────────────────────
        .add_plugins(CraftingPlugin::<DefaultItem>::new())
        // ── Day/night cycle (manages sun + sky color) ────────────────────────
        .add_plugins(DayCyclePlugin)
        // ── Debug overlay (F3 toggle) ────────────────────────────────────────
        .add_plugins(DebugOverlayPlugin)
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
            time_of_day: 0.25,      // start at dawn
            ..default()
        })
        .insert_resource(Inventory::<DefaultItem>::default())
        .add_systems(Startup, setup_crafting)
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
