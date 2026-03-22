//! Voxel collision detection and resolution.
//!
//! Provides sphere-based collision against the loaded voxel world.
//! Suitable for player movement, physics objects, or any entity
//! that should not pass through solid voxels.

use bevy::ecs::query::QueryFilter;
use bevy::prelude::*;

use crate::chunk::{Chunk, VOXEL_SIZE, world_to_chunk_coord, world_to_local_coord};
use crate::voxel::VoxelType;
use crate::world::ChunkManager;

/// Check if a sphere centered at `center` with `radius` overlaps any solid voxel.
///
/// Returns `true` if any voxel within the sphere's AABB is solid and the
/// sphere-vs-AABB test passes for that voxel.
///
/// The `F` query filter lets callers restrict which chunk entities are considered.
/// Use `Query<&Chunk<V>>` (filter `()`) for the default case.
pub fn sphere_overlaps_voxels<V: VoxelType, F: QueryFilter>(
    center: Vec3,
    radius: f32,
    manager: &ChunkManager,
    chunks: &Query<&Chunk<V>, F>,
) -> bool {
    let min = center - Vec3::splat(radius);
    let max = center + Vec3::splat(radius);

    let x_min = min.x.floor() as i32;
    let y_min = min.y.floor() as i32;
    let z_min = min.z.floor() as i32;
    let x_max = max.x.ceil() as i32;
    let y_max = max.y.ceil() as i32;
    let z_max = max.z.ceil() as i32;

    for vx in x_min..x_max {
        for vy in y_min..y_max {
            for vz in z_min..z_max {
                let voxel_world = Vec3::new(vx as f32, vy as f32, vz as f32);
                if sphere_vs_voxel_aabb(center, radius, voxel_world) {
                    if sample_voxel(voxel_world + Vec3::splat(0.5), manager, chunks) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Resolve sphere collision against solid voxels.
///
/// Given the current `position` for a sphere of `radius`, returns the resolved
/// position after sliding along voxel surfaces by accumulating push-out vectors.
/// Does NOT modify velocity — the caller decides how to handle that.
///
/// The `F` query filter lets callers restrict which chunk entities are considered.
/// Use `Query<&Chunk<V>>` (filter `()`) for the default case.
pub fn resolve_sphere_collision<V: VoxelType, F: QueryFilter>(
    position: Vec3,
    radius: f32,
    manager: &ChunkManager,
    chunks: &Query<&Chunk<V>, F>,
) -> Vec3 {
    let min = position - Vec3::splat(radius);
    let max = position + Vec3::splat(radius);

    let x_min = min.x.floor() as i32;
    let y_min = min.y.floor() as i32;
    let z_min = min.z.floor() as i32;
    let x_max = max.x.ceil() as i32;
    let y_max = max.y.ceil() as i32;
    let z_max = max.z.ceil() as i32;

    let mut total_pushout = Vec3::ZERO;

    for vx in x_min..x_max {
        for vy in y_min..y_max {
            for vz in z_min..z_max {
                let voxel_min = Vec3::new(vx as f32, vy as f32, vz as f32);
                let voxel_center_sample = voxel_min + Vec3::splat(0.5);

                if !sample_voxel(voxel_center_sample, manager, chunks) {
                    continue;
                }

                // Closest point on the voxel AABB to the sphere center.
                let voxel_max = voxel_min + Vec3::splat(VOXEL_SIZE);
                let closest = position.clamp(voxel_min, voxel_max);
                let diff = position - closest;
                let dist_sq = diff.length_squared();

                if dist_sq >= radius * radius {
                    continue;
                }

                if dist_sq > 1e-12 {
                    // Normal case: sphere center is outside the AABB.
                    // Push out along the sphere-center-to-closest-point direction.
                    let dist = dist_sq.sqrt();
                    let push_dir = diff / dist;
                    let overlap = radius - dist;
                    total_pushout += push_dir * overlap;
                } else {
                    // Sphere center is inside (or exactly on the surface of) the AABB.
                    // Use the minimum-penetration-depth axis to find the shortest exit.
                    let center = position + total_pushout;
                    let depth_neg_x = center.x - voxel_min.x;
                    let depth_pos_x = voxel_max.x - center.x;
                    let depth_neg_y = center.y - voxel_min.y;
                    let depth_pos_y = voxel_max.y - center.y;
                    let depth_neg_z = center.z - voxel_min.z;
                    let depth_pos_z = voxel_max.z - center.z;

                    let (min_depth, push_dir) = [
                        (depth_neg_x, Vec3::NEG_X),
                        (depth_pos_x, Vec3::X),
                        (depth_neg_y, Vec3::NEG_Y),
                        (depth_pos_y, Vec3::Y),
                        (depth_neg_z, Vec3::NEG_Z),
                        (depth_pos_z, Vec3::Z),
                    ]
                    .into_iter()
                    .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
                    .unwrap();

                    // Push the center out by (penetration depth + radius) so the
                    // sphere surface clears the voxel face entirely.
                    total_pushout += push_dir * (min_depth + radius);
                }
            }
        }
    }

    position + total_pushout
}

/// Resolve capsule collision against solid voxels.
///
/// A capsule is defined by a `bottom` point, a `top` point (forming the
/// central axis), and a `radius`. This is the standard representation for
/// player collision in first-person games.
///
/// Returns the resolved `bottom` position after sliding against voxels.
/// The top is assumed to be `bottom + (top - bottom)` (same offset).
pub fn resolve_capsule_collision<V: VoxelType, F: QueryFilter>(
    bottom: Vec3,
    top: Vec3,
    radius: f32,
    manager: &ChunkManager,
    chunks: &Query<&Chunk<V>, F>,
) -> Vec3 {
    let cap_min = bottom.min(top) - Vec3::splat(radius);
    let cap_max = bottom.max(top) + Vec3::splat(radius);

    let x_min = cap_min.x.floor() as i32;
    let y_min = cap_min.y.floor() as i32;
    let z_min = cap_min.z.floor() as i32;
    let x_max = cap_max.x.ceil() as i32;
    let y_max = cap_max.y.ceil() as i32;
    let z_max = cap_max.z.ceil() as i32;

    let mut total_pushout = Vec3::ZERO;

    for vx in x_min..x_max {
        for vy in y_min..y_max {
            for vz in z_min..z_max {
                let voxel_min = Vec3::new(vx as f32, vy as f32, vz as f32);
                let voxel_center_sample = voxel_min + Vec3::splat(0.5);

                if !sample_voxel(voxel_center_sample, manager, chunks) {
                    continue;
                }

                // Closest point on the capsule axis segment to the voxel AABB center.
                let voxel_max = voxel_min + Vec3::splat(VOXEL_SIZE);
                let aabb_center = (voxel_min + voxel_max) * 0.5;
                let axis_point = closest_point_on_segment(bottom, top, aabb_center);

                // Run sphere-vs-AABB push-out at that axis point.
                let closest = axis_point.clamp(voxel_min, voxel_max);
                let diff = axis_point - closest;
                let dist_sq = diff.length_squared();

                if dist_sq >= radius * radius {
                    continue;
                }

                if dist_sq > 1e-12 {
                    let dist = dist_sq.sqrt();
                    let push_dir = diff / dist;
                    let overlap = radius - dist;
                    total_pushout += push_dir * overlap;
                } else {
                    // Axis point is inside the AABB — use minimum-penetration-depth axis.
                    let center = axis_point + total_pushout;
                    let depth_neg_x = center.x - voxel_min.x;
                    let depth_pos_x = voxel_max.x - center.x;
                    let depth_neg_y = center.y - voxel_min.y;
                    let depth_pos_y = voxel_max.y - center.y;
                    let depth_neg_z = center.z - voxel_min.z;
                    let depth_pos_z = voxel_max.z - center.z;

                    let (min_depth, push_dir) = [
                        (depth_neg_x, Vec3::NEG_X),
                        (depth_pos_x, Vec3::X),
                        (depth_neg_y, Vec3::NEG_Y),
                        (depth_pos_y, Vec3::Y),
                        (depth_neg_z, Vec3::NEG_Z),
                        (depth_pos_z, Vec3::Z),
                    ]
                    .into_iter()
                    .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
                    .unwrap();

                    total_pushout += push_dir * (min_depth + radius);
                }
            }
        }
    }

    bottom + total_pushout
}

/// Check if a capsule overlaps any solid voxel.
pub fn capsule_overlaps_voxels<V: VoxelType, F: QueryFilter>(
    bottom: Vec3,
    top: Vec3,
    radius: f32,
    manager: &ChunkManager,
    chunks: &Query<&Chunk<V>, F>,
) -> bool {
    let cap_min = bottom.min(top) - Vec3::splat(radius);
    let cap_max = bottom.max(top) + Vec3::splat(radius);

    let x_min = cap_min.x.floor() as i32;
    let y_min = cap_min.y.floor() as i32;
    let z_min = cap_min.z.floor() as i32;
    let x_max = cap_max.x.ceil() as i32;
    let y_max = cap_max.y.ceil() as i32;
    let z_max = cap_max.z.ceil() as i32;

    for vx in x_min..x_max {
        for vy in y_min..y_max {
            for vz in z_min..z_max {
                let voxel_min = Vec3::new(vx as f32, vy as f32, vz as f32);
                let voxel_center_sample = voxel_min + Vec3::splat(0.5);

                if !sample_voxel(voxel_center_sample, manager, chunks) {
                    continue;
                }

                let voxel_max = voxel_min + Vec3::splat(VOXEL_SIZE);
                let aabb_center = (voxel_min + voxel_max) * 0.5;
                let axis_point = closest_point_on_segment(bottom, top, aabb_center);

                if sphere_vs_voxel_aabb(axis_point, radius, voxel_min) {
                    return true;
                }
            }
        }
    }

    false
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Returns `true` if the sphere at `center` / `radius` overlaps the unit-cube
/// AABB whose minimum corner is at `voxel_min`.
#[inline]
fn sphere_vs_voxel_aabb(center: Vec3, radius: f32, voxel_min: Vec3) -> bool {
    let voxel_max = voxel_min + Vec3::splat(VOXEL_SIZE);
    let closest = center.clamp(voxel_min, voxel_max);
    let diff = center - closest;
    diff.length_squared() < radius * radius
}

/// Returns the closest point on segment AB to point P.
#[inline]
fn closest_point_on_segment(a: Vec3, b: Vec3, p: Vec3) -> Vec3 {
    let ab = b - a;
    let len_sq = ab.length_squared();
    if len_sq < 1e-12 {
        return a;
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    a + ab * t
}

/// Sample whether a world-space position falls inside a solid voxel.
/// Returns `false` if the chunk is not loaded (treat as passable).
#[inline]
fn sample_voxel<V: VoxelType, F: QueryFilter>(
    world_pos: Vec3,
    manager: &ChunkManager,
    chunks: &Query<&Chunk<V>, F>,
) -> bool {
    let coord = world_to_chunk_coord(world_pos);
    let Some(entity) = manager.get_entity(coord) else {
        return false;
    };
    let Ok(chunk) = chunks.get(entity) else {
        return false;
    };
    let (x, y, z) = world_to_local_coord(world_pos);
    chunk.is_solid_at(x, y, z)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::Chunk;
    use crate::voxel::DefaultVoxel;
    use crate::world::ChunkManager;
    use bevy::ecs::system::SystemState;

    // Helper: build a minimal Bevy World with a ChunkManager and one chunk.
    // If `fill_solid` is true every voxel in the chunk at coord (0,0,0) is Rock.
    fn build_world(fill_solid: bool) -> bevy::ecs::world::World {
        let mut world = bevy::ecs::world::World::new();

        let mut chunk = Chunk::<DefaultVoxel>::new();
        if fill_solid {
            for x in 0..crate::chunk::CHUNK_SIZE {
                for y in 0..crate::chunk::CHUNK_SIZE {
                    for z in 0..crate::chunk::CHUNK_SIZE {
                        chunk.set(x, y, z, DefaultVoxel::Rock);
                    }
                }
            }
        }

        let entity = world.spawn(chunk).id();
        let mut manager = ChunkManager::default();
        manager.register(IVec3::ZERO, entity);
        world.insert_resource(manager);
        world
    }

    #[test]
    fn test_no_overlap_in_empty_space() {
        let mut world = build_world(false);
        let mut state: SystemState<(Res<ChunkManager>, Query<&Chunk<DefaultVoxel>>)> =
            SystemState::new(&mut world);
        let (manager, chunks) = state.get(&world);

        // Sphere floating well above the (empty) chunk.
        let result = sphere_overlaps_voxels::<DefaultVoxel, ()>(
            Vec3::new(8.0, 200.0, 8.0),
            0.5,
            &manager,
            &chunks,
        );
        assert!(!result, "empty chunk should report no overlap");
    }

    #[test]
    fn test_overlap_with_solid_voxel() {
        let mut world = build_world(true);
        let mut state: SystemState<(Res<ChunkManager>, Query<&Chunk<DefaultVoxel>>)> =
            SystemState::new(&mut world);
        let (manager, chunks) = state.get(&world);

        // Sphere centered right on the voxel at (8, 8, 8).
        let result = sphere_overlaps_voxels::<DefaultVoxel, ()>(
            Vec3::new(8.5, 8.5, 8.5),
            0.5,
            &manager,
            &chunks,
        );
        assert!(result, "sphere inside solid chunk should overlap");
    }

    #[test]
    fn test_resolve_pushes_out() {
        // Use a single solid voxel at the chunk origin (local 0,0,0 → world 0..1).
        let mut world = bevy::ecs::world::World::new();
        let mut chunk = Chunk::<DefaultVoxel>::new();
        chunk.set(0, 0, 0, DefaultVoxel::Rock);
        let entity = world.spawn(chunk).id();
        let mut manager = ChunkManager::default();
        manager.register(IVec3::ZERO, entity);
        world.insert_resource(manager);

        let mut state: SystemState<(Res<ChunkManager>, Query<&Chunk<DefaultVoxel>>)> =
            SystemState::new(&mut world);
        let (manager, chunks) = state.get(&world);

        // Sphere center placed 0.1 units inside the voxel AABB [0,1]^3.
        let inside = Vec3::new(0.1, 0.1, 0.1);
        let resolved = resolve_sphere_collision::<DefaultVoxel, ()>(inside, 0.5, &manager, &chunks);

        // After a single resolution pass the sphere must be pushed clear of the voxel.
        let still_overlapping =
            sphere_overlaps_voxels::<DefaultVoxel, ()>(resolved, 0.5, &manager, &chunks);
        assert!(
            !still_overlapping,
            "resolved position {resolved} should not overlap solid voxels"
        );
    }

    // ─── Capsule tests ─────────────────────────────────────────────────────

    #[test]
    fn test_capsule_no_overlap_in_empty() {
        let mut world = build_world(false);
        let mut state: SystemState<(Res<ChunkManager>, Query<&Chunk<DefaultVoxel>>)> =
            SystemState::new(&mut world);
        let (manager, chunks) = state.get(&world);

        // Capsule floating well above the (empty) chunk.
        let result = capsule_overlaps_voxels::<DefaultVoxel, ()>(
            Vec3::new(8.0, 200.0, 8.0),
            Vec3::new(8.0, 201.8, 8.0),
            0.4,
            &manager,
            &chunks,
        );
        assert!(!result, "capsule in empty chunk should report no overlap");
    }

    #[test]
    fn test_capsule_overlap_with_solid() {
        let mut world = build_world(true);
        let mut state: SystemState<(Res<ChunkManager>, Query<&Chunk<DefaultVoxel>>)> =
            SystemState::new(&mut world);
        let (manager, chunks) = state.get(&world);

        // Capsule whose bottom is inside the solid chunk (voxel at (8,8,8)).
        let result = capsule_overlaps_voxels::<DefaultVoxel, ()>(
            Vec3::new(8.5, 8.5, 8.5),
            Vec3::new(8.5, 10.3, 8.5),
            0.4,
            &manager,
            &chunks,
        );
        assert!(result, "capsule touching solid chunk should overlap");
    }

    #[test]
    fn test_capsule_resolve_pushes_out() {
        // Single solid voxel at world [0,1]^3.
        let mut world = bevy::ecs::world::World::new();
        let mut chunk = Chunk::<DefaultVoxel>::new();
        chunk.set(0, 0, 0, DefaultVoxel::Rock);
        let entity = world.spawn(chunk).id();
        let mut manager = ChunkManager::default();
        manager.register(IVec3::ZERO, entity);
        world.insert_resource(manager);

        let mut state: SystemState<(Res<ChunkManager>, Query<&Chunk<DefaultVoxel>>)> =
            SystemState::new(&mut world);
        let (manager, chunks) = state.get(&world);

        // Capsule near the corner of the voxel — unambiguous minimum-exit direction.
        let bottom = Vec3::new(0.1, 0.1, 0.1);
        let top = Vec3::new(0.1, 1.9, 0.1);
        let resolved_bottom = resolve_capsule_collision::<DefaultVoxel, ()>(
            bottom, top, 0.4, &manager, &chunks,
        );
        let resolved_top = resolved_bottom + (top - bottom);

        let still_overlapping = capsule_overlaps_voxels::<DefaultVoxel, ()>(
            resolved_bottom,
            resolved_top,
            0.4,
            &manager,
            &chunks,
        );
        assert!(
            !still_overlapping,
            "resolved capsule at {resolved_bottom} should not overlap solid voxels"
        );
    }

    #[test]
    fn test_closest_point_on_segment_midpoint() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(4.0, 0.0, 0.0);
        // Point equidistant from both ends, directly above the midpoint.
        let p = Vec3::new(2.0, 5.0, 0.0);
        let result = closest_point_on_segment(a, b, p);
        assert!(
            (result - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-6,
            "expected midpoint (2,0,0), got {result}"
        );
    }

    #[test]
    fn test_closest_point_on_segment_clamped() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(4.0, 0.0, 0.0);
        // Point beyond the end of the segment.
        let p = Vec3::new(10.0, 0.0, 0.0);
        let result = closest_point_on_segment(a, b, p);
        assert!(
            (result - b).length() < 1e-6,
            "expected endpoint (4,0,0), got {result}"
        );
    }
}
