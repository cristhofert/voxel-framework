//! Voxel collision detection and resolution.
//!
//! # Primitives
//!
//! Two collision shapes are provided:
//!
//! | Shape | Functions | When to use |
//! |-------|-----------|-------------|
//! | **Sphere** | [`sphere_overlaps_voxels`], [`resolve_sphere_collision`] | Balls, projectiles, simple objects |
//! | **Capsule** | [`capsule_overlaps_voxels`], [`resolve_capsule_collision`] | Humanoid players, NPCs, any tall entity |
//!
//! # Push-out algorithm
//!
//! Both resolve functions use an **accumulative push-out** approach:
//!
//! 1. Enumerate all voxels whose axis-aligned bounding box (AABB) overlaps the
//!    shape's bounding box.
//! 2. For each solid voxel, compute the **closest point** on the voxel AABB to
//!    the shape's representative point (center for sphere, nearest spine point
//!    for capsule).
//! 3. If the distance from that point to the representative point is less than
//!    `radius`, calculate the overlap depth and accumulate a push-out vector
//!    along the separation direction.
//! 4. When the representative point is fully **inside** the AABB (distance
//!    ≈ 0), the minimum-penetration-depth axis is chosen for the push-out
//!    instead.
//! 5. The accumulated push-out is added to the input position and returned.
//!
//! The caller is responsible for velocity response (zeroing, reflecting, or
//! sliding the velocity component along the contact normal).
//!
//! # Integration pattern
//!
//! ```no_run
//! # use bevy::prelude::*;
//! # use voxel_framework::world::collision::resolve_capsule_collision;
//! # use voxel_framework::world::ChunkManager;
//! # use voxel_framework::chunk::Chunk;
//! # use voxel_framework::voxel::DefaultVoxel;
//! fn move_player(
//!     mut transform: Mut<Transform>,
//!     manager:       Res<ChunkManager>,
//!     chunks:        Query<&Chunk<DefaultVoxel>>,
//! ) {
//!     let bottom = transform.translation;
//!     let top    = bottom + Vec3::Y * 1.8;
//!     let resolved = resolve_capsule_collision(bottom, top, 0.4, &manager, &chunks);
//!     transform.translation = resolved;
//! }
//! ```

use bevy::ecs::query::QueryFilter;
use bevy::prelude::*;

use crate::chunk::{Chunk, VOXEL_SIZE, world_to_chunk_coord, world_to_local_coord};
use crate::voxel::VoxelType;
use crate::world::ChunkManager;

/// Returns `true` if a sphere overlaps any solid voxel in the loaded world.
///
/// Iterates all voxels within the sphere's axis-aligned bounding box and
/// performs a sphere-vs-AABB intersection test for each solid one.  Stops
/// at the first overlap found.
///
/// # Parameters
///
/// - `center` — World-space center of the sphere.
/// - `radius` — Radius of the sphere in world units.
/// - `manager` — Chunk registry used to resolve coordinates to entities.
/// - `chunks` — ECS query providing read access to chunk data.  The `F`
///   query filter lets callers restrict which chunk entities are considered;
///   pass `()` for the default (all loaded chunks).
///
/// # Return value
///
/// `true` if the sphere intersects at least one solid voxel, `false` otherwise.
/// Unloaded chunks are treated as passable (no overlap).
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

/// Resolve sphere collision against solid voxels and return the corrected position.
///
/// For each solid voxel that overlaps the sphere, computes a push-out vector
/// that moves the sphere centre outside the voxel AABB by exactly `radius`
/// units.  All per-voxel push-outs are **accumulated** (summed) and added to
/// `position` in one step.
///
/// Does **not** modify velocity.  The caller is responsible for zeroing or
/// reflecting the velocity component along the push-out direction if needed.
///
/// Unloaded chunks are treated as passable and do not contribute push-out.
///
/// # Parameters
///
/// - `position` — World-space sphere centre before resolution.
/// - `radius` — Sphere radius in world units.
/// - `manager` — Chunk registry.
/// - `chunks` — Read query for chunk data.  The `F` filter restricts which
///   entities are sampled; use `()` for the default (all loaded chunks).
///
/// # Return value
///
/// The corrected world-space sphere centre after push-out.  May still overlap
/// in extreme tunnelling cases (high-speed objects, very large radius); run
/// multiple resolution passes if needed.
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

/// Resolve capsule collision against solid voxels and return the corrected bottom position.
///
/// A capsule is defined by two **spine endpoints** (`bottom` and `top`) and a
/// `radius`.  This is the standard representation for humanoid player
/// collision: `bottom` is the foot position, `top` is the head position, and
/// `radius` is the half-width of the body.
///
/// # Algorithm
///
/// For each solid voxel overlapping the capsule's bounding box:
///
/// 1. Find the point on the spine segment `[bottom, top]` that is closest to
///    the voxel AABB centre (the projection clamped to the segment).
/// 2. Perform the same sphere-vs-AABB push-out as [`resolve_sphere_collision`]
///    but centred on that projected spine point instead of a fixed centre.
/// 3. Accumulate push-out vectors across all overlapping voxels.
///
/// The accumulated displacement is applied to `bottom`; `top` is implicitly
/// translated by the same amount (the spine offset `top - bottom` is preserved).
///
/// # Parameters
///
/// - `bottom` — World-space foot (lower spine endpoint) of the capsule.
/// - `top` — World-space head (upper spine endpoint) of the capsule.
/// - `radius` — Capsule radius in world units.
/// - `manager` — Chunk registry.
/// - `chunks` — Read query for chunk data.  The `F` filter restricts which
///   entities are sampled; use `()` for the default (all loaded chunks).
///
/// # Return value
///
/// The corrected world-space position for `bottom`.  Compute the new `top` as
/// `resolved_bottom + (top - bottom)`.
pub fn resolve_capsule_collision<V: VoxelType, F: QueryFilter>(
    bottom: Vec3,
    top: Vec3,
    radius: f32,
    manager: &ChunkManager,
    chunks: &Query<&Chunk<V>, F>,
) -> Vec3 {
    crate::profile_span!("voxel::world::collision::resolve_capsule");
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

/// Returns `true` if a capsule overlaps any solid voxel in the loaded world.
///
/// Uses the same spine-projection technique as [`resolve_capsule_collision`]:
/// for each candidate solid voxel the closest point on the spine segment
/// `[bottom, top]` is found and a sphere-vs-AABB test is performed at that
/// point with the given `radius`.
///
/// # Parameters
///
/// - `bottom` — World-space foot (lower spine endpoint).
/// - `top` — World-space head (upper spine endpoint).
/// - `radius` — Capsule radius in world units.
/// - `manager` — Chunk registry.
/// - `chunks` — Read query for chunk data.  The `F` filter restricts which
///   entities are sampled; use `()` for the default (all loaded chunks).
///
/// # Return value
///
/// `true` if the capsule intersects at least one solid voxel, `false` otherwise.
/// Unloaded chunks are treated as passable.
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
    use crate::test_support::{filled_chunk, world_with_chunks};
    use crate::voxel::DefaultVoxel;
    use crate::world::ChunkManager;
    use bevy::ecs::system::SystemState;

    // Minimal World with one chunk at (0,0,0): all Rock if `fill_solid`, else empty.
    fn build_world(fill_solid: bool) -> bevy::ecs::world::World {
        let chunk = if fill_solid {
            filled_chunk(DefaultVoxel::Rock)
        } else {
            Chunk::<DefaultVoxel>::new()
        };
        world_with_chunks([(IVec3::ZERO, chunk)])
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
        let resolved_bottom =
            resolve_capsule_collision::<DefaultVoxel, ()>(bottom, top, 0.4, &manager, &chunks);
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
