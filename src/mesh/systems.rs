//! Mesh generation system that responds to chunk modification events.
//!
//! [`chunk_mesh_system`] listens for [`ChunkModified`] messages, retrieves
//! the chunk entity from [`ChunkManager`], runs the greedy mesher at the
//! appropriate LOD level, and either updates an existing [`Mesh`] asset handle
//! or inserts the full set of render components (`Mesh3d`, `MeshMaterial3d`,
//! `Transform`, `Visibility`, AABB, [`ChunkLod`]).
//!
//! [`lod_update_system`] runs every frame, recomputes the LOD level for every
//! rendered chunk based on distance to the viewer, and emits [`ChunkModified`]
//! when the level changes so that `chunk_mesh_system` regenerates the mesh.

use bevy::camera::primitives::Aabb;
use bevy::prelude::*;

use crate::chunk::{Chunk, ChunkCoordComp, ChunkLod, CHUNK_SIZE};
use crate::render::material::{ChunkMaterial, ChunkRendered};
use crate::voxel::VoxelType;
use crate::world::{ChunkManager, ChunkModified, ChunkUnloadRequest, ViewerPosition};

use super::generate_chunk_mesh;
use super::lod::{compute_lod, compute_lod_with_hysteresis, lod_to_step};

// ─── chunk_mesh_system ────────────────────────────────────────────────────────

/// Regenerates the mesh for any chunk that received a [`ChunkModified`] message.
///
/// For each modified coord this system:
///
/// 1. Looks up the entity in [`ChunkManager`].
/// 2. Fetches its [`Chunk<V>`] component.
/// 3. Computes the LOD level from the viewer distance using [`compute_lod`].
/// 4. Runs [`generate_chunk_mesh`] at the step size for that LOD level.
/// 5. **Update path** — if the entity already has a `Mesh3d` handle, replaces
///    the mesh asset data in-place and updates the [`ChunkLod`] component.
/// 6. **First-time path** — otherwise, allocates a new handle and inserts
///    `Mesh3d`, `MeshMaterial3d<StandardMaterial>`, `Transform`, `Visibility`,
///    AABB, [`ChunkRendered`], and [`ChunkLod`].
///
/// If the chunk material has not yet been created (startup system hasn't run)
/// or the entity no longer exists, the event is silently skipped.
pub fn chunk_mesh_system<V: VoxelType>(
    mut commands: Commands,
    mut modified_reader: MessageReader<ChunkModified>,
    manager: Res<ChunkManager>,
    chunk_material: Res<ChunkMaterial>,
    viewer: Res<ViewerPosition>,
    mut meshes: ResMut<Assets<Mesh>>,
    chunks: Query<(&Chunk<V>, Option<&Mesh3d>, Option<&ChunkLod>)>,
) {
    let Some(material_handle) = chunk_material.handle.clone() else {
        // Material not yet set up — skip until next frame.
        return;
    };

    let chunk_size_f = CHUNK_SIZE as f32;

    for &ChunkModified(coord) in modified_reader.read() {
        let Some(entity) = manager.get_entity(coord) else {
            // Entity not registered — task may have been cancelled or coord
            // arrived out of order; ignore.
            continue;
        };

        let Ok((chunk, maybe_mesh3d, _maybe_lod)) = chunks.get(entity) else {
            // Chunk component not yet attached (race between systems).
            continue;
        };

        // Compute LOD from the chunk center to the viewer.
        let chunk_center = coord.as_vec3() * chunk_size_f + Vec3::splat(chunk_size_f * 0.5);
        let lod = compute_lod(chunk_center, viewer.position);
        let step = lod_to_step(lod);

        // Run the greedy mesher at the computed LOD step.
        let new_mesh = generate_chunk_mesh(chunk, step);

        if let Some(mesh3d) = maybe_mesh3d {
            // Update path: reuse the existing handle, replace asset data.
            if let Some(existing) = meshes.get_mut(&mesh3d.0) {
                *existing = new_mesh;
            }
            // Update the LOD component to reflect the current level.
            commands.entity(entity).insert(ChunkLod(lod));
        } else {
            // First-time path: allocate new handle and attach all render components.
            let mesh_handle = meshes.add(new_mesh);

            // World-space origin of this chunk's minimum corner.
            let translation = coord.as_vec3() * chunk_size_f;

            // AABB sized to the full chunk extent for frustum culling.
            let aabb = Aabb::from_min_max(Vec3::ZERO, Vec3::splat(chunk_size_f));

            commands.entity(entity).insert((
                Mesh3d(mesh_handle),
                MeshMaterial3d(material_handle.clone()),
                Transform::from_translation(translation),
                Visibility::default(),
                aabb,
                ChunkRendered,
                ChunkLod(lod),
            ));
        }
    }
}

// ─── lod_update_system ────────────────────────────────────────────────────────

/// Every frame, recomputes the LOD level for every rendered chunk.
///
/// For each chunk entity that has a [`Transform`] and a [`ChunkLod`] component,
/// this system:
///
/// 1. Reads the chunk's world-space center from its [`ChunkCoordComp`] (or
///    falls back to the entity's [`Transform`] translation).
/// 2. Calls [`compute_lod`] with the current [`ViewerPosition`].
/// 3. If the computed LOD differs from the stored one, emits [`ChunkModified`]
///    so that [`chunk_mesh_system`] regenerates the mesh at the new level.
///
/// The [`ChunkLod`] component is NOT updated here; `chunk_mesh_system` updates
/// it once the new mesh has actually been built.
pub fn lod_update_system<V: VoxelType>(
    viewer: Res<ViewerPosition>,
    chunks: Query<(Entity, &Transform, &ChunkLod, Option<&ChunkCoordComp>), With<ChunkRendered>>,
    mut modified_writer: MessageWriter<ChunkModified>,
    manager: Res<ChunkManager>,
) {
    let chunk_size_f = CHUNK_SIZE as f32;

    for (entity, transform, current_lod, maybe_coord_comp) in &chunks {
        // Determine the chunk's world-space center.
        // Prefer the ChunkCoordComp (grid coord → center) over raw Transform
        // translation, since the transform is at the minimum corner.
        let chunk_center = if let Some(coord_comp) = maybe_coord_comp {
            coord_comp.coord().as_vec3() * chunk_size_f + Vec3::splat(chunk_size_f * 0.5)
        } else {
            // Fallback: treat the transform translation as the minimum corner
            // and offset to find the center.
            transform.translation + Vec3::splat(chunk_size_f * 0.5)
        };

        let new_lod = compute_lod_with_hysteresis(chunk_center, viewer.position, current_lod.0, 5.0);

        if new_lod != current_lod.0 {
            // Find the coord for this entity so we can emit ChunkModified(coord).
            // We scan the manager's loaded set; this is O(n) but only runs when
            // LOD actually changes, which is rare.
            if let Some(coord) = manager
                .iter_loaded()
                .find_map(|(coord, e)| if e == entity { Some(coord) } else { None })
            {
                modified_writer.write(ChunkModified(coord));
            }
        }
    }
}

// ─── chunk_despawn_system ─────────────────────────────────────────────────────

/// Responds to [`ChunkUnloadRequest`] messages by despawning chunk entities.
///
/// After despawning, the chunk is also removed from [`ChunkManager`] so the
/// streaming system is free to re-request the same coord if the viewer returns.
pub fn chunk_despawn_system(
    mut commands: Commands,
    mut unload_reader: MessageReader<ChunkUnloadRequest>,
    mut manager: ResMut<ChunkManager>,
) {
    for &ChunkUnloadRequest(coord) in unload_reader.read() {
        if let Some(entity) = manager.unregister(coord) {
            commands.entity(entity).despawn();
        }
    }
}
