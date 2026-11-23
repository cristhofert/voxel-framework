//! Explicit item icons and the optional inventory-backed hotbar.
use std::{collections::HashMap, marker::PhantomData};

use bevy::prelude::*;

use crate::items::{Container, ContainerComponent, HotbarState, ItemType, PrimaryContainer};
use crate::voxel::VoxelType;

use super::{HotbarSlot, HotbarUiRoot, spawn_hotbar_ui};

/// Explicit item-to-icon colors in linear RGBA. No byte-id or name inference is used.
///
/// An unregistered occupied item renders a generic `?`, even if it implements
/// `ItemType::color`. Use [`Self::register_voxel`] to reuse a material palette
/// without duplicating its color constants. Registration samples the voxel's
/// current color; register again if the game changes its palette.
#[derive(Resource, Debug)]
pub struct ItemIconPalette<I: ItemType> {
    colors: HashMap<I, [f32; 4]>,
}

impl<I: ItemType> Default for ItemIconPalette<I> {
    fn default() -> Self {
        Self {
            colors: HashMap::new(),
        }
    }
}

impl<I: ItemType> ItemIconPalette<I> {
    /// Creates an empty icon palette.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers/replaces a color. Channels follow the `VoxelType::color`
    /// contract: linear RGBA values in [0, 1].
    pub fn register(&mut self, item: I, color: [f32; 4]) {
        self.colors.insert(item, color);
    }

    /// Explicitly associates an item with any voxel material's color.
    /// The item and voxel types and their byte ids can differ.
    pub fn register_voxel<V: VoxelType>(&mut self, item: I, voxel: V) {
        self.register(item, voxel.color());
    }

    /// Returns the explicitly registered color, or None for a generic icon.
    pub fn color(&self, item: I) -> Option<[f32; 4]> {
        self.colors.get(&item).copied()
    }

    /// Removes an association; occupied slots revert to the generic symbol.
    pub fn remove(&mut self, item: I) -> Option<[f32; 4]> {
        self.colors.remove(&item)
    }
}

/// Adds inventory icons, counts, and the selected name to `HotbarUiPlugin`.
///
/// Register exactly one item type per hotbar and explicitly add both plugins.
/// This plugin does not install input, inventory storage, or the legacy UI.
/// Configure `HotbarState::slots` and `HotbarUiConfig` before Startup. Register
/// colors before the first PostUpdate (for example, in a Startup system).
///
/// Reads the unique `PrimaryContainer` with `ContainerComponent<I>` each frame;
/// absent/ambiguous containers clear all content. A slot indexes the container
/// directly. The selected name is "Empty hands" for empty/out-of-range slots.
/// Presentation runs in PostUpdate before Bevy UI preparation, after Update
/// inventory/input writers, without changing inventory or selection.
///
/// ```no_run
/// use bevy::prelude::*;
/// use voxel_framework::prelude::*;
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .insert_resource(HotbarState::new(14))
///     .add_plugins((HotbarPlugin, HotbarUiPlugin, HotbarContentPlugin::<DefaultItem>::new()))
///     .add_systems(Startup, |mut icons: ResMut<ItemIconPalette<DefaultItem>>| {
///         icons.register_voxel(DefaultItem::RockFragment, DefaultVoxel::Rock);
///     });
/// ```
pub struct HotbarContentPlugin<I: ItemType>(PhantomData<I>);

impl<I: ItemType> HotbarContentPlugin<I> {
    /// Creates the optional hotbar content extension.
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<I: ItemType> Default for HotbarContentPlugin<I> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Resource)]
pub(super) struct ContentEnabled;

impl<I: ItemType> Plugin for HotbarContentPlugin<I> {
    fn build(&self, app: &mut App) {
        app.init_resource::<ItemIconPalette<I>>()
            .insert_resource(ContentEnabled)
            .add_systems(Startup, spawn_content.after(spawn_hotbar_ui))
            .add_systems(
                PostUpdate,
                (publish_content::<I>, fit_content_text)
                    .chain()
                    .after(super::fit_hotbar)
                    .before(bevy::ui::UiSystems::Prepare),
            );
    }
}

#[derive(Component)]
struct SlotIcon(usize);
#[derive(Component)]
struct SlotSymbol(usize);
#[derive(Component)]
struct SlotCount(usize);
#[derive(Component)]
struct SelectedName;

fn spawn_content(
    mut commands: Commands,
    roots: Query<Entity, With<HotbarUiRoot>>,
    slots: Query<(Entity, &HotbarSlot)>,
) {
    for root in &roots {
        let name = commands
            .spawn((
                SelectedName,
                Text::new("Empty hands"),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    margin: UiRect::bottom(px(6)),
                    ..default()
                },
            ))
            .id();
        commands.entity(root).insert_children(0, &[name]);
    }
    for (entity, slot) in &slots {
        commands.entity(entity).with_children(|parent| {
            parent.spawn((
                SlotIcon(slot.0),
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(22),
                    top: percent(22),
                    width: percent(56),
                    height: percent(56),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ));
            parent.spawn((
                SlotSymbol(slot.0),
                Text::new(""),
                TextFont {
                    font_size: 22.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            parent.spawn((
                SlotCount(slot.0),
                Text::new(""),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    position_type: PositionType::Absolute,
                    right: px(2),
                    bottom: px(0),
                    ..default()
                },
            ));
        });
    }
}

fn publish_content<I: ItemType>(
    containers: Query<&ContainerComponent<I>, With<PrimaryContainer>>,
    hotbar: Res<HotbarState>,
    palette: Res<ItemIconPalette<I>>,
    mut icons: Query<(&SlotIcon, &mut Node, &mut BackgroundColor)>,
    mut texts: Query<(
        &mut Text,
        Option<&SlotSymbol>,
        Option<&SlotCount>,
        Option<&SelectedName>,
    )>,
) {
    let container = containers.single().ok();
    let slot_at = |index| {
        container
            .and_then(|c| c.slot_at(index))
            .filter(|slot| slot.count > 0 && !slot.item.is_empty())
    };
    for (icon, mut node, mut background) in &mut icons {
        let color = slot_at(icon.0).and_then(|slot| palette.color(slot.item));
        let display = if color.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
        let color = color
            .map(|[r, g, b, a]| Color::linear_rgba(r, g, b, a))
            .unwrap_or(Color::NONE);
        if background.0 != color {
            background.0 = color;
        }
    }
    for (mut text, symbol, count, name) in &mut texts {
        let value = if let Some(symbol) = symbol {
            if slot_at(symbol.0).is_some_and(|slot| palette.color(slot.item).is_none()) {
                "?"
            } else {
                ""
            }
            .to_string()
        } else if let Some(count) = count {
            slot_at(count.0)
                .map(|slot| slot.count.to_string())
                .unwrap_or_default()
        } else if name.is_some() {
            if hotbar.selected < hotbar.slots {
                slot_at(hotbar.selected)
                    .map(|slot| slot.item.name())
                    .unwrap_or("Empty hands")
            } else {
                "Empty hands"
            }
            .to_string()
        } else {
            continue;
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}

// Keep every decimal digit visible without changing the stored stack quantity.
// Counts use Bevy's default font; one em per ASCII digit is a conservative bound.
fn fit_content_text(
    slots: Query<&Node, With<HotbarSlot>>,
    mut texts: Query<
        (&ChildOf, &Text, &mut TextFont, Option<&SlotCount>),
        Or<(With<SlotCount>, With<SlotSymbol>)>,
    >,
) {
    for (parent, text, mut font, count) in &mut texts {
        let Ok(node) = slots.get(parent.parent()) else {
            continue;
        };
        let Val::Px(width) = node.width else {
            continue;
        };
        let available = (width - 8.0).max(0.0);
        let size = if count.is_some() {
            14.0_f32.min(available / text.0.len().max(1) as f32)
        } else {
            22.0_f32.min(available)
        };
        if font.font_size != size {
            font.font_size = size;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::{DefaultItem, HotbarPlugin, InventorySlot};
    use crate::ui::{HotbarUiConfig, HotbarUiPlugin};
    use crate::voxel::DefaultVoxel;
    use bevy::input::mouse::{MouseScrollUnit, MouseWheel};

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .add_message::<MouseWheel>()
            .insert_resource(HotbarState::new(14))
            .add_plugins((
                HotbarPlugin,
                HotbarUiPlugin,
                HotbarContentPlugin::<DefaultItem>::new(),
            ));
        app.world_mut()
            .resource_mut::<ItemIconPalette<DefaultItem>>()
            .register_voxel(DefaultItem::RockFragment, DefaultVoxel::Rock);
        app
    }

    fn container(app: &mut App) -> Entity {
        let mut container = ContainerComponent::new(14);
        container.0.slots[0] = Some(InventorySlot {
            item: DefaultItem::RockFragment,
            count: 1,
        });
        container.0.slots[2] = Some(InventorySlot {
            item: DefaultItem::Crystal,
            count: 7,
        });
        app.world_mut().spawn((PrimaryContainer, container)).id()
    }

    fn count(app: &mut App, index: usize) -> String {
        let world = app.world_mut();
        world
            .query::<(&SlotCount, &Text)>()
            .iter(world)
            .find(|(s, _)| s.0 == index)
            .unwrap()
            .1
            .0
            .clone()
    }

    fn symbol(app: &mut App, index: usize) -> String {
        let world = app.world_mut();
        world
            .query::<(&SlotSymbol, &Text)>()
            .iter(world)
            .find(|(s, _)| s.0 == index)
            .unwrap()
            .1
            .0
            .clone()
    }

    fn name(app: &mut App) -> String {
        let world = app.world_mut();
        world
            .query_filtered::<&Text, With<SelectedName>>()
            .single(world)
            .unwrap()
            .0
            .clone()
    }

    fn icon(app: &mut App, index: usize) -> (Display, Color) {
        let world = app.world_mut();
        let (_, n, c) = world
            .query::<(&SlotIcon, &Node, &BackgroundColor)>()
            .iter(world)
            .find(|(s, _, _)| s.0 == index)
            .unwrap();
        (n.display, c.0)
    }

    #[test]
    fn explicit_voxel_mapping_does_not_use_item_byte_or_default_item_color() {
        let mut palette = ItemIconPalette::new();
        assert_ne!(
            DefaultItem::RockFragment.to_byte(),
            DefaultVoxel::IronOre.to_byte()
        );
        palette.register_voxel(DefaultItem::RockFragment, DefaultVoxel::IronOre);
        assert_eq!(
            palette.color(DefaultItem::RockFragment),
            Some(DefaultVoxel::IronOre.color())
        );
        assert_eq!(palette.color(DefaultItem::Crystal), None);
        let replacement = DefaultVoxel::SiliconCrystal.color();
        palette.register(DefaultItem::RockFragment, replacement);
        assert_eq!(palette.color(DefaultItem::RockFragment), Some(replacement));
        assert_eq!(palette.remove(DefaultItem::RockFragment), Some(replacement));
        assert_eq!(palette.color(DefaultItem::RockFragment), None);
    }

    #[test]
    fn content_tracks_exact_indices_quantities_replacement_emptying_and_palette_edits() {
        let mut app = app();
        let entity = container(&mut app);
        app.update();
        let [r, g, b, a] = DefaultVoxel::Rock.color();
        assert_eq!(
            icon(&mut app, 0),
            (Display::Flex, Color::linear_rgba(r, g, b, a))
        );
        assert_eq!(symbol(&mut app, 0), "");
        assert_eq!(count(&mut app, 0), "1");
        assert_eq!(count(&mut app, 1), "");
        assert_eq!(count(&mut app, 2), "7");
        assert_eq!(symbol(&mut app, 2), "?");
        assert_eq!(icon(&mut app, 2).0, Display::None);
        assert_eq!(name(&mut app), "Rock Fragment");
        app.world_mut()
            .get_mut::<ContainerComponent<DefaultItem>>(entity)
            .unwrap()
            .0
            .add(DefaultItem::RockFragment, 4);
        app.update();
        assert_eq!(count(&mut app, 0), "5");
        app.world_mut()
            .get_mut::<ContainerComponent<DefaultItem>>(entity)
            .unwrap()
            .0
            .slots[0] = Some(InventorySlot {
            item: DefaultItem::Crystal,
            count: 1,
        });
        app.update();
        assert_eq!(count(&mut app, 0), "1");
        assert_eq!(name(&mut app), "Crystal");
        assert_eq!(symbol(&mut app, 0), "?");
        app.world_mut()
            .resource_mut::<ItemIconPalette<DefaultItem>>()
            .register_voxel(DefaultItem::Crystal, DefaultVoxel::SiliconCrystal);
        app.update();
        assert_eq!(symbol(&mut app, 0), "");
        assert_eq!(icon(&mut app, 0).0, Display::Flex);
        app.world_mut()
            .get_mut::<ContainerComponent<DefaultItem>>(entity)
            .unwrap()
            .0
            .slots[0] = None;
        app.update();
        assert_eq!(count(&mut app, 0), "");
        assert_eq!(symbol(&mut app, 0), "");
        assert_eq!(icon(&mut app, 0), (Display::None, Color::NONE));
        assert_eq!(name(&mut app), "Empty hands");
    }

    #[test]
    fn selection_and_wheel_update_name_and_highlight_without_inventory_mutation() {
        let mut app = app();
        let entity = container(&mut app);
        let before = app
            .world()
            .get::<ContainerComponent<DefaultItem>>(entity)
            .unwrap()
            .0
            .slots
            .clone();
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Digit3);
        app.update();
        assert_eq!(name(&mut app), "Crystal");
        let config = app.world().resource::<HotbarUiConfig>().clone();
        let [r, g, b, a] = config.selected_color;
        let world = app.world_mut();
        for (slot, bg) in world.query::<(&HotbarSlot, &BackgroundColor)>().iter(world) {
            assert_eq!(bg.0 == Color::srgba(r, g, b, a), slot.0 == 2);
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut().resource_mut::<HotbarState>().select(0);
        // Actual wheel input crosses every slot and wraps back to zero.
        for expected in (1..14).chain(std::iter::once(0)) {
            app.world_mut().write_message(MouseWheel {
                unit: MouseScrollUnit::Line,
                x: 0.0,
                y: -1.0,
                window: Entity::PLACEHOLDER,
            });
            app.update();
            assert_eq!(app.world().resource::<HotbarState>().selected, expected);
            assert_eq!(
                name(&mut app),
                match expected {
                    0 => "Rock Fragment",
                    2 => "Crystal",
                    _ => "Empty hands",
                }
            );
        }
        app.world_mut().write_message(MouseWheel {
            unit: MouseScrollUnit::Pixel,
            x: 0.0,
            y: 53.0,
            window: Entity::PLACEHOLDER,
        });
        app.update();
        assert_eq!(app.world().resource::<HotbarState>().selected, 13);
        for (index, key) in [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
            KeyCode::Digit7,
            KeyCode::Digit8,
            KeyCode::Digit9,
        ]
        .into_iter()
        .enumerate()
        {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
            app.update();
            assert_eq!(app.world().resource::<HotbarState>().selected, index);
        }
        assert_eq!(
            app.world()
                .get::<ContainerComponent<DefaultItem>>(entity)
                .unwrap()
                .0
                .slots,
            before
        );
    }

    #[test]
    fn missing_replaced_and_ambiguous_containers_clear_all_stale_content() {
        let mut app = app();
        app.update();
        assert_eq!(name(&mut app), "Empty hands");
        let entity = container(&mut app);
        app.update();
        assert_eq!(count(&mut app, 0), "1");
        let other = container(&mut app);
        app.update();
        for i in 0..14 {
            assert_eq!(count(&mut app, i), "");
            assert_eq!(symbol(&mut app, i), "");
            assert_eq!(icon(&mut app, i).0, Display::None);
        }
        assert_eq!(name(&mut app), "Empty hands");
        app.world_mut().despawn(entity);
        app.update();
        assert_eq!(name(&mut app), "Rock Fragment");
        app.world_mut().despawn(other);
        app.update();
        assert_eq!(name(&mut app), "Empty hands");
        assert_eq!(count(&mut app, 0), "");
        // Short replacement inventory leaves all out-of-range slots empty.
        app.world_mut()
            .spawn((PrimaryContainer, ContainerComponent::<DefaultItem>::new(1)));
        app.world_mut().resource_mut::<HotbarState>().select(13);
        app.update();
        assert_eq!(count(&mut app, 13), "");
        assert_eq!(name(&mut app), "Empty hands");
    }

    #[test]
    fn disabled_hotbar_does_not_spawn_content() {
        let mut app = app();
        app.insert_resource(HotbarUiConfig {
            visible: false,
            ..default()
        });
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<&SelectedName>().iter(world).count(), 0);
        assert_eq!(world.query::<&SlotIcon>().iter(world).count(), 0);
    }
    #[test]
    fn preconfigured_palette_and_reverse_ui_plugin_order_are_supported() {
        let mut app = App::new();
        let mut palette = ItemIconPalette::new();
        palette.register_voxel(DefaultItem::RockFragment, DefaultVoxel::IronOre);
        app.insert_resource(palette)
            .insert_resource(HotbarState::new(14))
            .add_plugins((HotbarContentPlugin::<DefaultItem>::new(), HotbarUiPlugin));
        container(&mut app);
        app.update();
        let [r, g, b, a] = DefaultVoxel::IronOre.color();
        assert_eq!(
            icon(&mut app, 0),
            (Display::Flex, Color::linear_rgba(r, g, b, a))
        );
    }
    #[test]
    fn full_u32_quantity_is_preserved_and_text_fits_the_slot() {
        let mut app = app();
        let entity = container(&mut app);
        app.world_mut()
            .get_mut::<ContainerComponent<DefaultItem>>(entity)
            .unwrap()
            .0
            .slots[0]
            .as_mut()
            .unwrap()
            .count = u32::MAX;
        app.update();
        assert_eq!(count(&mut app, 0), u32::MAX.to_string());
        let world = app.world_mut();
        let (_, text, font) = world
            .query::<(&SlotCount, &Text, &TextFont)>()
            .iter(world)
            .find(|(slot, _, _)| slot.0 == 0)
            .unwrap();
        assert!(font.font_size * text.0.len() as f32 <= 42.0);
        assert_eq!(
            world
                .get::<ContainerComponent<DefaultItem>>(entity)
                .unwrap()
                .0
                .slots[0]
                .as_ref()
                .unwrap()
                .count,
            u32::MAX
        );
    }
}
