//! Minimal author-defined effect registry example.
//!
//! This example is intentionally headless: it demonstrates the 0.7 effects
//! definition layer without creating a window or a playable game.

use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::prelude::*;
use voxel_framework::prelude::*;

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(VoxelEffectsPlugin)
        .init_resource::<SpawnedSubjects>()
        .init_resource::<ObservedEffectEvents>()
        .add_systems(Startup, (register_effects, spawn_subject).chain())
        .add_systems(
            Update,
            (
                apply_global_effect,
                apply_subject_effect,
                tick_global_effects,
                record_effect_events,
            )
                .chain(),
        );

    app.update();

    let world = app.world();
    let registry = world.resource::<EffectRegistry>();
    let active = world.resource::<ActiveEffectSet>();
    let subjects = world.resource::<SpawnedSubjects>();
    let observed = world.resource::<ObservedEffectEvents>();

    for effect in registry.snapshot() {
        println!(
            "{} -> {} duration {:?}, max stacks {}",
            effect.id, effect.display_name, effect.default_duration_secs, effect.max_stacks
        );
    }

    println!(
        "global effects:\n{}",
        serde_json::to_string_pretty(&active.snapshot())
            .expect("active effect snapshots should serialize")
    );
    println!(
        "low_oxygen active={} stacks={} remaining_secs={:?} ratio={:?}",
        active.is_active("low_oxygen"),
        active.stacks("low_oxygen"),
        active.remaining_secs("low_oxygen"),
        active.remaining_ratio("low_oxygen")
    );

    for entity in &subjects.0 {
        if let Some(effects) = world.get::<ActiveEffectSet>(*entity) {
            println!(
                "entity {entity:?} effects:\n{}",
                serde_json::to_string_pretty(&effects.snapshot())
                    .expect("entity effect snapshots should serialize")
            );
            println!(
                "magnetic_boots active={} stacks={} remaining_secs={:?} ratio={:?}",
                effects.is_active("magnetic_boots"),
                effects.stacks("magnetic_boots"),
                effects.remaining_secs("magnetic_boots"),
                effects.remaining_ratio("magnetic_boots")
            );
        }
    }

    for event in &observed.applied {
        println!(
            "effect applied: {} stacks {} previous={}",
            event.id,
            event.current.stacks(),
            event.previous.is_some()
        );
    }

    for event in &observed.expired {
        println!(
            "effect expired: {} after {:?}s",
            event.id,
            event.expired.remaining_secs()
        );
    }
}

#[derive(Resource, Default)]
struct SpawnedSubjects(Vec<Entity>);

#[derive(Resource, Default)]
struct ObservedEffectEvents {
    applied: Vec<EffectApplied>,
    expired: Vec<EffectExpired>,
}

#[derive(Component)]
struct Explorer;

fn register_effects(mut registry: ResMut<EffectRegistry>) {
    registry
        .register(
            EffectDefinition::new("low_oxygen")
                .named("Low Oxygen")
                .duration_secs(10.0)
                .max_stacks(3)
                .tag("space_suit")
                .attribute("ui_hint", "warning"),
        )
        .expect("low_oxygen id should be unique");

    registry
        .register(
            EffectDefinition::new("magnetic_boots")
                .named("Magnetic Boots")
                .indefinite()
                .tag("movement")
                .attribute("ui_hint", "status_icon"),
        )
        .expect("magnetic_boots id should be unique");
}

fn spawn_subject(mut commands: Commands, mut subjects: ResMut<SpawnedSubjects>) {
    let entity = commands.spawn((Explorer, ActiveEffectSet::new())).id();
    subjects.0.push(entity);
}

fn apply_global_effect(
    registry: Res<EffectRegistry>,
    mut active: ResMut<ActiveEffectSet>,
    mut applied: MessageWriter<EffectApplied>,
) {
    apply_effect("low_oxygen", &registry, &mut active, &mut applied)
        .expect("low_oxygen should be registered");
}

fn apply_subject_effect(
    registry: Res<EffectRegistry>,
    mut subjects: Query<&mut ActiveEffectSet, With<Explorer>>,
    mut applied: MessageWriter<EffectApplied>,
) {
    for mut effects in &mut subjects {
        apply_effect("magnetic_boots", &registry, &mut effects, &mut applied)
            .expect("magnetic_boots should be registered");
    }
}

fn tick_global_effects(
    mut active: ResMut<ActiveEffectSet>,
    mut expired: MessageWriter<EffectExpired>,
) {
    tick_effects(10.0, &mut active, &mut expired);
}

fn record_effect_events(
    mut applied: MessageReader<EffectApplied>,
    mut expired: MessageReader<EffectExpired>,
    mut observed: ResMut<ObservedEffectEvents>,
) {
    observed.applied.extend(applied.read().cloned());
    observed.expired.extend(expired.read().cloned());
}
