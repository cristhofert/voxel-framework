//! Chunk occlusion culling.
//!
//! A chunk is occluded when all 6 of its face-adjacent neighbors are
//! fully solid (every voxel is non-air). In that case none of its
//! border faces are visible and the mesh can be hidden.

use bevy::prelude::*;

use crate::chunk::Chunk;
use crate::generator::GeneratorSystems;
use crate::voxel::VoxelType;
use crate::world::{ChunkManager, ChunkModified};

/// The six face-adjacent neighbor directions for a chunk.
const NEIGHBOR_DIRS: [IVec3; 6] = [
    IVec3::new(1, 0, 0),
    IVec3::new(-1, 0, 0),
    IVec3::new(0, 1, 0),
    IVec3::new(0, -1, 0),
    IVec3::new(0, 0, 1),
    IVec3::new(0, 0, -1),
];

/// Returns true if the chunk at `coord` is surrounded on all 6 sides
/// by fully-solid chunks, meaning it has no visible border faces.
pub fn is_chunk_occluded<V: VoxelType>(
    coord: IVec3,
    manager: &ChunkManager,
    chunks: &Query<&Chunk<V>>,
) -> bool {
    for dir in NEIGHBOR_DIRS {
        let neighbor_coord = coord + dir;
        let Some(entity) = manager.get_entity(neighbor_coord) else {
            return false;
        };
        let Ok(chunk) = chunks.get(entity) else {
            return false;
        };
        if !chunk.is_fully_solid() {
            return false;
        }
    }
    true
}

/// Bevy system that updates [`Visibility`] for chunks based on occlusion.
///
/// Runs every frame in `Update` after [`GeneratorSystems`]. For each
/// [`ChunkModified`] coord and its 6 face-adjacent neighbors, re-evaluates
/// occlusion and sets `Visibility::Hidden` or `Visibility::Visible`
/// accordingly.
pub fn occlusion_update_system<V: VoxelType>(
    mut modified_reader: MessageReader<ChunkModified>,
    manager: Res<ChunkManager>,
    chunks: Query<&Chunk<V>>,
    mut visibility: Query<&mut Visibility>,
) {
    // Collect all coords that need re-evaluation (the modified coord + its 6
    // neighbors). Use a small stack-allocated buffer to avoid heap allocation
    // for the common case.
    let mut to_check: Vec<IVec3> = Vec::new();

    for &ChunkModified(coord) in modified_reader.read() {
        to_check.push(coord);
        for dir in NEIGHBOR_DIRS {
            to_check.push(coord + dir);
        }
    }

    // De-duplicate so we don't re-evaluate the same coord multiple times when
    // several ChunkModified events arrive in the same frame.
    to_check.sort_unstable_by(|a, b| a.x.cmp(&b.x).then(a.y.cmp(&b.y)).then(a.z.cmp(&b.z)));
    to_check.dedup();

    for coord in to_check {
        let Some(entity) = manager.get_entity(coord) else {
            continue;
        };
        let Ok(mut vis) = visibility.get_mut(entity) else {
            continue;
        };
        let occluded = is_chunk_occluded::<V>(coord, &manager, &chunks);
        *vis = if occluded {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
    }
}

// ─── OcclusionPlugin ──────────────────────────────────────────────────────────

/// Plugin that registers [`occlusion_update_system`] into the Bevy app.
///
/// Add this plugin after [`crate::world::WorldPlugin`] and
/// [`crate::generator::GeneratorPlugin`]:
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::{VoxelPlugin, world::occlusion::OcclusionPlugin, voxel::DefaultVoxel};
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(VoxelPlugin)
///     .add_plugins(OcclusionPlugin::<DefaultVoxel>::new())
///     .run();
/// ```
pub struct OcclusionPlugin<V: VoxelType> {
    _marker: std::marker::PhantomData<V>,
}

impl<V: VoxelType> OcclusionPlugin<V> {
    /// Creates a new `OcclusionPlugin`.
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<V: VoxelType> Default for OcclusionPlugin<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: VoxelType> Plugin for OcclusionPlugin<V> {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            occlusion_update_system::<V>.after(GeneratorSystems),
        );
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::CHUNK_SIZE;
    use crate::voxel::DefaultVoxel;

    fn make_solid_chunk() -> Chunk<DefaultVoxel> {
        let mut chunk = Chunk::<DefaultVoxel>::new();
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    chunk.set(x, y, z, DefaultVoxel::Rock);
                }
            }
        }
        chunk
    }

    /// A chunk with a missing neighbor is never occluded.
    #[test]
    fn test_not_occluded_when_neighbor_missing() {
        use bevy::ecs::system::SystemState;

        let mut app = App::new();
        // Spawn the center chunk (solid).
        let center = app.world_mut().spawn(make_solid_chunk()).id();

        // Register only the center chunk — its neighbors are absent.
        let mut manager = ChunkManager::default();
        manager.register(IVec3::ZERO, center);
        app.world_mut().insert_resource(manager);

        // Use SystemState to get properly-initialized Query and Res handles.
        let mut state: SystemState<(Res<ChunkManager>, Query<&Chunk<DefaultVoxel>>)> =
            SystemState::new(app.world_mut());
        let (manager, query) = state.get(app.world());

        let result = is_chunk_occluded::<DefaultVoxel>(IVec3::ZERO, &manager, &query);
        assert!(!result, "chunk with missing neighbor must not be occluded");
    }

    /// A chunk whose neighbor is all-air is never occluded.
    #[test]
    fn test_not_occluded_when_neighbor_not_solid() {
        use bevy::ecs::system::SystemState;

        let mut app = App::new();

        // Spawn center chunk (solid) and one air neighbor; leave the other 5
        // neighbors absent so we can confirm early-return on the air one.
        let center = app.world_mut().spawn(make_solid_chunk()).id();
        let air_neighbor = app
            .world_mut()
            .spawn(Chunk::<DefaultVoxel>::new()) // all-air
            .id();

        let mut manager = ChunkManager::default();
        manager.register(IVec3::ZERO, center);
        manager.register(IVec3::new(1, 0, 0), air_neighbor);
        app.world_mut().insert_resource(manager);

        let mut state: SystemState<(Res<ChunkManager>, Query<&Chunk<DefaultVoxel>>)> =
            SystemState::new(app.world_mut());
        let (manager, query) = state.get(app.world());

        let result = is_chunk_occluded::<DefaultVoxel>(IVec3::ZERO, &manager, &query);
        assert!(!result, "chunk with air neighbor must not be occluded");
    }
}
