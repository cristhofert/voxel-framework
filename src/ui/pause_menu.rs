//! Optional pause menu plugin with settings submenu.
//!
//! Add [`PauseMenuPlugin`] to your app to get a pause menu triggered by Escape.
//! The plugin manages its own [`PauseMenuState`] — gate your game systems on it:
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use voxel_framework::ui::PauseMenuState;
//!
//! App::new()
//!     .add_systems(Update,
//!         my_gameplay_system.run_if(in_state(PauseMenuState::Playing)),
//!     );
//! ```

use bevy::prelude::*;

use crate::audio::{SfxConfig, SfxEvent, SfxRequest};

// ─── State ────────────────────────────────────────────────────────────────────

/// Internal state managed by [`PauseMenuPlugin`].
///
/// Use this to gate your game systems so they pause correctly:
/// ```rust,no_run
/// .run_if(in_state(PauseMenuState::Playing))
/// ```
#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum PauseMenuState {
    #[default]
    Playing,
    Paused,
    Settings,
}

// ─── Styling constants ────────────────────────────────────────────────────────

const BTN_NORMAL:  Color = Color::srgb(0.15, 0.15, 0.20);
const BTN_HOVERED: Color = Color::srgb(0.25, 0.25, 0.35);
const BTN_PRESSED: Color = Color::srgb(0.35, 0.35, 0.50);
const OVERLAY:     Color = Color::srgba(0.0, 0.0, 0.0, 0.78);

// ─── Markers ─────────────────────────────────────────────────────────────────

#[derive(Component)] struct PauseMenuRoot;
#[derive(Component)] struct SettingsMenuRoot;
#[derive(Component)] struct FovLabel;
#[derive(Component)] struct VolumeLabel;

#[derive(Component)]
enum PauseButton { Resume, Settings, Quit }

#[derive(Component)]
enum SettingsButton { FovInc, FovDec, VolumeInc, VolumeDec, Back }

// ─── Config ───────────────────────────────────────────────────────────────────

/// Configuration for [`PauseMenuPlugin`].
///
/// Insert before adding the plugin to override defaults:
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::ui::PauseMenuConfig;
///
/// App::new()
///     .insert_resource(PauseMenuConfig {
///         font: asset_server.load("fonts/MyFont.ttf"),
///     });
/// ```
#[derive(Resource, Clone)]
pub struct PauseMenuConfig {
    /// Font handle used for all pause menu text. Default: Bevy's built-in font.
    pub font: Handle<Font>,
}

impl Default for PauseMenuConfig {
    fn default() -> Self {
        Self { font: Handle::default() }
    }
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Optional pause menu plugin.
///
/// Adds [`PauseMenuState`] and wires Escape to toggle between Playing and Paused.
/// The Settings submenu lets the player adjust FOV and SFX volume at runtime.
///
/// # Usage
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::prelude::*;
/// use voxel_framework::player::PlayerPlugin;
/// use voxel_framework::ui::{PauseMenuPlugin, PauseMenuState};
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(VoxelPlugin)
///     .add_plugins(PauseMenuPlugin)
///     .add_systems(Startup, spawn_player)
///     .add_systems(Update,
///         (player_look, player_move, player_resolve_collision::<DefaultVoxel>)
///             .chain()
///             .run_if(in_state(PauseMenuState::Playing)),
///     );
/// ```
pub struct PauseMenuPlugin;

impl Plugin for PauseMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PauseMenuConfig>();
        app.init_state::<PauseMenuState>();

        // Playing — toggle on Escape
        app.add_systems(Update, toggle_pause.run_if(in_state(PauseMenuState::Playing)));

        // Paused
        app.add_systems(OnEnter(PauseMenuState::Paused), (spawn_pause_menu, release_cursor));
        app.add_systems(OnExit(PauseMenuState::Paused),  despawn_pause_menu);
        app.add_systems(Update, (handle_pause_buttons, escape_to_resume)
            .run_if(in_state(PauseMenuState::Paused)));

        // Settings
        app.add_systems(OnEnter(PauseMenuState::Settings), spawn_settings_menu);
        app.add_systems(OnExit(PauseMenuState::Settings),  despawn_settings_menu);
        app.add_systems(Update, handle_settings_buttons.run_if(in_state(PauseMenuState::Settings)));
    }
}

// ─── Cursor helpers ───────────────────────────────────────────────────────────

fn release_cursor(mut windows: Query<&mut bevy::window::CursorOptions>) {
    for mut cursor in &mut windows {
        cursor.grab_mode = bevy::window::CursorGrabMode::None;
        cursor.visible = true;
    }
}

fn lock_cursor(windows: &mut Query<&mut bevy::window::CursorOptions>) {
    for mut cursor in windows.iter_mut() {
        cursor.grab_mode = bevy::window::CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

// ─── Toggle pause ─────────────────────────────────────────────────────────────

fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    mut next: ResMut<NextState<PauseMenuState>>,
    mut sfx: MessageWriter<SfxRequest>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        next.set(PauseMenuState::Paused);
        sfx.write(SfxRequest { event: SfxEvent::PauseOpen, position: None });
    }
}

fn escape_to_resume(
    keys: Res<ButtonInput<KeyCode>>,
    mut next: ResMut<NextState<PauseMenuState>>,
    mut windows: Query<&mut bevy::window::CursorOptions>,
    mut sfx: MessageWriter<SfxRequest>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        next.set(PauseMenuState::Playing);
        lock_cursor(&mut windows);
        sfx.write(SfxRequest { event: SfxEvent::PauseClose, position: None });
    }
}

// ─── Spawn helpers ────────────────────────────────────────────────────────────

fn spawn_btn<C: Component>(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: C,
    width: f32,
    height: f32,
    font: Handle<Font>,
) {
    parent
        .spawn((
            action,
            Button,
            Node {
                width: Val::Px(width),
                height: Val::Px(height),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(BTN_NORMAL),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont { font, font_size: 20.0, ..default() },
                TextColor(Color::WHITE),
            ));
        });
}

fn overlay_node() -> (Node, BackgroundColor) {
    (
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(14.0),
            ..default()
        },
        BackgroundColor(OVERLAY),
    )
}

// ─── Pause menu ───────────────────────────────────────────────────────────────

fn spawn_pause_menu(mut commands: Commands, config: Res<PauseMenuConfig>) {
    let (node, bg) = overlay_node();
    commands.spawn((PauseMenuRoot, node, bg)).with_children(|p| {
        p.spawn((
            Text::new("PAUSED"),
            TextFont { font: config.font.clone(), font_size: 52.0, ..default() },
            TextColor(Color::WHITE),
            Node { margin: UiRect::bottom(Val::Px(28.0)), ..default() },
        ));
        spawn_btn(p, "Resume",   PauseButton::Resume,   280.0, 52.0, config.font.clone());
        spawn_btn(p, "Settings", PauseButton::Settings, 280.0, 52.0, config.font.clone());
        spawn_btn(p, "Quit",     PauseButton::Quit,     280.0, 52.0, config.font.clone());
    });
}

fn despawn_pause_menu(mut commands: Commands, q: Query<Entity, With<PauseMenuRoot>>) {
    for e in &q { commands.entity(e).despawn(); }
}

fn handle_pause_buttons(
    mut q: Query<(&Interaction, &mut BackgroundColor, &PauseButton),
                 (Changed<Interaction>, With<Button>)>,
    mut next: ResMut<NextState<PauseMenuState>>,
    mut windows: Query<&mut bevy::window::CursorOptions>,
    mut app_exit: MessageWriter<AppExit>,
    mut sfx: MessageWriter<SfxRequest>,
) {
    for (interaction, mut bg, action) in &mut q {
        *bg = BackgroundColor(match interaction {
            Interaction::Hovered => BTN_HOVERED,
            Interaction::Pressed => BTN_PRESSED,
            Interaction::None    => BTN_NORMAL,
        });
        if *interaction != Interaction::Pressed { continue; }
        match action {
            PauseButton::Resume => {
                next.set(PauseMenuState::Playing);
                lock_cursor(&mut windows);
                sfx.write(SfxRequest { event: SfxEvent::PauseClose, position: None });
            }
            PauseButton::Settings => next.set(PauseMenuState::Settings),
            PauseButton::Quit     => { app_exit.write(AppExit::Success); }
        }
    }
}

// ─── Settings menu ────────────────────────────────────────────────────────────

fn spawn_settings_menu(
    mut commands: Commands,
    sfx_config: Res<SfxConfig>,
    camera_q: Query<&Projection, With<Camera3d>>,
    config: Res<PauseMenuConfig>,
) {
    let fov_deg = camera_q
        .single()
        .ok()
        .and_then(|p| if let Projection::Perspective(pp) = p { Some(pp.fov.to_degrees()) } else { None })
        .unwrap_or(90.0);
    let volume_pct = sfx_config.volume * 100.0;

    let (node, bg) = overlay_node();
    commands.spawn((SettingsMenuRoot, node, bg)).with_children(|p| {
        p.spawn((
            Text::new("SETTINGS"),
            TextFont { font: config.font.clone(), font_size: 42.0, ..default() },
            TextColor(Color::WHITE),
            Node { margin: UiRect::bottom(Val::Px(20.0)), ..default() },
        ));

        spawn_slider_row(p, "Field of View", format!("{fov_deg:.0}°"), FovLabel,
            SettingsButton::FovDec, SettingsButton::FovInc, config.font.clone());

        spawn_slider_row(p, "SFX Volume", format!("{volume_pct:.0}%"), VolumeLabel,
            SettingsButton::VolumeDec, SettingsButton::VolumeInc, config.font.clone());

        p.spawn(Node { margin: UiRect::top(Val::Px(20.0)), ..default() })
            .with_children(|row| {
                spawn_btn(row, "← Back", SettingsButton::Back, 200.0, 50.0, config.font.clone());
            });
    });
}

fn spawn_slider_row<L: Component, D: Component, I: Component>(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    value: String,
    value_marker: L,
    dec: D,
    inc: I,
    font: Handle<Font>,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(12.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(label),
                TextFont { font: font.clone(), font_size: 20.0, ..default() },
                TextColor(Color::srgb(0.8, 0.8, 0.8)),
                Node { width: Val::Px(200.0), ..default() },
            ));
            spawn_btn(row, "◀", dec, 44.0, 44.0, font.clone());
            row.spawn((
                value_marker,
                Text::new(value),
                TextFont { font: font.clone(), font_size: 22.0, ..default() },
                TextColor(Color::WHITE),
                Node { width: Val::Px(80.0), justify_content: JustifyContent::Center, ..default() },
            ));
            spawn_btn(row, "▶", inc, 44.0, 44.0, font);
        });
}

fn despawn_settings_menu(mut commands: Commands, q: Query<Entity, With<SettingsMenuRoot>>) {
    for e in &q { commands.entity(e).despawn(); }
}

fn handle_settings_buttons(
    mut q: Query<(&Interaction, &mut BackgroundColor, &SettingsButton),
                 (Changed<Interaction>, With<Button>)>,
    mut sfx_config: ResMut<SfxConfig>,
    mut next: ResMut<NextState<PauseMenuState>>,
    mut fov_label: Query<&mut Text, (With<FovLabel>, Without<VolumeLabel>)>,
    mut vol_label: Query<&mut Text, (With<VolumeLabel>, Without<FovLabel>)>,
    mut camera_q: Query<&mut Projection, With<Camera3d>>,
) {
    for (interaction, mut bg, action) in &mut q {
        *bg = BackgroundColor(match interaction {
            Interaction::Hovered => BTN_HOVERED,
            Interaction::Pressed => BTN_PRESSED,
            Interaction::None    => BTN_NORMAL,
        });
        if *interaction != Interaction::Pressed { continue; }

        match action {
            SettingsButton::FovInc | SettingsButton::FovDec => {
                if let Ok(mut proj) = camera_q.single_mut() {
                    if let Projection::Perspective(ref mut pp) = *proj {
                        let delta = if matches!(action, SettingsButton::FovInc) { 5.0 } else { -5.0 };
                        let new_fov = (pp.fov.to_degrees() + delta).clamp(50.0, 120.0);
                        pp.fov = new_fov.to_radians();
                        if let Ok(mut t) = fov_label.single_mut() {
                            **t = format!("{new_fov:.0}°");
                        }
                    }
                }
            }
            SettingsButton::VolumeInc | SettingsButton::VolumeDec => {
                let delta = if matches!(action, SettingsButton::VolumeInc) { 0.1 } else { -0.1 };
                sfx_config.volume = (sfx_config.volume + delta).clamp(0.0, 1.0);
                let pct = sfx_config.volume * 100.0;
                if let Ok(mut t) = vol_label.single_mut() {
                    **t = format!("{pct:.0}%");
                }
            }
            SettingsButton::Back => next.set(PauseMenuState::Paused),
        }
    }
}
