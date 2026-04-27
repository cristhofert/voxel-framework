//! Chunk occlusion culling.
//!
//! # Six-neighbour check
//!
//! A chunk is considered **occluded** when every one of its six face-adjacent
//! neighbours (±X, ±Y, ±Z) is **fully solid** — meaning every voxel in that
//! neighbour chunk returns `is_solid()`.  When a chunk is occluded, none of
//! its boundary faces can be seen from outside: the surrounding solid mass
//! hides it completely.  The chunk's `Visibility` component is then set to
//! `Hidden`, skipping both CPU culling and GPU rasterisation.
//!
//! # Performance benefit
//!
//! For underground or densely packed voxel worlds (e.g. caves, the interior
//! of large structures) this can eliminate a large fraction of draw calls
//! without any per-frame ray testing.  The check itself is O(1) per chunk
//! (six neighbour lookups, one `is_fully_solid` call each).
//!
//! # Integration
//!
//! Re-evaluation is triggered by [`crate::world::ChunkModified`] events.
//! Every time a chunk is modified, both that chunk and its six neighbours are
//! re-evaluated so that newly placed or destroyed voxels propagate correctly
//! to adjacent visibility.
//!
//! Add [`OcclusionPlugin`] to your app after
//! [`crate::world::WorldPlugin`] and the generator plugin:
//!
//! ```no_run
//! # use bevy::prelude::*;
//! # use voxel_framework::{VoxelPlugin, world::occlusion::OcclusionPlugin, voxel::DefaultVoxel};
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
//!     .add_plugins(OcclusionPlugin::<DefaultVoxel>::new())
//!     .run();
//! ```

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

/// Returns `true` if the chunk at `coord` is fully occluded by its neighbours.
///
/// A chunk is occluded when **all six** face-adjacent neighbours (±X, ±Y, ±Z)
/// are loaded and every voxel in each neighbour is solid.  If any neighbour is
/// missing (unloaded) or contains at least one non-solid voxel, the function
/// returns `false` and the chunk remains visible.
///
/// # Parameters
///
/// - `coord` — Chunk-grid coordinate of the chunk to test.
/// - `manager` — Chunk registry used to look up neighbour entities.
/// - `chunks` — Read query for chunk voxel data.
///
/// # Return value
///
/// `true` if all six neighbours are fully solid, `false` otherwise.
///
/// # Examples
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use voxel_framework::world::{ChunkManager, occlusion::is_chunk_occluded};
/// # use voxel_framework::chunk::Chunk;
/// # use voxel_framework::voxel::DefaultVoxel;
/// # fn example(manager: &ChunkManager, chunks: &Query<&Chunk<DefaultVoxel>>) {
/// let occluded = is_chunk_occluded::<DefaultVoxel>(IVec3::new(2, 0, 2), manager, chunks);
/// println!("chunk is occluded: {occluded}");
/// # }
/// ```
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
    crate::profile_span!("voxel::world::occlusion");
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

/// Bevy plugin that registers chunk occlusion culling into the app.
///
/// Adds [`occlusion_update_system`] to the `Update` schedule, running
/// **after** `GeneratorSystems`.  The system listens for
/// [`crate::world::ChunkModified`] events and re-evaluates the `Visibility`
/// component for the modified chunk and all six of its face-adjacent neighbours.
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
///     .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
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
