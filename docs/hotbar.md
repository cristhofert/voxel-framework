# Inventory-backed hotbar

The `items` feature enables these APIs. This change is prepared for 0.10.4;
publication and its immutable tag are pending explicit maintainer authorization.

## Registration and order

```rust,no_run
use bevy::prelude::*;
use voxel_framework::prelude::*;

App::new()
    .add_plugins(DefaultPlugins)
    .insert_resource(HotbarState::new(14))
    .add_plugins((
        HotbarPlugin,
        HotbarUiPlugin,
        HotbarContentPlugin::<DefaultItem>::new(),
    ))
    .add_systems(Startup, setup);

fn setup(mut commands: Commands, mut icons: ResMut<ItemIconPalette<DefaultItem>>) {
    icons.register_voxel(DefaultItem::RockFragment, DefaultVoxel::Rock);
    icons.register_voxel(DefaultItem::Crystal, DefaultVoxel::SiliconCrystal);

    let mut inventory = ContainerComponent::<DefaultItem>::new(14);
    inventory.0.add(DefaultItem::RockFragment, 1);
    inventory.0.add(DefaultItem::Crystal, 12);
    commands.spawn((PrimaryContainer, inventory));
}
```

Configure HotbarState's slot count and HotbarUiConfig before Startup. Register
colors before the first PostUpdate; a normal Startup system is sufficient.
The palette uses init_resource, so you may also insert a prebuilt palette before
adding the content plugin. Adding the two UI plugins in either order works.
If UiPlugin or VoxelPlugin already supplies HotbarUiPlugin, add only the content
extension. Register one content item type per hotbar. The extension is opt-in;
convenience bundles keep their existing slot/selection-only behavior.

## Explicit material icons

ItemIconPalette<I> is keyed by full item equality, not item or voxel byte ids.
register(item, rgba) installs/replaces a linear RGBA color. register_voxel(item,
voxel) samples VoxelType::color() without any type/id correspondence requirement.
For example, DefaultItem::RockFragment (byte 20) can use DefaultVoxel::Rock
(byte 1). No duplicated material constants or automatic mappings are necessary.

color(item) returns Option<[f32; 4]>; remove(item) clears an association. Runtime
palette changes update the UI. Unregistered occupied items show a generic '?'
rather than silently using ItemType::color(). Registered colors render as inset
square UI nodes, leaving the slot background and selection highlight exposed.
Voxel colors are sampled at registration; register again after a palette change.
No textures, images, external assets, or 3D thumbnails are loaded for icons.

## Inventory and updates

The UI reads the single entity matching PrimaryContainer + ContainerComponent<I>.
Each displayed slot uses the exact same container index; empty slots never cause
later stacks to shift visually. Short inventories leave remaining UI slots empty.
Every occupied slot displays its count, including one. Long decimal counts use
a smaller font to fit their slot; stored quantities are unchanged. The selected item name
appears above the row; empty/out-of-range selection displays "Empty hands".

Inventory and input changes in Update appear together in PostUpdate, before Bevy
UI preparation. Use Update for game-owned inventory writers; changes made later in
PostUpdate may appear on the following frame. The plugin never mutates inventory or selection and does not
keep an inventory mirror. Missing/ambiguous containers clear icons, counts,
fallback symbols, and the selected name. A new unique container is read on the
next frame.

## Layout and input

One row is centered at the bottom with 16 logical pixels of bottom/horizontal
margin. The default gap is 4px. Content-mode slots are at most 50px, shrink to
the primary window width (accounting for UiScale), and remain square. The default
UI camera should target the full primary window; custom viewports/multiple UI
targets need game-owned layout. At widths too small even for all gaps, gaps shrink
to avoid overflowing the reserved margin. Legacy HotbarUiConfig field names
and paths remain valid; standalone HotbarUiPlugin still renders only slots and
selection, and retains custom larger preferred slot sizes. Configure the count
before Startup; rebuilding the number of slot entities at runtime is unsupported.

HotbarPlugin retains keys 1-9 (indices 0-8) and wheel navigation through every
slot, including indices 9-13, with wraparound. Direct HotbarState::select writes
also update the highlight and label; callers need not emit a UI message.
No inventory capacities, stack limits, cursor capture, storage, or crafting rules
change.

Run the native example (no terrain, external images, or textures):

```sh
cargo run --offline --example hotbar_icons -- 800 600 --verify
cargo run --offline --example hotbar_icons -- 1280 720 --verify
cargo run --offline --example hotbar_icons -- 1920 1080 --verify --fullscreen
```

On a 1920x1080 monitor, --fullscreen avoids window-manager decoration/work-area
limits; it uses the primary monitor's actual size. The optional --verify asserts actual computed slot bounds, square sizes, margins,
gaps, centering, and one-row placement. Keys A and C add rock or clear the selected
slot, respectively. Mouse wheel and keys 1-9 select. This is the only inventory
presentation in the example; no drag/drop or extra inventory screen is provided.
