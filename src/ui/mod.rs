//! UI overlay components for voxel games.
//!
//! Provides four ready-to-use HUD plugins:
//!
//! - [`CrosshairPlugin`] — center-screen crosshair made of four `Node` quads.
//! - [`LoadingScreenPlugin`] — overlay that shows while the world is loading.
//! - [`HotbarUiPlugin`] — bottom-center hotbar slot row.
//! - [`PickupNotificationPlugin`] — temporary pop-up text when items are picked up.
//!
//! Use [`UiPlugin`] to add all four at once, or add each plugin individually.

use bevy::prelude::*;

use crate::items::hotbar::{HotbarSelectionChanged, HotbarState};
use crate::items::inventory::ItemPickedUp;
use crate::items::inventory::ItemType;
use crate::world::loading::{LoadingProgress, WorldLoadingState};

// ─── Crosshair ────────────────────────────────────────────────────────────────

/// Configuration for the crosshair overlay.
#[derive(Resource, Debug, Clone)]
pub struct CrosshairConfig {
    /// RGBA color of the crosshair lines.
    pub color: [f32; 4],
    /// Half-length of each arm in logical pixels.
    pub size: f32,
    /// Gap between the crosshair center and the start of each arm.
    pub gap: f32,
    /// Thickness of each arm in logical pixels.
    pub thickness: f32,
    /// Whether the crosshair is visible.
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

/// Plugin that spawns a configurable crosshair at screen center.
///
/// Insert [`CrosshairConfig`] as a resource before adding this plugin to
/// customize the appearance.
pub struct CrosshairPlugin;

impl Plugin for CrosshairPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CrosshairConfig>();
        app.add_systems(Startup, spawn_crosshair);
    }
}

// ─── Loading Screen ───────────────────────────────────────────────────────────

/// Configuration for the loading screen overlay.
#[derive(Resource, Debug, Clone)]
pub struct LoadingScreenConfig {
    /// Background RGBA color of the overlay.
    pub background_color: [f32; 4],
    /// RGBA color of the loading text.
    pub text_color: [f32; 4],
    /// Title shown on the overlay.
    pub title: String,
    /// Whether to show a progress bar under the text.
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

/// Plugin that shows a full-screen loading overlay while the world loads.
///
/// The overlay is hidden and despawned automatically once
/// [`WorldLoadingState::ready`] becomes `true`.
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

/// Configuration for the hotbar HUD.
#[derive(Resource, Debug, Clone)]
pub struct HotbarUiConfig {
    /// Whether the hotbar is visible.
    pub visible: bool,
    /// Width and height of each slot in logical pixels.
    pub slot_size: f32,
    /// Gap between slots in logical pixels.
    pub slot_gap: f32,
    /// Background RGBA color for unselected slots.
    pub slot_color: [f32; 4],
    /// Background RGBA color for the selected slot.
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

/// Plugin that spawns a hotbar slot row at the bottom of the screen.
///
/// Reacts to [`HotbarSelectionChanged`] messages to highlight the active slot.
pub struct HotbarUiPlugin;

impl Plugin for HotbarUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HotbarUiConfig>();
        app.add_systems(Startup, spawn_hotbar_ui);
        app.add_systems(Update, update_hotbar_selection);
    }
}

// ─── Pickup Notification ──────────────────────────────────────────────────────

/// Configuration for item pickup notifications.
#[derive(Resource, Debug, Clone)]
pub struct PickupNotificationConfig {
    /// Whether notifications are shown.
    pub visible: bool,
    /// How many seconds each notification stays on screen.
    pub duration_secs: f32,
    /// RGBA text color.
    pub text_color: [f32; 4],
    /// Maximum simultaneous notifications on screen.
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

/// Plugin that shows temporary pickup notifications at the top-right of the
/// screen when [`ItemPickedUp<I>`] messages fire.
///
/// Generic over the item type. Use [`UiPlugin`] for [`DefaultItem`], or add
/// this plugin manually with your own item type:
///
/// ```rust,ignore
/// app.add_plugins(PickupNotificationPlugin::<MyItem>::new());
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
        app.add_systems(Startup, spawn_notification_container);
        app.add_systems(
            Update,
            (on_item_picked_up::<I>, tick_notifications),
        );
    }
}

// ─── UiPlugin (umbrella) ──────────────────────────────────────────────────────

use crate::items::inventory::DefaultItem;

/// Umbrella plugin that adds all four UI sub-plugins using [`DefaultItem`].
///
/// To use a custom item type for pickup notifications, add
/// [`PickupNotificationPlugin::<YourItem>::new()`] individually instead.
pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CrosshairPlugin);
        app.add_plugins(LoadingScreenPlugin);
        app.add_plugins(HotbarUiPlugin);
        app.add_plugins(PickupNotificationPlugin::<DefaultItem>::new());
    }
}
