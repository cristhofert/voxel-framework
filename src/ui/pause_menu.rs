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
//! **Cursor management:** entering `Paused` releases the primary window's
//! cursor (`CursorGrabMode::None`, visible). Returning to `Playing` re-locks
//! it (`CursorGrabMode::Locked`, hidden) when the window is focused.
//!
//! This plugin does not capture the cursor on the first click or release it
//! when the window loses focus. Add
//! [`PlayerPlugin`](crate::player::PlayerPlugin), or
//! [`player_cursor_system`](crate::player::player_cursor_system) on its own,
//! for that: when this plugin is installed the cursor system leaves Escape to
//! the pause menu, so the two work together.
//!
//! # Integration
//!
//! [`PlayerPlugin`](crate::player::PlayerPlugin) pauses with the menu on its
//! own. Gate your other gameplay systems on [`PauseMenuState::Playing`]:
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use voxel_framework::prelude::*;
//! use voxel_framework::ui::{PauseMenuPlugin, PauseMenuState};
//!
//! fn my_gameplay_system() {}
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
//!     .add_plugins(PauseMenuPlugin)
//!     .add_plugins(PlayerPlugin::<DefaultVoxel>::new())
//!     .add_systems(Update, my_gameplay_system.run_if(in_state(PauseMenuState::Playing)));
//! ```

use bevy::asset::embedded_asset;
use bevy::prelude::*;

use crate::audio::{SfxConfig, SfxEvent, SfxRequest};
use crate::player::{Player, PlayerConfig};
use crate::world::{ChunkManager, WorldConfig};

/// Asset path of the NotoSans font embedded into the framework binary.
///
/// The `embedded://` scheme is registered by the Bevy `EmbeddedAssetPlugin`
/// (part of `AssetPlugin` / `DefaultPlugins`). The path format is
/// `embedded://{crate_name}/{path relative to the crate's `src/` dir}`, where
/// the crate name uses underscores. See [`PauseMenuPlugin`] for how this font
/// becomes the default value of [`PauseMenuConfig::font`].
pub const EMBEDDED_NOTO_SANS_PATH: &str = "embedded://voxel_framework/ui/NotoSans-Regular.ttf";

/// Asset path of the NotoSansSymbols2 font embedded into the framework binary.
///
/// Used exclusively to render symbol glyphs (e.g. `◀`, `▶`) that are absent
/// from the subset of NotoSans shipped at [`EMBEDDED_NOTO_SANS_PATH`]. The
/// `embedded://` scheme is registered by the Bevy `EmbeddedAssetPlugin` (part
/// of `AssetPlugin` / `DefaultPlugins`).
pub const EMBEDDED_NOTO_SYMBOLS_PATH: &str =
    "embedded://voxel_framework/ui/NotoSansSymbols2-Regular.ttf";

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

const BTN_NORMAL: Color = Color::srgb(0.15, 0.15, 0.20);
const BTN_HOVERED: Color = Color::srgb(0.25, 0.25, 0.35);
const BTN_PRESSED: Color = Color::srgb(0.35, 0.35, 0.50);
const OVERLAY: Color = Color::srgba(0.0, 0.0, 0.0, 0.78);

// ─── Markers ─────────────────────────────────────────────────────────────────

#[derive(Component)]
struct PauseMenuRoot;
#[derive(Component)]
struct SettingsMenuRoot;
#[derive(Component)]
struct FovLabel;
#[derive(Component)]
struct VolumeLabel;
#[derive(Component)]
struct RenderDistLabel;
#[derive(Component)]
struct SensitivityLabel;

#[derive(Component)]
enum PauseButton {
    Resume,
    Settings,
    Quit,
}

#[derive(Component)]
enum SettingsButton {
    FovInc,
    FovDec,
    VolumeInc,
    VolumeDec,
    RenderDistInc,
    RenderDistDec,
    SensitivityInc,
    SensitivityDec,
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
/// font. If omitted, [`PauseMenuPlugin`] populates `font` with the framework's
/// embedded [NotoSans](https://fonts.google.com/noto/specimen/Noto+Sans)
/// regular TTF — a unicode-capable font that renders the settings submenu's
/// arrow glyphs (`←`, `◀`, `▶`) without tofu.
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
///         symbol_font: asset_server.load("fonts/MyFont.ttf"),
///     });
/// }
/// ```
#[derive(Resource, Clone)]
pub struct PauseMenuConfig {
    /// Font handle used for every text element in both the pause menu and the
    /// settings submenu (title, button labels, slider values).
    ///
    /// Defaults to [`Handle::default()`] at construction time. [`PauseMenuPlugin`]
    /// replaces the default handle with the embedded NotoSans font during
    /// `Startup` (see `EMBEDDED_NOTO_SANS_PATH`) so games inherit a legible
    /// unicode-capable font without loading any asset themselves. To override,
    /// insert [`PauseMenuConfig`] with a custom `font` handle before the menu is
    /// first spawned.
    pub font: Handle<Font>,
    /// Font handle used for symbol-only glyphs that are absent from the main
    /// [`font`](Self::font) subset — specifically the `◀` and `▶` triangle
    /// glyphs used on the settings slider inc/dec buttons.
    ///
    /// Defaults to [`Handle::default()`] at construction time. [`PauseMenuPlugin`]
    /// replaces the default handle with the embedded NotoSansSymbols2 font
    /// during `Startup` (see `EMBEDDED_NOTO_SYMBOLS_PATH`). To override,
    /// insert [`PauseMenuConfig`] with a custom `symbol_font` handle before the
    /// menu is first spawned.
    pub symbol_font: Handle<Font>,
}

impl Default for PauseMenuConfig {
    fn default() -> Self {
        Self {
            font: Handle::default(),
            symbol_font: Handle::default(),
        }
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
/// Handles cursor releasing and locking as part of the `Playing ↔ Paused`
/// transition. First-click capture and focus-loss release come from
/// [`player_cursor_system`](crate::player::player_cursor_system), which
/// [`PlayerPlugin`](crate::player::PlayerPlugin) registers and which defers
/// Escape to this plugin when both are present.
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
///     .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
///     .add_plugins(PauseMenuPlugin)
///     .run();
/// ```
///
/// # With hand-registered player systems
///
/// Include [`player_cursor_system`](crate::player::player_cursor_system) so the
/// first click captures the cursor and focus loss releases it:
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::prelude::*;
/// use voxel_framework::player::{spawn_player, player_cursor_system, player_look,
///     player_move, player_resolve_collision};
/// use voxel_framework::ui::{PauseMenuPlugin, PauseMenuState};
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
///     .add_plugins(PauseMenuPlugin)
///     .add_systems(Startup, spawn_player)
///     .add_systems(Update,
///         (player_cursor_system, player_look, player_move::<DefaultVoxel>,
///             player_resolve_collision::<DefaultVoxel>)
///             .chain()
///             .run_if(in_state(PauseMenuState::Playing)),
///     );
/// ```
pub struct PauseMenuPlugin;

impl Plugin for PauseMenuPlugin {
    fn build(&self, app: &mut App) {
        // Embed the NotoSans TTF into the framework binary. The resulting asset
        // is reachable from any `AssetServer::load(...)` call via the
        // `embedded://` scheme — see [`EMBEDDED_NOTO_SANS_PATH`].
        embedded_asset!(app, "NotoSans-Regular.ttf");
        embedded_asset!(app, "NotoSansSymbols2-Regular.ttf");

        app.init_resource::<PauseMenuConfig>();
        app.init_state::<PauseMenuState>();
        app.add_message::<SaveSettingsRequest>();

        // Populate `PauseMenuConfig::font` with the embedded NotoSans handle
        // when the user hasn't supplied their own font. Runs once on `Startup`,
        // which fires before any `OnEnter(Paused)` transition can spawn the
        // menu, so the handle is always ready by the time the menu is built.
        app.add_systems(Startup, load_default_font);

        // Playing — toggle on Escape
        app.add_systems(
            Update,
            toggle_pause.run_if(in_state(PauseMenuState::Playing)),
        );

        // Paused
        app.add_systems(
            OnEnter(PauseMenuState::Paused),
            (spawn_pause_menu, release_cursor),
        );
        app.add_systems(OnExit(PauseMenuState::Paused), despawn_pause_menu);
        app.add_systems(
            Update,
            (handle_pause_buttons, escape_to_resume).run_if(in_state(PauseMenuState::Paused)),
        );

        // Settings
        app.add_systems(OnEnter(PauseMenuState::Settings), spawn_settings_menu);
        app.add_systems(OnExit(PauseMenuState::Settings), despawn_settings_menu);
        app.add_systems(
            Update,
            handle_settings_buttons.run_if(in_state(PauseMenuState::Settings)),
        );
    }
}

// ─── Default font loader ──────────────────────────────────────────────────────

/// Populates [`PauseMenuConfig::font`] with the embedded NotoSans font on
/// startup if the game has not supplied a custom font.
///
/// `Default` can't access the [`AssetServer`], so we use a startup sentinel:
/// when `font == Handle::default()` (the weak, uninitialised handle produced
/// by [`PauseMenuConfig::default()`]), we load the embedded font and overwrite
/// the field. Games that insert their own [`PauseMenuConfig`] before this
/// plugin — or before `Startup` runs — keep their custom handle untouched.
fn load_default_font(mut cfg: ResMut<PauseMenuConfig>, asset_server: Res<AssetServer>) {
    if cfg.font == Handle::default() {
        cfg.font = asset_server.load(EMBEDDED_NOTO_SANS_PATH);
    }
    if cfg.symbol_font == Handle::default() {
        cfg.symbol_font = asset_server.load(EMBEDDED_NOTO_SYMBOLS_PATH);
    }
}

// ─── Cursor helpers ───────────────────────────────────────────────────────────

/// Releases the cursor when entering [`PauseMenuState::Paused`].
///
/// Called via `OnEnter(Paused)`. Sets `CursorGrabMode::None` on the primary
/// window and makes the cursor visible so the player can interact with the
/// menu buttons.
fn release_cursor(mut windows: PrimaryCursor) {
    for (_, mut cursor) in &mut windows {
        cursor.grab_mode = bevy::window::CursorGrabMode::None;
        cursor.visible = true;
    }
}

/// The primary window and its cursor options: the only cursor the pause menu
/// manages.
type PrimaryCursor<'w, 's> = Query<
    'w,
    's,
    (&'static Window, &'static mut bevy::window::CursorOptions),
    With<bevy::window::PrimaryWindow>,
>;

/// Re-locks the cursor when returning to [`PauseMenuState::Playing`].
///
/// Called as a plain function (not a system) from [`escape_to_resume`] and
/// [`handle_pause_buttons`] so that cursor re-lock and state transition happen
/// in the same frame without a one-frame gap.
///
/// An unfocused window is left alone: a window must be focused before its
/// cursor is captured, and a click captures it later.
fn lock_cursor(windows: &mut PrimaryCursor) {
    for (window, mut cursor) in windows.iter_mut() {
        if !window.focused {
            continue;
        }
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
        sfx.write(SfxRequest {
            event: SfxEvent::PauseOpen,
            position: None,
        });
    }
}

/// Transitions from `Paused` → `Playing` when Escape is pressed.
///
/// Runs only in [`PauseMenuState::Paused`]. Re-locks the cursor via
/// [`lock_cursor`] in the same frame and fires [`SfxEvent::PauseClose`].
fn escape_to_resume(
    keys: Res<ButtonInput<KeyCode>>,
    mut next: ResMut<NextState<PauseMenuState>>,
    mut windows: PrimaryCursor,
    mut sfx: MessageWriter<SfxRequest>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        next.set(PauseMenuState::Playing);
        lock_cursor(&mut windows);
        sfx.write(SfxRequest {
            event: SfxEvent::PauseClose,
            position: None,
        });
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
                TextFont {
                    font,
                    font_size: 20.0,
                    ..default()
                },
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
    commands
        .spawn((PauseMenuRoot, node, bg))
        .with_children(|p| {
            p.spawn((
                Text::new("PAUSED"),
                TextFont {
                    font: config.font.clone(),
                    font_size: 52.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    margin: UiRect::bottom(Val::Px(28.0)),
                    ..default()
                },
            ));
            spawn_btn(
                p,
                "Resume",
                PauseButton::Resume,
                280.0,
                52.0,
                config.font.clone(),
            );
            spawn_btn(
                p,
                "Settings",
                PauseButton::Settings,
                280.0,
                52.0,
                config.font.clone(),
            );
            spawn_btn(
                p,
                "Quit",
                PauseButton::Quit,
                280.0,
                52.0,
                config.font.clone(),
            );
        });
}

/// Despawns the pause menu overlay when leaving [`PauseMenuState::Paused`].
///
/// Runs via `OnExit(Paused)`, which fires for both `Paused → Playing` and
/// `Paused → Settings` transitions.
fn despawn_pause_menu(mut commands: Commands, q: Query<Entity, With<PauseMenuRoot>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

/// Handles hover/press styling and click actions for pause menu buttons.
///
/// Runs every frame in [`PauseMenuState::Paused`]. Updates button background
/// colors on hover and press, then on `Pressed` dispatches: Resume → `Playing`
/// (re-locks cursor), Settings → `Settings`, Quit → [`AppExit`].
fn handle_pause_buttons(
    mut q: Query<
        (&Interaction, &mut BackgroundColor, &PauseButton),
        (Changed<Interaction>, With<Button>),
    >,
    mut next: ResMut<NextState<PauseMenuState>>,
    mut windows: PrimaryCursor,
    mut app_exit: MessageWriter<AppExit>,
    mut sfx: MessageWriter<SfxRequest>,
) {
    for (interaction, mut bg, action) in &mut q {
        *bg = BackgroundColor(match interaction {
            Interaction::Hovered => BTN_HOVERED,
            Interaction::Pressed => BTN_PRESSED,
            Interaction::None => BTN_NORMAL,
        });
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            PauseButton::Resume => {
                next.set(PauseMenuState::Playing);
                lock_cursor(&mut windows);
                sfx.write(SfxRequest {
                    event: SfxEvent::PauseClose,
                    position: None,
                });
            }
            PauseButton::Settings => next.set(PauseMenuState::Settings),
            PauseButton::Quit => {
                app_exit.write(AppExit::Success);
            }
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
        .and_then(|p| {
            if let Projection::Perspective(pp) = p {
                Some(pp.fov.to_degrees())
            } else {
                None
            }
        })
        .unwrap_or(90.0);
    let volume_pct = sfx_config.volume * 100.0;
    let render_dist_chunks = world_config
        .as_ref()
        .map(|wc| ((wc.load_distance / 16.0).round() as i32).clamp(4, 32) as i32)
        .unwrap_or(10);
    // Round to nearest even
    let render_dist_chunks = if render_dist_chunks % 2 == 0 {
        render_dist_chunks
    } else {
        render_dist_chunks + 1
    };
    let sensitivity_pct = player_config
        .as_ref()
        .map(|pc| ((pc.sensitivity * 100.0).round() as i32).clamp(10, 100))
        .unwrap_or(100);

    let (node, bg) = overlay_node();
    commands
        .spawn((SettingsMenuRoot, node, bg))
        .with_children(|p| {
            p.spawn((
                Text::new("SETTINGS"),
                TextFont {
                    font: config.font.clone(),
                    font_size: 42.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    margin: UiRect::bottom(Val::Px(20.0)),
                    ..default()
                },
            ));

            spawn_slider_row(
                p,
                "Field of View",
                format!("{fov_deg:.0}°"),
                FovLabel,
                SettingsButton::FovDec,
                SettingsButton::FovInc,
                config.font.clone(),
                config.symbol_font.clone(),
            );

            spawn_slider_row(
                p,
                "SFX Volume",
                format!("{volume_pct:.0}%"),
                VolumeLabel,
                SettingsButton::VolumeDec,
                SettingsButton::VolumeInc,
                config.font.clone(),
                config.symbol_font.clone(),
            );

            spawn_slider_row(
                p,
                "Render Distance",
                format!("{render_dist_chunks} chunks"),
                RenderDistLabel,
                SettingsButton::RenderDistDec,
                SettingsButton::RenderDistInc,
                config.font.clone(),
                config.symbol_font.clone(),
            );

            spawn_slider_row(
                p,
                "Sensitivity",
                format!("{sensitivity_pct:.0}%"),
                SensitivityLabel,
                SettingsButton::SensitivityDec,
                SettingsButton::SensitivityInc,
                config.font.clone(),
                config.symbol_font.clone(),
            );

            p.spawn(Node {
                margin: UiRect::top(Val::Px(20.0)),
                ..default()
            })
            .with_children(|row| {
                spawn_btn(
                    row,
                    "< Back",
                    SettingsButton::Back,
                    200.0,
                    50.0,
                    config.font.clone(),
                );
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
    symbol_font: Handle<Font>,
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
                TextFont {
                    font: font.clone(),
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.8, 0.8)),
                Node {
                    width: Val::Px(200.0),
                    ..default()
                },
            ));
            spawn_btn(row, "\u{25C0}", dec, 44.0, 44.0, symbol_font.clone());
            row.spawn((
                value_marker,
                Text::new(value),
                TextFont {
                    font: font.clone(),
                    font_size: 22.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    width: Val::Px(80.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
            ));
            spawn_btn(row, "\u{25B6}", inc, 44.0, 44.0, symbol_font);
        });
}

/// Despawns the settings overlay when leaving [`PauseMenuState::Settings`].
///
/// Runs via `OnExit(Settings)`. The only exit transition is `Settings → Paused`
/// via the Back button.
fn despawn_settings_menu(mut commands: Commands, q: Query<Entity, With<SettingsMenuRoot>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
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
///   ±1% (clamped `10`–`100`), applies it to every live [`Player`] so mouse
///   look changes immediately, and updates [`SensitivityLabel`] text.
/// - `Back`: emits [`SaveSettingsRequest`] and transitions to [`PauseMenuState::Paused`].
#[allow(clippy::too_many_arguments)]
fn handle_settings_buttons(
    mut q: Query<
        (&Interaction, &mut BackgroundColor, &SettingsButton),
        (Changed<Interaction>, With<Button>),
    >,
    mut sfx_config: ResMut<SfxConfig>,
    mut world_config: Option<ResMut<WorldConfig>>,
    mut chunk_manager: Option<ResMut<ChunkManager>>,
    mut player_config: Option<ResMut<PlayerConfig>>,
    mut next: ResMut<NextState<PauseMenuState>>,
    mut fov_label: Query<
        &mut Text,
        (
            With<FovLabel>,
            Without<VolumeLabel>,
            Without<RenderDistLabel>,
            Without<SensitivityLabel>,
        ),
    >,
    mut vol_label: Query<
        &mut Text,
        (
            With<VolumeLabel>,
            Without<FovLabel>,
            Without<RenderDistLabel>,
            Without<SensitivityLabel>,
        ),
    >,
    mut rdist_label: Query<
        &mut Text,
        (
            With<RenderDistLabel>,
            Without<FovLabel>,
            Without<VolumeLabel>,
            Without<SensitivityLabel>,
        ),
    >,
    mut sens_label: Query<
        &mut Text,
        (
            With<SensitivityLabel>,
            Without<FovLabel>,
            Without<VolumeLabel>,
            Without<RenderDistLabel>,
        ),
    >,
    mut camera_q: Query<&mut Projection, With<Camera3d>>,
    mut players: Query<&mut Player>,
    mut save_writer: MessageWriter<SaveSettingsRequest>,
) {
    for (interaction, mut bg, action) in &mut q {
        *bg = BackgroundColor(match interaction {
            Interaction::Hovered => BTN_HOVERED,
            Interaction::Pressed => BTN_PRESSED,
            Interaction::None => BTN_NORMAL,
        });
        if *interaction != Interaction::Pressed {
            continue;
        }

        match action {
            SettingsButton::FovInc | SettingsButton::FovDec => {
                if let Ok(mut proj) = camera_q.single_mut() {
                    if let Projection::Perspective(ref mut pp) = *proj {
                        let new_fov = step_fov_degrees(
                            pp.fov.to_degrees(),
                            matches!(action, SettingsButton::FovInc),
                        );
                        pp.fov = new_fov.to_radians();
                        if let Ok(mut t) = fov_label.single_mut() {
                            **t = format!("{new_fov:.0}°");
                        }
                    }
                }
            }
            SettingsButton::VolumeInc | SettingsButton::VolumeDec => {
                sfx_config.volume = step_volume(
                    sfx_config.volume,
                    matches!(action, SettingsButton::VolumeInc),
                );
                let pct = sfx_config.volume * 100.0;
                if let Ok(mut t) = vol_label.single_mut() {
                    **t = format!("{pct:.0}%");
                }
            }
            SettingsButton::RenderDistInc | SettingsButton::RenderDistDec => {
                if let Some(ref mut wc) = world_config {
                    let new_chunks = step_render_distance_chunks(
                        wc.load_distance,
                        matches!(action, SettingsButton::RenderDistInc),
                    );
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
                    let new_pct = step_sensitivity_pct(
                        pc.sensitivity,
                        matches!(action, SettingsButton::SensitivityInc),
                    );
                    pc.sensitivity = new_pct as f32 / 100.0;
                    // `Player::sensitivity` is copied from the config at spawn
                    // and is what mouse look reads, so update live players too;
                    // otherwise the change only applied after a restart.
                    for mut player in &mut players {
                        player.sensitivity = pc.sensitivity;
                    }
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

// ─── Slider steps ─────────────────────────────────────────────────────────────
//
// Pure helpers behind the settings buttons, kept separate from the Bevy system
// so the rules (step size, limits, rounding) are unit-testable.

/// Next field of view in degrees: ±5°, clamped to `50`–`120`.
fn step_fov_degrees(current_deg: f32, increase: bool) -> f32 {
    let delta = if increase { 5.0 } else { -5.0 };
    (current_deg + delta).clamp(50.0, 120.0)
}

/// Next SFX volume: ±0.1, clamped to `0.0`–`1.0`.
fn step_volume(current: f32, increase: bool) -> f32 {
    let delta = if increase { 0.1 } else { -0.1 };
    (current + delta).clamp(0.0, 1.0)
}

/// Next render distance in chunks: the current `load_distance` is rounded to
/// whole chunks, clamped to `4`–`32` and bumped to even, then moved ±2 chunks
/// and clamped again.
fn step_render_distance_chunks(load_distance: f32, increase: bool) -> i32 {
    let current = ((load_distance / 16.0).round() as i32).clamp(4, 32);
    let current = if current % 2 == 0 {
        current
    } else {
        current + 1
    };
    let delta = if increase { 2 } else { -2 };
    (current + delta).clamp(4, 32)
}

/// Next mouse sensitivity in percent: ±1%, clamped to `10`–`100`.
fn step_sensitivity_pct(sensitivity: f32, increase: bool) -> i32 {
    let current_pct = (sensitivity * 100.0).round() as i32;
    let delta = if increase { 1 } else { -1 };
    (current_pct + delta).clamp(10, 100)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use bevy::ecs::system::RunSystemOnce;

    use crate::player::player_cursor_system;
    use bevy::state::app::StatesPlugin;
    use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

    /// Headless app with the pause transitions wired as [`PauseMenuPlugin`]
    /// does (minus UI and assets) plus the player's cursor system, and a
    /// focused primary window with a free cursor.
    fn pause_cursor_app() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(StatesPlugin);
        app.init_state::<PauseMenuState>();
        app.add_message::<SfxRequest>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<ButtonInput<MouseButton>>();
        app.add_systems(
            Update,
            (
                toggle_pause.run_if(in_state(PauseMenuState::Playing)),
                escape_to_resume.run_if(in_state(PauseMenuState::Paused)),
                player_cursor_system.run_if(in_state(PauseMenuState::Playing)),
            ),
        );
        app.add_systems(OnEnter(PauseMenuState::Paused), release_cursor);
        let window = app
            .world_mut()
            .spawn((
                Window {
                    focused: true,
                    ..default()
                },
                PrimaryWindow,
            ))
            .id();
        (app, window)
    }

    /// Presses `key` for one frame, then runs a second frame so the state
    /// transition it requested is applied.
    fn tap(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.update();
    }

    fn click(app: &mut App) {
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .reset_all();
    }

    fn pause_state(app: &App) -> PauseMenuState {
        app.world()
            .resource::<State<PauseMenuState>>()
            .get()
            .clone()
    }

    fn grab(app: &App, window: Entity) -> CursorGrabMode {
        app.world()
            .get::<CursorOptions>(window)
            .expect("window should still exist")
            .grab_mode
    }

    #[test]
    fn cursor_follows_pause_focus_and_click_with_the_player_cursor_system() {
        let (mut app, window) = pause_cursor_app();

        // Escape before the first click opens the menu; it must not close the window.
        tap(&mut app, KeyCode::Escape);
        assert_eq!(pause_state(&app), PauseMenuState::Paused);
        assert_eq!(grab(&app, window), CursorGrabMode::None);

        // Escape again resumes with the cursor captured.
        tap(&mut app, KeyCode::Escape);
        assert_eq!(pause_state(&app), PauseMenuState::Playing);
        assert_eq!(grab(&app, window), CursorGrabMode::Locked);

        // Losing focus releases the cursor without pausing.
        app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
        app.update();
        assert_eq!(grab(&app, window), CursorGrabMode::None);
        assert_eq!(pause_state(&app), PauseMenuState::Playing);

        // Regaining focus alone does not recapture; a click does.
        app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
        app.update();
        assert_eq!(grab(&app, window), CursorGrabMode::None);
        click(&mut app);
        assert_eq!(grab(&app, window), CursorGrabMode::Locked);

        // Pausing from a captured cursor releases it for the menu.
        tap(&mut app, KeyCode::Escape);
        assert_eq!(pause_state(&app), PauseMenuState::Paused);
        assert_eq!(grab(&app, window), CursorGrabMode::None);
    }

    #[test]
    fn resuming_does_not_capture_an_unfocused_window() {
        let (mut app, window) = pause_cursor_app();
        tap(&mut app, KeyCode::Escape);
        assert_eq!(pause_state(&app), PauseMenuState::Paused);
        app.world_mut().get_mut::<Window>(window).unwrap().focused = false;

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        // Checked before the next frame, where the player's cursor system
        // would release an unfocused capture anyway.
        assert_eq!(grab(&app, window), CursorGrabMode::None);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.update();

        assert_eq!(pause_state(&app), PauseMenuState::Playing);
        assert_eq!(grab(&app, window), CursorGrabMode::None);
    }

    #[test]
    fn pause_cursor_changes_only_touch_the_primary_window() {
        let (mut app, window) = pause_cursor_app();
        let secondary = app
            .world_mut()
            .spawn((
                Window {
                    focused: false,
                    ..default()
                },
                CursorOptions {
                    grab_mode: CursorGrabMode::Confined,
                    ..default()
                },
            ))
            .id();

        tap(&mut app, KeyCode::Escape);
        tap(&mut app, KeyCode::Escape);

        assert_eq!(grab(&app, window), CursorGrabMode::Locked);
        assert_eq!(grab(&app, secondary), CursorGrabMode::Confined);
    }

    #[test]
    fn fov_steps_by_five_degrees_within_limits() {
        assert_eq!(step_fov_degrees(70.0, true), 75.0);
        assert_eq!(step_fov_degrees(70.0, false), 65.0);
        assert_eq!(step_fov_degrees(118.0, true), 120.0);
        assert_eq!(step_fov_degrees(52.0, false), 50.0);
    }

    #[test]
    fn volume_steps_by_a_tenth_within_limits() {
        assert!((step_volume(0.5, true) - 0.6).abs() < 1e-6);
        assert!((step_volume(0.5, false) - 0.4).abs() < 1e-6);
        assert_eq!(step_volume(0.95, true), 1.0);
        assert_eq!(step_volume(0.05, false), 0.0);
    }

    #[test]
    fn render_distance_steps_by_two_chunks_within_limits() {
        // Default load distance is 150 units ≈ 9.4 chunks → 9 → bumped to 10.
        assert_eq!(step_render_distance_chunks(150.0, true), 12);
        assert_eq!(step_render_distance_chunks(150.0, false), 8);
        assert_eq!(step_render_distance_chunks(8.0 * 16.0, true), 10);
        assert_eq!(step_render_distance_chunks(32.0 * 16.0, true), 32);
        assert_eq!(step_render_distance_chunks(4.0 * 16.0, false), 4);
        // Values outside the slider range are clamped first.
        assert_eq!(step_render_distance_chunks(1000.0 * 16.0, false), 30);
    }

    #[test]
    fn sensitivity_steps_by_one_percent_within_limits() {
        assert_eq!(step_sensitivity_pct(1.0, false), 99);
        assert_eq!(step_sensitivity_pct(1.0, true), 100);
        assert_eq!(step_sensitivity_pct(0.5, true), 51);
        assert_eq!(step_sensitivity_pct(0.1, false), 10);
    }

    /// World with the resources `handle_settings_buttons` needs and one
    /// pressed button.
    fn settings_world(pressed: SettingsButton) -> World {
        let mut world = World::new();
        world.insert_resource(SfxConfig::default());
        world.init_resource::<NextState<PauseMenuState>>();
        world.init_resource::<Messages<SaveSettingsRequest>>();
        world.spawn((
            Button,
            Interaction::Pressed,
            BackgroundColor(BTN_NORMAL),
            pressed,
        ));
        world
    }

    fn press(world: &mut World) {
        world
            .run_system_once(handle_settings_buttons)
            .expect("handle_settings_buttons should run");
    }

    #[test]
    fn sensitivity_button_updates_config_and_live_players() {
        let mut world = settings_world(SettingsButton::SensitivityDec);
        world.insert_resource(PlayerConfig::default());
        let player = world.spawn(Player::default()).id();
        assert_eq!(world.get::<Player>(player).unwrap().sensitivity, 1.0);

        press(&mut world);

        assert!((world.resource::<PlayerConfig>().sensitivity - 0.99).abs() < 1e-6);
        assert!(
            (world.get::<Player>(player).unwrap().sensitivity - 0.99).abs() < 1e-6,
            "the live player must pick up the new sensitivity without a restart"
        );
    }

    #[test]
    fn volume_button_updates_sfx_config() {
        let mut world = settings_world(SettingsButton::VolumeDec);
        let before = world.resource::<SfxConfig>().volume;

        press(&mut world);

        let after = world.resource::<SfxConfig>().volume;
        assert!((after - (before - 0.1).clamp(0.0, 1.0)).abs() < 1e-6);
    }

    #[test]
    fn back_button_requests_save_and_returns_to_pause() {
        let mut world = settings_world(SettingsButton::Back);

        press(&mut world);

        let saves = world.resource::<Messages<SaveSettingsRequest>>();
        assert_eq!(saves.iter_current_update_messages().count(), 1);
        assert!(matches!(
            *world.resource::<NextState<PauseMenuState>>(),
            NextState::Pending(PauseMenuState::Paused)
        ));
    }
}
