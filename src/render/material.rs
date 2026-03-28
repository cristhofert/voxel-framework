//! Shared material and rendering components for voxel chunks.
//!
//! Provides two rendering paths for voxel chunks:
//!
//! 1. **Texture array path** (opt-in) — when [`ChunkTextureArray`] is inserted
//!    as a resource before the plugin initialises, chunks are rendered with
//!    [`ChunkMaterial`], a custom PBR material that samples a `texture_2d_array`.
//!    The embedded WGSL shader applies per-vertex AO and simple diffuse lighting.
//!
//! 2. **Fallback path** (default) — when [`ChunkTextureArray`] is absent, chunks
//!    use a shared [`bevy::pbr::StandardMaterial`] with vertex colors. This path
//!    requires no texture assets and is compatible with all `VoxelType` palettes.

use bevy::asset::RenderAssetUsages;
use bevy::math::bounding::Aabb3d;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MeshPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;

use crate::mesh::ATTRIBUTE_TEXTURE_LAYER;

// ─── ChunkMaterial (texture array path) ───────────────────────────────────────

/// Custom chunk material that samples a GPU `texture_2d_array`.
///
/// Register this via [`ChunkTextureArray`] before adding [`super::RenderPlugin`].
/// The embedded WGSL shader multiplies the sampled texture color by the
/// per-vertex AO value (stored in `ATTRIBUTE_COLOR`) and applies simple diffuse
/// lighting with a fixed sun direction.
///
/// # Shader bindings
///
/// | Binding | Name              | Type                  |
/// |---------|-------------------|-----------------------|
/// | 0       | `texture_array`   | `texture_2d_array<f32>` |
/// | 1       | `texture_sampler` | `sampler`             |
///
/// # Vertex attributes
///
/// | Location | Attribute              | Type       |
/// |----------|------------------------|------------|
/// | 0        | `ATTRIBUTE_POSITION`   | `Float32x3` |
/// | 1        | `ATTRIBUTE_NORMAL`     | `Float32x3` |
/// | 2        | `ATTRIBUTE_UV_0`       | `Float32x2` |
/// | 3        | `ATTRIBUTE_COLOR`      | `Float32x4` |
/// | 4        | `ATTRIBUTE_TEXTURE_LAYER` | `Uint32` |
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct ChunkMaterial {
    /// Handle to the assembled texture array image (all voxel layers stacked).
    #[texture(0, sample_type = "float", dimension = "2d_array")]
    #[sampler(1)]
    pub texture_array: Handle<Image>,
}

impl Material for ChunkMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://voxel_framework/render/chunk.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://voxel_framework/render/chunk.wgsl".into()
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // The prepass pipelines (depth, normal, motion-vector) use Bevy's
        // built-in prepass vertex shader, which has its own vertex attribute
        // expectations. Don't override the vertex buffer layout for them —
        // Bevy sets it up correctly. Our custom layout is only needed for the
        // main pass where chunk.wgsl's vertex function runs.
        let is_prepass = key.mesh_key.intersects(
            MeshPipelineKey::DEPTH_PREPASS
                | MeshPipelineKey::NORMAL_PREPASS
                | MeshPipelineKey::MOTION_VECTOR_PREPASS,
        );
        if is_prepass {
            return Ok(());
        }

        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(3),
            ATTRIBUTE_TEXTURE_LAYER.at_shader_location(4),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

// ─── ChunkTextureArray ────────────────────────────────────────────────────────

/// Optional resource that opts chunks into the texture-array rendering path.
///
/// Insert this resource before adding [`super::RenderPlugin`]. The plugin reads
/// it during startup to create a [`ChunkMaterial`] handle that is shared across
/// all chunk entities.
///
/// When this resource is absent the plugin falls back to [`bevy::pbr::StandardMaterial`]
/// with vertex colors, which requires no texture setup.
///
/// # Example
///
/// ```rust,no_run
/// // In your Bevy app setup, before adding VoxelCorePlugin / VoxelPlugin:
/// app.insert_resource(ChunkTextureArray {
///     handle: my_texture_array_handle,
/// });
/// ```
#[derive(Resource)]
pub struct ChunkTextureArray {
    /// Handle to a `texture_2d_array` [`Image`] containing one layer per voxel type.
    pub handle: Handle<Image>,
}

// ─── ChunkMaterialHandle ─────────────────────────────────────────────────────

/// Internal resource that tracks which material handle all chunk entities share.
///
/// Populated by [`setup_chunk_material`] during [`bevy::prelude::Startup`].
/// The `systems.rs` mesh system reads this to attach materials to chunk entities.
#[derive(Resource, Default)]
pub struct ChunkMaterialHandle {
    /// Handle to the shared `StandardMaterial` (fallback path). `None` until setup runs.
    pub standard: Option<Handle<StandardMaterial>>,
    /// Handle to the shared `ChunkMaterial` (texture-array path). `None` unless
    /// [`ChunkTextureArray`] was inserted before plugin setup.
    pub custom: Option<Handle<ChunkMaterial>>,
}

// ─── ChunkMaterialConfig ─────────────────────────────────────────────────────

/// Configuration for the fallback `StandardMaterial` chunk material.
///
/// Insert this resource before adding `RenderPlugin` to customise fallback
/// material properties. If absent, sensible defaults are used (white base color,
/// high roughness, no metallic).
///
/// This resource has no effect when [`ChunkTextureArray`] is present because
/// the texture-array path uses the embedded WGSL shader.
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

// ─── ChunkRendered ────────────────────────────────────────────────────────────

/// Marker component indicating a chunk entity has been set up for rendering.
///
/// Inserted alongside `Mesh3d` and `MeshMaterial3d` to prevent duplicate
/// render setup on the same entity.
#[derive(Component)]
pub struct ChunkRendered;

// ─── setup_chunk_material ────────────────────────────────────────────────────

/// Startup system that creates the shared chunk material.
///
/// If [`ChunkTextureArray`] is present, creates a [`ChunkMaterial`] backed by
/// the provided texture array. Otherwise creates a `StandardMaterial` with
/// vertex colors. The handle is stored in [`ChunkMaterialHandle`].
pub fn setup_chunk_material(
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    mut chunk_materials: ResMut<Assets<ChunkMaterial>>,
    mut handle: ResMut<ChunkMaterialHandle>,
    config: Res<ChunkMaterialConfig>,
    texture_array: Option<Res<ChunkTextureArray>>,
) {
    if let Some(tex) = texture_array {
        // Texture-array path — use the embedded WGSL shader.
        let mat = ChunkMaterial {
            texture_array: tex.handle.clone(),
        };
        handle.custom = Some(chunk_materials.add(mat));
    } else {
        // Fallback path — standard PBR material with vertex colors.
        let material = StandardMaterial {
            base_color: config.base_color,
            perceptual_roughness: config.perceptual_roughness,
            metallic: config.metallic,
            reflectance: config.reflectance,
            ..default()
        };
        handle.standard = Some(standard_materials.add(material));
    }
}

// ─── chunk_aabb ──────────────────────────────────────────────────────────────

/// Creates an axis-aligned bounding box sized for a standard voxel chunk.
///
/// The AABB is centered at half the chunk extent along each axis, covering
/// the full 16x16x16 voxel region. This enables Bevy's built-in frustum
/// culling to skip off-screen chunks.
pub fn chunk_aabb() -> Aabb3d {
    use bevy::math::Vec3A;
    Aabb3d::new(Vec3A::splat(8.0), Vec3A::splat(8.0))
}

// ─── upgrade_to_texture_material ─────────────────────────────────────────────

/// Upgrades existing chunk entities from `StandardMaterial` to `ChunkMaterial`
/// once [`ChunkTextureArray`] becomes available (i.e. after async texture loading).
///
/// `setup_chunk_material` runs at `Startup` — before any textures have loaded —
/// so it always falls back to `StandardMaterial`. This system runs every `Update`
/// frame, does nothing until `ChunkTextureArray` is inserted AND
/// `ChunkMaterialHandle.custom` is still `None`, then:
///
/// 1. Creates a `ChunkMaterial` from the texture array handle.
/// 2. Stores it in `ChunkMaterialHandle.custom`.
/// 3. Replaces `MeshMaterial3d<StandardMaterial>` with `MeshMaterial3d<ChunkMaterial>`
///    on every entity that already has a [`ChunkRendered`] marker.
///
/// Because the greedy mesher always emits `ATTRIBUTE_TEXTURE_LAYER` vertices,
/// existing meshes are already compatible with `ChunkMaterial` — no remesh needed.
pub fn upgrade_to_texture_material(
    mut commands: Commands,
    mut chunk_materials: ResMut<Assets<ChunkMaterial>>,
    mut handle: ResMut<ChunkMaterialHandle>,
    texture_array: Option<Res<ChunkTextureArray>>,
    rendered_chunks: Query<Entity, (With<ChunkRendered>, With<MeshMaterial3d<StandardMaterial>>)>,
) {
    // Only act once: when ChunkTextureArray appears but we haven't upgraded yet.
    if handle.custom.is_some() {
        return;
    }
    let Some(tex) = texture_array else { return; };

    let mat = ChunkMaterial {
        texture_array: tex.handle.clone(),
    };
    let mat_handle = chunk_materials.add(mat);
    handle.custom = Some(mat_handle.clone());

    // Swap the material on every already-rendered chunk.
    for entity in &rendered_chunks {
        commands.entity(entity)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert(MeshMaterial3d(mat_handle.clone()));
    }

    eprintln!("voxel texture array active — {} chunk(s) upgraded to ChunkMaterial", rendered_chunks.iter().count());
}

// ─── build_texture_array ─────────────────────────────────────────────────────

/// Assembles N identically-sized [`Image`]s into a single GPU `texture_2d_array`.
///
/// Each input image is expected to be the same pixel dimensions and format.
/// The images are stacked in the order provided: layer 0 = `images[0]`,
/// layer 1 = `images[1]`, etc.
///
/// Returns an [`Image`] whose `texture_descriptor` is configured as a
/// `D2` texture with `depth_or_array_layers = N`, suitable for binding as a
/// `texture_2d_array` in a WGSL shader.
///
/// # Panics
///
/// Panics if `images` is empty or if images have different sizes / formats.
pub fn build_texture_array(images: &[Image]) -> Image {
    assert!(!images.is_empty(), "build_texture_array: must provide at least one image");
    let first = &images[0];
    let width = first.width();
    let height = first.height();
    let format = first.texture_descriptor.format;
    let layers = images.len() as u32;

    // Validate all images have matching dimensions and format.
    for (i, img) in images.iter().enumerate() {
        assert_eq!(img.width(), width, "image {i} width mismatch");
        assert_eq!(img.height(), height, "image {i} height mismatch");
        assert_eq!(img.texture_descriptor.format, format, "image {i} format mismatch");
    }

    // Concatenate raw pixel data from all layers.
    let mut combined: Vec<u8> = Vec::new();
    for img in images {
        if let Some(data) = &img.data {
            combined.extend_from_slice(data);
        }
    }

    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureViewDescriptor, TextureViewDimension};

    let size = Extent3d {
        width,
        height,
        depth_or_array_layers: layers,
    };

    let mut array_image = Image::new(
        size,
        TextureDimension::D2,
        combined,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );

    // Tell Bevy/wgpu to interpret this as a 2D array texture view.
    array_image.texture_view_descriptor = Some(TextureViewDescriptor {
        label: Some("chunk_texture_array_view"),
        dimension: Some(TextureViewDimension::D2Array),
        ..Default::default()
    });

    array_image
}
