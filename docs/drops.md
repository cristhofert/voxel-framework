# Collectible drops

Enable the `items` feature (included by default). Gameplay and presentation are
separate explicit plugins:

```rust,no_run
use bevy::prelude::*;
use voxel_framework::prelude::*;

App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins((
        MiningPlugin,
        DropsPlugin::<DefaultItem>::new(),
        DropVisualPlugin::<DefaultItem>::new(),
    ))
    .add_systems(Startup, setup);

fn setup(mut commands: Commands, mut table: ResMut<DropTable<DefaultItem>>) {
    table.register(1, DefaultItem::RockFragment, 1, 3);
    // In a game, attach these components to the actual moving player instead.
    commands.spawn((
        Transform::default(),
        ItemCollector,
        PrimaryContainer,
        ContainerComponent::<DefaultItem>::new(9),
    ));
}
```

Mining emits `VoxelBroken`; register the message through `MiningPlugin` or a
bundle that contains it. Populate the drop table before mining begins. Your game
still owns the camera, mining input, player movement, and UI. Drop and collector
transforms must represent world positions (use unparented entities). The current
pickup model expects exactly one collector and one primary container; they need
not be the same entity. Ambiguous collectors disable spatial pickup, not expiry.

`DropsPlugin<I>` spawns at the broken voxel center, attracts nearby drops,
advances velocity while a unique collector exists, and inserts accepted items
into the primary container. Full inventories leave the drop intact. Partial
pickups subtract only accepted items; `ItemPickedUp<I>` reports only that amount.
Remaining lifetime continues to decrease, including when no collector exists or
a drop has no transform. Expired drops disappear without pickup messages.

`DropVisualPlugin<I>` draws a 0.25-unit cube per positioned drop, using
`ItemType::color()` and an unlit material, so no lights or texture assets are
required. It runs in PostUpdate before transform/visibility propagation and
bounds calculation. It handles existing drops and late-added transforms as well
as newly spawned drops. Mesh/material components live on the drop entity:
partial pickup preserves the visual; full pickup and expiry remove it.

Assets are shared: one mesh and one material per encountered item variant, kept
for reuse until the app ends. Colors are sampled on first use. This default does
not track changes to an already rendered drop's item identity. For custom art,
omit the plugin or attach your own Mesh3d/material before PostUpdate; existing
Mesh3d or MeshMaterial3d<StandardMaterial> components are never overwritten.
DefaultPlugins supplies the required asset stores; headless ECS tests may supply
Assets<Mesh> and Assets<StandardMaterial> explicitly.

The visual plugin never integrates motion, changes quantities, or installs
DropsPlugin. It does not implement gravity, terrain collision, or bouncing;
the initial upward velocity is not a complete ballistic simulation. Game-owned
physics may update velocity, but must not integrate the same movement twice.

Run `cargo run --example drops --no-default-features --features items` with a
graphical display. Colored drops are initially stationary with no collector.
Press Space to enable a collector at the center; nearby drops move toward it
and disappear on pickup. Press R to spawn another batch. Leave collection off
for 10 seconds to observe expiration. The example logs accepted quantities.
