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

use crate::chunk::{CHUNK_SIZE, Chunk, ChunkCoord, ChunkCoordComp, ChunkLod};
use crate::render::material::{ChunkMaterial, ChunkMaterialHandle, ChunkRendered};
use crate::voxel::VoxelType;
use crate::world::persistence::{PersistenceConfig, save_chunk_to_dir};
use crate::world::{ChunkManager, ChunkModified, ChunkUnloadRequest, ViewerPosition};

use super::lod::{compute_lod, compute_lod_with_hysteresis, lod_to_step};
use super::{ChunkNeighbors, generate_chunk_mesh_with_neighbors};

// ─── EmptyMesh ────────────────────────────────────────────────────────────────

/// Marker component attached to chunk entities whose last meshing pass produced
/// zero vertices (all faces were culled by loaded neighbors).
///
/// When a neighbor subsequently loads and triggers a re-mesh that produces
/// geometry, [`chunk_mesh_system`] removes this marker and propagates a
/// [`ChunkModified`] event to all six neighbors so they too can re-evaluate
/// their border faces.  This enables recursive multi-layer propagation: each
/// chunk that transitions from empty → non-empty signals its neighbors, which
/// may then transition themselves the following frame.
#[derive(Component)]
pub struct EmptyMesh;

// ─── PendingNeighborRemesh ────────────────────────────────────────────────────

/// Staging buffer for neighbor re-mesh requests emitted by [`chunk_mesh_system`].
///
/// `chunk_mesh_system` reads `MessageReader<ChunkModified>` (which requires
/// `Res` access to the message storage) and therefore cannot hold a
/// `MessageWriter<ChunkModified>` (which requires `ResMut`) in the same system
/// without triggering a Bevy B0002 access-conflict panic.
///
/// Instead, newly-rendered chunks push their six neighbor coords here, and
/// [`flush_neighbor_remesh_system`] drains the buffer and emits the
/// `ChunkModified` messages in a separate, sequentially-ordered system.
#[derive(Resource, Default)]
pub struct PendingNeighborRemesh(pub Vec<ChunkCoord>);

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
///    `Mesh3d`, `MeshMaterial3d<StandardMaterial>` or `MeshMaterial3d<ChunkMaterial>`,
///    `Transform`, `Visibility`, AABB, [`ChunkRendered`], and [`ChunkLod`].
///
/// If the chunk material has not yet been created (startup system hasn't run)
/// or the entity no longer exists, the event is silently skipped.
pub fn chunk_mesh_system<V: VoxelType>(
    mut commands: Commands,
    mut modified_reader: MessageReader<ChunkModified>,
    mut pending_neighbors: ResMut<PendingNeighborRemesh>,
    manager: Res<ChunkManager>,
    chunk_material: Res<ChunkMaterialHandle>,
    viewer: Res<ViewerPosition>,
    mut meshes: ResMut<Assets<Mesh>>,
    chunks: Query<(
        &Chunk<V>,
        Option<&Mesh3d>,
        Option<&ChunkLod>,
        Has<EmptyMesh>,
    )>,
) {
    // Determine which material path is active.
    // Custom (texture array) takes priority; fall back to StandardMaterial.
    let use_custom = chunk_material.custom.is_some();
    let has_material = chunk_material.custom.is_some() || chunk_material.standard.is_some();
    if !has_material {
        // Material not yet set up — skip until next frame.
        return;
    }

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

        let Ok((chunk, maybe_mesh3d, _maybe_lod, has_empty_mesh)) = chunks.get(entity) else {
            // Chunk component not yet attached — deferred command not flushed yet.
            // Re-queue so we retry next frame when the component is available.
            pending_neighbors.0.push(coord);
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
                    if let Ok((_, _, Some(neighbor_lod), _)) = chunks.get(neighbor_entity) {
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
                if let Ok((neighbor_chunk, _, _, _)) = chunks.get(neighbor_entity) {
                    // The face of the neighbor that is adjacent to our face_index
                    // is the opposite direction: face_index ^ 1.
                    let opposite_face = face_index ^ 1;
                    neighbors.faces[face_index] = Some(neighbor_chunk.border_slice(opposite_face));
                }
            }
        }

        // Helper: remove all material-related render components from an entity.
        // Both MeshMaterial3d variants must be removed regardless of which path
        // is currently active, because the entity might have been created under
        // the other path (e.g. if ChunkTextureArray was inserted mid-session).
        let remove_render = |cmd: &mut EntityCommands| {
            cmd.remove::<(
                Mesh3d,
                MeshMaterial3d<StandardMaterial>,
                MeshMaterial3d<ChunkMaterial>,
                ChunkRendered,
                ChunkLod,
            )>();
        };

        // Skip meshing entirely for all-air chunks — no render components needed.
        if chunk.is_empty() {
            if maybe_mesh3d.is_some() {
                // Chunk was previously non-empty (e.g. all voxels mined out);
                // strip render components so it becomes invisible.
                let mut ec = commands.entity(entity);
                remove_render(&mut ec);
                // Notify neighbors — they may now have exposed border faces.
                for &offset in &NEIGHBOR_OFFSETS {
                    let neighbor_coord = coord + offset;
                    if manager.is_loaded(neighbor_coord) {
                        pending_neighbors.0.push(neighbor_coord);
                    }
                }
            } else if !has_empty_mesh {
                // First time seeing this air chunk — notify neighbors so they
                // can generate faces that border this empty space.
                commands.entity(entity).insert(EmptyMesh);
                for &offset in &NEIGHBOR_OFFSETS {
                    let neighbor_coord = coord + offset;
                    if manager.is_loaded(neighbor_coord) {
                        pending_neighbors.0.push(neighbor_coord);
                    }
                }
            }
            continue;
        }

        // Run the neighbor-aware greedy mesher at the effective LOD step.
        let new_mesh = generate_chunk_mesh_with_neighbors(chunk, &neighbors, effective_step);

        if new_mesh.count_vertices() == 0 {
            // Mesher produced no geometry (e.g. all faces are hidden by neighbors).
            // Remove render components if they were previously attached.
            if maybe_mesh3d.is_some() {
                let mut ec = commands.entity(entity);
                remove_render(&mut ec);
            }
            // Tag this entity so we know its mesh is empty. When a neighbor
            // loads later and triggers another re-mesh that produces geometry,
            // we propagate the signal to our own neighbors at that point.
            if !has_empty_mesh {
                commands.entity(entity).insert(EmptyMesh);
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

            // If this chunk was previously empty (all faces culled) and now has
            // geometry, its border faces changed from invisible to visible.
            // Propagate to neighbors so they can re-evaluate their own faces.
            // This enables multi-layer propagation: each depth level transitions
            // one frame at a time until all chunks converge.
            if has_empty_mesh {
                commands.entity(entity).remove::<EmptyMesh>();
                for &offset in &NEIGHBOR_OFFSETS {
                    let neighbor_coord = coord + offset;
                    if manager.is_loaded(neighbor_coord) {
                        pending_neighbors.0.push(neighbor_coord);
                    }
                }
            }
        } else {
            // First-time path: allocate new handle and attach all render components.
            let mesh_handle = meshes.add(new_mesh);

            // World-space origin of this chunk's minimum corner.
            let translation = coord.as_vec3() * chunk_size_f;

            // AABB sized to the full chunk extent for frustum culling.
            let aabb = Aabb::from_min_max(Vec3::ZERO, Vec3::splat(chunk_size_f));

            let mut entity_cmd = commands.entity(entity);

            if use_custom {
                // Texture-array path.
                let mat = chunk_material.custom.as_ref().unwrap().clone();
                entity_cmd.insert((
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(mat),
                    Transform::from_translation(translation),
                    Visibility::default(),
                    aabb,
                    ChunkRendered,
                    ChunkLod(lod),
                ));
            } else {
                // Fallback StandardMaterial path.
                let mat = chunk_material.standard.as_ref().unwrap().clone();
                entity_cmd.insert((
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(mat),
                    Transform::from_translation(translation),
                    Visibility::default(),
                    aabb,
                    ChunkRendered,
                    ChunkLod(lod),
                ));
            }

            // If this chunk had EmptyMesh from a previous 0-vertex pass, remove it.
            if has_empty_mesh {
                commands.entity(entity).remove::<EmptyMesh>();
            }

            // A newly rendered chunk changes the border visibility of every
            // adjacent chunk.  Stage ChunkModified for each loaded neighbor so
            // they re-mesh and cull (or un-cull) the faces that touch this chunk.
            // Messages are emitted by flush_neighbor_remesh_system (runs after
            // this system) to avoid a ResMut/Res conflict on the message storage.
            // This is only done on the first-time path to avoid cascading
            // re-meshes on every LOD or voxel-edit update.
            for &offset in &NEIGHBOR_OFFSETS {
                let neighbor_coord = coord + offset;
                if manager.is_loaded(neighbor_coord) {
                    pending_neighbors.0.push(neighbor_coord);
                }
            }
        }
    }
}

// ─── flush_neighbor_remesh_system ────────────────────────────────────────────

/// Drains [`PendingNeighborRemesh`] and emits a [`ChunkModified`] message for
/// each staged coord.
///
/// This system exists solely to decouple the *write* of `ChunkModified` from
/// [`chunk_mesh_system`], which holds a `MessageReader<ChunkModified>` (i.e.
/// `Res` access to the message store).  Having both `Res` and `ResMut` on the
/// same resource in one system triggers Bevy's B0002 access-conflict panic.
///
/// By staging neighbor coords in [`PendingNeighborRemesh`] and draining them
/// here — after `chunk_mesh_system` has finished — we keep the two accesses in
/// separate systems, which Bevy schedules sequentially via `.after()`.
pub fn flush_neighbor_remesh_system(
    mut pending: ResMut<PendingNeighborRemesh>,
    mut modified_writer: MessageWriter<ChunkModified>,
) {
    for coord in pending.0.drain(..) {
        modified_writer.write(ChunkModified(coord));
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

        let new_lod =
            compute_lod_with_hysteresis(chunk_center, viewer.position, current_lod.0, 5.0);

        if new_lod != current_lod.0 {
            // Find the coord for this entity so we can emit ChunkModified(coord).
            // Chunks spawned by the framework carry a ChunkCoordComp, so this is
            // a direct lookup; only entities without one fall back to an O(n)
            // scan of the manager's loaded set.
            let registered_coord = maybe_coord_comp
                .map(|c| c.coord())
                .filter(|&c| manager.get_entity(c) == Some(entity))
                .or_else(|| {
                    manager
                        .iter_loaded()
                        .find_map(|(coord, e)| if e == entity { Some(coord) } else { None })
                });
            if let Some(coord) = registered_coord {
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
/// 1. If [`PersistenceConfig::auto_save`] is `true` and the chunk has unsaved
///    changes ([`Chunk::is_dirty`]), saves it to disk before the entity is
///    despawned. Unmodified chunks are not written: they can be generated or
///    restored again.
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
    config: Option<Res<PersistenceConfig>>,
    chunks: Query<&Chunk<V>>,
) {
    crate::profile_span!("voxel::world::chunk_unload");
    for &ChunkUnloadRequest(coord) in unload_reader.read() {
        // Save before unregistering so the entity is still valid.
        if let Some(config) = config.as_ref().filter(|c| c.auto_save)
            && let Some(entity) = manager.get_entity(coord)
            && let Ok(chunk) = chunks.get(entity)
            && chunk.is_dirty()
        {
            save_chunk_to_dir(chunk, coord, &config.save_dir, " on unload");
        }

        if let Some(entity) = manager.unregister(coord) {
            commands.entity(entity).despawn();
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::persistence as chunk_persistence;
    use crate::test_support::{filled_chunk, world_with_chunks};
    use crate::voxel::DefaultVoxel;
    use bevy::ecs::message::Messages;
    use bevy::ecs::system::RunSystemOnce;

    type V = DefaultVoxel;

    /// World with the resources the mesh systems read, plus a standard
    /// material so `chunk_mesh_system` does not early-return.
    fn mesh_world(chunks: impl IntoIterator<Item = (IVec3, Chunk<V>)>) -> World {
        let mut world = world_with_chunks(chunks);
        world.init_resource::<Messages<ChunkModified>>();
        world.init_resource::<Messages<ChunkUnloadRequest>>();
        world.init_resource::<PendingNeighborRemesh>();
        world.init_resource::<ViewerPosition>();
        world.init_resource::<Assets<Mesh>>();
        let mut materials = Assets::<StandardMaterial>::default();
        let standard = materials.add(StandardMaterial::default());
        world.insert_resource(materials);
        world.insert_resource(ChunkMaterialHandle {
            standard: Some(standard),
            custom: None,
        });
        world
    }

    fn entity_at(world: &World, coord: IVec3) -> Entity {
        world
            .resource::<ChunkManager>()
            .get_entity(coord)
            .expect("chunk should be registered")
    }

    fn modify(world: &mut World, coord: IVec3) {
        world.write_message(ChunkModified(coord));
    }

    fn run_mesh(world: &mut World) {
        world
            .run_system_once(chunk_mesh_system::<V>)
            .expect("chunk_mesh_system should run");
    }

    /// Clears the message buffer so the next run only sees new messages.
    fn clear_modified(world: &mut World) {
        world.resource_mut::<Messages<ChunkModified>>().clear();
    }

    fn pending(world: &World) -> Vec<IVec3> {
        let mut coords = world.resource::<PendingNeighborRemesh>().0.clone();
        coords.sort_by_key(|c| (c.x, c.y, c.z));
        coords
    }

    fn written_modified(world: &World) -> Vec<IVec3> {
        let messages = world.resource::<Messages<ChunkModified>>();
        let mut coords: Vec<IVec3> = messages
            .iter_current_update_messages()
            .map(|m| m.0)
            .collect();
        coords.sort_by_key(|c| (c.x, c.y, c.z));
        coords
    }

    fn mesh_vertex_count(world: &World, entity: Entity) -> usize {
        let handle = &world.entity(entity).get::<Mesh3d>().expect("Mesh3d").0;
        world
            .resource::<Assets<Mesh>>()
            .get(handle)
            .expect("mesh asset")
            .count_vertices()
    }

    fn single_voxel_chunk() -> Chunk<V> {
        let mut chunk = Chunk::<V>::new();
        chunk.set(8, 8, 8, DefaultVoxel::Rock);
        chunk
    }

    // ── chunk_mesh_system ────────────────────────────────────────────────────

    #[test]
    fn solid_chunk_gets_render_components_at_its_world_position() {
        let coord = IVec3::new(1, -2, 3);
        let mut world = mesh_world([(coord, single_voxel_chunk())]);
        modify(&mut world, coord);
        run_mesh(&mut world);

        let entity = entity_at(&world, coord);
        let e = world.entity(entity);
        assert!(e.contains::<MeshMaterial3d<StandardMaterial>>());
        assert!(e.contains::<ChunkRendered>());
        assert!(e.contains::<Aabb>());
        assert!(!e.contains::<EmptyMesh>());
        // ~65 units from the viewer at the origin → full-detail LOD 0.
        assert_eq!(e.get::<ChunkLod>().map(|l| l.0), Some(0));
        assert_eq!(
            e.get::<Transform>().unwrap().translation,
            coord.as_vec3() * CHUNK_SIZE as f32
        );
        // One isolated voxel → 6 quads → 24 vertices.
        assert_eq!(mesh_vertex_count(&world, entity), 24);
    }

    #[test]
    fn new_mesh_queues_loaded_neighbors_for_remesh() {
        let coord = IVec3::ZERO;
        let neighbor_x = IVec3::X;
        let neighbor_down = IVec3::NEG_Y;
        let mut world = mesh_world([
            (coord, single_voxel_chunk()),
            (neighbor_x, Chunk::<V>::new()),
            (neighbor_down, Chunk::<V>::new()),
        ]);
        modify(&mut world, coord);
        run_mesh(&mut world);

        assert_eq!(pending(&world), vec![IVec3::NEG_Y, IVec3::X]);
    }

    #[test]
    fn no_material_means_no_meshing() {
        let coord = IVec3::ZERO;
        let mut world = mesh_world([(coord, single_voxel_chunk())]);
        world.insert_resource(ChunkMaterialHandle {
            standard: None,
            custom: None,
        });
        modify(&mut world, coord);
        run_mesh(&mut world);

        assert!(!world.entity(entity_at(&world, coord)).contains::<Mesh3d>());
    }

    #[test]
    fn empty_chunk_is_marked_empty_without_a_mesh() {
        let coord = IVec3::ZERO;
        let mut world = mesh_world([(coord, Chunk::<V>::new()), (IVec3::Z, Chunk::<V>::new())]);
        modify(&mut world, coord);
        run_mesh(&mut world);

        let e = world.entity(entity_at(&world, coord));
        assert!(e.contains::<EmptyMesh>());
        assert!(!e.contains::<Mesh3d>());
        assert_eq!(pending(&world), vec![IVec3::Z]);
    }

    #[test]
    fn chunk_emptied_after_meshing_loses_its_render_components() {
        let coord = IVec3::ZERO;
        let mut world = mesh_world([(coord, single_voxel_chunk())]);
        modify(&mut world, coord);
        run_mesh(&mut world);
        let entity = entity_at(&world, coord);
        assert!(world.entity(entity).contains::<Mesh3d>());

        // Mine the only voxel and remesh.
        world
            .get_mut::<Chunk<V>>(entity)
            .unwrap()
            .set(8, 8, 8, DefaultVoxel::Air);
        clear_modified(&mut world);
        modify(&mut world, coord);
        run_mesh(&mut world);

        let e = world.entity(entity);
        assert!(!e.contains::<Mesh3d>());
        assert!(!e.contains::<MeshMaterial3d<StandardMaterial>>());
        assert!(!e.contains::<ChunkRendered>());
        assert!(!e.contains::<ChunkLod>());
    }

    #[test]
    fn remesh_updates_the_existing_mesh_asset_in_place() {
        let coord = IVec3::ZERO;
        let mut world = mesh_world([(coord, single_voxel_chunk())]);
        modify(&mut world, coord);
        run_mesh(&mut world);
        let entity = entity_at(&world, coord);
        let handle_before = world.entity(entity).get::<Mesh3d>().unwrap().0.id();
        let assets_before = world.resource::<Assets<Mesh>>().len();

        // A second, separate voxel adds 6 more quads.
        world
            .get_mut::<Chunk<V>>(entity)
            .unwrap()
            .set(2, 2, 2, DefaultVoxel::Rock);
        clear_modified(&mut world);
        modify(&mut world, coord);
        run_mesh(&mut world);

        let handle_after = world.entity(entity).get::<Mesh3d>().unwrap().0.id();
        assert_eq!(handle_before, handle_after, "mesh handle must be reused");
        assert_eq!(world.resource::<Assets<Mesh>>().len(), assets_before);
        assert_eq!(mesh_vertex_count(&world, entity), 48);
    }

    #[test]
    fn fully_enclosed_solid_chunk_produces_no_mesh() {
        // A solid chunk surrounded by solid chunks on all six faces has no
        // visible face, so it must be marked empty instead of rendered.
        let center = IVec3::ZERO;
        let mut chunks = vec![(center, filled_chunk(DefaultVoxel::Rock))];
        for offset in [
            IVec3::X,
            IVec3::NEG_X,
            IVec3::Y,
            IVec3::NEG_Y,
            IVec3::Z,
            IVec3::NEG_Z,
        ] {
            chunks.push((offset, filled_chunk(DefaultVoxel::Rock)));
        }
        let mut world = mesh_world(chunks);
        modify(&mut world, center);
        run_mesh(&mut world);

        let e = world.entity(entity_at(&world, center));
        assert!(e.contains::<EmptyMesh>());
        assert!(!e.contains::<Mesh3d>());
    }

    #[test]
    fn modified_entity_without_chunk_data_is_retried() {
        // The coord is registered but the generator has not inserted the
        // Chunk component yet: the coord goes back to the pending queue.
        let coord = IVec3::new(4, 0, 0);
        let mut world = mesh_world(std::iter::empty());
        let entity = world.spawn_empty().id();
        world.resource_mut::<ChunkManager>().register(coord, entity);
        modify(&mut world, coord);
        run_mesh(&mut world);

        assert_eq!(pending(&world), vec![coord]);
    }

    // ── flush_neighbor_remesh_system ─────────────────────────────────────────

    #[test]
    fn flush_turns_pending_coords_into_chunk_modified_messages() {
        let mut world = mesh_world(std::iter::empty());
        world.resource_mut::<PendingNeighborRemesh>().0 = vec![IVec3::X, IVec3::NEG_Z];
        world
            .run_system_once(flush_neighbor_remesh_system)
            .expect("flush should run");

        assert!(pending(&world).is_empty());
        assert_eq!(written_modified(&world), vec![IVec3::NEG_Z, IVec3::X]);
    }

    // ── lod_update_system ────────────────────────────────────────────────────

    fn rendered_chunk_world(coord: IVec3, lod: u8, with_coord_comp: bool) -> World {
        let mut world = mesh_world([
            (coord, single_voxel_chunk()),
            (coord + IVec3::Y, Chunk::new()),
        ]);
        let entity = entity_at(&world, coord);
        let mut e = world.entity_mut(entity);
        e.insert((
            Transform::from_translation(coord.as_vec3() * CHUNK_SIZE as f32),
            ChunkLod(lod),
            ChunkRendered,
        ));
        if with_coord_comp {
            e.insert(ChunkCoordComp(coord));
        }
        world
    }

    fn run_lod(world: &mut World) {
        world
            .run_system_once(lod_update_system::<V>)
            .expect("lod_update_system should run");
    }

    #[test]
    fn lod_change_remeshes_the_chunk_and_its_loaded_neighbors() {
        for with_coord_comp in [true, false] {
            // Viewer at the origin, chunk ~0 units away, but stored as LOD 1.
            let coord = IVec3::ZERO;
            let mut world = rendered_chunk_world(coord, 1, with_coord_comp);
            run_lod(&mut world);

            assert_eq!(
                written_modified(&world),
                vec![IVec3::ZERO, IVec3::Y],
                "with_coord_comp = {with_coord_comp}"
            );
        }
    }

    #[test]
    fn unchanged_lod_emits_nothing() {
        let coord = IVec3::ZERO;
        let mut world = rendered_chunk_world(coord, 0, true);
        run_lod(&mut world);

        assert!(written_modified(&world).is_empty());
    }

    #[test]
    fn far_chunk_downgrades_to_coarse_lod() {
        // 20 chunks away = 320 units, well past the 200-unit threshold.
        let coord = IVec3::new(20, 0, 0);
        let mut world = rendered_chunk_world(coord, 0, true);
        run_lod(&mut world);

        assert_eq!(written_modified(&world), vec![coord, coord + IVec3::Y]);
    }

    // ── chunk_despawn_system ─────────────────────────────────────────────────

    fn run_despawn(world: &mut World, coord: IVec3) {
        world.write_message(ChunkUnloadRequest(coord));
        world
            .run_system_once(chunk_despawn_system::<V>)
            .expect("chunk_despawn_system should run");
    }

    #[test]
    fn unload_with_auto_save_writes_the_chunk_and_despawns_it() {
        let temp = tempfile::tempdir().unwrap();
        let save_dir = temp.path().join("nested").join("world");
        let coord = IVec3::new(-3, 1, 2);
        let mut world = mesh_world([(coord, single_voxel_chunk())]);
        world.insert_resource(PersistenceConfig {
            save_dir: save_dir.to_string_lossy().into_owned(),
            auto_save: true,
        });
        let entity = entity_at(&world, coord);

        run_despawn(&mut world, coord);

        assert!(
            world.get_entity(entity).is_err(),
            "entity should be despawned"
        );
        assert!(!world.resource::<ChunkManager>().is_loaded(coord));
        let path = save_dir.join(chunk_persistence::chunk_filename(coord));
        let saved: Chunk<V> = chunk_persistence::load_chunk(&path).expect("chunk saved on unload");
        assert_eq!(saved.get(8, 8, 8), DefaultVoxel::Rock);
        assert_eq!(saved.get(0, 0, 0), DefaultVoxel::Air);
    }

    #[test]
    fn unload_of_an_unmodified_chunk_writes_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let coord = IVec3::new(2, 0, -1);
        let mut chunk = single_voxel_chunk();
        // As handed out by the generator pipeline: nothing to save.
        chunk.mark_clean();
        let mut world = mesh_world([(coord, chunk)]);
        world.insert_resource(PersistenceConfig {
            save_dir: temp.path().to_string_lossy().into_owned(),
            auto_save: true,
        });
        let entity = entity_at(&world, coord);

        run_despawn(&mut world, coord);

        assert!(world.get_entity(entity).is_err());
        assert!(!world.resource::<ChunkManager>().is_loaded(coord));
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }

    #[test]
    fn unload_writes_a_chunk_whose_only_change_is_a_state_byte() {
        let temp = tempfile::tempdir().unwrap();
        let coord = IVec3::ZERO;
        let mut chunk = single_voxel_chunk();
        chunk.mark_clean();
        // State-only edits emit no ChunkModified, so unload is their only save.
        chunk.set_state(8, 8, 8, 9);
        let mut world = mesh_world([(coord, chunk)]);
        world.insert_resource(PersistenceConfig {
            save_dir: temp.path().to_string_lossy().into_owned(),
            auto_save: true,
        });

        run_despawn(&mut world, coord);

        let path = temp.path().join(chunk_persistence::chunk_filename(coord));
        let saved: Chunk<V> = chunk_persistence::load_chunk(&path).expect("chunk saved on unload");
        assert_eq!(saved.get_state(8, 8, 8), 9);
    }

    #[test]
    fn unload_without_auto_save_despawns_without_writing() {
        let temp = tempfile::tempdir().unwrap();
        let coord = IVec3::ZERO;
        let mut world = mesh_world([(coord, single_voxel_chunk())]);
        world.insert_resource(PersistenceConfig {
            save_dir: temp.path().to_string_lossy().into_owned(),
            auto_save: false,
        });
        let entity = entity_at(&world, coord);

        run_despawn(&mut world, coord);

        assert!(world.get_entity(entity).is_err());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }

    #[test]
    fn unload_without_persistence_config_still_despawns() {
        let coord = IVec3::ZERO;
        let mut world = mesh_world([(coord, single_voxel_chunk())]);
        let entity = entity_at(&world, coord);

        run_despawn(&mut world, coord);

        assert!(world.get_entity(entity).is_err());
        assert!(!world.resource::<ChunkManager>().is_loaded(coord));
    }
}
