//! Minimal author-defined stat registry example.
//!
//! This example is intentionally headless: it demonstrates the 0.6 stats
//! definition layer without creating a window or a playable game.

use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::prelude::*;
use voxel_framework::prelude::*;

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(VoxelStatsPlugin)
        .init_resource::<ObservedStatChanges>()
        .init_resource::<SpawnedSubjects>()
        .add_systems(Startup, (register_stats, spawn_subject).chain())
        .add_systems(
            Update,
            (
                simulate_environment_tick,
                simulate_subject_tick,
                record_stat_changes,
            )
                .chain(),
        );

    app.update();
    app.update();

    let world = app.world();
    let registry = world.resource::<StatRegistry>();
    let values = world.resource::<StatSet>();
    let observed = world.resource::<ObservedStatChanges>();
    let subjects = world.resource::<SpawnedSubjects>();

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
    println!(
        "global oxygen: {:.0}% full, critical={}",
        values
            .normalized("oxygen", registry)
            .expect("oxygen should be registered")
            * 100.0,
        values
            .is_at_or_below_percent("oxygen", 0.15, registry)
            .expect("oxygen should be registered")
    );

    for entity in &subjects.0 {
        if let Some(values) = world.get::<StatSet>(*entity) {
            println!(
                "entity {entity:?} stat values:\n{}",
                serde_json::to_string_pretty(&values.snapshot())
                    .expect("entity stat snapshots should serialize")
            );
            println!(
                "entity {entity:?} oxygen: {:.0}% full, critical={}",
                values
                    .normalized("oxygen", registry)
                    .expect("oxygen should be registered")
                    * 100.0,
                values
                    .is_at_or_below_percent("oxygen", 0.15, registry)
                    .expect("oxygen should be registered")
            );
        }
    }

    for change in &observed.0 {
        println!(
            "stat changed: {} {} -> {}",
            change.id, change.previous, change.current
        );
    }
}

#[derive(Resource, Default)]
struct ObservedStatChanges(Vec<StatChanged>);

#[derive(Resource, Default)]
struct SpawnedSubjects(Vec<Entity>);

#[derive(Component)]
struct Explorer;

fn register_stats(mut registry: ResMut<StatRegistry>, mut values: ResMut<StatSet>) {
    registry
        .register(
            StatDefinition::new("oxygen", 100.0)
                .named("Suit Oxygen")
                .bounds(0.0, 100.0)
                .tag("space_suit")
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

fn spawn_subject(
    mut commands: Commands,
    registry: Res<StatRegistry>,
    mut subjects: ResMut<SpawnedSubjects>,
) {
    let entity = commands
        .spawn((Explorer, StatSet::from_registry(&registry)))
        .id();
    subjects.0.push(entity);
}

fn simulate_environment_tick(
    registry: Res<StatRegistry>,
    mut values: ResMut<StatSet>,
    mut changed: MessageWriter<StatChanged>,
) {
    drain_stat("oxygen", 1.5, &registry, &mut values, &mut changed)
        .expect("oxygen should be registered");
    fill_stat("radiation", 0.25, &registry, &mut values, &mut changed)
        .expect("radiation should be registered");
}

fn simulate_subject_tick(
    registry: Res<StatRegistry>,
    mut subjects: Query<&mut StatSet, With<Explorer>>,
    mut changed: MessageWriter<StatChanged>,
) {
    for mut values in &mut subjects {
        drain_stat("oxygen", 5.0, &registry, &mut values, &mut changed)
            .expect("oxygen should be registered");
    }
}

fn record_stat_changes(
    mut reader: MessageReader<StatChanged>,
    mut observed: ResMut<ObservedStatChanges>,
) {
    observed.0.extend(reader.read().cloned());
}
