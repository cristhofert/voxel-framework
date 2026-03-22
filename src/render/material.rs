//! Shared material and rendering components for voxel chunks.
//!
//! All chunks share a single `StandardMaterial` instance with vertex colors
//! enabled. This avoids creating duplicate materials for every chunk.

use bevy::prelude::*;
use bevy::math::bounding::Aabb3d;

/// Resource holding the shared material handle for all voxel chunks.
///
/// All chunks share a single `StandardMaterial` with vertex colors enabled.
/// The material is created during startup by [`setup_chunk_material`].
#[derive(Resource, Default)]
pub struct ChunkMaterial {
    /// Handle to the shared `StandardMaterial`. `None` until the startup system runs.
    pub handle: Option<Handle<StandardMaterial>>,
}

/// Startup system that creates the shared chunk material.
///
/// Reads configuration from [`ChunkMaterialConfig`] (if present) and allocates
/// a `StandardMaterial` in the asset store. The resulting handle is stored in
/// [`ChunkMaterial`] for use by chunk spawning systems.
pub fn setup_chunk_material(
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut chunk_material: ResMut<ChunkMaterial>,
    config: Res<ChunkMaterialConfig>,
) {
    let material = StandardMaterial {
        base_color: config.base_color,
        perceptual_roughness: config.perceptual_roughness,
        metallic: config.metallic,
        reflectance: config.reflectance,
        ..default()
    };
    chunk_material.handle = Some(materials.add(material));
}

/// Marker component indicating a chunk entity has been set up for rendering.
///
/// Inserted alongside `Mesh3d` and `MeshMaterial3d` to prevent duplicate
/// render setup on the same entity.
#[derive(Component)]
pub struct ChunkRendered;

/// Configuration for the chunk material.
///
/// Insert this resource before adding `RenderPlugin` to customize material
/// properties. If not inserted, sensible defaults are used (white base color,
/// high roughness, no metallic).
#[derive(Resource, Clone, Debug)]
pub struct ChunkMaterialConfig {
    /// Base color of the material. Vertex colors are multiplied by this value.
    pub base_color: Color,
    /// Perceptual roughness of the PBR material (0.0 = smooth, 1.0 = rough).
    pub perceptual_roughness: f32,
    /// Metallic factor (0.0 = dielectric, 1.0 = metal).
    pub metallic: f32,
    /// Reflectance at normal incidence for dielectric materials.
    pub reflectance: f32,
}

impl Default for ChunkMaterialConfig {
    fn default() -> Self {
        Self {
            base_color: Color::WHITE,
            perceptual_roughness: 0.9,
            metallic: 0.0,
            reflectance: 0.1,
        }
    }
}

/// Creates an axis-aligned bounding box sized for a standard voxel chunk.
///
/// The AABB is centered at half the chunk extent along each axis, covering
/// the full 16x16x16 voxel region. This enables Bevy's built-in frustum
/// culling to skip off-screen chunks.
pub fn chunk_aabb() -> Aabb3d {
    use bevy::math::Vec3A;
    Aabb3d::new(Vec3A::splat(8.0), Vec3A::splat(8.0))
}
