//! DDA (Digital Differential Analysis) voxel raycasting.

use bevy::prelude::*;

use crate::chunk::{Chunk, ChunkCoord, CHUNK_SIZE, VOXEL_SIZE, world_to_chunk_coord};
use crate::voxel::VoxelType;

use super::ChunkManager;

/// Result of a voxel raycast hit.
#[derive(Debug, Clone)]
pub struct RaycastHit {
    /// World position of the hit voxel's center.
    pub position: Vec3,
    /// Chunk coordinate containing the hit voxel.
    pub chunk_coord: ChunkCoord,
    /// Local voxel coordinate within the chunk.
    pub local: (usize, usize, usize),
    /// Face normal of the hit surface (unit vector).
    pub face_normal: Vec3,
    /// Distance from ray origin to the hit.
    pub distance: f32,
}

/// Cast a ray from `origin` in `direction` up to `max_distance` units.
/// Returns the first solid voxel hit, or `None`.
pub fn raycast<V: VoxelType>(
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    manager: &ChunkManager,
    chunks: &Query<&Chunk<V>>,
) -> Option<RaycastHit> {
    let dir = direction.normalize();

    // Current voxel position (integer grid)
    let mut vx = origin.x.floor() as i32;
    let mut vy = origin.y.floor() as i32;
    let mut vz = origin.z.floor() as i32;

    // Step direction: +1 or -1 per axis
    let step_x = if dir.x >= 0.0 { 1i32 } else { -1i32 };
    let step_y = if dir.y >= 0.0 { 1i32 } else { -1i32 };
    let step_z = if dir.z >= 0.0 { 1i32 } else { -1i32 };

    // tDelta: how far along the ray (in t units) to cross one voxel on each axis
    let delta_x = if dir.x.abs() > f32::EPSILON { VOXEL_SIZE / dir.x.abs() } else { f32::MAX };
    let delta_y = if dir.y.abs() > f32::EPSILON { VOXEL_SIZE / dir.y.abs() } else { f32::MAX };
    let delta_z = if dir.z.abs() > f32::EPSILON { VOXEL_SIZE / dir.z.abs() } else { f32::MAX };

    // tMax: distance along ray to the next voxel boundary on each axis
    let next_x = if step_x > 0 { vx as f32 + 1.0 } else { vx as f32 };
    let next_y = if step_y > 0 { vy as f32 + 1.0 } else { vy as f32 };
    let next_z = if step_z > 0 { vz as f32 + 1.0 } else { vz as f32 };

    let mut t_max_x = if dir.x.abs() > f32::EPSILON { (next_x - origin.x) / dir.x } else { f32::MAX };
    let mut t_max_y = if dir.y.abs() > f32::EPSILON { (next_y - origin.y) / dir.y } else { f32::MAX };
    let mut t_max_z = if dir.z.abs() > f32::EPSILON { (next_z - origin.z) / dir.z } else { f32::MAX };

    // Check the initial voxel before any stepping
    {
        let world_pos = Vec3::new(vx as f32 + 0.5, vy as f32 + 0.5, vz as f32 + 0.5);
        let coord = world_to_chunk_coord(world_pos);
        if let Some(entity) = manager.get_entity(coord)
            && let Ok(chunk) = chunks.get(entity)
        {
            let lx = (vx.rem_euclid(CHUNK_SIZE as i32)) as usize;
            let ly = (vy.rem_euclid(CHUNK_SIZE as i32)) as usize;
            let lz = (vz.rem_euclid(CHUNK_SIZE as i32)) as usize;
            if chunk.is_solid_at(lx, ly, lz) {
                return Some(RaycastHit {
                    position: world_pos,
                    chunk_coord: coord,
                    local: (lx, ly, lz),
                    face_normal: Vec3::ZERO,
                    distance: 0.0,
                });
            }
        }
    }

    loop {
        // Advance to next voxel boundary — pick the closest axis and record which one.
        let (t, last_step_axis): (f32, u8);
        if t_max_x < t_max_y && t_max_x < t_max_z {
            t = t_max_x;
            if t > max_distance {
                break;
            }
            vx += step_x;
            t_max_x += delta_x;
            last_step_axis = 0;
        } else if t_max_y < t_max_z {
            t = t_max_y;
            if t > max_distance {
                break;
            }
            vy += step_y;
            t_max_y += delta_y;
            last_step_axis = 1;
        } else {
            t = t_max_z;
            if t > max_distance {
                break;
            }
            vz += step_z;
            t_max_z += delta_z;
            last_step_axis = 2;
        }

        let world_pos = Vec3::new(vx as f32 + 0.5, vy as f32 + 0.5, vz as f32 + 0.5);
        let coord = world_to_chunk_coord(world_pos);

        let entity = match manager.get_entity(coord) {
            Some(e) => e,
            None => continue,
        };
        let chunk = match chunks.get(entity) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let lx = (vx.rem_euclid(CHUNK_SIZE as i32)) as usize;
        let ly = (vy.rem_euclid(CHUNK_SIZE as i32)) as usize;
        let lz = (vz.rem_euclid(CHUNK_SIZE as i32)) as usize;

        if chunk.is_solid_at(lx, ly, lz) {
            // Face normal: opposite to the step direction on the last-crossed axis
            let face_normal = match last_step_axis {
                0 => Vec3::new(-(step_x as f32), 0.0, 0.0),
                1 => Vec3::new(0.0, -(step_y as f32), 0.0),
                _ => Vec3::new(0.0, 0.0, -(step_z as f32)),
            };

            return Some(RaycastHit {
                position: world_pos,
                chunk_coord: coord,
                local: (lx, ly, lz),
                face_normal,
                distance: t,
            });
        }
    }

    None
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;
    use crate::chunk::{Chunk, CHUNK_SIZE};
    use crate::voxel::DefaultVoxel;
    use crate::world::ChunkManager;

    /// Build a minimal Bevy World, spawn a chunk entity, register it in a
    /// ChunkManager, and return (world, manager).
    fn setup_world_with_chunk(
        fill: impl Fn(&mut Chunk<DefaultVoxel>),
    ) -> (World, ChunkManager) {
        let mut world = World::new();
        let mut chunk = Chunk::<DefaultVoxel>::new();
        fill(&mut chunk);
        let entity = world.spawn(chunk).id();

        let mut manager = ChunkManager::default();
        // Chunk at coord (0,0,0) spans world positions [0, 16)^3
        manager.register(IVec3::ZERO, entity);

        (world, manager)
    }

    #[test]
    fn test_raycast_hits_solid_voxel() {
        // Place a single solid voxel at local (8,8,8) → world center ~(8.5, 8.5, 8.5)
        let (mut world, manager) = setup_world_with_chunk(|chunk| {
            chunk.set(8, 8, 8, DefaultVoxel::Rock);
        });

        // Ray starts far on +X axis, aimed toward the solid voxel
        let origin = Vec3::new(-5.0, 8.5, 8.5);
        let direction = Vec3::new(1.0, 0.0, 0.0);

        let mut system_state =
            bevy::ecs::system::SystemState::<Query<&Chunk<DefaultVoxel>>>::new(&mut world);
        let chunks = system_state.get(&world);

        let hit = raycast(origin, direction, 50.0, &manager, &chunks);
        assert!(hit.is_some(), "expected a hit but got None");
        let hit = hit.unwrap();
        assert_eq!(hit.local, (8, 8, 8));
        assert_eq!(hit.chunk_coord, IVec3::ZERO);
    }

    #[test]
    fn test_raycast_misses_empty_chunk() {
        // All-air chunk
        let (mut world, manager) = setup_world_with_chunk(|_| {});

        let origin = Vec3::new(-5.0, 8.5, 8.5);
        let direction = Vec3::new(1.0, 0.0, 0.0);

        let mut system_state =
            bevy::ecs::system::SystemState::<Query<&Chunk<DefaultVoxel>>>::new(&mut world);
        let chunks = system_state.get(&world);

        let hit = raycast(origin, direction, 50.0, &manager, &chunks);
        assert!(hit.is_none(), "expected None but got a hit");
    }

    #[test]
    fn test_raycast_correct_face_normal() {
        // Solid voxel at local (4,8,8) → world ~(4.5, 8.5, 8.5)
        // Ray coming from +X (right side), so the hit face normal should point +X
        let (mut world, manager) = setup_world_with_chunk(|chunk| {
            chunk.set(4, 8, 8, DefaultVoxel::Rock);
        });

        // Approach from the right: origin at x=20, shoot in -X direction
        let origin = Vec3::new(20.0, 8.5, 8.5);
        let direction = Vec3::new(-1.0, 0.0, 0.0);

        let mut system_state =
            bevy::ecs::system::SystemState::<Query<&Chunk<DefaultVoxel>>>::new(&mut world);
        let chunks = system_state.get(&world);

        let hit = raycast(origin, direction, 50.0, &manager, &chunks);
        assert!(hit.is_some(), "expected a hit but got None");
        let hit = hit.unwrap();
        assert_eq!(hit.local, (4, 8, 8));
        // Approaching from +X with step=-1 → face_normal = -(-1) = +1 on X
        assert_eq!(hit.face_normal, Vec3::new(1.0, 0.0, 0.0));
    }
}
