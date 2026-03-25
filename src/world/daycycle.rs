//! Procedural day/night cycle plugin for voxel worlds.
//!
//! [`DayCyclePlugin`] manages a single directional "sun" light whose
//! illuminance and angle are updated every frame based on the current
//! time of day.  Ambient light brightness also tracks the sun elevation
//! so the world darkens naturally at night.
//!
//! # Integration
//!
//! ```no_run
//! use bevy::prelude::*;
//! use voxel_framework::world::daycycle::{DayCycleConfig, DayCyclePlugin};
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .insert_resource(DayCycleConfig {
//!         day_length_secs: 300.0, // 5-minute days
//!         ..Default::default()
//!     })
//!     .add_plugins(DayCyclePlugin);
//! ```

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::prelude::*;

// ─── Resource ─────────────────────────────────────────────────────────────────

/// Configuration and runtime state for the day/night cycle.
///
/// Insert this resource before adding [`DayCyclePlugin`] to override the
/// defaults, or mutate it at runtime to skip time, set a starting hour, etc.
///
/// `time_of_day` advances automatically: `0.0` = midnight, `0.25` = dawn,
/// `0.5` = noon, `0.75` = dusk, `1.0` wraps back to midnight.
#[derive(Resource, Debug, Clone)]
pub struct DayCycleConfig {
    /// Length of one full day/night cycle in seconds.
    ///
    /// **Default: `600.0`** (10 minutes per day).
    pub day_length_secs: f32,
    /// Current time of day in the range `[0.0, 1.0)`.
    ///
    /// `0.0` = midnight, `0.5` = noon.  Wraps at `1.0`.
    /// **Default: `0.25`** (starts at dawn for a good first impression).
    pub time_of_day: f32,
    /// Maximum sun illuminance in lux, reached at solar noon.
    ///
    /// **Default: `100_000.0`.**
    pub max_illuminance: f32,
    /// Ambient light brightness during full daylight (sun at zenith).
    ///
    /// **Default: `800.0`.**
    pub day_ambient: f32,
    /// Ambient light brightness at night (no direct sun).
    ///
    /// **Default: `200.0`.**
    pub night_ambient: f32,
}

impl Default for DayCycleConfig {
    fn default() -> Self {
        Self {
            day_length_secs: 600.0,
            time_of_day: 0.35, // start mid-morning — sun is visible and lit
            max_illuminance: 100_000.0,
            day_ambient: 800.0,
            night_ambient: 200.0,
        }
    }
}

// ─── Marker ───────────────────────────────────────────────────────────────────

/// Marker component placed on the directional light managed by [`DayCyclePlugin`].
#[derive(Component)]
pub struct SunLight;

// ─── Systems ──────────────────────────────────────────────────────────────────

fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        SunLight,
        DirectionalLight {
            illuminance: 80_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 0.3, -1.0, 0.0)),
    ));
}

fn advance_day_cycle(
    time: Res<Time>,
    mut config: ResMut<DayCycleConfig>,
    mut sun_q: Query<(&mut DirectionalLight, &mut Transform), With<SunLight>>,
    mut clear_color: ResMut<ClearColor>,
) {
    // Advance time.
    config.time_of_day += time.delta_secs() / config.day_length_secs;
    config.time_of_day %= 1.0;

    // Map time_of_day to a sun elevation angle.
    // sin(TAU * t - PI/2): -1 at midnight (t=0), +1 at noon (t=0.5).
    let elevation = (TAU * config.time_of_day - FRAC_PI_2).sin();

    // t_day: 0 at horizon, 1 at zenith.  Add a soft floor so dawn/dusk is
    // never pitch-black — night_ambient / max_illuminance gives a moonlight
    // equivalent expressed as a fraction of full sun.
    let night_fraction = config.night_ambient / config.max_illuminance;
    let t_day = elevation.max(-night_fraction).max(0.0) + night_fraction;
    let illuminance = t_day * config.max_illuminance;

    // Sky/background color: dark blue at night → light blue at day.
    let t_sky = ((elevation + 1.0) * 0.5).clamp(0.0, 1.0);
    let sky_r = 0.05 + t_sky * 0.48;
    let sky_g = 0.05 + t_sky * 0.68;
    let sky_b = 0.15 + t_sky * 0.75;
    clear_color.0 = Color::srgb(sky_r, sky_g, sky_b);

    // Sun pitch: overhead at noon, below horizon at night.
    let sun_yaw = std::f32::consts::FRAC_PI_4;
    let sun_pitch = -(elevation * FRAC_PI_2 * 0.9);

    for (mut light, mut transform) in sun_q.iter_mut() {
        light.illuminance = illuminance;
        transform.rotation = Quat::from_euler(EulerRot::YXZ, sun_yaw, sun_pitch, 0.0);
    }
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Plugin that adds a procedural day/night cycle.
///
/// On startup, spawns a [`DirectionalLight`] tagged [`SunLight`] and advances
/// its illuminance and angle every frame according to [`DayCycleConfig`].
/// The sun illuminance uses a soft floor (`night_ambient / max_illuminance`)
/// so the scene is never completely dark even when the sun is below the horizon.
///
/// Configure via [`DayCycleConfig`] inserted before this plugin. The
/// `time_of_day` field advances automatically and wraps at `1.0`.
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use voxel_framework::world::daycycle::{DayCycleConfig, DayCyclePlugin};
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .insert_resource(DayCycleConfig {
///         day_length_secs: 120.0, // 2-minute days
///         time_of_day: 0.3,       // start mid-morning
///         ..Default::default()
///     })
///     .add_plugins(DayCyclePlugin);
/// ```
pub struct DayCyclePlugin;

impl Plugin for DayCyclePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DayCycleConfig>();
        app.add_systems(Startup, spawn_sun);
        app.add_systems(Update, advance_day_cycle);
    }
}
