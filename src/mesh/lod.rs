//! Level-of-detail (LOD) computation for chunk meshing.
//!
//! Distance-based LOD reduces GPU triangle load for chunks that are far from
//! the viewer.  Rather than storing separate mesh assets per level, the greedy
//! mesher's `step` parameter is used directly: a step of `2` makes each
//! super-voxel cover a `2×2×2` region, halving linear resolution and roughly
//! quartering the triangle count for typical terrain.
//!
//! # Levels
//!
//! Only two discrete levels are used.  More levels were prototyped but
//! discarded because steps larger than `2` produce visually jarring flat quads
//! and make seam-free stitching between neighboring chunks more complex.
//!
//! | Level | Step | Viewer distance        |
//! |-------|------|------------------------|
//! | `0`   | `1`  | < [`LOD0_MAX_DISTANCE`] |
//! | `1`   | `2`  | ≥ [`LOD0_MAX_DISTANCE`] |
//!
//! # Hysteresis
//!
//! A chunk exactly at the 200-unit boundary would switch LOD level every frame
//! as floating-point distance fluctuates.  [`compute_lod_with_hysteresis`]
//! adds a dead band: upgrading to a finer level requires moving *inside* the
//! threshold by at least `hysteresis` units, and downgrading requires moving
//! *outside* by the same amount.  The system uses a ±5-unit band.

use bevy::prelude::Vec3;

/// Distance threshold below which LOD 0 (full detail, step=1) is used.
/// At or beyond this distance LOD 1 (step=2) is used.
pub const LOD0_MAX_DISTANCE: f32 = 200.0;

/// Computes the LOD level for a chunk from its world-space position and the
/// viewer's world-space position.
///
/// The distance is the Euclidean distance between `chunk_world_pos` and
/// `viewer_pos`.  Pass the chunk's **center** (minimum corner + half-chunk
/// offset) so that all six faces are equidistant from the viewer; passing the
/// minimum corner biases the result toward finer LOD for large chunks.
///
/// For LOD transitions that should not flicker, use
/// [`compute_lod_with_hysteresis`] instead.
///
/// # Returns
///
/// - `0` — distance < [`LOD0_MAX_DISTANCE`] → step `1` (full detail)
/// - `1` — distance ≥ [`LOD0_MAX_DISTANCE`] → step `2` (half detail)
///
/// # Examples
///
/// ```rust,no_run
/// use bevy::math::Vec3;
/// use voxel_framework::mesh::lod::compute_lod;
///
/// let chunk_center = Vec3::new(150.0, 0.0, 0.0);
/// let viewer = Vec3::ZERO;
/// assert_eq!(compute_lod(chunk_center, viewer), 0); // close chunk, full detail
/// ```
#[inline]
pub fn compute_lod(chunk_world_pos: Vec3, viewer_pos: Vec3) -> u8 {
    let dist = chunk_world_pos.distance(viewer_pos);
    if dist < LOD0_MAX_DISTANCE {
        0
    } else {
        1
    }
}

/// Computes the LOD level with a hysteresis dead band to prevent rapid
/// level oscillation as the viewer moves along the threshold boundary.
///
/// The returned level only changes when the viewer has moved `hysteresis` units
/// past the threshold in the direction of the transition:
///
/// - **Upgrade** (`1 → 0`, coarse → fine): requires `distance < LOD0_MAX_DISTANCE − hysteresis`
/// - **Downgrade** (`0 → 1`, fine → coarse): requires `distance > LOD0_MAX_DISTANCE + hysteresis`
///
/// If neither condition is met, `current_lod` is returned unchanged.  The
/// [`super::systems::lod_update_system`] uses a `hysteresis` of `5.0` units.
///
/// # Parameters
///
/// - `chunk_world_pos` — world-space position to measure from (typically chunk center).
/// - `viewer_pos` — current viewer world-space position.
/// - `current_lod` — the LOD level currently assigned to this chunk.
/// - `hysteresis` — dead-band width in world units (must be ≥ 0.0).
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

/// Converts a LOD level to the `step` parameter expected by
/// [`super::generate_chunk_mesh`] and [`super::generate_chunk_mesh_with_neighbors`].
///
/// | LOD | Step | Resolution                   |
/// |-----|------|------------------------------|
/// | `0` | `1`  | Full — one voxel per cell    |
/// | ≥ 1 | `2`  | Half — `2×2×2` super-voxels  |
///
/// Any level above `0` maps to step `2`; the framework currently defines only
/// two levels, but this function is forward-compatible with additional levels.
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
