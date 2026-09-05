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
        .add_plugins(ItemContentPlugin::<DefaultItem>::new())
        .add_plugins(BlockDropContentPlugin::<DefaultItem>::new())
        .add_systems(Startup, (register_blocks, register_items, register_drops));

    app.update();

    let world = app.world();
    let blocks = world.resource::<BlockRegistry<DefaultTerrainVoxel>>();
    let items = world.resource::<ItemRegistry<DefaultItem>>();
    let drops = world.resource::<BlockDropRegistry<DefaultItem>>();

    drops
        .validate_blocks(blocks)
        .expect("drop definitions should reference registered blocks");
    drops
        .validate_items(items)
        .expect("drop definitions should reference registered items");
    let runtime_drop_table = drops
        .to_drop_table(blocks)
        .expect("content drops should convert to runtime drops");

    for block in blocks.iter() {
        println!(
            "{} -> {} ({:?}, hardness {:?})",
            block.id(),
            block.display_name(),
            block.voxel(),
            block.number_attribute("hardness")
        );
    }

    for item in items.iter() {
        println!(
            "{} -> {} ({:?})",
            item.id(),
            item.display_name(),
            item.item()
        );
    }

    for drop in drops.get("moon_regolith") {
        println!(
            "moon_regolith drops {:?} x{}..{}",
            drop.item(),
            drop.count().min,
            drop.count().max
        );
    }

    for (voxel_byte, entry) in runtime_drop_table.iter_drop_entries() {
        println!(
            "runtime voxel byte {} drops {:?} x{}..{} @ {:.0}%",
            voxel_byte,
            entry.item,
            entry.min_count,
            entry.max_count,
            entry.chance * 100.0
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

fn register_items(mut items: ResMut<ItemRegistry<DefaultItem>>) {
    items
        .register(
            ItemDefinition::new("moon_regolith", DefaultItem::Sand)
                .named("Moon Regolith")
                .tag("building_material")
                .attribute("mass", 0.4),
        )
        .expect("item id and item value should be unique");

    items
        .register(
            ItemDefinition::new("dream_glass_shard", DefaultItem::Crystal)
                .named("Dream Glass Shard")
                .tag("ritual_material")
                .attribute("mana_conductivity", 12.0),
        )
        .expect("item id and item value should be unique");
}

fn register_drops(mut drops: ResMut<BlockDropRegistry<DefaultItem>>) {
    drops.register_drop(
        "moon_regolith",
        BlockDropDefinition::new(DefaultItem::Sand, DropCount::range(1, 3))
            .attribute("source", "surface_layer"),
    );

    drops.register_drop(
        "dream_glass",
        BlockDropDefinition::fixed(DefaultItem::Crystal, 1).with_chance(0.35),
    );
}
