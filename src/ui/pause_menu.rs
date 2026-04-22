//! Optional pause menu with a settings submenu for FOV and SFX volume.
//!
//! This module implements a self-contained pause flow driven by the
//! [`PauseMenuState`] state machine. Add [`PauseMenuPlugin`] to wire it up;
//! then gate your own gameplay systems on [`PauseMenuState::Playing`] so they
//! freeze automatically when the player pauses.
//!
//! # State machine
//!
//! ```text
//!                  Escape
//! ┌─────────┐ ──────────────► ┌────────┐
//! │ Playing │                 │ Paused │ ◄─────────────────┐
//! └─────────┘ ◄────────────── └────────┘                   │
//!                Resume btn /       │ Settings btn          │
//!                Escape             ▼                       │
//!                             ┌──────────┐  ← Back btn     │
//!                             │ Settings │ ────────────────►┘
//!                             └──────────┘
//! ```
//!
//! **Cursor management:** entering `Paused` releases the cursor
//! (`CursorGrabMode::None`, visible). Returning to `Playing` re-locks it
//! (`CursorGrabMode::Locked`, hidden). Do not combine this plugin with
//! [`player_cursor_system`](crate::player::player_cursor_system), which
//! implements a conflicting two-stage Escape handler.
//!
//! # Integration
//!
//! Gate all gameplay systems on [`PauseMenuState::Playing`]:
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use voxel_framework::prelude::*;
//! use voxel_framework::player::{spawn_player, player_look, player_move,
//!     player_resolve_collision};
//! use voxel_framework::ui::{PauseMenuPlugin, PauseMenuState};
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(VoxelPlugin)
//!     .add_plugins(PauseMenuPlugin)
//!     .add_systems(Startup, spawn_player)
//!     .add_systems(Update,
//!         (player_look, player_move, player_resolve_collision::<DefaultVoxel>)
//!             .chain()
//!             .run_if(in_state(PauseMenuState::Playing)),
//!     );
//! ```

use bevy::prelude::*;

use crate::audio::{SfxConfig, SfxEvent, SfxRequest};
use crate::player::PlayerConfig;
use crate::world::{ChunkManager, WorldConfig};

// ─── State ────────────────────────────────────────────────────────────────────

/// Bevy state enum that drives the pause menu lifecycle.
///
/// [`PauseMenuPlugin`] registers this as a Bevy `State` and transitions between
/// variants automatically. Your own systems should use `run_if(in_state(…))`
/// to participate in the pause flow.
///
/// # Variant transitions
///
/// | From | Trigger | To |
/// |------|---------|-----|
/// | `Playing` | Escape key | `Paused` |
/// | `Paused` | Resume button or Escape | `Playing` |
/// | `Paused` | Settings button | `Settings` |
/// | `Settings` | Back button | `Paused` |
///
/// # Gating gameplay systems
///
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use voxel_framework::ui::PauseMenuState;
/// # fn my_gameplay_system() {}
/// App::new()
///     .add_systems(Update,
///         my_gameplay_system.run_if(in_state(PauseMenuState::Playing)),
///     );
/// ```
#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum PauseMenuState {
    /// The game is running normally. Player input is active and the pause
    /// menu is not visible. This is the default starting state.
    #[default]
    Playing,
    /// The game is paused. The pause menu overlay is displayed and gameplay
    /// systems gated on `Playing` are inactive. The cursor is free.
    Paused,
    /// The settings submenu is open. The pause menu overlay is replaced by the
    /// settings panel. FOV and SFX volume can be adjusted here.
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
#[derive(Component)] struct RenderDistLabel;
#[derive(Component)] struct SensitivityLabel;

#[derive(Component)]
enum PauseButton { Resume, Settings, Quit }

#[derive(Component)]
enum SettingsButton {
    FovInc, FovDec,
    VolumeInc, VolumeDec,
    RenderDistInc, RenderDistDec,
    SensitivityInc, SensitivityDec,
    Back,
}

// ─── Config ───────────────────────────────────────────────────────────────────

// ─── Messages ─────────────────────────────────────────────────────────────────

/// Emitted when the player clicks Back in the Settings submenu.
///
/// The host game should listen for this message and persist the current
/// settings (FOV, volume, render distance, sensitivity) to disk using
/// [`crate::settings::PersistedSettings`].
#[derive(Message, Debug, Clone)]
pub struct SaveSettingsRequest;

// ─── Config ───────────────────────────────────────────────────────────────────

/// Visual configuration for [`PauseMenuPlugin`].
///
/// Insert this resource before adding [`PauseMenuPlugin`] to supply a custom
/// font. If omitted, the plugin initializes with [`Handle::default()`], which
/// resolves to Bevy's built-in monospace font.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::ui::PauseMenuConfig;
///
/// fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
///     commands.insert_resource(PauseMenuConfig {
///         font: asset_server.load("fonts/MyFont.ttf"),
///     });
/// }
/// ```
#[derive(Resource, Clone)]
pub struct PauseMenuConfig {
    /// Font handle used for every text element in both the pause menu and the
    /// settings submenu (title, button labels, slider values). Defaults to
    /// [`Handle::default()`], which Bevy resolves to its built-in font.
    pub font: Handle<Font>,
}

impl Default for PauseMenuConfig {
    fn default() -> Self {
        Self { font: Handle::default() }
    }
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Plugin that adds a full-screen pause menu with a settings submenu.
///
/// Registers [`PauseMenuState`] and wires all transitions, UI spawning, and
/// cursor management. The settings submenu exposes real-time controls for FOV
/// (±5° per click, clamped to `50°`–`120°`) and SFX volume (±10% per click,
/// clamped to `0%`–`100%`).
///
/// **Does not** include [`player_cursor_system`](crate::player::player_cursor_system).
/// If you use this plugin, omit that system — this plugin handles cursor locking
/// and releasing as part of the `Playing ↔ Paused` transition.
///
/// # Simple usage
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::prelude::*;
/// use voxel_framework::ui::{PauseMenuPlugin, PauseMenuState};
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(VoxelPlugin)
///     .add_plugins(PauseMenuPlugin)
///     .run();
/// ```
///
/// # With state-gated player systems
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::prelude::*;
/// use voxel_framework::player::{spawn_player, player_look, player_move,
///     player_resolve_collision};
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
        app.add_message::<SaveSettingsRequest>();

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

/// Releases the cursor when entering [`PauseMenuState::Paused`].
///
/// Called via `OnEnter(Paused)`. Sets `CursorGrabMode::None` and makes the
/// cursor visible so the player can interact with the menu buttons.
fn release_cursor(mut windows: Query<&mut bevy::window::CursorOptions>) {
    for mut cursor in &mut windows {
        cursor.grab_mode = bevy::window::CursorGrabMode::None;
        cursor.visible = true;
    }
}

/// Re-locks the cursor when returning to [`PauseMenuState::Playing`].
///
/// Called as a plain function (not a system) from [`escape_to_resume`] and
/// [`handle_pause_buttons`] so that cursor re-lock and state transition happen
/// in the same frame without a one-frame gap.
fn lock_cursor(windows: &mut Query<&mut bevy::window::CursorOptions>) {
    for mut cursor in windows.iter_mut() {
        cursor.grab_mode = bevy::window::CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

// ─── Toggle pause ─────────────────────────────────────────────────────────────

/// Transitions from `Playing` → `Paused` when Escape is pressed.
///
/// Runs only in [`PauseMenuState::Playing`]. Also fires a
/// [`SfxEvent::PauseOpen`] sound request.
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

/// Transitions from `Paused` → `Playing` when Escape is pressed.
///
/// Runs only in [`PauseMenuState::Paused`]. Re-locks the cursor via
/// [`lock_cursor`] in the same frame and fires [`SfxEvent::PauseClose`].
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

/// Spawns the pause menu overlay when entering [`PauseMenuState::Paused`].
///
/// Builds a full-screen semi-transparent overlay with three buttons: Resume,
/// Settings, and Quit. Runs via `OnEnter(Paused)`.
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

/// Despawns the pause menu overlay when leaving [`PauseMenuState::Paused`].
///
/// Runs via `OnExit(Paused)`, which fires for both `Paused → Playing` and
/// `Paused → Settings` transitions.
fn despawn_pause_menu(mut commands: Commands, q: Query<Entity, With<PauseMenuRoot>>) {
    for e in &q { commands.entity(e).despawn(); }
}

/// Handles hover/press styling and click actions for pause menu buttons.
///
/// Runs every frame in [`PauseMenuState::Paused`]. Updates button background
/// colors on hover and press, then on `Pressed` dispatches: Resume → `Playing`
/// (re-locks cursor), Settings → `Settings`, Quit → [`AppExit`].
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

/// Spawns the settings overlay when entering [`PauseMenuState::Settings`].
///
/// Reads the current FOV from the [`Camera3d`] projection, the current SFX
/// volume from [`SfxConfig`], the render distance from [`WorldConfig`], and the
/// mouse sensitivity from [`PlayerConfig`] to initialise the slider labels.
/// Runs via `OnEnter(Settings)`.
fn spawn_settings_menu(
    mut commands: Commands,
    sfx_config: Res<SfxConfig>,
    world_config: Option<Res<WorldConfig>>,
    player_config: Option<Res<PlayerConfig>>,
    camera_q: Query<&Projection, With<Camera3d>>,
    config: Res<PauseMenuConfig>,
) {
    let fov_deg = camera_q
        .single()
        .ok()
        .and_then(|p| if let Projection::Perspective(pp) = p { Some(pp.fov.to_degrees()) } else { None })
        .unwrap_or(90.0);
    let volume_pct = sfx_config.volume * 100.0;
    let render_dist_chunks = world_config
        .as_ref()
        .map(|wc| ((wc.load_distance / 16.0).round() as i32).clamp(4, 32) as i32)
        .unwrap_or(10);
    // Round to nearest even
    let render_dist_chunks = if render_dist_chunks % 2 == 0 { render_dist_chunks } else { render_dist_chunks + 1 };
    let sensitivity_pct = player_config
        .as_ref()
        .map(|pc| (pc.sensitivity * 100.0).round() as i32)
        .unwrap_or(100);

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

        spawn_slider_row(p, "Render Distance", format!("{render_dist_chunks} chunks"), RenderDistLabel,
            SettingsButton::RenderDistDec, SettingsButton::RenderDistInc, config.font.clone());

        spawn_slider_row(p, "Sensitivity", format!("{sensitivity_pct:.0}%"), SensitivityLabel,
            SettingsButton::SensitivityDec, SettingsButton::SensitivityInc, config.font.clone());

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

/// Despawns the settings overlay when leaving [`PauseMenuState::Settings`].
///
/// Runs via `OnExit(Settings)`. The only exit transition is `Settings → Paused`
/// via the Back button.
fn despawn_settings_menu(mut commands: Commands, q: Query<Entity, With<SettingsMenuRoot>>) {
    for e in &q { commands.entity(e).despawn(); }
}

/// Handles hover/press styling and click actions for settings menu buttons.
///
/// Runs every frame in [`PauseMenuState::Settings`]. Adjusts values on press:
/// - `FovInc` / `FovDec`: changes FOV by ±5° (clamped `50°`–`120°`) and updates
///   [`FovLabel`] text and the [`Camera3d`] perspective projection.
/// - `VolumeInc` / `VolumeDec`: changes [`SfxConfig::volume`] by ±0.1 (clamped
///   `0.0`–`1.0`) and updates [`VolumeLabel`] text.
/// - `RenderDistInc` / `RenderDistDec`: changes [`WorldConfig::load_distance`] by
///   ±2 chunks (clamped `4`–`32`) and updates [`RenderDistLabel`] text and
///   [`ChunkManager`] distances.
/// - `SensitivityInc` / `SensitivityDec`: changes [`PlayerConfig::sensitivity`] by
///   ±10% (clamped `10`–`200`) and updates [`SensitivityLabel`] text.
/// - `Back`: emits [`SaveSettingsRequest`] and transitions to [`PauseMenuState::Paused`].
#[allow(clippy::too_many_arguments)]
fn handle_settings_buttons(
    mut q: Query<(&Interaction, &mut BackgroundColor, &SettingsButton),
                 (Changed<Interaction>, With<Button>)>,
    mut sfx_config: ResMut<SfxConfig>,
    mut world_config: Option<ResMut<WorldConfig>>,
    mut chunk_manager: Option<ResMut<ChunkManager>>,
    mut player_config: Option<ResMut<PlayerConfig>>,
    mut next: ResMut<NextState<PauseMenuState>>,
    mut fov_label: Query<&mut Text, (With<FovLabel>, Without<VolumeLabel>, Without<RenderDistLabel>, Without<SensitivityLabel>)>,
    mut vol_label: Query<&mut Text, (With<VolumeLabel>, Without<FovLabel>, Without<RenderDistLabel>, Without<SensitivityLabel>)>,
    mut rdist_label: Query<&mut Text, (With<RenderDistLabel>, Without<FovLabel>, Without<VolumeLabel>, Without<SensitivityLabel>)>,
    mut sens_label: Query<&mut Text, (With<SensitivityLabel>, Without<FovLabel>, Without<VolumeLabel>, Without<RenderDistLabel>)>,
    mut camera_q: Query<&mut Projection, With<Camera3d>>,
    mut save_writer: MessageWriter<SaveSettingsRequest>,
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
            SettingsButton::RenderDistInc | SettingsButton::RenderDistDec => {
                if let Some(ref mut wc) = world_config {
                    // Current value in chunks (rounded to nearest even)
                    let current = ((wc.load_distance / 16.0).round() as i32).clamp(4, 32);
                    let current = if current % 2 == 0 { current } else { current + 1 };
                    let delta: i32 = if matches!(action, SettingsButton::RenderDistInc) { 2 } else { -2 };
                    let new_chunks = (current + delta).clamp(4, 32);
                    wc.load_distance = new_chunks as f32 * 16.0;
                    wc.unload_distance = wc.load_distance + 32.0;
                    if let Some(ref mut cm) = chunk_manager {
                        cm.load_distance = wc.load_distance;
                        cm.unload_distance = wc.unload_distance;
                    }
                    if let Ok(mut t) = rdist_label.single_mut() {
                        **t = format!("{new_chunks} chunks");
                    }
                }
            }
            SettingsButton::SensitivityInc | SettingsButton::SensitivityDec => {
                if let Some(ref mut pc) = player_config {
                    let current_pct = (pc.sensitivity * 100.0).round() as i32;
                    let delta: i32 = if matches!(action, SettingsButton::SensitivityInc) { 10 } else { -10 };
                    let new_pct = (current_pct + delta).clamp(10, 200);
                    pc.sensitivity = new_pct as f32 / 100.0;
                    if let Ok(mut t) = sens_label.single_mut() {
                        **t = format!("{new_pct:.0}%");
                    }
                }
            }
            SettingsButton::Back => {
                save_writer.write(SaveSettingsRequest);
                next.set(PauseMenuState::Paused);
            }
        }
    }
}
