//! Minimal author-defined retained HUD example.
//!
//! This example is intentionally headless and domain-neutral. It registers the
//! four presentation kinds, publishes screen-ready values in the HUD publish
//! schedule, and performs multiple updates without opening a window or loading
//! assets.

use bevy::ecs::message::MessageWriter;
use bevy::prelude::*;
use voxel_framework::prelude::*;

fn main() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, VoxelHudPlugin))
        .init_resource::<ExampleUpdate>()
        .add_systems(Startup, register_hud)
        .add_systems(Update, publish_example_values.in_set(HudSystemSet::Publish));

    // The first update runs Startup/PostStartup, then publishes and renders the
    // first replacement values in the same frame. Later updates reuse the
    // retained element entities and mutate only changed presentation state.
    for label in ["first update", "second update", "unchanged update"] {
        app.update();
        print_snapshot(label, app.world());
    }
}

#[derive(Resource, Default)]
struct ExampleUpdate(u8);

fn register_hud(
    mut registry: ResMut<HudRegistry>,
    mut values: ResMut<HudValueSet>,
    mut changed: MessageWriter<HudValueChanged>,
) {
    let definitions = [
        HudElementDefinition::new(
            "sample.text",
            HudPresentation::Text(HudText::new("sample.text.value").prefix("Text: ")),
        ),
        HudElementDefinition::new(
            "sample.bar",
            HudPresentation::Bar(HudBar::new("sample.bar.value")),
        )
        .placement(HudPlacement::new(HudAnchor::TopCenter))
        .fallback(HudScalar::new(0.0, 0.0, 1.0).expect("fallback range is valid")),
        HudElementDefinition::new(
            "sample.icon",
            HudPresentation::Icon(HudIcon::bound("sample.icon.value")),
        )
        .placement(HudPlacement::new(HudAnchor::TopRight)),
        HudElementDefinition::new(
            "sample.segments",
            HudPresentation::Segments(HudSegments::new("sample.segments.value", 5)),
        )
        .placement(HudPlacement::new(HudAnchor::BottomCenter)),
    ];

    for definition in definitions {
        registry
            .register(definition)
            .expect("sample HUD element ids should be unique and valid");
    }

    set_hud_value("sample.text.value", "startup", &mut values, &mut changed);
    set_hud_value(
        "sample.bar.value",
        HudScalar::new(0.25, 0.0, 1.0).expect("sample scalar is valid"),
        &mut values,
        &mut changed,
    );
    // With MinimalPlugins there is no AssetServer, so these illustrative paths
    // are inspected as values but never read from disk.
    set_hud_value(
        "sample.icon.value",
        HudValue::icon("icons/sample-a.png"),
        &mut values,
        &mut changed,
    );
    set_hud_value("sample.segments.value", 1_u32, &mut values, &mut changed);
}

fn publish_example_values(
    mut update: ResMut<ExampleUpdate>,
    mut values: ResMut<HudValueSet>,
    mut changed: MessageWriter<HudValueChanged>,
) {
    let Some((text, ratio, icon, segments)) = [
        ("alpha", 0.5, "icons/sample-b.png", 3),
        ("beta", 0.75, "icons/sample-c.png", 4),
    ]
    .get(update.0 as usize) else {
        return;
    };

    set_hud_value("sample.text.value", *text, &mut values, &mut changed);
    set_hud_value(
        "sample.bar.value",
        HudScalar::new(*ratio, 0.0, 1.0).expect("sample scalar is valid"),
        &mut values,
        &mut changed,
    );
    set_hud_value(
        "sample.icon.value",
        HudValue::icon(*icon),
        &mut values,
        &mut changed,
    );
    set_hud_value(
        "sample.segments.value",
        *segments,
        &mut values,
        &mut changed,
    );
    update.0 += 1;
}

fn print_snapshot(label: &str, world: &World) {
    let registry = world.resource::<HudRegistry>();
    let values = world.resource::<HudValueSet>();

    println!(
        "{label}: {} retained definitions at revision {}, values at revision {}",
        registry.len(),
        registry.revision(),
        values.revision()
    );
    for snapshot in values.snapshot() {
        println!("  {} = {:?}", snapshot.id, snapshot.value);
    }
}
