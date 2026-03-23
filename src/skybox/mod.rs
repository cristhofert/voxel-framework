//! Configurable skybox system for the voxel framework.
//!
//! # Modes
//!
//! Two display modes are available through [`SkyboxMode`]:
//!
//! * **`Starfield`** (default) — spawns a procedural field of unlit cube-shaped
//!   stars distributed on a large sphere using a golden-angle sunflower spiral.
//!   The background clear color is set to black automatically.
//! * **`Color`** — sets the Bevy [`ClearColor`] to a solid color and spawns no
//!   geometry.  Use this for day-sky tints or minimalist environments.
//!
//! # Render pipeline integration
//!
//! The starfield is composed of standard Bevy `Mesh3d` + `MeshMaterial3d`
//! entities marked with [`SkyboxRoot`].  All star meshes are parented to a
//! single root entity so they move with the scene.  Because the star material
//! has `unlit: true`, the starfield is unaffected by scene lighting and remains
//! visible even in completely dark environments.
//!
//! For the `Color` mode, no entities are spawned; [`ClearColor`] is simply
//! overwritten as a resource during the `Startup` schedule.
//!
//! # Integration
//!
//! ```no_run
//! use bevy::prelude::*;
//! use voxel_framework::skybox::{SkyboxConfig, SkyboxMode, SkyboxPlugin, StarfieldConfig};
//!
//! fn main() {
//!     App::new()
//!         .insert_resource(SkyboxConfig {
//!             mode: SkyboxMode::Starfield(StarfieldConfig {
//!                 count: 1200,
//!                 radius: 900.0,
//!                 star_size: 0.4,
//!                 color: Color::WHITE,
//!             }),
//!         })
//!         .add_plugins(SkyboxPlugin)
//!         .run();
//! }
//! ```

use bevy::prelude::*;

// ─── Starfield configuration ──────────────────────────────────────────────────

/// Visual parameters for the procedural starfield rendered by
/// [`SkyboxMode::Starfield`].
///
/// Stars are distributed using a golden-angle sunflower spiral on a sphere of
/// radius [`radius`](StarfieldConfig::radius).  Each star is a tiny unlit cube
/// of side length [`star_size`](StarfieldConfig::star_size).
#[derive(Debug, Clone)]
pub struct StarfieldConfig {
    /// Total number of star entities spawned.
    ///
    /// Higher values produce a denser sky at the cost of more draw calls.
    /// Default: `800`.
    pub count: u32,
    /// Radius of the sphere on which stars are placed, in world units.
    ///
    /// This should be large enough to always surround the camera but smaller
    /// than the far clipping plane of the active camera.  Default: `900.0`.
    pub radius: f32,
    /// Side length of each star cube, in world units.
    ///
    /// Smaller values look more point-like; larger values produce visible
    /// polygonal stars.  Default: `0.5`.
    pub star_size: f32,
    /// Base color applied to all star meshes via an unlit [`StandardMaterial`].
    ///
    /// Because the material is unlit, scene lighting has no effect on this
    /// color.  Default: [`Color::WHITE`].
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

/// Selects how the sky background is rendered.
///
/// Set this inside [`SkyboxConfig`] before adding [`SkyboxPlugin`].  Changing
/// the mode at runtime requires re-spawning the skybox manually, as the setup
/// system only runs at `Startup`.
#[derive(Debug, Clone)]
pub enum SkyboxMode {
    /// Render a procedural starfield composed of small unlit cube meshes.
    ///
    /// The Bevy [`ClearColor`] is set to black automatically so the space
    /// between stars appears dark.  Appearance is controlled by the inner
    /// [`StarfieldConfig`].
    Starfield(StarfieldConfig),
    /// Fill the background with a solid color by setting [`ClearColor`].
    ///
    /// No geometry is spawned.  Use this for day-sky tints, sunset gradients
    /// baked into a single color, or any environment where star geometry is
    /// not needed.
    Color(Color),
    // Note: an HDR cubemap / asset mode is intentionally omitted here because
    // it requires the "bevy_asset" and image features which this crate does not
    // enable by default.
}

impl Default for SkyboxMode {
    fn default() -> Self {
        SkyboxMode::Starfield(StarfieldConfig::default())
    }
}

// ─── SkyboxConfig resource ────────────────────────────────────────────────────

/// Top-level configuration resource for the skybox system.
///
/// Insert a customized instance before adding [`SkyboxPlugin`] to override the
/// default starfield, or rely on [`Default`] to get an 800-star white
/// starfield.
///
/// # Examples
///
/// ```no_run
/// use bevy::prelude::*;
/// use voxel_framework::skybox::{SkyboxConfig, SkyboxMode};
///
/// // Solid-color sky (no star geometry spawned).
/// let sky = SkyboxConfig { mode: SkyboxMode::Color(Color::srgb(0.5, 0.7, 1.0)) };
/// ```
#[derive(Resource, Debug, Clone, Default)]
pub struct SkyboxConfig {
    /// The rendering mode to use.  Defaults to
    /// `SkyboxMode::Starfield(StarfieldConfig::default())`.
    pub mode: SkyboxMode,
}

// ─── Marker component ─────────────────────────────────────────────────────────

/// Marker component attached to the skybox root entity.
///
/// When [`SkyboxMode::Starfield`] is active, all star mesh entities are
/// parented to a single entity carrying this component.  Query for `SkyboxRoot`
/// to locate or despawn the entire starfield in one operation.
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

/// Plugin that enables the skybox system.
///
/// # What it registers
///
/// * [`SkyboxConfig`] — initialized as a default resource (starfield with 800
///   white stars).  Insert a pre-populated instance before adding this plugin
///   to override the defaults.
/// * A `Startup` system that reads [`SkyboxConfig`] and either spawns the
///   procedural starfield geometry or sets [`ClearColor`], depending on
///   [`SkyboxMode`].
///
/// # Examples
///
/// ```no_run
/// use bevy::prelude::*;
/// use voxel_framework::skybox::{SkyboxConfig, SkyboxMode, SkyboxPlugin, StarfieldConfig};
///
/// App::new()
///     .insert_resource(SkyboxConfig {
///         mode: SkyboxMode::Color(Color::srgb(0.1, 0.1, 0.2)),
///     })
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
