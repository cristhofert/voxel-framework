//! Configurable skybox system.
//!
//! Supports three modes: procedural starfield, solid background color,
//! or a custom asset image (HDR cubemap). Configure via [`SkyboxConfig`].
//! Add [`SkyboxPlugin`] to enable.

use bevy::prelude::*;

// ─── Starfield configuration ──────────────────────────────────────────────────

/// Configuration for the procedural starfield.
#[derive(Debug, Clone)]
pub struct StarfieldConfig {
    /// Number of star points. Default: 800
    pub count: u32,
    /// Distance from origin. Default: 900.0
    pub radius: f32,
    /// Star cube size. Default: 0.5
    pub star_size: f32,
    /// Star color. Default: Color::WHITE
    pub color: Color,
}

impl Default for StarfieldConfig {
    fn default() -> Self {
        Self {
            count: 800,
            radius: 900.0,
            star_size: 0.5,
            color: Color::WHITE,
        }
    }
}

// ─── SkyboxMode ───────────────────────────────────────────────────────────────

/// Skybox display mode.
#[derive(Debug, Clone)]
pub enum SkyboxMode {
    /// Procedural starfield (default).
    Starfield(StarfieldConfig),
    /// Solid background color.
    Color(Color),
    // Note: Asset mode requires bevy_asset + image features.
    // Omit for now — can be added when those features are enabled.
}

impl Default for SkyboxMode {
    fn default() -> Self {
        SkyboxMode::Starfield(StarfieldConfig::default())
    }
}

// ─── SkyboxConfig resource ────────────────────────────────────────────────────

/// Top-level skybox configuration resource.
#[derive(Resource, Debug, Clone, Default)]
pub struct SkyboxConfig {
    pub mode: SkyboxMode,
}

// ─── Marker component ─────────────────────────────────────────────────────────

/// Marker component on the skybox root entity.
#[derive(Component)]
pub struct SkyboxRoot;

// ─── Setup system ─────────────────────────────────────────────────────────────

/// Startup system that spawns the skybox based on [`SkyboxConfig`].
fn setup_skybox_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    config: Res<SkyboxConfig>,
) {
    match &config.mode {
        SkyboxMode::Starfield(cfg) => {
            commands.insert_resource(ClearColor(Color::BLACK));
            spawn_starfield(&mut commands, &mut meshes, &mut materials, cfg);
        }
        SkyboxMode::Color(color) => {
            commands.insert_resource(ClearColor(*color));
        }
    }
}

/// Spawns star entities distributed on a sphere using a golden-angle sunflower
/// spiral. Each star is a tiny unlit cube parented to a [`SkyboxRoot`] entity.
fn spawn_starfield(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    cfg: &StarfieldConfig,
) {
    let star_mesh = meshes.add(Cuboid::new(cfg.star_size, cfg.star_size, cfg.star_size));
    let star_material = materials.add(StandardMaterial {
        base_color: cfg.color,
        unlit: true,
        ..default()
    });

    let golden_angle: f32 = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());

    let mut children = Vec::with_capacity(cfg.count as usize);

    for i in 0..cfg.count {
        let t = i as f32 / cfg.count as f32;
        // Map t -> [-1, 1] for the cosine of the polar angle
        let cos_theta = 1.0 - 2.0 * t;
        let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
        let phi = golden_angle * i as f32;

        let x = cfg.radius * sin_theta * phi.cos();
        let y = cfg.radius * cos_theta;
        let z = cfg.radius * sin_theta * phi.sin();

        let child = commands
            .spawn((
                Mesh3d(star_mesh.clone()),
                MeshMaterial3d(star_material.clone()),
                Transform::from_translation(Vec3::new(x, y, z)),
            ))
            .id();
        children.push(child);
    }

    commands
        .spawn((SkyboxRoot, Transform::default(), Visibility::default()))
        .add_children(&children);
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Plugin that registers the skybox system.
///
/// Initialises [`SkyboxConfig`] as a resource (defaults to starfield) and
/// adds the startup system that spawns the skybox.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::skybox::{SkyboxPlugin, SkyboxConfig, SkyboxMode};
///
/// App::new()
///     .add_plugins(SkyboxPlugin)
///     .run();
/// ```
pub struct SkyboxPlugin;

impl Plugin for SkyboxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SkyboxConfig>();
        app.add_systems(Startup, setup_skybox_system);
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skybox_default_is_starfield() {
        let config = SkyboxConfig::default();
        assert!(
            matches!(config.mode, SkyboxMode::Starfield(_)),
            "default SkyboxMode should be Starfield"
        );
    }

    #[test]
    fn test_starfield_config_defaults() {
        let cfg = StarfieldConfig::default();
        assert_eq!(cfg.count, 800);
        assert_eq!(cfg.radius, 900.0);
        assert_eq!(cfg.star_size, 0.5);
    }
}
