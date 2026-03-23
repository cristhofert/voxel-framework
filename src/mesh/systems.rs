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

use std::path::PathBuf;

use bevy::camera::primitives::Aabb;
use bevy::prelude::*;

use crate::chunk::{Chunk, ChunkCoordComp, ChunkLod, CHUNK_SIZE};
use crate::chunk::persistence as chunk_persistence;
use crate::render::material::{ChunkMaterial, ChunkRendered};
use crate::voxel::VoxelType;
use crate::world::{ChunkManager, ChunkModified, ChunkUnloadRequest, ViewerPosition};
use crate::world::persistence::PersistenceConfig;

use super::{ChunkNeighbors, generate_chunk_mesh_with_neighbors};
use super::lod::{compute_lod, compute_lod_with_hysteresis, lod_to_step};

// ─── chunk_mesh_system ────────────────────────────────────────────────────────

/// Regenerates the mesh for any chunk that received a [`ChunkModified`] message.
///
/// For each modified coord this system:
///
/// 1. Looks up the entity in [`ChunkManager`].
/// 2. Fetches its [`Chunk<V>`] component.
/// 3. Computes the LOD level from the viewer distance using [`compute_lod`].
/// 4. Builds a [`ChunkNeighbors`] struct from the six adjacent chunks.
/// 5. Runs [`generate_chunk_mesh_with_neighbors`] at the step size for that LOD level.
/// 6. **Update path** — if the entity already has a `Mesh3d` handle, replaces
///    the mesh asset data in-place and updates the [`ChunkLod`] component.
/// 7. **First-time path** — otherwise, allocates a new handle and inserts
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

    // Face direction offsets: index matches ChunkNeighbors.faces convention.
    // [+Y, -Y, +X, -X, +Z, -Z]
    const NEIGHBOR_OFFSETS: [IVec3; 6] = [
        IVec3::new(0, 1, 0),  // 0: +Y
        IVec3::new(0, -1, 0), // 1: -Y
        IVec3::new(1, 0, 0),  // 2: +X
        IVec3::new(-1, 0, 0), // 3: -X
        IVec3::new(0, 0, 1),  // 4: +Z
        IVec3::new(0, 0, -1), // 5: -Z
    ];

    // For each face index i, the adjacent chunk's face that touches ours is
    // the opposite direction. The face convention pairs are (0,1), (2,3), (4,5),
    // so opposite = i ^ 1.

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
        let lod_step = lod_to_step(lod);

        // Clamp the step to the minimum among this chunk and all loaded neighbors
        // to prevent border cracks when adjacent chunks have finer LOD levels.
        let effective_step = {
            let mut min_step = lod_step;
            for &offset in &NEIGHBOR_OFFSETS {
                let neighbor_coord = coord + offset;
                if let Some(neighbor_entity) = manager.get_entity(neighbor_coord) {
                    if let Ok((_, _, Some(neighbor_lod))) = chunks.get(neighbor_entity) {
                        let neighbor_step = lod_to_step(neighbor_lod.0);
                        min_step = min_step.min(neighbor_step);
                    }
                }
            }
            min_step
        };

        // Build ChunkNeighbors from the six adjacent loaded chunks.
        let mut neighbors = ChunkNeighbors::<V>::default();
        for (face_index, &offset) in NEIGHBOR_OFFSETS.iter().enumerate() {
            let neighbor_coord = coord + offset;
            if let Some(neighbor_entity) = manager.get_entity(neighbor_coord) {
                if let Ok((neighbor_chunk, _, _)) = chunks.get(neighbor_entity) {
                    // The face of the neighbor that is adjacent to our face_index
                    // is the opposite direction: face_index ^ 1.
                    let opposite_face = face_index ^ 1;
                    neighbors.faces[face_index] = Some(neighbor_chunk.border_slice(opposite_face));
                }
            }
        }

        // Skip meshing entirely for all-air chunks — no render components needed.
        if chunk.is_empty() {
            if maybe_mesh3d.is_some() {
                // Chunk was previously non-empty (e.g. all voxels mined out);
                // strip render components so it becomes invisible.
                commands.entity(entity).remove::<(
                    Mesh3d,
                    MeshMaterial3d<StandardMaterial>,
                    ChunkRendered,
                    ChunkLod,
                )>();
            }
            continue;
        }

        // Run the neighbor-aware greedy mesher at the effective LOD step.
        let new_mesh = generate_chunk_mesh_with_neighbors(chunk, &neighbors, effective_step);

        if new_mesh.count_vertices() == 0 {
            // Mesher produced no geometry (e.g. all faces are hidden by neighbors).
            // Remove render components if they were previously attached.
            if maybe_mesh3d.is_some() {
                commands.entity(entity).remove::<(
                    Mesh3d,
                    MeshMaterial3d<StandardMaterial>,
                    ChunkRendered,
                    ChunkLod,
                )>();
            }
            continue;
        }

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

                // Also remesh each loaded neighbor: their effective_step may
                // change now that this chunk's LOD has changed.
                const NEIGHBOR_OFFSETS: [IVec3; 6] = [
                    IVec3::new(0, 1, 0),
                    IVec3::new(0, -1, 0),
                    IVec3::new(1, 0, 0),
                    IVec3::new(-1, 0, 0),
                    IVec3::new(0, 0, 1),
                    IVec3::new(0, 0, -1),
                ];
                for &offset in &NEIGHBOR_OFFSETS {
                    let neighbor_coord = coord + offset;
                    if manager.is_loaded(neighbor_coord) {
                        modified_writer.write(ChunkModified(neighbor_coord));
                    }
                }
            }
        }
    }
}

// ─── chunk_despawn_system ─────────────────────────────────────────────────────

/// Responds to [`ChunkUnloadRequest`] messages by saving and despawning chunk entities.
///
/// For each unload request this system:
/// 1. If [`PersistenceConfig::auto_save`] is `true`, saves the chunk's voxel
///    data to disk before the entity is despawned.
/// 2. Removes the chunk from [`ChunkManager`] so the streaming system can
///    re-request the same coord if the viewer returns.
/// 3. Despawns the entity.
///
/// Save errors are reported with `eprintln!` and do not panic.
///
/// This system is generic over `V` because it must query the [`Chunk<V>`]
/// component to serialise voxel data. It is registered by [`MeshPlugin`](crate::mesh::MeshPlugin).
pub fn chunk_despawn_system<V: VoxelType>(
    mut commands: Commands,
    mut unload_reader: MessageReader<ChunkUnloadRequest>,
    mut manager: ResMut<ChunkManager>,
    config: Res<PersistenceConfig>,
    chunks: Query<&Chunk<V>>,
) {
    for &ChunkUnloadRequest(coord) in unload_reader.read() {
        // Save before unregistering so the entity is still valid.
        if config.auto_save {
            if let Some(entity) = manager.get_entity(coord) {
                if let Ok(chunk) = chunks.get(entity) {
                    let filename = chunk_persistence::chunk_filename(coord);
                    let path: PathBuf = [config.save_dir.as_str(), &filename].iter().collect();

                    if let Some(parent) = path.parent() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            eprintln!(
                                "voxel-framework: failed to create chunk save directory '{}': {e}",
                                parent.display()
                            );
                        }
                    }

                    if let Err(e) = chunk_persistence::save_chunk(chunk, &path) {
                        eprintln!(
                            "voxel-framework: failed to save chunk {coord} on unload to '{}': {e}",
                            path.display()
                        );
                    }
                }
            }
        }

        if let Some(entity) = manager.unregister(coord) {
            commands.entity(entity).despawn();
        }
    }
}
