//! Optional hotbar presentation and inventory content.

use crate::items::HotbarState;
use bevy::prelude::*;

mod content;
pub use content::{HotbarContentPlugin, ItemIconPalette};

// ─── Hotbar UI ────────────────────────────────────────────────────────────────
//
// Gated on the `items` feature: presentation reads the shared HotbarState.

/// Visual configuration for the [`HotbarUiPlugin`] slot row.
///
/// Insert this resource before adding [`HotbarUiPlugin`] to customize the
/// hotbar. Visibility and slot count are consumed on `Startup`; rebuilding the
/// slot count at runtime is unsupported. Colors and sizing are read each frame.
///
/// # Default values
///
/// | Field            | Default                             |
/// |------------------|-------------------------------------|
/// | `visible`        | `true`                              |
/// | `slot_size`      | `50.0` px                           |
/// | `slot_gap`       | `4.0` px                            |
/// | `slot_color`     | `[0.15, 0.15, 0.15, 0.85]` (dark)  |
/// | `selected_color` | `[0.45, 0.45, 0.45, 0.95]` (light) |
#[derive(Resource, Debug, Clone)]
pub struct HotbarUiConfig {
    /// When `false`, the hotbar is not spawned. Use this to disable the hotbar
    /// entirely without removing [`HotbarUiPlugin`] from the app.
    pub visible: bool,
    /// Preferred square slot size in logical pixels; shrinks to fit the row.
    /// Content mode caps this at 50px; legacy mode preserves larger sizes.
    pub slot_size: f32,
    /// Horizontal gap between adjacent slot nodes in logical pixels.
    pub slot_gap: f32,
    /// RGBA background color applied to unselected hotbar slots in sRGB.
    pub slot_color: [f32; 4],
    /// RGBA background color applied to the currently selected slot in sRGB. Updated from [`HotbarState`] every frame.
    pub selected_color: [f32; 4],
}

impl Default for HotbarUiConfig {
    fn default() -> Self {
        Self {
            visible: true,
            slot_size: 50.0,
            slot_gap: 4.0,
            slot_color: [0.15, 0.15, 0.15, 0.85],
            selected_color: [0.45, 0.45, 0.45, 0.95],
        }
    }
}

/// Marker for the hotbar root container.
#[derive(Component)]
struct HotbarUiRoot;

#[derive(Component)]
struct HotbarUiRow;

/// Marker for individual hotbar slot nodes. Holds the slot index.
#[derive(Component)]
struct HotbarSlot(usize);

fn spawn_hotbar_ui(
    mut commands: Commands,
    config: Res<HotbarUiConfig>,
    hotbar: Res<HotbarState>,
    content: Option<Res<content::ContentEnabled>>,
) {
    if !config.visible {
        return;
    }
    let size = if content.is_some() {
        config.slot_size.clamp(0.0, 50.0)
    } else {
        config.slot_size.max(0.0)
    };
    commands
        .spawn((
            HotbarUiRoot,
            Node {
                width: percent(100),
                position_type: PositionType::Absolute,
                bottom: px(16),
                padding: UiRect::horizontal(px(16)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                HotbarUiRow,
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    column_gap: px(config.slot_gap.max(0.0)),
                    ..default()
                },
            ))
            .with_children(|row| {
                for i in 0..hotbar.slots {
                    row.spawn((
                        HotbarSlot(i),
                        Node {
                            width: px(size),
                            min_width: px(0),
                            aspect_ratio: Some(1.0),
                            flex_shrink: 1.0,
                            border: UiRect::all(px(2)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(slot_color(&config, i == hotbar.selected)),
                        BorderColor::all(Color::srgba(0.6, 0.6, 0.6, 0.6)),
                    ));
                }
            });
        });
}

// Flex shrinking alone does not recompute the aspect-ratio-derived height.
fn fit_hotbar(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    scale: Option<Res<bevy::ui::UiScale>>,
    config: Res<HotbarUiConfig>,
    content: Option<Res<content::ContentEnabled>>,
    mut slots: Query<&mut Node, With<HotbarSlot>>,
    mut rows: Query<&mut Node, (With<HotbarUiRow>, Without<HotbarSlot>)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let scale = scale.map_or(1.0, |scale| scale.0).max(f32::EPSILON);
    let available = (window.width() / scale - 32.0).max(0.0);
    let count = slots.iter().count();
    let gaps = count.saturating_sub(1) as f32;
    let gap = config.slot_gap.max(0.0).min(available / gaps.max(1.0));
    let preferred = if content.is_some() {
        config.slot_size.clamp(0.0, 50.0)
    } else {
        config.slot_size.max(0.0)
    };
    let size = ((available - gap * gaps) / count.max(1) as f32).clamp(0.0, preferred);
    for mut node in &mut slots {
        if node.width != px(size) || node.height != px(size) {
            node.width = px(size);
            node.height = px(size);
            node.flex_shrink = 0.0;
            node.border = UiRect::all(px(2.0_f32.min(size / 2.0)));
        }
    }
    for mut row in &mut rows {
        if row.column_gap != px(gap) {
            row.column_gap = px(gap);
        }
    }
}

fn slot_color(config: &HotbarUiConfig, selected: bool) -> Color {
    let [r, g, b, a] = if selected {
        config.selected_color
    } else {
        config.slot_color
    };
    Color::srgba(r, g, b, a)
}

fn update_hotbar_selection(
    hotbar: Res<HotbarState>,
    config: Res<HotbarUiConfig>,
    mut slots: Query<(&HotbarSlot, &mut BackgroundColor)>,
) {
    for (slot, mut bg) in &mut slots {
        let color = slot_color(&config, slot.0 == hotbar.selected);
        if bg.0 != color {
            bg.0 = color;
        }
    }
}

/// Plugin that renders a horizontal hotbar at the bottom center of the screen.
///
/// Reads [`HotbarState`] on startup to determine the number of slots, spawns
/// one [`Node`] quad per slot, and reads selection in PostUpdate before UI layout.
/// It works without an inventory. Add [`HotbarContentPlugin<I>`] for item content.
///
/// Customize via [`HotbarUiConfig`] inserted before this plugin.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::ui::{HotbarUiPlugin, HotbarUiConfig};
///
/// App::new()
///     .insert_resource(HotbarUiConfig {
///         slot_size: 64.0,
///         slot_gap: 6.0,
///         ..Default::default()
///     })
///     .add_plugins(HotbarUiPlugin);
/// ```
pub struct HotbarUiPlugin;

impl Plugin for HotbarUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HotbarUiConfig>();
        app.add_systems(Startup, spawn_hotbar_ui);
        app.add_systems(
            PostUpdate,
            (fit_hotbar, update_hotbar_selection).before(bevy::ui::UiSystems::Prepare),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fourteen_slots_reserve_horizontal_screen_margin() {
        let mut app = App::new();
        app.insert_resource(HotbarState::new(14))
            .add_plugins(HotbarUiPlugin);
        app.update();
        let world = app.world_mut();
        let root = world
            .query_filtered::<&Node, With<HotbarUiRoot>>()
            .single(world)
            .unwrap();
        assert_eq!(root.padding.left, Val::Px(16.0));
        assert_eq!(root.padding.right, Val::Px(16.0));
        assert_eq!(world.query::<&HotbarSlot>().iter(world).count(), 14);
    }
    #[test]
    fn legacy_ui_needs_no_inventory_or_content_plugin() {
        let mut app = App::new();
        app.insert_resource(HotbarState::new(9))
            .add_plugins(HotbarUiPlugin);
        app.update();
        app.world_mut().resource_mut::<HotbarState>().select(8);
        app.update();
        let config = app.world().resource::<HotbarUiConfig>().clone();
        let world = app.world_mut();
        assert_eq!(world.query::<&HotbarSlot>().iter(world).count(), 9);
        for (slot, bg) in world.query::<(&HotbarSlot, &BackgroundColor)>().iter(world) {
            assert_eq!(bg.0, slot_color(&config, slot.0 == 8));
        }
        assert_eq!(world.query::<&Text>().iter(world).count(), 0);
    }
    #[test]
    fn fourteen_slots_resize_both_dimensions_and_respect_ui_scale() {
        let mut app = App::new();
        app.insert_resource(HotbarState::new(14))
            .insert_resource(bevy::ui::UiScale(1.0))
            .add_plugins(HotbarUiPlugin);
        let window = app
            .world_mut()
            .spawn((
                Window {
                    resolution: (600, 600).into(),
                    ..default()
                },
                bevy::window::PrimaryWindow,
            ))
            .id();
        for (width, scale) in [(600, 1.0), (800, 1.0), (1280, 1.0), (1920, 1.0), (800, 2.0)] {
            app.world_mut()
                .get_mut::<Window>(window)
                .unwrap()
                .resolution
                .set(width as f32, 600.0);
            app.world_mut().resource_mut::<bevy::ui::UiScale>().0 = scale;
            app.update();
            let expected = ((width as f32 / scale - 32.0 - 13.0 * 4.0) / 14.0).min(50.0);
            let world = app.world_mut();
            for node in world
                .query_filtered::<&Node, With<HotbarSlot>>()
                .iter(world)
            {
                assert_eq!(node.width, px(expected));
                assert_eq!(node.height, px(expected));
            }
        }
    }
}
