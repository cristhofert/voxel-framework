# Author-defined retained HUD

`VoxelHudPlugin` is an explicit, game-neutral presentation plugin. Your game
registers retained element definitions in `HudRegistry` and publishes
screen-ready values in `HudValueSet`; the framework does not decide what an id
means or query gameplay state on your behalf.

The plugin is non-generic, has no Cargo feature, and is not included in
`UiPlugin`, `VoxelCorePlugin<V>`, or `VoxelPlugin<V>`:

```rust
use bevy::prelude::*;
use voxel_framework::prelude::*;

App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins(VoxelHudPlugin);
```

It may also run headlessly with `MinimalPlugins`. An empty registry is valid.

## Register definitions

Definitions are ordinary Rust data. The supported retained presentations are
text, continuous bar, fixed or value-bound icon, and bounded repeated segments.
Placement supports nine screen anchors, pixel offsets, stack direction, and
stable signed ordering.

```rust
fn register_hud(mut registry: ResMut<HudRegistry>) {
    registry
        .register(HudElementDefinition::new(
            "sample.text",
            HudPresentation::Text(HudText::new("sample.text.value").prefix("Text: ")),
        ))
        .expect("sample.text should be unique and valid");

    registry
        .register(
            HudElementDefinition::new(
                "sample.bar",
                HudPresentation::Bar(HudBar::new("sample.bar.value")),
            )
            .placement(HudPlacement::new(HudAnchor::TopCenter))
            .fallback(HudScalar::new(0.0, 0.0, 1.0).unwrap()),
        )
        .expect("sample.bar should be unique and valid");

    registry
        .register(HudElementDefinition::new(
            "sample.icon",
            HudPresentation::Icon(HudIcon::bound("sample.icon.value")),
        ))
        .expect("sample.icon should be unique and valid");

    registry
        .register(HudElementDefinition::new(
            "sample.segments",
            HudPresentation::Segments(HudSegments::new("sample.segments.value", 5)),
        ))
        .expect("sample.segments should be unique and valid");
}
```

Use `register` when duplicates are mistakes. Use `insert_or_replace` only when
definition replacement is intentional. Equal replacements are no-ops, while
real changes advance the registry revision and reconcile the existing retained
tree.

## Publish values

`HudValue` is a closed presentation vocabulary: `Text`, validated `Scalar`,
`Count`, `Bool`, and owned icon asset path. It is deliberately not a gameplay
model. Convert domain state into these values in systems owned by your game.

Use `set_hud_value` when you want both real-change detection and a
`HudValueChanged` message:

```rust
use bevy::ecs::message::MessageWriter;

fn publish_values(
    mut values: ResMut<HudValueSet>,
    mut changed: MessageWriter<HudValueChanged>,
) {
    set_hud_value("sample.text.value", "alpha", &mut values, &mut changed);
    set_hud_value(
        "sample.bar.value",
        HudScalar::new(0.5, 0.0, 1.0).unwrap(),
        &mut values,
        &mut changed,
    );
    set_hud_value(
        "sample.icon.value",
        HudValue::icon("icons/sample.png"),
        &mut values,
        &mut changed,
    );
    set_hud_value("sample.segments.value", 3_u32, &mut values, &mut changed);
}
```

The helper does not emit a message for an equal value. `remove_hud_value`
likewise emits only when a value existed. Direct `HudValueSet` operations remain
available when messages are unnecessary.

## Scheduling and retained updates

Register definitions during `Startup`; `VoxelHudPlugin` reconciles and renders
them during `PostStartup`. Put dynamic publication systems in
`HudSystemSet::Publish` when values must reach retained UI state in the same
`Update`:

```rust
app.add_systems(Startup, register_hud).add_systems(
    Update,
    publish_values.in_set(HudSystemSet::Publish),
);
```

The public order is `Publish` → `Reconcile` → deferred-command barrier →
`Render`. Definitions and theme revisions trigger structural reconciliation;
value updates change only affected presentation components. Unchanged frames do
not rebuild element trees, and each element keeps its root entity identity.

## Fallbacks and diagnostics

Missing values, incompatible kinds, invalid scalars, excessive segment counts,
and unavailable image assets never panic. By default the affected element is
hidden. A definition may provide a compatible `.fallback(...)` value instead.
Resolution failures emit `HudDiagnostic` when the active failure state changes,
rather than repeating the same diagnostic every frame.

Image paths are owned author data. The renderer resolves them only when an
`AssetServer` is present. With `MinimalPlugins`, icon paths remain inspectable
values but no asset I/O occurs and the icon resolves to its safe hidden state.

## Inspection

`HudRegistry::snapshot()` and `HudValueSet::snapshot()` return deterministic,
id-ordered snapshots. Use them for tests, diagnostics, and tools. They do not
define a file format, persistence contract, configuration language, or JSON
schema.

## Coexistence and migration

`VoxelHudPlugin` does not replace `UiPlugin` or any custom Bevy UI. They can be
composed side by side:

```rust
app.add_plugins((UiPlugin, VoxelHudPlugin));
```

To migrate an existing overlay incrementally:

1. Keep the overlay and its game-owned queries unchanged.
2. Register one equivalent neutral HUD definition.
3. Move that overlay's presentation-ready output into a game-owned publisher in
   `HudSystemSet::Publish`.
4. Remove the old UI entity only after the retained element is verified.
5. Repeat element by element.

The framework intentionally provides no automatic stats/effects/content bridge,
player discovery, thresholds, rules, interactions, vertical presets, arbitrary
layout language, animation system, or JSON/YAML configuration. Those choices
belong to the consuming game.

## Headless example

The bundled example registers and updates all four presentation kinds across
multiple frames, then prints deterministic values without opening a window or
reading assets:

```sh
cargo run --example hud
cargo run --no-default-features --example hud
```

See [`examples/hud.rs`](../examples/hud.rs) for the complete source.
