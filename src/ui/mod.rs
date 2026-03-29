//! HUD overlay plugins for voxel game user interfaces.
//!
//! This module provides four composable HUD plugins that cover the most common
//! in-game overlay needs. Each plugin is self-contained and can be added
//! independently; [`UiPlugin`] is an umbrella that adds all four at once using
//! [`DefaultItem`] as the item type.
//!
//! # Available plugins
//!
//! | Plugin | What it provides |
//! |--------|-----------------|
//! | [`CrosshairPlugin`] | Center-screen crosshair rendered as four `Node` quads |
//! | [`LoadingScreenPlugin`] | Full-screen overlay with progress bar while the world loads |
//! | [`HotbarUiPlugin`] | Bottom-center row of inventory slots |
//! | [`PickupNotificationPlugin`] | Timed top-right pop-up text on item pickup |
//!
//! # Choosing between [`UiPlugin`] and individual plugins
//!
//! Use [`UiPlugin`] when your game uses [`DefaultItem`] and wants all four HUD
//! elements with zero configuration:
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use voxel_framework::ui::UiPlugin;
//!
//! App::new().add_plugins(UiPlugin);
//! ```
//!
//! Use individual plugins when you need a custom item type for pickup
//! notifications, want to omit specific HUD elements, or need to insert
//! configuration resources before the plugin initializes them:
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use voxel_framework::ui::{CrosshairPlugin, CrosshairConfig, HotbarUiPlugin};
//!
//! App::new()
//!     .insert_resource(CrosshairConfig {
//!         color: [1.0, 0.3, 0.3, 1.0],
//!         size: 14.0,
//!         ..Default::default()
//!     })
//!     .add_plugins(CrosshairPlugin)
//!     .add_plugins(HotbarUiPlugin);
//! ```
//!
//! # Pause menu
//!
//! The optional [`PauseMenuPlugin`] lives in the
//! `pause_menu` submodule and is re-exported from this module. It is **not**
//! included in [`UiPlugin`] because it manages its own [`PauseMenuState`] and
//! cursor lifetime, which must be coordinated with your game's state machine.

use bevy::prelude::*;

use crate::items::hotbar::{HotbarSelectionChanged, HotbarState};
use crate::items::inventory::ItemPickedUp;
use crate::items::inventory::ItemType;
use crate::world::loading::{LoadingProgress, WorldLoadingState};

// ─── Crosshair ────────────────────────────────────────────────────────────────

/// Visual configuration for the [`CrosshairPlugin`] overlay.
///
/// Insert this resource before adding [`CrosshairPlugin`] to override any
/// default. The crosshair is spawned once on `Startup`; changes made after
/// that point have no effect unless you despawn and re-spawn the crosshair
/// nodes.
///
/// # Default values
///
/// | Field       | Default              | Notes |
/// |-------------|----------------------|-------|
/// | `color`     | `[1,1,1,1]` (white)  | RGBA, linear |
/// | `size`      | `20.0` px            | Half-length of each arm |
/// | `gap`       | `4.0` px             | Empty space between center and arm start |
/// | `thickness` | `2.0` px             | Width of each arm quad |
/// | `visible`   | `true`               | `false` skips spawning entirely |
#[derive(Resource, Debug, Clone)]
pub struct CrosshairConfig {
    /// RGBA color of the four crosshair arm quads in linear sRGB.
    /// Alpha controls overall transparency of the overlay.
    pub color: [f32; 4],
    /// Half-length of each arm in logical (UI) pixels. The total visual width
    /// of the crosshair is approximately `2 × (gap + size)`.
    pub size: f32,
    /// Empty gap in logical pixels between the screen center and the start of
    /// each arm. Increase this to leave a clear center dot.
    pub gap: f32,
    /// Width of each arm quad in logical pixels. Thicker values (`4.0`–`6.0`)
    /// are easier to see on busy backgrounds.
    pub thickness: f32,
    /// When `false`, [`CrosshairPlugin`] returns early from `spawn_crosshair`
    /// without creating any entities. Useful for games that manage their own
    /// reticle or disable the crosshair entirely in certain modes.
    pub visible: bool,
}

impl Default for CrosshairConfig {
    fn default() -> Self {
        Self {
            color: [1.0, 1.0, 1.0, 1.0],
            size: 20.0,
            gap: 4.0,
            thickness: 2.0,
            visible: true,
        }
    }
}

/// Marker component for crosshair quad entities.
#[derive(Component)]
struct CrosshairNode;

fn spawn_crosshair(mut commands: Commands, config: Res<CrosshairConfig>) {
    if !config.visible {
        return;
    }

    let c = &*config;
    let color = Color::srgba(c.color[0], c.color[1], c.color[2], c.color[3]);

    // Root container — full screen, no interaction, just a layout anchor.
    commands
        .spawn((
            CrosshairNode,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|root| {
            // Left arm
            root.spawn((
                CrosshairNode,
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(c.size),
                    height: Val::Px(c.thickness),
                    right: Val::Px(c.gap + c.size / 2.0 + c.size / 2.0),
                    top: Val::Px(-c.thickness / 2.0),
                    ..default()
                },
                BackgroundColor(color),
            ));
            // Right arm
            root.spawn((
                CrosshairNode,
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(c.size),
                    height: Val::Px(c.thickness),
                    left: Val::Px(c.gap + c.size / 2.0 - c.size / 2.0),
                    top: Val::Px(-c.thickness / 2.0),
                    ..default()
                },
                BackgroundColor(color),
            ));
            // Top arm
            root.spawn((
                CrosshairNode,
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(c.thickness),
                    height: Val::Px(c.size),
                    left: Val::Px(-c.thickness / 2.0),
                    bottom: Val::Px(c.gap + c.size / 2.0 - c.size / 2.0),
                    ..default()
                },
                BackgroundColor(color),
            ));
            // Bottom arm
            root.spawn((
                CrosshairNode,
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(c.thickness),
                    height: Val::Px(c.size),
                    left: Val::Px(-c.thickness / 2.0),
                    top: Val::Px(c.gap + c.size / 2.0 - c.size / 2.0),
                    ..default()
                },
                BackgroundColor(color),
            ));
        });
}

/// Plugin that renders a fixed center-screen crosshair using four UI quads.
///
/// The crosshair is built from four [`Node`] rectangles (left, right, top,
/// bottom arms) positioned with `PositionType::Absolute` relative to a full-
/// screen container. No images or asset loading are required.
///
/// Customize appearance by inserting [`CrosshairConfig`] before adding this
/// plugin. If [`CrosshairConfig::visible`] is `false`, no entities are spawned.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::ui::{CrosshairPlugin, CrosshairConfig};
///
/// App::new()
///     .insert_resource(CrosshairConfig {
///         color: [0.0, 1.0, 0.0, 0.9],
///         gap: 6.0,
///         ..Default::default()
///     })
///     .add_plugins(CrosshairPlugin);
/// ```
pub struct CrosshairPlugin;

impl Plugin for CrosshairPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CrosshairConfig>();
        app.add_systems(Startup, spawn_crosshair);
    }
}

// ─── Loading Screen ───────────────────────────────────────────────────────────

/// Visual configuration for the [`LoadingScreenPlugin`] overlay.
///
/// Insert this resource before adding [`LoadingScreenPlugin`] to override
/// defaults. Values are read once on `Startup` when the overlay is spawned.
///
/// # Default values
///
/// | Field              | Default                                   |
/// |--------------------|-------------------------------------------|
/// | `background_color` | `[0.05, 0.05, 0.05, 0.95]` (near-black)  |
/// | `text_color`       | `[1, 1, 1, 1]` (white)                    |
/// | `title`            | `"Loading..."`                             |
/// | `show_progress_bar`| `true`                                    |
#[derive(Resource, Debug, Clone)]
pub struct LoadingScreenConfig {
    /// RGBA background color of the full-screen overlay in linear sRGB. The
    /// default near-black with high alpha (`0.95`) fully obscures the 3D scene
    /// while the world loads.
    pub background_color: [f32; 4],
    /// RGBA color of the loading title text in linear sRGB.
    pub text_color: [f32; 4],
    /// Text displayed in the center of the overlay. Updated dynamically with
    /// `"Loading… X/Y chunks"` as [`LoadingProgress`] messages arrive.
    pub title: String,
    /// When `true`, a 400×20 px progress bar is rendered below the title text.
    /// The bar fill advances from 0 % to 100 % as [`LoadingProgress::progress`]
    /// increases from `0.0` to `1.0`.
    pub show_progress_bar: bool,
}

impl Default for LoadingScreenConfig {
    fn default() -> Self {
        Self {
            background_color: [0.05, 0.05, 0.05, 0.95],
            text_color: [1.0, 1.0, 1.0, 1.0],
            title: "Loading...".to_string(),
            show_progress_bar: true,
        }
    }
}

/// Marker for the loading overlay root.
#[derive(Component)]
struct LoadingOverlay;

/// Marker for the loading text entity.
#[derive(Component)]
struct LoadingText;

/// Marker for the progress bar fill entity.
#[derive(Component)]
struct LoadingProgressBar;

fn spawn_loading_overlay(mut commands: Commands, config: Res<LoadingScreenConfig>) {
    let bg = Color::srgba(
        config.background_color[0],
        config.background_color[1],
        config.background_color[2],
        config.background_color[3],
    );
    let tc = Color::srgba(
        config.text_color[0],
        config.text_color[1],
        config.text_color[2],
        config.text_color[3],
    );

    commands
        .spawn((
            LoadingOverlay,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(bg),
            ZIndex(100),
        ))
        .with_children(|parent| {
            // Title + progress text
            parent.spawn((
                LoadingText,
                Text::new(config.title.clone()),
                TextFont {
                    font_size: 32.0,
                    ..default()
                },
                TextColor(tc),
            ));

            if config.show_progress_bar {
                // Progress bar background
                parent
                    .spawn((
                        Node {
                            width: Val::Px(400.0),
                            height: Val::Px(20.0),
                            margin: UiRect::top(Val::Px(20.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.2, 0.2, 0.2, 1.0)),
                    ))
                    .with_children(|bar| {
                        // Fill
                        bar.spawn((
                            LoadingProgressBar,
                            Node {
                                width: Val::Percent(0.0),
                                height: Val::Percent(100.0),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.2, 0.7, 0.3, 1.0)),
                        ));
                    });
            }
        });
}

fn update_loading_overlay(
    loading_state: Res<WorldLoadingState>,
    mut progress_reader: bevy::ecs::message::MessageReader<LoadingProgress>,
    mut overlay_query: Query<&mut Visibility, With<LoadingOverlay>>,
    mut text_query: Query<&mut Text, With<LoadingText>>,
    mut bar_query: Query<&mut Node, With<LoadingProgressBar>>,
) {
    // Hide overlay once world is ready
    if loading_state.ready {
        if let Ok(mut vis) = overlay_query.single_mut() {
            *vis = Visibility::Hidden;
        }
        return;
    }

    // Process latest progress message
    let mut latest: Option<LoadingProgress> = None;
    for msg in progress_reader.read() {
        latest = Some(msg.clone());
    }

    if let Some(progress) = latest {
        // Update text
        if let Ok(mut text) = text_query.single_mut() {
            *text = Text::new(format!(
                "Loading... {}/{} chunks",
                progress.loaded, progress.total
            ));
        }

        // Update progress bar width
        if let Ok(mut node) = bar_query.single_mut() {
            node.width = Val::Percent((progress.progress * 100.0).clamp(0.0, 100.0));
        }
    }
}

fn despawn_loading_overlay_on_ready(
    mut commands: Commands,
    loading_state: Res<WorldLoadingState>,
    overlay_query: Query<Entity, With<LoadingOverlay>>,
) {
    if loading_state.ready {
        for entity in overlay_query.iter() {
            commands.entity(entity).despawn();
        }
    }
}

/// Plugin that displays a full-screen loading overlay during world generation.
///
/// The overlay is spawned on `Startup` and updates every frame by reading
/// [`LoadingProgress`] messages broadcast by the chunk streaming system. Once
/// [`WorldLoadingState::ready`] flips to `true`, the overlay is hidden and then
/// despawned to free the UI node tree.
///
/// Customize appearance via [`LoadingScreenConfig`] inserted before this plugin.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::ui::{LoadingScreenPlugin, LoadingScreenConfig};
///
/// App::new()
///     .insert_resource(LoadingScreenConfig {
///         title: "Generating universe…".to_string(),
///         show_progress_bar: false,
///         ..Default::default()
///     })
///     .add_plugins(LoadingScreenPlugin);
/// ```
pub struct LoadingScreenPlugin;

impl Plugin for LoadingScreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadingScreenConfig>();
        app.add_systems(Startup, spawn_loading_overlay);
        app.add_systems(
            Update,
            (update_loading_overlay, despawn_loading_overlay_on_ready),
        );
    }
}

// ─── Hotbar UI ────────────────────────────────────────────────────────────────

/// Visual configuration for the [`HotbarUiPlugin`] slot row.
///
/// Insert this resource before adding [`HotbarUiPlugin`] to customize the
/// hotbar. Values are consumed on `Startup`; runtime slot-count changes are not
/// supported (the hotbar is a static layout).
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
    /// Width and height of each square slot node in logical pixels.
    pub slot_size: f32,
    /// Horizontal gap between adjacent slot nodes in logical pixels.
    pub slot_gap: f32,
    /// RGBA background color applied to unselected hotbar slots in linear sRGB.
    pub slot_color: [f32; 4],
    /// RGBA background color applied to the currently selected slot in linear
    /// sRGB. Updated in real time when a [`HotbarSelectionChanged`] message fires.
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

/// Marker for individual hotbar slot nodes. Holds the slot index.
#[derive(Component)]
struct HotbarSlot(usize);

fn spawn_hotbar_ui(
    mut commands: Commands,
    config: Res<HotbarUiConfig>,
    hotbar: Res<HotbarState>,
) {
    if !config.visible {
        return;
    }

    let slot_color = Color::srgba(
        config.slot_color[0],
        config.slot_color[1],
        config.slot_color[2],
        config.slot_color[3],
    );
    let selected_color = Color::srgba(
        config.selected_color[0],
        config.selected_color[1],
        config.selected_color[2],
        config.selected_color[3],
    );

    commands
        .spawn((
            HotbarUiRoot,
            Node {
                width: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                bottom: Val::Px(16.0),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: Val::Px(config.slot_gap),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|row| {
            for i in 0..hotbar.slots {
                let bg = if i == hotbar.selected {
                    selected_color
                } else {
                    slot_color
                };
                row.spawn((
                    HotbarSlot(i),
                    Node {
                        width: Val::Px(config.slot_size),
                        height: Val::Px(config.slot_size),
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(bg),
                    BorderColor::all(Color::srgba(0.6, 0.6, 0.6, 0.6)),
                ));
            }
        });
}

fn update_hotbar_selection(
    mut selection_reader: bevy::ecs::message::MessageReader<HotbarSelectionChanged>,
    config: Res<HotbarUiConfig>,
    mut slot_query: Query<(&HotbarSlot, &mut BackgroundColor)>,
) {
    let mut new_selected: Option<usize> = None;
    for msg in selection_reader.read() {
        new_selected = Some(msg.slot);
    }

    let Some(selected) = new_selected else {
        return;
    };

    let slot_color = Color::srgba(
        config.slot_color[0],
        config.slot_color[1],
        config.slot_color[2],
        config.slot_color[3],
    );
    let selected_color = Color::srgba(
        config.selected_color[0],
        config.selected_color[1],
        config.selected_color[2],
        config.selected_color[3],
    );

    for (slot, mut bg) in slot_query.iter_mut() {
        bg.0 = if slot.0 == selected {
            selected_color
        } else {
            slot_color
        };
    }
}

/// Plugin that renders a horizontal hotbar at the bottom center of the screen.
///
/// Reads [`HotbarState`] on startup to determine the number of slots, spawns
/// one [`Node`] quad per slot, and listens for [`HotbarSelectionChanged`]
/// messages to update the selected-slot highlight in real time.
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
        app.add_systems(Update, update_hotbar_selection);
    }
}

// ─── Pickup Notification ──────────────────────────────────────────────────────

/// Visual configuration for the [`PickupNotificationPlugin`] pop-up system.
///
/// Insert this resource before adding [`PickupNotificationPlugin`] to override
/// defaults. Configuration is read once per notification spawn.
///
/// # Default values
///
/// | Field               | Default              |
/// |---------------------|----------------------|
/// | `visible`           | `true`               |
/// | `duration_secs`     | `3.0` s              |
/// | `text_color`        | `[1, 1, 1, 1]` (white) |
/// | `max_notifications` | `5`                  |
#[derive(Resource, Debug, Clone)]
pub struct PickupNotificationConfig {
    /// When `false`, no notification entities are spawned even when
    /// [`ItemPickedUp`] messages fire.
    pub visible: bool,
    /// How long each notification text persists on screen in seconds before
    /// being automatically despawned. Applied per-notification via a
    /// [`Timer`].
    pub duration_secs: f32,
    /// RGBA color of the notification text in linear sRGB.
    pub text_color: [f32; 4],
    /// Maximum number of notifications displayed at the same time. When a new
    /// pickup would exceed this limit, the oldest visible notification is
    /// despawned first to make room.
    pub max_notifications: usize,
}

impl Default for PickupNotificationConfig {
    fn default() -> Self {
        Self {
            visible: true,
            duration_secs: 3.0,
            text_color: [1.0, 1.0, 1.0, 1.0],
            max_notifications: 5,
        }
    }
}

/// Timer component on each notification entity.
#[derive(Component)]
struct NotificationTimer(Timer);

/// Marker for the notification container.
#[derive(Component)]
struct NotificationContainer;

fn spawn_notification_container(mut commands: Commands) {
    commands.spawn((
        NotificationContainer,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(16.0),
            right: Val::Px(16.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            align_items: AlignItems::FlexEnd,
            ..default()
        },
        BackgroundColor(Color::NONE),
    ));
}

fn on_item_picked_up<I: ItemType>(
    mut commands: Commands,
    mut pickup_reader: bevy::ecs::message::MessageReader<ItemPickedUp<I>>,
    config: Res<PickupNotificationConfig>,
    container_query: Query<Entity, With<NotificationContainer>>,
    existing_query: Query<Entity, With<NotificationTimer>>,
) {
    if !config.visible {
        return;
    }

    // Collect all pickups this frame
    let pickups: Vec<(String, u32)> = pickup_reader
        .read()
        .map(|p| (p.item.name().to_string(), p.count))
        .collect();

    if pickups.is_empty() {
        return;
    }

    let Ok(container) = container_query.single() else {
        return;
    };

    let tc = Color::srgba(
        config.text_color[0],
        config.text_color[1],
        config.text_color[2],
        config.text_color[3],
    );

    // Enforce max_notifications — despawn oldest first
    let existing: Vec<Entity> = existing_query.iter().collect();
    let available = config.max_notifications.saturating_sub(existing.len());
    let spawn_count = pickups.len().min(available);

    if existing.len() + pickups.len() > config.max_notifications {
        let overflow = (existing.len() + pickups.len()).saturating_sub(config.max_notifications);
        for entity in existing.iter().take(overflow) {
            commands.entity(*entity).despawn();
        }
    }

    for (name, count) in pickups.iter().take(spawn_count) {
        let label = format!("+{count} {name}");
        let notification = commands
            .spawn((
                NotificationTimer(Timer::from_seconds(
                    config.duration_secs,
                    TimerMode::Once,
                )),
                Text::new(label),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(tc),
                Node {
                    ..default()
                },
            ))
            .id();
        commands.entity(container).add_child(notification);
    }
}

fn tick_notifications(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut NotificationTimer)>,
) {
    for (entity, mut timer) in query.iter_mut() {
        timer.0.tick(time.delta());
        if timer.0.just_finished() {
            commands.entity(entity).despawn();
        }
    }
}

/// Plugin that displays timed item-pickup notifications at the top-right corner.
///
/// Listens for [`ItemPickedUp<I>`] messages and spawns a text node formatted as
/// `"+N ItemName"` for each pickup. Notifications fade out after
/// [`PickupNotificationConfig::duration_secs`] and are despawned automatically.
///
/// When the number of simultaneous notifications would exceed
/// [`PickupNotificationConfig::max_notifications`], the oldest notifications are
/// despawned first.
///
/// The plugin is **generic over the item type** `I: ItemType`. Use [`UiPlugin`]
/// for the built-in [`DefaultItem`], or add this plugin manually with a custom
/// item type:
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::ui::PickupNotificationPlugin;
/// # use voxel_framework::items::inventory::ItemType;
/// # #[derive(Clone)] struct MyItem;
/// # impl ItemType for MyItem {
/// #     fn name(&self) -> &str { "item" }
/// # }
///
/// App::new().add_plugins(PickupNotificationPlugin::<MyItem>::new());
/// ```
pub struct PickupNotificationPlugin<I: ItemType> {
    _marker: std::marker::PhantomData<I>,
}

impl<I: ItemType> PickupNotificationPlugin<I> {
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<I: ItemType> Default for PickupNotificationPlugin<I> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: ItemType> Plugin for PickupNotificationPlugin<I> {
    fn build(&self, app: &mut App) {
        app.init_resource::<PickupNotificationConfig>();
        // Ensure the message is registered even if DropsPlugin<I> is not used.
        app.add_message::<ItemPickedUp<I>>();
        app.add_systems(Startup, spawn_notification_container);
        app.add_systems(
            Update,
            (on_item_picked_up::<I>, tick_notifications),
        );
    }
}

mod pause_menu;
pub use pause_menu::{PauseMenuConfig, PauseMenuPlugin, PauseMenuState};

// ─── Debug Overlay ────────────────────────────────────────────────────────────

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use crate::chunk::world_to_chunk_coord;
use crate::world::ChunkManager;
use crate::player::Player;

/// Tracks whether the debug overlay (F3) is currently visible.
#[derive(Resource, Default)]
pub struct DebugOverlayState {
    /// When `true` the overlay is rendered; toggle with **F3**.
    pub visible: bool,
}

/// Marker component for the debug overlay root node.
#[derive(Component)]
struct DebugOverlay;

/// Marker for the individual text lines (index 0–3).
#[derive(Component)]
struct DebugLine(usize);

fn spawn_debug_overlay(mut commands: Commands) {
    commands
        .spawn((
            DebugOverlay,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(8.0),
                left: Val::Px(8.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                padding: UiRect::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            Visibility::Hidden,
            ZIndex(200),
        ))
        .with_children(|root| {
            let style = TextFont {
                font_size: 14.0,
                ..default()
            };
            let color = TextColor(Color::WHITE);
            for i in 0..4 {
                root.spawn((
                    DebugLine(i),
                    Text::new(""),
                    style.clone(),
                    color,
                ));
            }
        });
}

fn update_debug_overlay(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<DebugOverlayState>,
    mut overlay_q: Query<&mut Visibility, With<DebugOverlay>>,
    mut line_q: Query<(&DebugLine, &mut Text)>,
    player_q: Query<&Transform, With<Player>>,
    manager: Res<ChunkManager>,
    diagnostics: Res<DiagnosticsStore>,
) {
    // Toggle visibility on F3 press.
    if keys.just_pressed(KeyCode::F3) {
        state.visible = !state.visible;
        if let Ok(mut vis) = overlay_q.single_mut() {
            *vis = if state.visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }

    if !state.visible {
        return;
    }

    let pos = player_q
        .single()
        .map(|t| t.translation)
        .unwrap_or(Vec3::ZERO);

    let chunk = world_to_chunk_coord(pos);
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let chunk_count = manager.loaded_count();

    let lines = [
        format!("XYZ: {:.1} / {:.1} / {:.1}", pos.x, pos.y, pos.z),
        format!("Chunk: {} {} {}", chunk.x, chunk.y, chunk.z),
        format!("FPS: {fps:.0}"),
        format!("Chunks: {chunk_count}"),
    ];

    for (line_marker, mut text) in line_q.iter_mut() {
        if let Some(content) = lines.get(line_marker.0) {
            *text = Text::new(content.clone());
        }
    }
}

/// Plugin that renders a debug information overlay toggled with **F3**.
///
/// Shows player XYZ position, current chunk coordinate, FPS, and loaded chunk
/// count. The overlay is hidden by default; press **F3** to show or hide it.
///
/// Automatically adds [`FrameTimeDiagnosticsPlugin`] for FPS measurement.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::ui::DebugOverlayPlugin;
///
/// App::new().add_plugins(DebugOverlayPlugin);
/// ```
pub struct DebugOverlayPlugin;

impl Plugin for DebugOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        app.init_resource::<DebugOverlayState>();
        app.add_systems(Startup, spawn_debug_overlay);
        app.add_systems(Update, update_debug_overlay);
    }
}

// ─── (InventoryUiPlugin removed — games provide their own inventory UI) ───────

// ─── UiPlugin (umbrella) ──────────────────────────────────────────────────────

use crate::items::inventory::DefaultItem;

/// Umbrella plugin that adds all four HUD sub-plugins with [`DefaultItem`].
///
/// Equivalent to adding [`CrosshairPlugin`], [`LoadingScreenPlugin`],
/// [`HotbarUiPlugin`], and [`PickupNotificationPlugin::<DefaultItem>`] in one
/// call. Configuration resources for each sub-plugin can still be inserted
/// individually before `UiPlugin` is added to override their defaults.
///
/// To use a custom item type for pickup notifications, skip `UiPlugin` and add
/// [`PickupNotificationPlugin::<YourItem>::new()`] alongside the other plugins
/// individually.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::ui::UiPlugin;
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(UiPlugin);
/// ```
pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CrosshairPlugin);
        app.add_plugins(LoadingScreenPlugin);
        app.add_plugins(HotbarUiPlugin);
        app.add_plugins(PickupNotificationPlugin::<DefaultItem>::new());
    }
}
