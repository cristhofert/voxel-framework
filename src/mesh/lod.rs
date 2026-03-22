//! LOD level computation for chunk meshing.
//!
//! Provides [`compute_lod`], which maps the Euclidean distance between a
//! chunk's world-space center and the viewer to one of three discrete levels:
//!
//! | Level | Distance       | Greedy-mesh step |
//! |-------|----------------|-----------------|
//! | `0`   | < 50.0 units   | 1 (full detail) |
//! | `1`   | 50–100 units   | 2 (half detail) |
//! | `2`   | > 100 units    | 4 (coarse)      |

use bevy::prelude::Vec3;

/// Distance threshold below which LOD 0 (full detail) is used.
pub const LOD0_MAX_DISTANCE: f32 = 50.0;

/// Distance threshold below which LOD 1 (reduced detail) is used.
/// Beyond this distance LOD 2 (coarse) is used.
pub const LOD1_MAX_DISTANCE: f32 = 100.0;

/// Compute the LOD level for a chunk given the chunk's world-space position
/// and the viewer's world-space position.
///
/// The distance is measured from the viewer to the **center** of the chunk's
/// minimum-corner position. Callers that want center-to-center distance should
/// offset `chunk_world_pos` by half a chunk before calling.
///
/// # Returns
///
/// * `0` — distance < 50.0 units
/// * `1` — distance 50–100 units
/// * `2` — distance > 100 units
#[inline]
pub fn compute_lod(chunk_world_pos: Vec3, viewer_pos: Vec3) -> u8 {
    let dist = chunk_world_pos.distance(viewer_pos);
    if dist < LOD0_MAX_DISTANCE {
        0
    } else if dist < LOD1_MAX_DISTANCE {
        1
    } else {
        2
    }
}

/// Same as [`compute_lod`] but with a hysteresis band to prevent oscillation.
///
/// The new LOD is only adopted if the distance has moved PAST the threshold
/// by at least `hysteresis` units.
///
/// Rules:
/// - Upgrade to finer LOD (e.g., 1→0): distance must be < threshold - hysteresis
/// - Downgrade to coarser LOD (e.g., 0→1): distance must be > threshold + hysteresis
pub fn compute_lod_with_hysteresis(
    chunk_world_pos: Vec3,
    viewer_pos: Vec3,
    current_lod: u8,
    hysteresis: f32,
) -> u8 {
    let dist = chunk_world_pos.distance(viewer_pos);

    match current_lod {
        0 => {
            if dist > LOD0_MAX_DISTANCE + hysteresis {
                1
            } else {
                0
            }
        }
        1 => {
            if dist < LOD0_MAX_DISTANCE - hysteresis {
                0
            } else if dist > LOD1_MAX_DISTANCE + hysteresis {
                2
            } else {
                1
            }
        }
        2 => {
            if dist < LOD1_MAX_DISTANCE - hysteresis {
                1
            } else {
                2
            }
        }
        _ => compute_lod(chunk_world_pos, viewer_pos),
    }
}

/// Convert a LOD level to the greedy-mesher step size.
///
/// | LOD | Step |
/// |-----|------|
/// | 0   | 1    |
/// | 1   | 2    |
/// | 2   | 4    |
/// | _   | 4    | (any unknown level falls back to coarsest)
#[inline]
pub fn lod_to_step(lod: u8) -> usize {
    match lod {
        0 => 1,
        1 => 2,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Vec3;

    #[test]
    fn close_chunk_is_lod0() {
        let lod = compute_lod(Vec3::new(10.0, 0.0, 0.0), Vec3::ZERO);
        assert_eq!(lod, 0);
    }

    #[test]
    fn mid_range_chunk_is_lod1() {
        let lod = compute_lod(Vec3::new(75.0, 0.0, 0.0), Vec3::ZERO);
        assert_eq!(lod, 1);
    }

    #[test]
    fn far_chunk_is_lod2() {
        let lod = compute_lod(Vec3::new(150.0, 0.0, 0.0), Vec3::ZERO);
        assert_eq!(lod, 2);
    }

    #[test]
    fn boundary_at_50_is_lod1() {
        // At exactly 50.0 distance, LOD should be 1 (not < 50.0).
        let lod = compute_lod(Vec3::new(50.0, 0.0, 0.0), Vec3::ZERO);
        assert_eq!(lod, 1);
    }

    #[test]
    fn boundary_at_100_is_lod2() {
        // At exactly 100.0 distance, LOD should be 2 (not < 100.0).
        let lod = compute_lod(Vec3::new(100.0, 0.0, 0.0), Vec3::ZERO);
        assert_eq!(lod, 2);
    }

    // ── hysteresis tests ──────────────────────────────────────────────────────

    #[test]
    fn test_hysteresis_prevents_upgrade_at_boundary() {
        // At exactly 50.0 units, current LOD is 1.
        // With hysteresis=5.0, upgrade requires dist < 45.0 → stays at 1.
        let lod = compute_lod_with_hysteresis(Vec3::new(50.0, 0.0, 0.0), Vec3::ZERO, 1, 5.0);
        assert_eq!(lod, 1);
    }

    #[test]
    fn test_hysteresis_allows_upgrade_well_inside() {
        // At 44.0 units (< 50.0 - 5.0 = 45.0), current LOD is 1 → upgrades to 0.
        let lod = compute_lod_with_hysteresis(Vec3::new(44.0, 0.0, 0.0), Vec3::ZERO, 1, 5.0);
        assert_eq!(lod, 0);
    }

    #[test]
    fn test_hysteresis_prevents_downgrade_at_boundary() {
        // At exactly 50.0 units, current LOD is 0.
        // With hysteresis=5.0, downgrade requires dist > 55.0 → stays at 0.
        let lod = compute_lod_with_hysteresis(Vec3::new(50.0, 0.0, 0.0), Vec3::ZERO, 0, 5.0);
        assert_eq!(lod, 0);
    }

    #[test]
    fn test_hysteresis_allows_downgrade_well_outside() {
        // At 56.0 units (> 50.0 + 5.0 = 55.0), current LOD is 0 → downgrades to 1.
        let lod = compute_lod_with_hysteresis(Vec3::new(56.0, 0.0, 0.0), Vec3::ZERO, 0, 5.0);
        assert_eq!(lod, 1);
    }

    #[test]
    fn lod_to_step_values() {
        assert_eq!(lod_to_step(0), 1);
        assert_eq!(lod_to_step(1), 2);
        assert_eq!(lod_to_step(2), 4);
        assert_eq!(lod_to_step(99), 4); // fallback
    }
}
