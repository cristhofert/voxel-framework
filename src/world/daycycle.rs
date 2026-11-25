//! Procedural day/night cycle plugin for voxel worlds.
//!
//! [`DayCyclePlugin`] manages a single directional "sun" light whose
//! illuminance and angle are updated every frame based on the current
//! time of day.  The [`GlobalAmbientLight`] brightness also tracks the sun
//! elevation (from `night_ambient` to `day_ambient`) so the world darkens
//! naturally at night.
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

use bevy::light::GlobalAmbientLight;
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
    /// Length of one full day/night cycle in seconds. A value `<= 0.0`
    /// freezes the clock at the current `time_of_day`.
    ///
    /// **Default: `600.0`** (10 minutes per day).
    pub day_length_secs: f32,
    /// Current time of day in the range `[0.0, 1.0)`.
    ///
    /// `0.0` = midnight, `0.5` = noon.  Wraps at `1.0`.
    /// **Default: `0.35`** (starts mid-morning, with the sun up and lit).
    pub time_of_day: f32,
    /// Maximum sun illuminance in lux, reached at solar noon.
    ///
    /// **Default: `100_000.0`.**
    pub max_illuminance: f32,
    /// [`GlobalAmbientLight`] brightness during full daylight (sun at zenith).
    /// Interpolated from `night_ambient` as the sun rises.
    ///
    /// **Default: `800.0`.**
    pub day_ambient: f32,
    /// [`GlobalAmbientLight`] brightness at night (sun below the horizon).
    /// Also sets the moonlight floor of the sun's illuminance.
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
    ambient: Option<ResMut<GlobalAmbientLight>>,
) {
    config.time_of_day = advance_time_of_day(
        config.time_of_day,
        time.delta_secs(),
        config.day_length_secs,
    );
    let sun = sun_at(config.time_of_day, &config);

    clear_color.0 = sun.sky_color;
    if let Some(mut ambient) = ambient {
        ambient.brightness = sun.ambient_brightness;
    }
    for (mut light, mut transform) in sun_q.iter_mut() {
        light.illuminance = sun.illuminance;
        transform.rotation = sun.rotation;
    }
}

/// Moves `time_of_day` forward by `delta_secs` and wraps it into `[0.0, 1.0)`.
///
/// A non-positive (or non-finite) `day_length_secs` freezes the clock instead
/// of producing `inf`/`NaN`, which would otherwise leave the sun and sky
/// permanently broken.
fn advance_time_of_day(time_of_day: f32, delta_secs: f32, day_length_secs: f32) -> f32 {
    if !(day_length_secs > 0.0 && day_length_secs.is_finite()) {
        return time_of_day.rem_euclid(1.0);
    }
    (time_of_day + delta_secs / day_length_secs).rem_euclid(1.0)
}

/// Sun light and sky values for a given time of day.
#[derive(Debug, Clone, Copy)]
struct SunState {
    illuminance: f32,
    ambient_brightness: f32,
    sky_color: Color,
    rotation: Quat,
}

/// Pure mapping from `time_of_day` to sun illuminance, sky color and sun
/// rotation. Kept separate from the system so it can be unit-tested.
fn sun_at(time_of_day: f32, config: &DayCycleConfig) -> SunState {
    // Map time_of_day to a sun elevation angle.
    // sin(TAU * t - PI/2): -1 at midnight (t=0), +1 at noon (t=0.5).
    let elevation = (TAU * time_of_day - FRAC_PI_2).sin();

    // t_day: 0 at horizon, 1 at zenith.  Add a soft floor so dawn/dusk is
    // never pitch-black — night_ambient / max_illuminance gives a moonlight
    // equivalent expressed as a fraction of full sun.
    let night_fraction = config.night_ambient / config.max_illuminance;
    let t_day = elevation.max(0.0) + night_fraction;
    let illuminance = t_day * config.max_illuminance;

    // Ambient light: night_ambient while the sun is below the horizon, rising
    // to day_ambient with the sun at its zenith.
    let ambient_brightness =
        config.night_ambient + (config.day_ambient - config.night_ambient) * elevation.max(0.0);

    // Sky/background color: dark blue at night → light blue at day.
    let t_sky = ((elevation + 1.0) * 0.5).clamp(0.0, 1.0);
    let sky_color = Color::srgb(
        0.05 + t_sky * 0.48,
        0.05 + t_sky * 0.68,
        0.15 + t_sky * 0.75,
    );

    // Sun pitch: overhead at noon, below horizon at night.
    let sun_yaw = std::f32::consts::FRAC_PI_4;
    let sun_pitch = -(elevation * FRAC_PI_2 * 0.9);
    let rotation = Quat::from_euler(EulerRot::YXZ, sun_yaw, sun_pitch, 0.0);

    SunState {
        illuminance,
        ambient_brightness,
        sky_color,
        rotation,
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

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use std::time::Duration;

    const MIDNIGHT: f32 = 0.0;
    const DAWN: f32 = 0.25;
    const NOON: f32 = 0.5;

    fn luminance(c: Color) -> f32 {
        let s = c.to_srgba();
        s.red + s.green + s.blue
    }

    #[test]
    fn time_advances_by_fraction_of_day_length() {
        let t = advance_time_of_day(0.25, 60.0, 600.0);
        assert!((t - 0.35).abs() < 1e-6);
    }

    #[test]
    fn time_wraps_into_zero_one() {
        assert!((advance_time_of_day(0.9, 120.0, 600.0) - 0.1).abs() < 1e-5);
        // A game that sets a negative time gets a valid one back.
        assert!((advance_time_of_day(-0.25, 0.0, 600.0) - 0.75).abs() < 1e-6);
    }

    #[test]
    fn zero_or_invalid_day_length_freezes_time_instead_of_nan() {
        for len in [0.0, -10.0, f32::NAN, f32::INFINITY] {
            let t = advance_time_of_day(0.4, 1.0 / 60.0, len);
            assert_eq!(t, 0.4, "day_length_secs = {len}");
        }
    }

    #[test]
    fn night_keeps_a_moonlight_floor() {
        let cfg = DayCycleConfig::default();
        let night = sun_at(MIDNIGHT, &cfg);
        assert!((night.illuminance - cfg.night_ambient).abs() < 1e-2);
        // Dawn: sun on the horizon, still only the floor.
        assert!((sun_at(DAWN, &cfg).illuminance - cfg.night_ambient).abs() < 1.0);
    }

    #[test]
    fn ambient_goes_from_night_to_day_value() {
        let cfg = DayCycleConfig::default();
        assert!((sun_at(MIDNIGHT, &cfg).ambient_brightness - cfg.night_ambient).abs() < 1e-3);
        assert!((sun_at(DAWN, &cfg).ambient_brightness - cfg.night_ambient).abs() < 1e-2);
        assert!((sun_at(NOON, &cfg).ambient_brightness - cfg.day_ambient).abs() < 1e-2);
        let mid_morning = sun_at(0.35, &cfg).ambient_brightness;
        assert!(mid_morning > cfg.night_ambient && mid_morning < cfg.day_ambient);
    }

    #[test]
    fn noon_is_the_brightest_moment() {
        let cfg = DayCycleConfig::default();
        let noon = sun_at(NOON, &cfg);
        assert!(noon.illuminance >= cfg.max_illuminance);
        for i in 0..100 {
            let t = i as f32 / 100.0;
            let s = sun_at(t, &cfg);
            assert!(s.illuminance <= noon.illuminance + 1e-3, "t = {t}");
            assert!(luminance(s.sky_color) <= luminance(noon.sky_color) + 1e-5);
        }
    }

    #[test]
    fn morning_gets_brighter_until_noon() {
        let cfg = DayCycleConfig::default();
        let mut prev = sun_at(DAWN, &cfg).illuminance;
        for i in 1..=25 {
            let t = DAWN + i as f32 / 100.0;
            let now = sun_at(t, &cfg).illuminance;
            assert!(now > prev, "illuminance should rise at t = {t}");
            prev = now;
        }
    }

    #[test]
    fn sun_points_down_at_noon_and_up_at_midnight() {
        let cfg = DayCycleConfig::default();
        // Directional lights shine along their local -Z.
        let noon_dir = sun_at(NOON, &cfg).rotation * Vec3::NEG_Z;
        let night_dir = sun_at(MIDNIGHT, &cfg).rotation * Vec3::NEG_Z;
        assert!(
            noon_dir.y < -0.8,
            "noon light should shine downward: {noon_dir}"
        );
        assert!(
            night_dir.y > 0.8,
            "midnight sun is below the horizon: {night_dir}"
        );
    }

    #[test]
    fn system_advances_time_and_updates_sun_and_sky() {
        let mut world = World::new();
        let mut time = Time::<()>::default();
        time.advance_by(Duration::from_secs(150)); // a quarter of the default day
        world.insert_resource(time);
        world.insert_resource(DayCycleConfig {
            time_of_day: DAWN,
            ..Default::default()
        });
        world.insert_resource(ClearColor(Color::BLACK));
        world.insert_resource(GlobalAmbientLight::default());
        let sun = world
            .spawn((SunLight, DirectionalLight::default(), Transform::default()))
            .id();

        world
            .run_system_once(advance_day_cycle)
            .expect("advance_day_cycle should run");

        let cfg = world.resource::<DayCycleConfig>().clone();
        assert!((cfg.time_of_day - NOON).abs() < 1e-5);
        let expected = sun_at(NOON, &cfg);
        let light = world.get::<DirectionalLight>(sun).unwrap();
        assert!((light.illuminance - expected.illuminance).abs() < 1e-2);
        assert_eq!(world.resource::<ClearColor>().0, expected.sky_color);
        let ambient = world.resource::<GlobalAmbientLight>().brightness;
        assert!((ambient - cfg.day_ambient).abs() < 1e-2);
    }

    #[test]
    fn system_runs_without_an_ambient_light_resource() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(DayCycleConfig::default());
        world.insert_resource(ClearColor(Color::BLACK));

        world
            .run_system_once(advance_day_cycle)
            .expect("advance_day_cycle should run without GlobalAmbientLight");
    }
}
