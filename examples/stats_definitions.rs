//! Minimal author-defined stat registry example.
//!
//! This example is intentionally headless: it demonstrates the 0.6 stats
//! definition layer without creating a window or a playable game.

use bevy::prelude::*;
use voxel_framework::prelude::*;

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(VoxelStatsPlugin)
        .add_systems(Startup, register_stats)
        .add_systems(Update, simulate_environment_tick);

    app.update();
    app.update();

    let world = app.world();
    let registry = world.resource::<StatRegistry>();
    let values = world.resource::<StatSet>();

    for stat in registry.snapshot() {
        println!(
            "{} -> {} [{}..{}] default {}",
            stat.id, stat.display_name, stat.min_value, stat.max_value, stat.default_value
        );
    }

    println!(
        "stat values:\n{}",
        serde_json::to_string_pretty(&values.snapshot()).expect("stat snapshots should serialize")
    );
}

fn register_stats(mut registry: ResMut<StatRegistry>, mut values: ResMut<StatSet>) {
    registry
        .register(
            StatDefinition::new("oxygen", 100.0)
                .named("Suit Oxygen")
                .bounds(0.0, 100.0)
                .tag("body")
                .attribute("ui_hint", "bar")
                .attribute("critical_threshold", 15.0),
        )
        .expect("oxygen id should be unique");

    registry
        .register(
            StatDefinition::new("radiation", 0.0)
                .named("Radiation Exposure")
                .bounds(0.0, 1_000.0)
                .tag("environment")
                .attribute("ui_hint", "meter"),
        )
        .expect("radiation id should be unique");

    values.ensure_registered(&registry);
}

fn simulate_environment_tick(registry: Res<StatRegistry>, mut values: ResMut<StatSet>) {
    values
        .drain("oxygen", 1.5, &registry)
        .expect("oxygen should be registered");
    values
        .fill("radiation", 0.25, &registry)
        .expect("radiation should be registered");
}
