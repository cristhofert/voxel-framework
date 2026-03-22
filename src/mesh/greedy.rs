//! Greedy meshing algorithm for voxel chunks.
//!
//! Processes each of the 6 face directions independently. For each slice
//! perpendicular to the current axis, a 2D mask is built indicating which
//! voxels have a visible face. Adjacent same-type cells are greedily merged
//! into the largest possible rectangle, each emitted as a single quad.

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

/// Returns the first solid voxel found in a `step`-sized cube starting at
/// `(sx, sy, sz)`, or the air voxel if all are air or out of bounds.
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

/// Generates a mesh for a single chunk using greedy meshing.
///
/// # Arguments
///
/// * `chunk` — The chunk to mesh.
/// * `step` — LOD step size. `1` for full detail (LOD 0), `2` for half
///   detail (LOD 1). Each super-voxel covers a `step x step x step` region.
///
/// # Returns
///
/// A Bevy [`Mesh`] with `PrimitiveTopology::TriangleList` containing
/// positions, normals, UVs, vertex colors, and indices.
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
            if voxel.is_solid() && face_visible(chunk, x, y, z, face, step) {
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
                face, d, u, v, width, height, step, voxel, positions, normals, uvs, colors,
                indices,
            );

            u += width;
        }
    }
}

/// Emits a single quad (4 vertices, 6 indices) for a merged rectangle.
#[allow(clippy::too_many_arguments)] // All args are distinct mesh output buffers; a wrapper struct adds no clarity.
#[inline]
fn emit_quad<V: VoxelType>(
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

    let u_start = u as f32 * s;
    let v_start = v as f32 * s;
    let u_end = (u + width) as f32 * s;
    let v_end = (v + height) as f32 * s;

    // Build the 4 corner positions in chunk-local space.
    // Corners in (u, v) order: (u_start, v_start), (u_end, v_start), (u_end, v_end), (u_start, v_end)
    let corners_uv = [
        (u_start, v_start),
        (u_end, v_start),
        (u_end, v_end),
        (u_start, v_end),
    ];

    let base_idx = positions.len() as u32;
    let color = voxel.color();

    for &(cu, cv) in &corners_uv {
        let mut pos = [0.0f32; 3];
        pos[face.axis] = d_pos;
        pos[face.u_axis] = cu;
        pos[face.v_axis] = cv;
        positions.push(pos);
        normals.push(face.normal);
        colors.push(color);
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
    let ccw = face.positive != (face.axis == 2);
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
