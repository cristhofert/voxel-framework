//! Integration tests for voxel-framework.
//!
//! Mostly pure logic; the runtime contract tests use a headless `App` with
//! `MinimalPlugins` (no display required).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::math::{IVec3, Vec3};
use bevy::mesh::VertexAttributeValues;
use bevy::mesh::{Indices, Mesh};
use bevy::prelude::{
    App, IntoScheduleConfigs, MinimalPlugins, Query, Res, ResMut, Resource, Update,
};

use voxel_framework::chunk::persistence::{chunk_filename, save_chunk};
use voxel_framework::chunk::{CHUNK_SIZE, Chunk};
use voxel_framework::generator::asteroid::{AsteroidConfig, AsteroidGenerator};
use voxel_framework::generator::{ChunkGenerator, GeneratorPlugin};
use voxel_framework::mesh::lod::{compute_lod, lod_to_step};
use voxel_framework::mesh::{
    ChunkNeighbors, generate_chunk_mesh, generate_chunk_mesh_with_neighbors,
};
use voxel_framework::noise::fbm;
use voxel_framework::voxel::{DefaultVoxel, VoxelType};
use voxel_framework::world::{
    ChunkLoadRequest, ChunkManager, ChunkModified, MiningConfig, MiningPlugin, MiningProgress,
    MiningState, PersistenceConfig, ViewerPosition, VoxelBroken, WorldPlugin, tick_mining,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn vertex_count(mesh: &Mesh) -> usize {
    match mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .expect("mesh has no ATTRIBUTE_POSITION")
    {
        VertexAttributeValues::Float32x3(v) => v.len(),
        _ => panic!("unexpected position attribute type"),
    }
}

fn index_count(mesh: &Mesh) -> usize {
    match mesh.indices().expect("mesh has no indices") {
        Indices::U32(idx) => idx.len(),
        _ => panic!("expected U32 indices"),
    }
}

fn fill_chunk(voxel: DefaultVoxel) -> Chunk<DefaultVoxel> {
    let mut chunk = Chunk::<DefaultVoxel>::new();
    for x in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                chunk.set(x, y, z, voxel);
            }
        }
    }
    chunk
}

fn solid_count(chunk: &Chunk<DefaultVoxel>) -> usize {
    (0..CHUNK_SIZE)
        .flat_map(|x| (0..CHUNK_SIZE).flat_map(move |y| (0..CHUNK_SIZE).map(move |z| (x, y, z))))
        .filter(|&(x, y, z)| chunk.is_solid_at(x, y, z))
        .count()
}

#[derive(Resource, Default)]
struct ObservedLoadRequests(Vec<IVec3>);

fn record_load_requests(
    mut reader: MessageReader<ChunkLoadRequest>,
    mut observed: ResMut<ObservedLoadRequests>,
) {
    observed.0.extend(reader.read().map(|request| request.0));
}

#[derive(Resource, Default)]
struct ObservedBrokenBytes(Vec<u8>);

fn mine_origin_once(
    mut state: ResMut<MiningState>,
    config: Res<MiningConfig>,
    manager: Res<ChunkManager>,
    mut chunks: Query<&mut Chunk<DefaultVoxel>>,
    mut broken: MessageWriter<VoxelBroken>,
    mut progress: MessageWriter<MiningProgress>,
    mut modified: MessageWriter<ChunkModified>,
) {
    tick_mining(
        IVec3::ZERO,
        (0, 0, 0),
        99.0,
        &mut state,
        &config,
        &manager,
        &mut chunks,
        &mut broken,
        &mut progress,
        &mut modified,
    );
}

fn record_broken_bytes(
    mut reader: MessageReader<VoxelBroken>,
    mut observed: ResMut<ObservedBrokenBytes>,
) {
    observed
        .0
        .extend(reader.read().map(|event| event.voxel_byte));
}

struct CountingGenerator {
    calls: Arc<AtomicUsize>,
}

impl ChunkGenerator<DefaultVoxel> for CountingGenerator {
    fn generate(&self, _coord: IVec3, chunk: &mut Chunk<DefaultVoxel>) {
        self.calls.fetch_add(1, Ordering::SeqCst);
        chunk.set(0, 0, 0, DefaultVoxel::IronOre);
    }
}

// ── 1. Chunk + Voxel roundtrip ────────────────────────────────────────────────

#[test]
fn chunk_fill_and_read() {
    let chunk = fill_chunk(DefaultVoxel::Rock);
    for x in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                assert_eq!(
                    chunk.get(x, y, z),
                    DefaultVoxel::Rock,
                    "expected Rock at ({x}, {y}, {z})"
                );
            }
        }
    }
}

#[test]
fn chunk_is_fully_solid_after_fill() {
    let chunk = fill_chunk(DefaultVoxel::Rock);
    assert!(
        chunk.is_fully_solid(),
        "fully filled chunk must report is_fully_solid"
    );
    assert!(
        !chunk.is_empty(),
        "fully filled chunk must not report is_empty"
    );
}

#[test]
fn chunk_is_empty_on_new() {
    let chunk = Chunk::<DefaultVoxel>::new();
    assert!(chunk.is_empty(), "new chunk must be empty");
    assert!(!chunk.is_fully_solid(), "new chunk must not be fully solid");
}

// ── 2. Mesh generation ────────────────────────────────────────────────────────

#[test]
fn empty_chunk_produces_no_geometry() {
    let chunk = Chunk::<DefaultVoxel>::new();
    let mesh = generate_chunk_mesh(&chunk, 1);
    assert_eq!(
        vertex_count(&mesh),
        0,
        "air-only chunk should produce 0 vertices"
    );
}

#[test]
fn single_voxel_produces_6_faces() {
    let mut chunk = Chunk::<DefaultVoxel>::new();
    chunk.set(0, 0, 0, DefaultVoxel::Rock);
    let mesh = generate_chunk_mesh(&chunk, 1);

    // 4 vertices per face × 6 faces = 24 vertices
    assert_eq!(
        vertex_count(&mesh),
        24,
        "single voxel should produce 24 vertices (4 per face × 6 faces)"
    );
    // 6 indices per face × 6 faces = 36 indices
    assert_eq!(
        index_count(&mesh),
        36,
        "single voxel should produce 36 indices (6 per face × 6 faces)"
    );
}

#[test]
fn full_chunk_only_has_outer_faces() {
    // Greedy meshing merges the entire surface of a uniform solid chunk into
    // one quad per face direction — 6 quads total.
    let chunk = fill_chunk(DefaultVoxel::Rock);
    let mesh = generate_chunk_mesh(&chunk, 1);

    assert_eq!(
        vertex_count(&mesh),
        24,
        "fully solid chunk should produce 24 vertices (4 per face × 6 outer faces)"
    );
    assert_eq!(
        index_count(&mesh),
        36,
        "fully solid chunk should produce 36 indices (6 per face × 6 outer faces)"
    );
}

#[test]
fn lod_1_produces_coarser_mesh_than_lod_0() {
    // Checkerboard of two solid voxel types: LOD 0 cannot merge across type
    // boundaries (many small quads), LOD 1 collapses 2×2×2 super-voxels so
    // much more merging is possible.
    let mut chunk = Chunk::<DefaultVoxel>::new();
    for x in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let voxel = if (x + y + z) % 2 == 0 {
                    DefaultVoxel::Rock
                } else {
                    DefaultVoxel::IronOre
                };
                chunk.set(x, y, z, voxel);
            }
        }
    }

    let mesh_lod0 = generate_chunk_mesh(&chunk, lod_to_step(0)); // step = 1
    let mesh_lod1 = generate_chunk_mesh(&chunk, lod_to_step(1)); // step = 2

    let verts_lod0 = vertex_count(&mesh_lod0);
    let verts_lod1 = vertex_count(&mesh_lod1);

    assert!(
        verts_lod1 <= verts_lod0,
        "LOD 1 ({verts_lod1} vertices) should be <= LOD 0 ({verts_lod0} vertices)"
    );
}

// ── 3. Generator determinism ──────────────────────────────────────────────────

#[test]
fn asteroid_generator_is_deterministic() {
    let generator = AsteroidGenerator::new(AsteroidConfig::default());
    let coord = bevy::math::IVec3::new(0, 0, 0);

    let mut chunk_a = Chunk::<DefaultVoxel>::new();
    generator.generate(coord, &mut chunk_a);

    let mut chunk_b = Chunk::<DefaultVoxel>::new();
    generator.generate(coord, &mut chunk_b);

    for x in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                assert_eq!(
                    chunk_a.get(x, y, z),
                    chunk_b.get(x, y, z),
                    "voxel mismatch at ({x}, {y}, {z}): generation must be deterministic"
                );
            }
        }
    }
}

#[test]
fn asteroid_generator_different_coords_can_differ() {
    // The first asteroid is always at the origin, so chunks at different
    // positions within the asteroid may differ. We generate coord (0,0,0) and
    // a far-away coord and expect at least one pair of voxels to differ (the
    // far one should be all-air, which differs from the near one).
    let generator = AsteroidGenerator::new(AsteroidConfig::default());

    let mut chunk_near = Chunk::<DefaultVoxel>::new();
    generator.generate(bevy::math::IVec3::new(0, 0, 0), &mut chunk_near);

    let mut chunk_far = Chunk::<DefaultVoxel>::new();
    generator.generate(bevy::math::IVec3::new(1000, 1000, 1000), &mut chunk_far);

    let near_has_solid = solid_count(&chunk_near) > 0;
    let far_is_empty = chunk_far.is_empty();

    // If the near chunk has any solid content and the far one is empty,
    // they are provably different.
    assert!(
        near_has_solid || far_is_empty,
        "expected the two chunks to differ in content"
    );

    // They should not be identical — at least one voxel must differ.
    let identical = (0..CHUNK_SIZE).all(|x| {
        (0..CHUNK_SIZE)
            .all(|y| (0..CHUNK_SIZE).all(|z| chunk_near.get(x, y, z) == chunk_far.get(x, y, z)))
    });
    assert!(
        !identical,
        "chunk at (0,0,0) and chunk at (1000,1000,1000) should not be identical"
    );
}

#[test]
fn asteroid_generator_produces_solid_voxels() {
    // The first asteroid is always centered at the world origin. A config with
    // a large enough radius guarantees solid voxels in the origin chunk.
    let config = AsteroidConfig {
        asteroid_count: 1,
        min_radius: 30.0,
        max_radius: 30.0,
        ..AsteroidConfig::default()
    };
    let generator = AsteroidGenerator::new(config);

    let mut chunk = Chunk::<DefaultVoxel>::new();
    generator.generate(bevy::math::IVec3::new(0, 0, 0), &mut chunk);

    assert!(
        solid_count(&chunk) > 0,
        "chunks near the asteroid center must contain at least one solid voxel"
    );
}

// ── 4. Noise ──────────────────────────────────────────────────────────────────

#[test]
fn fbm_noise_is_bounded() {
    // fbm is documented to return values in approximately [-1.0, 1.0].
    // We allow a tiny epsilon for floating-point accumulation.
    for i in 0..500 {
        let x = (i as f32) * 0.37 - 92.5;
        let y = (i as f32) * 0.23 + 3.0;
        let z = (i as f32) * 0.53 - 60.0;
        let v = fbm(x, y, z, 42, 4, 2.5, 2.0, 0.5);
        assert!(
            (-1.01..=1.01).contains(&v),
            "fbm({x}, {y}, {z}) = {v} is out of the expected [-1, 1] range"
        );
    }
}

#[test]
fn fbm_noise_varies_spatially() {
    // Sampling at a stride of 1.0 should not produce identical values.
    let values: Vec<f32> = (0..20)
        .map(|i| fbm(i as f32, 0.0, 0.0, 1234, 4, 2.5, 2.0, 0.5))
        .collect();

    let all_identical = values.windows(2).all(|w| w[0] == w[1]);
    assert!(
        !all_identical,
        "fbm must produce varying values across spatial samples, but all 20 were identical"
    );
}

// ── 5. LOD thresholds ─────────────────────────────────────────────────────────

#[test]
fn lod_0_within_200_units() {
    let lod = compute_lod(Vec3::new(25.0, 0.0, 0.0), Vec3::ZERO);
    assert_eq!(lod, 0, "distance 25 should map to LOD 0");

    let lod_mid = compute_lod(Vec3::new(100.0, 0.0, 0.0), Vec3::ZERO);
    assert_eq!(lod_mid, 0, "distance 100 should map to LOD 0");

    // Boundary: just under 200 is still LOD 0
    let lod_boundary = compute_lod(Vec3::new(199.9, 0.0, 0.0), Vec3::ZERO);
    assert_eq!(lod_boundary, 0, "distance 199.9 should still be LOD 0");
}

#[test]
fn lod_1_at_and_beyond_200_units() {
    // Exactly 200.0 is NOT < 200.0, so it maps to LOD 1.
    let lod_at_200 = compute_lod(Vec3::new(200.0, 0.0, 0.0), Vec3::ZERO);
    assert_eq!(lod_at_200, 1, "distance exactly 200 should map to LOD 1");

    let lod_far = compute_lod(Vec3::new(500.0, 0.0, 0.0), Vec3::ZERO);
    assert_eq!(lod_far, 1, "distance 500 should map to LOD 1");
}

// ── 6. Runtime plugin contracts ──────────────────────────────────────────────

#[test]
fn world_and_generator_plugins_can_run_in_minimal_app_without_panic() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(WorldPlugin);
    app.add_plugins(GeneratorPlugin::<DefaultVoxel>::new());

    // No renderer, window, audio, GPU, or real-time plugins are involved here.
    // This verifies the base runtime systems can be registered and ticked in a
    // headless ECS app.
    app.update();
}

#[test]
fn mining_emits_real_destroyed_voxel_byte_for_drop_mapping() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(WorldPlugin);
    app.add_plugins(MiningPlugin);
    app.init_resource::<ObservedBrokenBytes>();
    app.add_systems(Update, (mine_origin_once, record_broken_bytes).chain());

    let mut chunk = Chunk::<DefaultVoxel>::new();
    chunk.set(0, 0, 0, DefaultVoxel::IronOre);
    let entity = app.world_mut().spawn(chunk).id();
    app.world_mut()
        .resource_mut::<ChunkManager>()
        .register(IVec3::ZERO, entity);

    app.update();

    let observed = app.world().resource::<ObservedBrokenBytes>();
    assert_eq!(
        observed.0,
        vec![DefaultVoxel::IronOre.to_byte()],
        "VoxelBroken must carry the destroyed voxel byte so drops map from real block content"
    );
}

#[test]
fn streaming_requests_new_viewer_region_once_and_marks_pending() {
    let mut app = App::new();
    app.insert_resource(ChunkManager::new(14.0, 32.0));
    app.init_resource::<ObservedLoadRequests>();
    app.add_plugins(WorldPlugin);
    app.add_systems(
        Update,
        record_load_requests.after(voxel_framework::world::systems::streaming_system),
    );

    app.update();
    {
        let observed = app.world().resource::<ObservedLoadRequests>();
        assert!(
            observed.0.contains(&IVec3::ZERO),
            "initial stream around origin should include origin chunk; observed {:?}",
            observed.0
        );
    }
    assert!(
        app.world()
            .resource::<ChunkManager>()
            .is_pending(IVec3::ZERO)
    );

    // Running the same frame again must not duplicate the pending request.
    let initial_request_count = app.world().resource::<ObservedLoadRequests>().0.len();
    app.update();
    assert_eq!(
        app.world().resource::<ObservedLoadRequests>().0.len(),
        initial_request_count,
        "a second update at the same viewer position must not duplicate pending requests"
    );

    app.world_mut().resource_mut::<ViewerPosition>().position = Vec3::new(16.0, 0.0, 0.0);
    app.update();

    let observed = &app.world().resource::<ObservedLoadRequests>().0;
    assert!(
        observed.iter().any(|coord| coord.x == 1),
        "moving the viewer one chunk over should request at least one +X chunk; observed {observed:?}"
    );
    assert_eq!(
        observed
            .iter()
            .filter(|&&coord| coord == IVec3::ZERO)
            .count(),
        1,
        "pending origin chunk must not be requested twice"
    );
}

/// Counts every `ChunkModified` message it sees.
#[derive(Resource, Default)]
struct ModifiedSeen(usize);

fn count_modified(mut reader: MessageReader<ChunkModified>, mut seen: ResMut<ModifiedSeen>) {
    seen.0 += reader.read().count();
}

#[test]
fn generator_plugin_restores_persisted_chunk_instead_of_generating() {
    let coord = IVec3::new(7, -2, 3);
    let temp = tempfile::tempdir().expect("create temp chunk dir");
    let save_dir = temp.path();

    let mut saved_chunk = Chunk::<DefaultVoxel>::new();
    saved_chunk.set(0, 0, 0, DefaultVoxel::RareMineral);
    let file = save_dir.join(chunk_filename(coord));
    save_chunk(&saved_chunk, &file).expect("save test chunk");
    // Keep only the voxel half: a valid legacy file that any rewrite would
    // replace with the full, twice as long format.
    let voxel_bytes = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;
    let mut legacy = std::fs::read(&file).expect("read saved chunk");
    legacy.truncate(voxel_bytes);
    std::fs::write(&file, &legacy).expect("write legacy chunk");

    let generator_calls = Arc::new(AtomicUsize::new(0));
    let mut app = lifecycle_app(save_dir, generator_calls.clone());
    app.init_resource::<ModifiedSeen>();
    app.add_systems(Update, count_modified);

    app.world_mut().write_message(ChunkLoadRequest(coord));
    assert!(
        run_until(&mut app, 500, |app| loaded_voxel(app, coord, (0, 0, 0))
            .is_some()),
        "persisted chunk should be restored"
    );
    // Let the autosave system see the notification from the restore.
    app.update();
    app.update();

    assert_eq!(
        generator_calls.load(Ordering::SeqCst),
        0,
        "a saved chunk must win over the generator"
    );
    assert_eq!(
        loaded_voxel(&app, coord, (0, 0, 0)),
        Some(DefaultVoxel::RareMineral)
    );
    assert_eq!(
        app.world().resource::<ModifiedSeen>().0,
        1,
        "restoring a chunk should notify the mesh system once"
    );

    // Restoring must not write the chunk back: not on the notification that
    // followed the restore...
    assert_eq!(
        std::fs::metadata(&file).expect("saved chunk").len(),
        voxel_bytes as u64,
        "restoring a chunk must not rewrite its file"
    );
    // ...and not on a later one.
    std::fs::remove_file(&file).expect("remove saved chunk");
    app.world_mut().write_message(ChunkModified(coord));
    app.update();
    app.update();
    assert!(
        !file.exists(),
        "a restored, unedited chunk must not be saved again"
    );
}

#[test]
fn unreadable_persisted_chunk_falls_back_to_the_generator() {
    let coord = IVec3::new(-1, 0, 4);
    let temp = tempfile::tempdir().expect("create temp chunk dir");
    std::fs::write(temp.path().join(chunk_filename(coord)), b"truncated")
        .expect("write damaged chunk");

    let generator_calls = Arc::new(AtomicUsize::new(0));
    let mut app = lifecycle_app(temp.path(), generator_calls.clone());

    app.world_mut().write_message(ChunkLoadRequest(coord));
    assert!(
        run_until(&mut app, 500, |app| loaded_voxel(app, coord, (0, 0, 0))
            .is_some()),
        "chunk should still load"
    );

    assert_eq!(generator_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        loaded_voxel(&app, coord, (0, 0, 0)),
        Some(DefaultVoxel::IronOre)
    );
}

#[test]
fn neighbor_mesh_culls_border_face_against_solid_adjacent_chunk() {
    let mut chunk = Chunk::<DefaultVoxel>::new();
    chunk.set(CHUNK_SIZE - 1, 0, 0, DefaultVoxel::Rock);

    let mut right_neighbor = Chunk::<DefaultVoxel>::new();
    right_neighbor.set(0, 0, 0, DefaultVoxel::Rock);

    let mut neighbors = ChunkNeighbors::<DefaultVoxel>::default();
    // Current +X face sees the right neighbor's -X border.
    neighbors.faces[2] = Some(right_neighbor.border_slice(3));

    let isolated_mesh = generate_chunk_mesh(&chunk, 1);
    let seam_free_mesh = generate_chunk_mesh_with_neighbors(&chunk, &neighbors, 1);

    assert_eq!(vertex_count(&isolated_mesh), 24);
    assert_eq!(index_count(&isolated_mesh), 36);
    assert_eq!(
        vertex_count(&seam_free_mesh),
        20,
        "one solid neighbor should cull exactly one single-voxel border face"
    );
    assert_eq!(index_count(&seam_free_mesh), 30);
}

// ── 7. Full chunk lifecycle ──────────────────────────────────────────────────

/// Headless app with streaming, generation and autosave pointed at `save_dir`.
fn lifecycle_app(save_dir: &std::path::Path, calls: Arc<AtomicUsize>) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    // Small radius so streaming around the origin stays cheap.
    app.insert_resource(ChunkManager::new(1.0, 2.0));
    app.add_plugins(WorldPlugin);
    app.insert_resource(
        voxel_framework::generator::GeneratorResource::<DefaultVoxel> {
            generator: Arc::new(CountingGenerator { calls }),
        },
    );
    app.add_plugins(GeneratorPlugin::<DefaultVoxel>::new());
    app.add_plugins(voxel_framework::world::PersistencePlugin::<DefaultVoxel>::new());
    // Override the plugin's default `./worlds/...` path.
    app.insert_resource(PersistenceConfig {
        save_dir: save_dir.to_string_lossy().into_owned(),
        auto_save: true,
    });
    app
}

/// Runs `app.update()` until `done` holds or `max_frames` pass. Generation
/// runs on background threads, so frames are spaced slightly apart.
fn run_until(app: &mut App, max_frames: usize, done: impl Fn(&App) -> bool) -> bool {
    for _ in 0..max_frames {
        app.update();
        if done(app) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    false
}

fn loaded_voxel(app: &App, coord: IVec3, local: (usize, usize, usize)) -> Option<DefaultVoxel> {
    let entity = app.world().resource::<ChunkManager>().get_entity(coord)?;
    let chunk = app.world().entity(entity).get::<Chunk<DefaultVoxel>>()?;
    Some(chunk.get(local.0, local.1, local.2))
}

#[test]
fn chunk_lifecycle_generate_modify_autosave_and_reload_from_disk() {
    let temp = tempfile::tempdir().expect("create temp save dir");
    let coord = IVec3::ZERO;

    // 1. First session: the chunk is generated on a background thread.
    let first_calls = Arc::new(AtomicUsize::new(0));
    let mut app = lifecycle_app(temp.path(), first_calls.clone());
    app.world_mut().write_message(ChunkLoadRequest(coord));
    assert!(
        run_until(&mut app, 500, |app| loaded_voxel(app, coord, (0, 0, 0))
            .is_some()),
        "chunk should finish generating"
    );
    assert!(
        first_calls.load(Ordering::SeqCst) >= 1,
        "first load must use the generator"
    );
    assert_eq!(
        loaded_voxel(&app, coord, (0, 0, 0)),
        Some(DefaultVoxel::IronOre)
    );

    // Generated terrain is not saved: it can be generated again.
    let file = temp.path().join(chunk_filename(coord));
    app.update();
    app.update();
    assert!(
        !file.exists(),
        "an unedited generated chunk must not be written"
    );

    // 2. Modify it through the public voxel API, which emits ChunkModified.
    {
        let mut state = bevy::ecs::system::SystemState::<(
            Res<ChunkManager>,
            Query<&mut Chunk<DefaultVoxel>>,
            MessageWriter<ChunkModified>,
        )>::new(app.world_mut());
        let (manager, mut chunks, mut modified) = state.get_mut(app.world_mut());
        assert!(voxel_framework::world::set_voxel_at(
            Vec3::new(5.5, 6.5, 7.5),
            DefaultVoxel::RareMineral,
            &manager,
            &mut chunks,
            &mut modified,
        ));
        state.apply(app.world_mut());
    }

    // 3. Autosave writes it on the next frame.
    app.update();
    assert!(file.is_file(), "autosave should write {}", file.display());
    drop(app);

    // 4. Second session: the saved chunk wins over the generator.
    let mut app = lifecycle_app(temp.path(), Arc::new(AtomicUsize::new(0)));
    app.world_mut().write_message(ChunkLoadRequest(coord));
    assert!(
        run_until(&mut app, 500, |app| loaded_voxel(app, coord, (5, 6, 7))
            .is_some()),
        "saved chunk should load"
    );
    assert_eq!(
        loaded_voxel(&app, coord, (5, 6, 7)),
        Some(DefaultVoxel::RareMineral)
    );
    assert_eq!(
        loaded_voxel(&app, coord, (0, 0, 0)),
        Some(DefaultVoxel::IronOre)
    );
}
