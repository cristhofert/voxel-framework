//! World configuration loaded from a TOML file.
//!
//! [`WorldConfig`] is a Bevy [`Resource`] that controls seeding, streaming
//! distances, asteroid generation, and scene lighting. On startup,
//! [`WorldConfigPlugin`] attempts to read `world.toml` from the working
//! directory and falls back to [`Default`] values when the file is absent or
//! malformed.

use bevy::prelude::*;

use crate::generator::asteroid::{AsteroidConfig, AsteroidGenerator};
use crate::skybox::{SkyboxConfig, SkyboxMode, StarfieldConfig};
use crate::world::ChunkManager;

// ─── Config structs ───────────────────────────────────────────────────────────

/// Asteroid-generation parameters as read from TOML.
///
/// These are separate from [`crate::generator::asteroid::AsteroidConfig`] to
/// keep TOML deserialization decoupled from the runtime generator type (which
/// uses `u32` seeds and `usize` counts internally).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AsteroidConfigToml {
    /// Seed for deterministic asteroid placement.
    pub seed: u64,
    /// Number of asteroids to generate.
    pub count: u32,
    /// Minimum asteroid radius in world units.
    pub min_radius: f32,
    /// Maximum asteroid radius in world units.
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

/// Skybox parameters as read from TOML.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SkyboxConfigToml {
    /// "starfield" or "color". Default: "starfield"
    pub mode: String,
    /// Number of stars (used when mode = "starfield"). Default: 800
    pub star_count: u32,
    /// Distance of stars from origin (used when mode = "starfield"). Default: 900.0
    pub star_radius: f32,
    /// Background color as RGBA (used when mode = "color"). Default: None
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

/// Scene lighting parameters.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LightingConfig {
    /// Illuminance of the directional sun light in lux.
    pub sun_illuminance: f32,
    /// Brightness of the ambient light.
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

/// Top-level world configuration resource.
///
/// Loaded from `world.toml` at startup by [`WorldConfigPlugin`]. Falls back to
/// [`Default`] when the file is absent or cannot be parsed.
///
/// # Example
///
/// ```rust,no_run
/// use voxel_framework::world::WorldConfig;
///
/// let config = WorldConfig::from_file("world.toml").unwrap_or_default();
/// println!("seed = {}", config.seed);
/// ```
#[derive(Resource, Debug, Clone, serde::Deserialize)]
pub struct WorldConfig {
    /// Global world seed used for deterministic generation.
    pub seed: u64,
    /// Player/viewer spawn position as `[x, y, z]`.
    pub spawn_position: [f32; 3],
    /// Radius (world units) within which chunks are loaded.
    pub load_distance: f32,
    /// Radius (world units) beyond which chunks are unloaded.
    pub unload_distance: f32,
    /// Asteroid generation parameters.
    pub asteroids: AsteroidConfigToml,
    /// Scene lighting parameters.
    pub lighting: LightingConfig,
    /// Skybox configuration.
    #[serde(default)]
    pub skybox: SkyboxConfigToml,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
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
    /// Build an [`AsteroidGenerator`] from the asteroid settings in this config.
    ///
    /// The TOML `seed` (`u64`) is truncated to `u32` and `count` (`u32`) is
    /// cast to `usize`. All other fields that are not present in
    /// [`AsteroidConfigToml`] are filled from [`AsteroidConfig::default()`].
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

    /// Loads a [`WorldConfig`] from a TOML file at `path`.
    ///
    /// Returns an error if the file cannot be read or the TOML is invalid.
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
/// If `world.toml` is absent or malformed, [`WorldConfig::default`] is used.
/// Either way, [`WorldConfig`] is inserted as a Bevy resource and the
/// [`ChunkManager`]'s streaming distances are updated to match.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::world::WorldConfigPlugin;
///
/// App::new()
///     .add_plugins(WorldConfigPlugin)
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
