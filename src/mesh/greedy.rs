//! Core greedy meshing algorithm for voxel chunks.
//!
//! # Algorithm
//!
//! Greedy meshing eliminates redundant geometry by merging adjacent coplanar
//! faces of identical voxel type into a single large quad.  The implementation
//! follows the classic Mikola Lysenko approach adapted for a cubic grid:
//!
//! 1. **Iterate over face directions** — the six cardinal axes (`±X`, `±Y`,
//!    `±Z`) are processed independently via [`FACE_DIRS`].
//! 2. **Build a 2-D mask** per slice — for each depth `d` along the face's
//!    primary axis, a `size × size` mask records `Some(voxel)` when the
//!    super-voxel at that cell is solid *and* its outward face is visible
//!    (i.e., the neighbor cell is air).  The `face_visible` callback is
//!    injected so the neighbor-aware variant can plug in its own logic without
//!    duplicating the merge loop.
//! 3. **Greedy merge** — the mask is scanned row by row.  For each non-empty
//!    cell the algorithm extends a rectangle as wide as possible in `u`, then
//!    as tall as possible in `v`, while every cell in the rectangle matches the
//!    same voxel type.  The merged region is cleared in the mask and a single
//!    quad is emitted.
//!
//! # LOD super-voxels
//!
//! The `step` parameter coarsens the grid: each super-voxel covers a
//! `step × step × step` region.  [`super_voxel`] picks the first solid voxel
//! within the region, so a coarser LOD level collapses fine detail but retains
//! the presence of solid material.  This is a "representative sample" strategy
//! chosen for its O(step³) simplicity and acceptable visual quality at distance.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};

use crate::chunk::{Chunk, CHUNK_SIZE, VOXEL_SIZE};
use crate::voxel::VoxelType;

/// Face direction descriptor used by the greedy mesher.
///
/// Each of the 6 faces has a primary axis, a sign (positive or negative),
/// and two tangent axes (u and v) that span the 2D slice plane.
pub(crate) struct FaceDir {
    /// Primary axis index: 0=X, 1=Y, 2=Z.
    pub axis: usize,
    /// `true` for positive direction (+X, +Y, +Z).
    pub positive: bool,
    /// First tangent axis index.
    pub u_axis: usize,
    /// Second tangent axis index.
    pub v_axis: usize,
    /// Face normal as `[f32; 3]`.
    normal: [f32; 3],
}

/// The six cardinal face directions in project convention order.
pub(crate) const FACE_DIRS: [FaceDir; 6] = [
    // Face 0: +Y (top) — axis=Y(1), u=X(0), v=Z(2)
    FaceDir {
        axis: 1,
        positive: true,
        u_axis: 0,
        v_axis: 2,
        normal: [0.0, 1.0, 0.0],
    },
    // Face 1: -Y (bottom) — axis=Y(1), u=X(0), v=Z(2)
    FaceDir {
        axis: 1,
        positive: false,
        u_axis: 0,
        v_axis: 2,
        normal: [0.0, -1.0, 0.0],
    },
    // Face 2: +X (right) — axis=X(0), u=Z(2), v=Y(1)
    FaceDir {
        axis: 0,
        positive: true,
        u_axis: 2,
        v_axis: 1,
        normal: [1.0, 0.0, 0.0],
    },
    // Face 3: -X (left) — axis=X(0), u=Z(2), v=Y(1)
    FaceDir {
        axis: 0,
        positive: false,
        u_axis: 2,
        v_axis: 1,
        normal: [-1.0, 0.0, 0.0],
    },
    // Face 4: +Z (front) — axis=Z(2), u=X(0), v=Y(1)
    FaceDir {
        axis: 2,
        positive: true,
        u_axis: 0,
        v_axis: 1,
        normal: [0.0, 0.0, 1.0],
    },
    // Face 5: -Z (back) — axis=Z(2), u=X(0), v=Y(1)
    FaceDir {
        axis: 2,
        positive: false,
        u_axis: 0,
        v_axis: 1,
        normal: [0.0, 0.0, -1.0],
    },
];

/// Returns the representative voxel for a `step × step × step` region rooted
/// at `(sx, sy, sz)`.
///
/// Iterates over the region in XYZ order and returns the first solid voxel
/// found.  If the entire region is air or out of bounds, returns [`VoxelType::air`].
///
/// This "first-solid-wins" strategy is used rather than majority voting because
/// it is O(1) in the common case (the first cell is solid), avoids allocations,
/// and produces correct LOD at chunk borders where the region may be partially
/// out of bounds.
#[inline]
pub(crate) fn super_voxel<V: VoxelType>(
    chunk: &Chunk<V>,
    sx: usize,
    sy: usize,
    sz: usize,
    step: usize,
) -> V {
    for dx in 0..step {
        for dy in 0..step {
            for dz in 0..step {
                let x = sx + dx;
                let y = sy + dy;
                let z = sz + dz;
                if x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE {
                    let v = chunk.get(x, y, z);
                    if v.is_solid() {
                        return v;
                    }
                }
            }
        }
    }
    V::air()
}

/// Generates a [`Mesh`] for a single chunk using greedy meshing.
///
/// This is the standalone variant: border faces that exit the chunk boundary
/// are always emitted as visible.  When adjacent chunks are loaded and their
/// data is available, prefer [`super::generate_chunk_mesh_with_neighbors`] to
/// avoid single-voxel gaps at chunk seams.
///
/// # Parameters
///
/// - `chunk` — The [`Chunk<V>`] to mesh.
/// - `step` — LOD step size.  `1` produces one voxel per grid cell (full
///   detail, LOD 0).  `2` groups each `2×2×2` block into a single super-voxel
///   (half detail, LOD 1).  Use [`super::lod::lod_to_step`] to convert a LOD
///   level to this value.
///
/// # Returns
///
/// A Bevy [`Mesh`] with [`bevy::mesh::PrimitiveTopology::TriangleList`]
/// topology.  The mesh contains the following vertex attributes:
///
/// - `ATTRIBUTE_POSITION` (`Float32x3`) — positions in chunk-local space.
/// - `ATTRIBUTE_NORMAL` (`Float32x3`) — outward face normals.
/// - `ATTRIBUTE_UV_0` (`Float32x2`) — UVs scaled by the merged quad size in
///   voxels, suitable for tiling textures.
/// - `ATTRIBUTE_COLOR` (`Float32x4`) — per-vertex color from [`VoxelType::color`].
/// - `U32` index buffer — CCW triangle winding in Bevy's right-hand coordinate system.
///
/// An all-air chunk produces a mesh with zero vertices and no index buffer entries.
///
/// # Examples
///
/// ```rust,no_run
/// use voxel_framework::{chunk::Chunk, mesh::generate_chunk_mesh, voxel::DefaultVoxel};
///
/// let mut chunk = Chunk::<DefaultVoxel>::new();
/// // ... fill chunk with voxel data ...
/// let mesh = generate_chunk_mesh(&chunk, 1); // full detail
/// let mesh_lod1 = generate_chunk_mesh(&chunk, 2); // half detail
/// ```
pub fn generate_chunk_mesh<V: VoxelType>(chunk: &Chunk<V>, step: usize) -> Mesh {
    build_mesh(chunk, step, &default_face_visible::<V>)
}

/// The default face-visibility test: a face is visible if the neighbor in
/// the given direction is air (not solid). At chunk borders, the neighbor
/// is always treated as air.
#[inline]
fn default_face_visible<V: VoxelType>(
    chunk: &Chunk<V>,
    x: usize,
    y: usize,
    z: usize,
    face: &FaceDir,
    step: usize,
) -> bool {
    let (nx, ny, nz) = neighbor_coords(x, y, z, face, step);
    if nx >= CHUNK_SIZE || ny >= CHUNK_SIZE || nz >= CHUNK_SIZE {
        // Outside chunk bounds — treat as air (face visible).
        return true;
    }
    !super_voxel(chunk, nx, ny, nz, step).is_solid()
}

/// Computes the neighbor coordinates in the direction of `face`, offset by `step`.
///
/// Uses wrapping arithmetic so that underflow (e.g., `0 - 1`) wraps to
/// `usize::MAX`, which is always `>= CHUNK_SIZE` and detected as out-of-bounds.
#[inline]
fn neighbor_coords(
    x: usize,
    y: usize,
    z: usize,
    face: &FaceDir,
    step: usize,
) -> (usize, usize, usize) {
    let mut result = [x, y, z];
    if face.positive {
        result[face.axis] = result[face.axis].wrapping_add(step);
    } else {
        result[face.axis] = result[face.axis].wrapping_sub(step);
    }
    (result[0], result[1], result[2])
}

/// Core greedy mesh builder. The `face_visible` callback determines whether
/// a particular face should be emitted, allowing neighbor-aware variants to
/// plug in their own logic.
#[allow(clippy::type_complexity)]
pub(crate) fn build_mesh<V: VoxelType>(
    chunk: &Chunk<V>,
    step: usize,
    face_visible: &dyn Fn(&Chunk<V>, usize, usize, usize, &FaceDir, usize) -> bool,
) -> Mesh {
    let size = CHUNK_SIZE / step;
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // Temporary mask buffer, reused across slices.
    let mut mask: Vec<Option<V>> = vec![None; size * size];

    for face in &FACE_DIRS {
        for d in 0..size {
            // Build the 2D mask for this slice.
            build_mask(chunk, face, d, step, size, &mut mask, face_visible);

            // Greedy merge the mask into quads.
            greedy_merge(
                chunk,
                &mut mask,
                size,
                face,
                d,
                step,
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut colors,
                &mut indices,
            );
        }
    }

    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_indices(Indices::U32(indices))
}

/// Builds the 2D visibility mask for a single slice.
///
/// For each cell `(u, v)` in the slice plane, the mask contains `Some(voxel)`
/// if the super-voxel is solid and its face in the current direction is
/// visible (neighbor is air), or `None` otherwise.
#[allow(clippy::type_complexity)]
#[inline]
fn build_mask<V: VoxelType>(
    chunk: &Chunk<V>,
    face: &FaceDir,
    d: usize,
    step: usize,
    size: usize,
    mask: &mut [Option<V>],
    face_visible: &dyn Fn(&Chunk<V>, usize, usize, usize, &FaceDir, usize) -> bool,
) {
    for v in 0..size {
        for u in 0..size {
            // Convert (d, u, v) in slice space to (x, y, z) in chunk space.
            let (x, y, z) = slice_to_chunk(face, d * step, u * step, v * step);
            let voxel = super_voxel(chunk, x, y, z, step);
            // Render solid voxels AND non-solid voxels that have visible color
            // (e.g. water: alpha > 0 but is_solid = false).  Air (alpha = 0)
            // is always skipped.
            let has_color = voxel.color()[3] > 0.0;
            if has_color && face_visible(chunk, x, y, z, face, step) {
                mask[v * size + u] = Some(voxel);
            } else {
                mask[v * size + u] = None;
            }
        }
    }
}

/// Converts slice-space coordinates `(d, u, v)` to chunk-space `(x, y, z)`.
#[inline]
fn slice_to_chunk(face: &FaceDir, d: usize, u: usize, v: usize) -> (usize, usize, usize) {
    let mut coords = [0usize; 3];
    coords[face.axis] = d;
    coords[face.u_axis] = u;
    coords[face.v_axis] = v;
    (coords[0], coords[1], coords[2])
}

/// Greedy-merges the mask and emits quads into the output buffers.
#[allow(clippy::too_many_arguments)] // All args are distinct output buffers; splitting would add indirection.
fn greedy_merge<V: VoxelType>(
    chunk: &Chunk<V>,
    mask: &mut [Option<V>],
    size: usize,
    face: &FaceDir,
    d: usize,
    step: usize,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
) {
    for v in 0..size {
        let mut u = 0;
        while u < size {
            let voxel = match mask[v * size + u] {
                Some(vx) => vx,
                None => {
                    u += 1;
                    continue;
                }
            };

            // Find width: extend in u while same voxel type.
            let mut width = 1;
            while u + width < size && mask[v * size + u + width] == Some(voxel) {
                width += 1;
            }

            // Find height: extend in v while entire row of width matches.
            let mut height = 1;
            'outer: while v + height < size {
                for du in 0..width {
                    if mask[(v + height) * size + u + du] != Some(voxel) {
                        break 'outer;
                    }
                }
                height += 1;
            }

            // Clear the merged region in the mask.
            for dv in 0..height {
                for du in 0..width {
                    mask[(v + dv) * size + u + du] = None;
                }
            }

            // Emit the quad.
            emit_quad(
                chunk, face, d, u, v, width, height, step, voxel, positions, normals, uvs, colors,
                indices,
            );

            u += width;
        }
    }
}

/// Returns `true` if the voxel at `coords` (indexed by axis) is solid.
///
/// Any coordinate outside `[0, CHUNK_SIZE)` is treated as air (not solid),
/// using wrapping arithmetic so that `usize` underflow is detected as
/// out-of-bounds.
#[inline]
fn ao_neighbor_solid<V: VoxelType>(chunk: &Chunk<V>, coords: [usize; 3]) -> bool {
    if coords[0] >= CHUNK_SIZE || coords[1] >= CHUNK_SIZE || coords[2] >= CHUNK_SIZE {
        return false;
    }
    chunk.get(coords[0], coords[1], coords[2]).is_solid()
}

/// Computes the ambient occlusion factor for a single vertex corner.
///
/// Queries the two edge neighbors (`side1`, `side2`) and the diagonal
/// corner neighbor around the vertex in the tangent plane at the face
/// surface.  The classic voxel AO formula maps 0–3 occluders to the range
/// `[1.0, 0.4]`:
///
/// | Occluders | AO factor |
/// |-----------|-----------|
/// | 0         | 1.0       |
/// | 1         | 0.8       |
/// | 2         | 0.6       |
/// | 3         | 0.4       |
///
/// When both side neighbors are solid the corner is always fully occluded
/// (`0.4`) regardless of the actual corner voxel, matching standard voxel
/// AO behavior.
#[inline]
fn compute_ao(side1: bool, side2: bool, corner: bool) -> f32 {
    match side1 as u8 + side2 as u8 + corner as u8 {
        0 => 1.0,
        1 => 0.8,
        2 => 0.6,
        _ => 0.4,
    }
}

/// Computes the AO factor for one vertex of a quad.
///
/// `d_voxel` is the voxel-space coordinate along the face's primary axis
/// where the AO neighbors live (just beyond the face surface).
/// `cu_voxel` and `cv_voxel` are the corner's integer voxel coordinates
/// in the u and v tangent axes respectively.
///
/// The three neighbors are sampled using wrapping arithmetic; any sample
/// that falls outside `[0, CHUNK_SIZE)` on any axis is treated as air.
#[inline]
fn vertex_ao<V: VoxelType>(
    chunk: &Chunk<V>,
    face: &FaceDir,
    d_voxel: usize,
    cu_voxel: usize,
    cv_voxel: usize,
) -> f32 {
    // Side1: one step back along u (wrapping_sub so underflow → usize::MAX → out-of-bounds).
    let mut s1 = [0usize; 3];
    s1[face.axis] = d_voxel;
    s1[face.u_axis] = cu_voxel.wrapping_sub(1);
    s1[face.v_axis] = cv_voxel;
    let side1 = ao_neighbor_solid(chunk, s1);

    // Side2: one step back along v.
    let mut s2 = [0usize; 3];
    s2[face.axis] = d_voxel;
    s2[face.u_axis] = cu_voxel;
    s2[face.v_axis] = cv_voxel.wrapping_sub(1);
    let side2 = ao_neighbor_solid(chunk, s2);

    // Corner: one step back along both u and v.
    let mut sc = [0usize; 3];
    sc[face.axis] = d_voxel;
    sc[face.u_axis] = cu_voxel.wrapping_sub(1);
    sc[face.v_axis] = cv_voxel.wrapping_sub(1);
    let corner = ao_neighbor_solid(chunk, sc);

    compute_ao(side1, side2, corner)
}

/// Emits a single quad (4 vertices, 6 indices) for a merged rectangle.
///
/// Per-vertex ambient occlusion is computed by sampling the three voxels
/// surrounding each corner in the tangent plane at the face surface.  The AO
/// factor darkens the vertex color's RGB channels while leaving alpha intact.
/// The quad's triangulation is flipped when needed to avoid the AO anisotropy
/// seam artifact described in Mikola Lysenko's original voxel AO paper.
#[allow(clippy::too_many_arguments)] // All args are distinct mesh output buffers; a wrapper struct adds no clarity.
#[inline]
fn emit_quad<V: VoxelType>(
    chunk: &Chunk<V>,
    face: &FaceDir,
    d: usize,
    u: usize,
    v: usize,
    width: usize,
    height: usize,
    step: usize,
    voxel: V,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
) {
    let s = step as f32 * VOXEL_SIZE;

    // The slice position along the primary axis.
    // For positive faces, the quad sits at (d + 1) * step (the far side of the voxel).
    // For negative faces, the quad sits at d * step (the near side).
    let d_pos = if face.positive {
        (d as f32 + 1.0) * s
    } else {
        d as f32 * s
    };

    // The voxel-space depth coordinate for AO sampling.
    // For a positive face, the visible surface is the +face.axis side of voxel d*step,
    // so the AO neighbors live at (d+1)*step (the first layer beyond the surface).
    // For a negative face, the surface is the -face.axis side of voxel d*step,
    // so AO neighbors live at d*step - 1 (wrapping; out-of-bounds → treated as air).
    let ao_depth: usize = if face.positive {
        (d + 1) * step
    } else {
        d * step.wrapping_sub(1)  // wrapping so step=0 case yields usize::MAX → out-of-bounds
    };
    // Clamp out-of-bounds depth to a sentinel that ao_neighbor_solid will reject.
    let ao_depth = if !face.positive && d == 0 {
        usize::MAX // underflow → out-of-bounds → all AO neighbors treated as air
    } else {
        ao_depth
    };

    let u_start = u as f32 * s;
    let v_start = v as f32 * s;
    let u_end = (u + width) as f32 * s;
    let v_end = (v + height) as f32 * s;

    // Voxel-space integer coords for each of the 4 corners (in u, v axes).
    // AO is sampled in the tangent plane at the face surface:
    //   v0: (u,       v)        → AO looks at (u-1, d_ao, v-1), etc.
    //   v1: (u+width, v)        → AO looks at (u+width-1, d_ao, v-1), etc.
    //   v2: (u+width, v+height) → AO looks at (u+width-1, d_ao, v+height-1), etc.
    //   v3: (u,       v+height) → AO looks at (u-1, d_ao, v+height-1), etc.
    //
    // The vertex_ao helper uses wrapping_sub so corner (u,v)=(0,0) correctly
    // treats the missing neighbors as air without a bounds check.
    let ao_v0 = vertex_ao(chunk, face, ao_depth, u * step,             v * step);
    let ao_v1 = vertex_ao(chunk, face, ao_depth, (u + width) * step,   v * step);
    let ao_v2 = vertex_ao(chunk, face, ao_depth, (u + width) * step,   (v + height) * step);
    let ao_v3 = vertex_ao(chunk, face, ao_depth, u * step,             (v + height) * step);

    // Build the 4 corner positions in chunk-local space.
    // Corners in (u, v) order: (u_start, v_start), (u_end, v_start), (u_end, v_end), (u_start, v_end)
    let corners = [
        (u_start, v_start, ao_v0),
        (u_end,   v_start, ao_v1),
        (u_end,   v_end,   ao_v2),
        (u_start, v_end,   ao_v3),
    ];

    let base_idx = positions.len() as u32;
    // VoxelType::color() returns sRGB values (as humans define them).
    // Bevy's PBR shader treats ATTRIBUTE_COLOR as *linear* RGB, so we convert.
    let srgb = voxel.color();
    let to_linear = |c: f32| -> f32 {
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let base_color = [to_linear(srgb[0]), to_linear(srgb[1]), to_linear(srgb[2]), srgb[3]];

    for &(cu, cv, ao) in &corners {
        let mut pos = [0.0f32; 3];
        pos[face.axis] = d_pos;
        pos[face.u_axis] = cu;
        pos[face.v_axis] = cv;
        positions.push(pos);
        normals.push(face.normal);
        // Multiply linear RGB by the AO factor; alpha is left unchanged.
        colors.push([
            base_color[0] * ao,
            base_color[1] * ao,
            base_color[2] * ao,
            base_color[3],
        ]);
    }

    // UVs scaled by actual world-space size for texture coord consistency.
    let w_uv = (width * step) as f32;
    let h_uv = (height * step) as f32;
    uvs.push([0.0, 0.0]);
    uvs.push([w_uv, 0.0]);
    uvs.push([w_uv, h_uv]);
    uvs.push([0.0, h_uv]);

    // Two triangles with CCW winding when viewed from the normal direction.
    // Bevy uses a right-hand coordinate system with CCW front faces.
    //
    // Cross-product analysis per face:
    //   axis=0 (+X/-X) and axis=1 (+Y/-Y): positive faces need (0,2,1),(0,3,2)
    //   and negative faces need (0,1,2),(0,2,3).
    //
    //   axis=2 (+Z/-Z): the u=X, v=Y tangent frame is left-handed relative to
    //   +Z, so the winding must be inverted — positive Z needs (0,1,2),(0,2,3)
    //   and negative Z needs (0,2,1),(0,3,2).
    //
    // A single XOR captures this: flip when (positive XOR axis==2) is false,
    // i.e. use the "CCW" branch when face.positive != (face.axis == 2).
    //
    // Additionally, flip the triangulation when ao_v0 + ao_v2 < ao_v1 + ao_v3.
    // This avoids the AO anisotropy seam artifact that appears when the default
    // diagonal cuts across a corner with mismatched AO values (Mikola Lysenko's fix).
    let ao_flip = ao_v0 + ao_v2 < ao_v1 + ao_v3;
    let ccw = (face.positive != (face.axis == 2)) != ao_flip;
    if ccw {
        indices.push(base_idx);
        indices.push(base_idx + 2);
        indices.push(base_idx + 1);
        indices.push(base_idx);
        indices.push(base_idx + 3);
        indices.push(base_idx + 2);
    } else {
        indices.push(base_idx);
        indices.push(base_idx + 1);
        indices.push(base_idx + 2);
        indices.push(base_idx);
        indices.push(base_idx + 2);
        indices.push(base_idx + 3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;
    use crate::voxel::DefaultVoxel;

    /// Helper to extract vertex count from a mesh.
    fn vertex_count(mesh: &Mesh) -> usize {
        match mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions") {
            VertexAttributeValues::Float32x3(v) => v.len(),
            _ => panic!("unexpected attribute type"),
        }
    }

    /// Helper to extract index count from a mesh.
    fn index_count(mesh: &Mesh) -> usize {
        match mesh.indices().expect("indices") {
            Indices::U32(idx) => idx.len(),
            _ => panic!("expected U32 indices"),
        }
    }

    #[test]
    fn empty_chunk_produces_empty_mesh() {
        let chunk = Chunk::<DefaultVoxel>::new();
        let mesh = generate_chunk_mesh(&chunk, 1);
        assert_eq!(vertex_count(&mesh), 0, "empty chunk should produce 0 vertices");
    }

    #[test]
    fn fully_solid_chunk_produces_six_quads() {
        let mut chunk = Chunk::<DefaultVoxel>::new();
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    chunk.set(x, y, z, DefaultVoxel::Rock);
                }
            }
        }
        let mesh = generate_chunk_mesh(&chunk, 1);

        assert_eq!(
            vertex_count(&mesh),
            24,
            "fully solid chunk should have 24 vertices (4 per face x 6 faces)"
        );
        assert_eq!(
            index_count(&mesh),
            36,
            "fully solid chunk should have 36 indices (6 per face x 6 faces)"
        );
    }

    #[test]
    fn single_voxel_produces_six_faces() {
        let mut chunk = Chunk::<DefaultVoxel>::new();
        chunk.set(0, 0, 0, DefaultVoxel::Rock);
        let mesh = generate_chunk_mesh(&chunk, 1);

        assert_eq!(
            vertex_count(&mesh),
            24,
            "single voxel should have 24 vertices (4 per face x 6 faces)"
        );
        assert_eq!(
            index_count(&mesh),
            36,
            "single voxel should have 36 indices (6 per face x 6 faces)"
        );
    }

    #[test]
    fn lod_mesh_has_fewer_vertices() {
        // Create a chunk with a checkerboard pattern of two different voxel
        // types. At LOD 0, the greedy mesher cannot merge across different
        // types, producing many quads. At LOD 1, each 2x2x2 super-voxel
        // collapses to a single type, enabling much more merging.
        let mut chunk = Chunk::<DefaultVoxel>::new();
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    if (x + y + z) % 2 == 0 {
                        chunk.set(x, y, z, DefaultVoxel::Rock);
                    } else {
                        chunk.set(x, y, z, DefaultVoxel::IronOre);
                    }
                }
            }
        }

        let mesh_lod0 = generate_chunk_mesh(&chunk, 1);
        let mesh_lod1 = generate_chunk_mesh(&chunk, 2);

        let count_lod0 = vertex_count(&mesh_lod0);
        let count_lod1 = vertex_count(&mesh_lod1);

        assert!(
            count_lod1 < count_lod0,
            "LOD 1 ({count_lod1} verts) should have fewer vertices than LOD 0 ({count_lod0} verts)"
        );
    }
}
