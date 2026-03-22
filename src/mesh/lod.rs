//! LOD level computation for chunk meshing.
//!
//! Provides [`compute_lod`], which maps the Euclidean distance between a
//! chunk's world-space center and the viewer to one of two discrete levels:
//!
//! | Level | Distance        | Greedy-mesh step |
//! |-------|-----------------|-----------------|
//! | `0`   | < 200.0 units   | 1 (full detail) |
//! | `1`   | ≥ 200.0 units   | 2 (half detail) |
//!
//! Only two levels are used to avoid visible seams and large flat quads
//! that appear with more aggressive LOD steps.

use bevy::prelude::Vec3;

/// Distance threshold below which LOD 0 (full detail, step=1) is used.
/// At or beyond this distance LOD 1 (step=2) is used.
pub const LOD0_MAX_DISTANCE: f32 = 200.0;

/// Compute the LOD level for a chunk given the chunk's world-space position
/// and the viewer's world-space position.
///
/// The distance is measured from the viewer to the **center** of the chunk's
/// minimum-corner position. Callers that want center-to-center distance should
/// offset `chunk_world_pos` by half a chunk before calling.
///
/// # Returns
///
/// * `0` — distance < 200.0 units (step=1, full detail)
/// * `1` — distance ≥ 200.0 units (step=2, half detail)
#[inline]
pub fn compute_lod(chunk_world_pos: Vec3, viewer_pos: Vec3) -> u8 {
    let dist = chunk_world_pos.distance(viewer_pos);
    if dist < LOD0_MAX_DISTANCE {
        0
    } else {
        1
    }
}

/// Same as [`compute_lod`] but with a hysteresis band to prevent oscillation.
///
/// The new LOD is only adopted if the distance has moved PAST the threshold
/// by at least `hysteresis` units.
///
/// Rules:
/// - Upgrade to finer LOD (1→0): distance must be < LOD0_MAX_DISTANCE - hysteresis
/// - Downgrade to coarser LOD (0→1): distance must be > LOD0_MAX_DISTANCE + hysteresis
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
        _ => {
            if dist < LOD0_MAX_DISTANCE - hysteresis {
                0
            } else {
                1
            }
        }
    }
}

/// Convert a LOD level to the greedy-mesher step size.
///
/// | LOD | Step |
/// |-----|------|
/// | 0   | 1    |
/// | _   | 2    | (any level ≥ 1 uses half-detail)
#[inline]
pub fn lod_to_step(lod: u8) -> usize {
    if lod == 0 { 1 } else { 2 }
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
    fn mid_range_chunk_is_still_lod0() {
        // 150 units is well within the 200-unit threshold.
        let lod = compute_lod(Vec3::new(150.0, 0.0, 0.0), Vec3::ZERO);
        assert_eq!(lod, 0);
    }

    #[test]
    fn far_chunk_is_lod1() {
        let lod = compute_lod(Vec3::new(250.0, 0.0, 0.0), Vec3::ZERO);
        assert_eq!(lod, 1);
    }

    #[test]
    fn boundary_at_200_is_lod1() {
        // At exactly 200.0 distance, LOD should be 1 (not < 200.0).
        let lod = compute_lod(Vec3::new(200.0, 0.0, 0.0), Vec3::ZERO);
        assert_eq!(lod, 1);
    }

    // ── hysteresis tests ──────────────────────────────────────────────────────

    #[test]
    fn test_hysteresis_prevents_upgrade_at_boundary() {
        // At 200.0 units, current LOD is 1.
        // With hysteresis=5.0, upgrade requires dist < 195.0 → stays at 1.
        let lod = compute_lod_with_hysteresis(Vec3::new(200.0, 0.0, 0.0), Vec3::ZERO, 1, 5.0);
        assert_eq!(lod, 1);
    }

    #[test]
    fn test_hysteresis_allows_upgrade_well_inside() {
        // At 194.0 units (< 200.0 - 5.0 = 195.0), current LOD is 1 → upgrades to 0.
        let lod = compute_lod_with_hysteresis(Vec3::new(194.0, 0.0, 0.0), Vec3::ZERO, 1, 5.0);
        assert_eq!(lod, 0);
    }

    #[test]
    fn test_hysteresis_prevents_downgrade_at_boundary() {
        // At 200.0 units, current LOD is 0.
        // With hysteresis=5.0, downgrade requires dist > 205.0 → stays at 0.
        let lod = compute_lod_with_hysteresis(Vec3::new(200.0, 0.0, 0.0), Vec3::ZERO, 0, 5.0);
        assert_eq!(lod, 0);
    }

    #[test]
    fn test_hysteresis_allows_downgrade_well_outside() {
        // At 206.0 units (> 200.0 + 5.0 = 205.0), current LOD is 0 → downgrades to 1.
        let lod = compute_lod_with_hysteresis(Vec3::new(206.0, 0.0, 0.0), Vec3::ZERO, 0, 5.0);
        assert_eq!(lod, 1);
    }

    #[test]
    fn lod_to_step_values() {
        assert_eq!(lod_to_step(0), 1);
        assert_eq!(lod_to_step(1), 2);
        assert_eq!(lod_to_step(99), 2); // fallback to half-detail
    }
}
