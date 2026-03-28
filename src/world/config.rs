//! World configuration loaded from `world.toml` at startup.
//!
//! # Overview
//!
//! [`WorldConfig`] is a Bevy [`Resource`] that centralises every
//! data-driven parameter the engine needs at startup: the world seed,
//! streaming distances, asteroid generation settings, scene lighting, and
//! skybox mode.
//!
//! # Where it is read from
//!
//! [`WorldConfigPlugin`] runs a [`Startup`] system that calls
//! [`WorldConfig::from_file`] with the path `"world.toml"`, resolved relative
//! to the **process working directory**. For games launched via `cargo run`,
//! this is the workspace root. For distributed builds it is the directory
//! containing the executable.
//!
//! # Fallback behavior
//!
//! If `world.toml` is absent, unreadable, or contains invalid TOML,
//! [`WorldConfig::default`] is used and a diagnostic message is printed to
//! stderr. No panic occurs. The fallback values are documented on each field
//! of [`WorldConfig`].
//!
//! # When it is applied
//!
//! After loading (or falling back), the plugin inserts [`WorldConfig`] as a
//! Bevy [`Resource`] and copies `load_distance` / `unload_distance` into the
//! existing [`ChunkManager`] resource so the streaming system uses the
//! configured distances immediately.

use bevy::prelude::*;

use crate::generator::asteroid::{AsteroidConfig, AsteroidGenerator};
use crate::skybox::{SkyboxConfig, SkyboxMode, StarfieldConfig};
use crate::world::ChunkManager;

// ─── Config structs ───────────────────────────────────────────────────────────

/// Asteroid-generation parameters as read from the `[asteroids]` TOML section.
///
/// This is a serialization-only mirror of
/// [`crate::generator::asteroid::AsteroidConfig`], kept separate to avoid
/// coupling TOML deserialization to the runtime generator type (which uses
/// `u32` seeds and `usize` counts internally). Call
/// [`WorldConfig::build_asteroid_generator`] to convert this into an
/// [`crate::generator::asteroid::AsteroidGenerator`].
///
/// | Field | Default | Description |
/// |---|---|---|
/// | `seed` | `42` | `u64` seed truncated to `u32` for the PRNG |
/// | `count` | `20` | Number of asteroids to place |
/// | `min_radius` | `15.0` | Minimum asteroid radius in world units |
/// | `max_radius` | `40.0` | Maximum asteroid radius in world units |
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AsteroidConfigToml {
    /// Seed for deterministic asteroid placement. Truncated to `u32` at runtime.
    pub seed: u64,
    /// Number of asteroids to generate. Default: `20`.
    pub count: u32,
    /// Minimum asteroid radius in world units. Default: `15.0`.
    pub min_radius: f32,
    /// Maximum asteroid radius in world units. Default: `40.0`.
    pub max_radius: f32,
}

impl Default for AsteroidConfigToml {
    fn default() -> Self {
        Self {
            seed: 42,
            count: 20,
            min_radius: 15.0,
            max_radius: 40.0,
        }
    }
}

/// Skybox parameters as read from the `[skybox]` TOML section.
///
/// Converted into a [`crate::skybox::SkyboxConfig`] at startup by
/// [`WorldConfig::build_skybox_config`].
///
/// | Field | Default | Description |
/// |---|---|---|
/// | `mode` | `"starfield"` | `"starfield"` or `"color"` |
/// | `star_count` | `800` | Number of point-light stars (starfield mode only) |
/// | `star_radius` | `900.0` | Distance of stars from the origin in world units |
/// | `background_color` | `None` | RGBA clear color as `[r, g, b, a]` (color mode only) |
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SkyboxConfigToml {
    /// Skybox rendering mode: `"starfield"` or `"color"`. Default: `"starfield"`.
    pub mode: String,
    /// Number of stars rendered in starfield mode. Default: `800`.
    pub star_count: u32,
    /// Radius at which stars are placed from the world origin. Default: `900.0`.
    pub star_radius: f32,
    /// Clear color in `[r, g, b, a]` format, used only in `"color"` mode.
    /// Defaults to opaque black `[0.0, 0.0, 0.0, 1.0]` when `None`.
    pub background_color: Option<[f32; 4]>,
}

impl Default for SkyboxConfigToml {
    fn default() -> Self {
        Self {
            mode: "starfield".to_string(),
            star_count: 800,
            star_radius: 900.0,
            background_color: None,
        }
    }
}

/// Scene lighting parameters as read from the `[lighting]` TOML section.
///
/// | Field | Default | Description |
/// |---|---|---|
/// | `sun_illuminance` | `15000.0` | Directional sun light illuminance in lux |
/// | `ambient_brightness` | `0.1` | Global ambient light intensity (0 = dark, 1 = full) |
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LightingConfig {
    /// Illuminance of the directional sun light in lux. Default: `15000.0`.
    pub sun_illuminance: f32,
    /// Brightness of the scene's ambient light, in the range `[0.0, 1.0]`. Default: `0.1`.
    pub ambient_brightness: f32,
}

impl Default for LightingConfig {
    fn default() -> Self {
        Self {
            sun_illuminance: 15000.0,
            ambient_brightness: 0.1,
        }
    }
}

fn default_world_name() -> String { "world".to_string() }
fn default_seed() -> u64 { 42 }
fn default_load_distance() -> f32 { 150.0 }
fn default_unload_distance() -> f32 { 200.0 }

/// Top-level world configuration loaded from `world.toml`.
///
/// Inserted as a Bevy [`Resource`] by [`WorldConfigPlugin`] during the
/// [`Startup`] schedule. If `world.toml` is absent or malformed,
/// [`WorldConfig::default`] is used and an error is printed to stderr — no
/// panic occurs. See the [module docs](self) for the full fallback and
/// application flow.
///
/// After loading, call the builder helpers to construct runtime types:
/// - [`build_asteroid_generator`](WorldConfig::build_asteroid_generator) →
///   [`crate::generator::asteroid::AsteroidGenerator`]
/// - [`build_skybox_config`](WorldConfig::build_skybox_config) →
///   [`crate::skybox::SkyboxConfig`]
///
/// # Field defaults (when `world.toml` is absent)
///
/// | Field | Default |
/// |---|---|
/// | `world_name` | `"world"` |
/// | `seed` | `42` |
/// | `spawn_position` | `[0.0, 0.0, 0.0]` |
/// | `load_distance` | `150.0` |
/// | `unload_distance` | `200.0` |
/// | `asteroids` | see [`AsteroidConfigToml::default`] |
/// | `lighting` | see [`LightingConfig::default`] |
/// | `skybox` | see [`SkyboxConfigToml::default`] |
///
/// # Example
///
/// ```rust,no_run
/// use voxel_framework::world::WorldConfig;
///
/// let config = WorldConfig::from_file("world.toml").unwrap_or_default();
/// println!("world = {}", config.world_name);
/// println!("seed = {}", config.seed);
/// ```
#[derive(Resource, Debug, Clone, serde::Deserialize)]
pub struct WorldConfig {
    /// Name of this world, used as the folder under `worlds/` for chunk and
    /// player data storage.  Default: `"world"`.
    ///
    /// Set in the world TOML as `world_name = "survival"`.  The framework stores
    /// all data for this world under `./worlds/{world_name}/`.
    #[serde(default = "default_world_name")]
    pub world_name: String,
    /// Global world seed used for all deterministic generation. Default: `42`.
    #[serde(default = "default_seed")]
    pub seed: u64,
    /// Player/viewer spawn position in world space as `[x, y, z]`. Default: `[0.0, 0.0, 0.0]`.
    #[serde(default)]
    pub spawn_position: [f32; 3],
    /// Radius in world units within which chunks are loaded. Default: `150.0`.
    ///
    /// Copied into [`ChunkManager::load_distance`] by [`WorldConfigPlugin`].
    #[serde(default = "default_load_distance")]
    pub load_distance: f32,
    /// Radius in world units beyond which chunks are unloaded. Default: `200.0`.
    ///
    /// Must be greater than [`load_distance`](WorldConfig::load_distance) to
    /// maintain the hysteresis band. Copied into
    /// [`ChunkManager::unload_distance`] by [`WorldConfigPlugin`].
    #[serde(default = "default_unload_distance")]
    pub unload_distance: f32,
    /// Asteroid generation parameters from the `[asteroids]` section.
    #[serde(default)]
    pub asteroids: AsteroidConfigToml,
    /// Scene lighting parameters from the `[lighting]` section.
    #[serde(default)]
    pub lighting: LightingConfig,
    /// Skybox configuration from the `[skybox]` section. Defaults to a
    /// starfield when the section is absent.
    #[serde(default)]
    pub skybox: SkyboxConfigToml,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            world_name: "world".to_string(),
            seed: 42,
            spawn_position: [0.0, 0.0, 0.0],
            load_distance: 150.0,
            unload_distance: 200.0,
            asteroids: AsteroidConfigToml::default(),
            lighting: LightingConfig::default(),
            skybox: SkyboxConfigToml::default(),
        }
    }
}

impl WorldConfig {
    /// Constructs an [`AsteroidGenerator`] from the `[asteroids]` configuration.
    ///
    /// Bridges the TOML-friendly [`AsteroidConfigToml`] (which uses `u64` for
    /// the seed and `u32` for the count) to the runtime
    /// [`crate::generator::asteroid::AsteroidConfig`] type (which uses `u32`
    /// and `usize` respectively). The seed is truncated — values above
    /// `u32::MAX` wrap silently.
    ///
    /// Fields not present in [`AsteroidConfigToml`] (e.g. noise octaves) are
    /// filled from [`crate::generator::asteroid::AsteroidConfig::default`].
    ///
    /// Call this once during startup, typically inside the system that sets up
    /// the world generator:
    ///
    /// ```rust,no_run
    /// use bevy::prelude::*;
    /// use voxel_framework::world::WorldConfig;
    ///
    /// fn setup(config: Res<WorldConfig>) {
    ///     let generator = config.build_asteroid_generator();
    ///     // hand `generator` to your chunk generation system
    /// }
    /// ```
    pub fn build_asteroid_generator(&self) -> AsteroidGenerator {
        AsteroidGenerator::new(AsteroidConfig {
            seed: self.asteroids.seed as u32,
            asteroid_count: self.asteroids.count as usize,
            min_radius: self.asteroids.min_radius,
            max_radius: self.asteroids.max_radius,
            ..AsteroidConfig::default()
        })
    }

    /// Converts [`SkyboxConfigToml`] into a [`SkyboxConfig`] runtime resource.
    ///
    /// - `"starfield"` → [`SkyboxMode::Starfield`] using `star_count` and `star_radius`.
    /// - `"color"` → [`SkyboxMode::Color`] using `background_color` (falls back to black).
    /// - Any other value falls back to the default starfield.
    pub fn build_skybox_config(&self) -> SkyboxConfig {
        let mode = match self.skybox.mode.as_str() {
            "color" => {
                let [r, g, b, a] = self.skybox.background_color.unwrap_or([0.0, 0.0, 0.0, 1.0]);
                SkyboxMode::Color(Color::srgba(r, g, b, a))
            }
            _ => SkyboxMode::Starfield(StarfieldConfig {
                count: self.skybox.star_count,
                radius: self.skybox.star_radius,
                ..StarfieldConfig::default()
            }),
        };
        SkyboxConfig { mode }
    }

    /// Returns the base data directory for this world: `./worlds/{world_name}`.
    ///
    /// All world-specific files (chunks, player data) are stored under this path.
    pub fn world_data_dir(&self) -> String {
        format!("./worlds/{}", self.world_name)
    }

    /// Loads a [`WorldConfig`] from a TOML file at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error in either of these two situations:
    ///
    /// - **I/O failure** — the file does not exist, is not readable, or the
    ///   process lacks permission to open it.
    /// - **Parse failure** — the file content is not valid TOML, or a required
    ///   top-level field (`seed`, `spawn_position`, `load_distance`,
    ///   `unload_distance`, `asteroids`, `lighting`) is missing.
    ///
    /// The typical usage pattern is to fall back to [`Default`] on any error:
    ///
    /// ```rust,no_run
    /// use voxel_framework::world::WorldConfig;
    ///
    /// let config = WorldConfig::from_file("world.toml").unwrap_or_default();
    /// ```
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml_str(&content)
    }

    /// Parses a [`WorldConfig`] from a TOML string.
    ///
    /// Returns an error if the TOML is invalid or missing required fields.
    pub fn from_toml_str(s: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config: Self = toml::from_str(s)?;
        Ok(config)
    }
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Startup system that loads `WorldConfig` and applies it to [`ChunkManager`].
fn load_world_config(mut commands: Commands, mut chunk_manager: ResMut<ChunkManager>) {
    let config = WorldConfig::from_file("world.toml")
        .inspect_err(|e| {
            eprintln!("voxel-framework: world.toml not found or invalid ({e}), using default WorldConfig");
        })
        .unwrap_or_default();

    chunk_manager.load_distance = config.load_distance;
    chunk_manager.unload_distance = config.unload_distance;

    commands.insert_resource(config);
}

/// Plugin that loads [`WorldConfig`] from `world.toml` on startup.
///
/// Registers a [`Startup`] system (`load_world_config`) that:
///
/// 1. Calls [`WorldConfig::from_file`]`("world.toml")`.
/// 2. Falls back to [`WorldConfig::default`] and logs to stderr if the file is
///    absent or malformed.
/// 3. Inserts the resolved [`WorldConfig`] as a Bevy [`Resource`].
/// 4. Copies `load_distance` and `unload_distance` into the existing
///    [`ChunkManager`] resource so the streaming system honours them from the
///    very first [`Update`] tick.
///
/// Must be added **after** [`super::WorldPlugin`] (which inserts
/// [`ChunkManager`]):
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::world::{WorldPlugin, WorldConfigPlugin};
///
/// App::new()
///     .add_plugins((WorldPlugin, WorldConfigPlugin))
///     .run();
/// ```
pub struct WorldConfigPlugin;

impl Plugin for WorldConfigPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_world_config);
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_values() {
        let config = WorldConfig::default();
        assert_eq!(config.seed, 42);
        assert_eq!(config.spawn_position, [0.0, 0.0, 0.0]);
        assert_eq!(config.load_distance, 150.0);
        assert_eq!(config.unload_distance, 200.0);
        assert_eq!(config.asteroids.seed, 42);
        assert_eq!(config.asteroids.count, 20);
        assert_eq!(config.asteroids.min_radius, 15.0);
        assert_eq!(config.asteroids.max_radius, 40.0);
        assert_eq!(config.lighting.sun_illuminance, 15000.0);
        assert_eq!(config.lighting.ambient_brightness, 0.1);
    }

    #[test]
    fn test_from_str_parses_correctly() {
        let toml = r#"
seed = 99
spawn_position = [1.0, 2.0, 3.0]
load_distance = 120.0
unload_distance = 180.0

[asteroids]
seed = 7
count = 5
min_radius = 10.0
max_radius = 50.0

[lighting]
sun_illuminance = 20000.0
ambient_brightness = 0.2

[skybox]
mode = "starfield"
star_count = 800
star_radius = 900.0
"#;

        let config = WorldConfig::from_toml_str(toml).expect("TOML should parse without error");

        assert_eq!(config.seed, 99);
        assert_eq!(config.spawn_position, [1.0, 2.0, 3.0]);
        assert_eq!(config.load_distance, 120.0);
        assert_eq!(config.unload_distance, 180.0);
        assert_eq!(config.asteroids.seed, 7);
        assert_eq!(config.asteroids.count, 5);
        assert_eq!(config.asteroids.min_radius, 10.0);
        assert_eq!(config.asteroids.max_radius, 50.0);
        assert_eq!(config.lighting.sun_illuminance, 20000.0);
        assert_eq!(config.lighting.ambient_brightness, 0.2);
    }
}
