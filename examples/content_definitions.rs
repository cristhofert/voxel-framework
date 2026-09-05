//! Minimal author-defined content registry example.
//!
//! This example is intentionally headless: it demonstrates the 0.5 content
//! definition layer without creating a window or a playable game.

use bevy::prelude::*;
use voxel_framework::prelude::*;

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(VoxelContentPlugin::<DefaultTerrainVoxel>::new())
        .add_systems(Startup, register_blocks);

    app.update();

    let blocks = app.world().resource::<BlockRegistry<DefaultTerrainVoxel>>();
    for block in blocks.iter() {
        println!(
            "{} -> {} ({:?})",
            block.id(),
            block.display_name(),
            block.voxel()
        );
    }
}

fn register_blocks(mut blocks: ResMut<BlockRegistry<DefaultTerrainVoxel>>) {
    blocks
        .register(
            BlockDefinition::new("moon_regolith", DefaultTerrainVoxel::Sand)
                .named("Moon Regolith")
                .tag("terrain")
                .attribute("hardness", 0.8)
                .attribute("dusty", true)
                .attribute("drops", ContentValue::strings(["moon_regolith"])),
        )
        .expect("block id and voxel value should be unique");

    blocks
        .register(
            BlockDefinition::new("dream_glass", DefaultTerrainVoxel::Water)
                .named("Dream Glass")
                .engine_properties(BlockEngineProperties::new(
                    true,
                    false,
                    0.35,
                    [0.45, 0.75, 1.0, 0.35],
                    [9; 6],
                ))
                .tag("ritual_material")
                .attribute("mana_conductivity", 12.0)
                .attribute("fragile", true),
        )
        .expect("block id and voxel value should be unique");
}
