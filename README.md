# voxel-framework

> A reusable voxel engine framework built on [Bevy](https://bevyengine.org/).

## Features

- 16×16×16 voxel chunks with efficient flat-array storage
- Greedy meshing with LOD support and neighbor-aware face culling
- Seam-free borders via neighbor-aware meshing (default in the live system)
- Pluggable `ChunkGenerator` trait — bring your own world generation
- Built-in `AsteroidGenerator` with FBM noise and mineral distribution
- World streaming — automatic chunk load/unload based on viewer position
- Save-on-unload — chunks written to disk when streaming out
- PBR rendering via Bevy's `StandardMaterial` with vertex colors
- Binary chunk persistence
- Dynamic LOD (2 levels based on viewer distance)
- Auto-save chunk persistence (optional, disk-backed)
- TOML world configuration via `world.toml`
- Coordinate utilities for world/chunk/voxel space conversions
- Occlusion culling — chunks surrounded by solid neighbors are auto-hidden
- Capsule-voxel collision for player movement
- Mining API — break-time tracking and `VoxelBroken` events
- Block placement API — cooldown tracking and `VoxelPlaced` events
- Loading progress — `LoadingProgress` messages and a `WorldReady` event for loading screens

## Plugin Architecture

`voxel-framework` is a flat catalog of composable Bevy plugins grouped by
responsibility. Games choose the plugins they need from these official
categories:

| Category | Meaning | Examples |
|---|---|---|
| Engine | Foundation for streamable/renderable voxel worlds | `WorldPlugin`, `RenderPlugin<V>`, `MeshPlugin<V>`, `OcclusionPlugin<V>`, `LoadingPlugin` |
| World Data | Chunk sources, generation, persistence, lifecycle data | `GeneratorPlugin<V>`, `MultiGeneratorPlugin<V>`, `PersistencePlugin<V>` |
| Gameplay | Reusable rules and interactions | `MiningPlugin`, `PlacementPlugin`, `PlayerPlugin<V>`, `HotbarPlugin`, `DropsPlugin<I>` |
| Default Implementation | Replaceable UI/audio/visual defaults | `UiPlugin`, `SkyboxPlugin`, `FragmentPlugin`, `DefaultAudioPlugin` |
| Convenience | Bundles, input adapters, compatibility, recipes/prototyping helpers | `VoxelCorePlugin<V>`, `VoxelPlugin<V>`, `MiningInputPlugin<V, I>`, `PlacementInputPlugin<V>` |

The categories are also exposed as marker traits:
`EnginePlugin`, `WorldDataPlugin`, `GameplayPlugin`,
`DefaultImplementationPlugin`, and `ConveniencePlugin`.

`VoxelCorePlugin<V>` and `VoxelPlugin<V>` are convenience bundles kept for
compatibility and quick starts. They are useful, but they are not the primary
architecture:

| Bundle | What it composes |
|---|---|
| `VoxelCorePlugin<V>` | Engine foundation plus the lightweight gameplay primitives `MiningPlugin` and `PlacementPlugin` |
| `VoxelPlugin<V>` | `VoxelCorePlugin<V>` plus selected defaults: fragments, skybox, SFX events, UI, and hotbar when the `items` feature is enabled |

`GeneratorPlugin<V>` and `PersistencePlugin<V>` are explicit World Data choices
regardless of which convenience bundle you use.

```rust
// Compatibility bundle: historical core runtime
App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins(VoxelCorePlugin::<DefaultVoxel>::new())
    .add_plugins(GeneratorPlugin::<DefaultVoxel>::new());

// Quick-start bundle for examples/prototypes
App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
    .add_plugins(GeneratorPlugin::<DefaultVoxel>::new());
```

## Quick Start

Add the dependency to your `Cargo.toml`. Use crates.io for released versions, or pin the repository when you need an unreleased commit/tag:

```toml
[dependencies]
# Option A — released crate (recommended for consumers):
voxel-framework = "0.4"

# Option B — local clone side-by-side (development):
voxel-framework = { path = "../voxel-framework" }

# Option C — from git, pinned to a stable tag:
voxel-framework = { git = "https://github.com/cristhofert/voxel-framework", tag = "v0.4.0" }

# Option D — follow the dev branch (bleeding-edge):
voxel-framework = { git = "https://github.com/cristhofert/voxel-framework", branch = "dev" }

bevy = "0.18"
```

> To run the bundled examples, clone the repository — the demo `assets/` live in the repo and are not pulled in automatically when depending via git.

Minimal `main.rs` that spawns an asteroid world with config and persistence:

```rust
use std::sync::Arc;
use bevy::prelude::*;
use voxel_framework::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        // Registers WorldPlugin, RenderPlugin, MeshPlugin, and WorldConfigPlugin.
        .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
        // Registers the async generation system.
        .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())
        // Enables auto-save on chunk modification under ./worlds/{world_name}/chunks.
        .add_plugins(PersistencePlugin::<DefaultVoxel>::with_world("world"))
        // Supply the generator implementation.
        .insert_resource(GeneratorResource::<DefaultVoxel> {
            generator: Arc::new(AsteroidGenerator::new(AsteroidConfig::default())),
        })
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, mut viewer: ResMut<ViewerPosition>) {
    // Drive chunk streaming by keeping ViewerPosition in sync with the camera.
    viewer.position = Vec3::new(0.0, 50.0, 100.0);

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 50.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        DirectionalLight { illuminance: 10_000.0, shadows_enabled: true, ..default() },
        Transform::from_xyz(100.0, 200.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
```

Update `ViewerPosition` every frame so the streaming system knows where to load chunks:

```rust
fn sync_viewer(
    mut viewer: ResMut<ViewerPosition>,
    camera: Query<&Transform, With<Camera3d>>,
) {
    if let Ok(t) = camera.single() {
        viewer.position = t.translation;
    }
}
```

Run the bundled example:

```sh
cargo run --example basic_world
```

## Module Overview

| Module | Description |
|---|---|
| `voxel` | `VoxelType` trait, `DefaultVoxel` enum, and `VoxelPalette` resource |
| `chunk` | `Chunk<V>` flat-array storage, `ChunkCoord`, constants, coordinate utilities, and binary persistence |
| `noise` | Seed-based value noise, FBM, and `hash3` — no external noise crates |
| `generator` | `ChunkGenerator<V>` trait, `GeneratorPlugin`, and `GeneratorResource` |
| `generator::asteroid` | `AsteroidGenerator` with FBM surface deformation and mineral veins |
| `mesh` | Greedy meshing (`generate_chunk_mesh`, `generate_chunk_mesh_with_neighbors`), `MeshPlugin`, LOD computation |
| `world` | `ChunkManager`, `ViewerPosition`, lifecycle events, streaming system, `WorldPlugin`, `WorldConfig`, `PersistencePlugin` |
| `world/occlusion` | `OcclusionPlugin`, `is_chunk_occluded` — hides chunks fully enclosed by solid neighbors |
| `world/collision` | Sphere and capsule collision detection and resolution against solid voxels |
| `world/mining` | `MiningPlugin`, `MiningConfig`, `MiningState`, `tick_mining`, `stop_mining`, `VoxelBroken`, `MiningProgress` |
| `world/placement` | `PlacementPlugin`, `PlacementConfig`, `PlacementState`, `try_place_voxel`, `tick_placement`, `VoxelPlaced` |
| `world/loading` | `LoadingPlugin`, `LoadingProgress`, `WorldReady`, `WorldLoadingState` |
| `render` | Shared `StandardMaterial` for vertex-colored chunks, `RenderPlugin` |
| `player` | 6DOF jetpack player (`PlayerPlugin`, `PlayerConfig`, capsule collision) |
| `ui/pause_menu` | Optional `PauseMenuPlugin` with `PauseMenuState` and `PauseMenuConfig` |
| `prelude` | Convenience re-export of all public types |

## World Configuration

The framework reads `world.toml` from the working directory at startup via `WorldConfigPlugin` (included in `VoxelPlugin`). If the file is absent or malformed, all values fall back to their defaults.

The bundled examples also support loading persisted world metadata from `worlds/<name>/world.json` when re-opening an existing saved world.

### world.toml format

```toml
seed = 42
spawn_position = [0.0, 0.0, 0.0]
load_distance = 150.0
unload_distance = 200.0

[asteroids]
seed = 42
count = 20
min_radius = 15.0
max_radius = 40.0

[lighting]
sun_illuminance = 15000.0
ambient_brightness = 0.1
```

`WorldConfig` is inserted as a Bevy resource after loading and can be read in any system:

```rust
fn my_system(config: Res<WorldConfig>) {
    println!("world seed: {}", config.seed);
}
```

### Building a generator from config

`WorldConfig::build_asteroid_generator()` constructs an `AsteroidGenerator` directly from the TOML asteroid settings:

```rust
fn setup(config: Res<WorldConfig>, mut commands: Commands) {
    use std::sync::Arc;
    use voxel_framework::prelude::*;

    let generator = config.build_asteroid_generator();
    commands.insert_resource(GeneratorResource::<DefaultVoxel> {
        generator: Arc::new(generator),
    });
}
```

The `WorldConfigPlugin` is registered automatically by `VoxelPlugin`. To add it standalone:

```rust
use voxel_framework::world::WorldConfigPlugin;

App::new()
    .add_plugins(WorldConfigPlugin)
    .run();
```

## Persistence

`PersistencePlugin` wires chunk auto-save into the Bevy game loop. Every time a `ChunkModified` event fires, the affected chunk is written to disk in binary format. Chunks are also saved when they stream out of range (save-on-unload).

```rust
use voxel_framework::prelude::*;

App::new()
    .add_plugins(PersistencePlugin::<DefaultVoxel>::new());
```

Use a custom voxel type the same way:

```rust
App::new()
    .add_plugins(PersistencePlugin::<MyVoxel>::new());
```

### Overriding PersistenceConfig

`PersistenceConfig` controls the save directory and whether auto-save is active. `PersistencePlugin` installs its defaults when it is added, so insert your override after the plugin:

```rust
use voxel_framework::world::PersistenceConfig;

App::new()
    .add_plugins(PersistencePlugin::<DefaultVoxel>::new())
    .insert_resource(PersistenceConfig {
        save_dir: "./saves/chunks".to_string(),
        auto_save: true,
    });
```

| Field | Default | Description |
|---|---|---|
| `save_dir` | `"./worlds/world/chunks"` | Directory for chunk binary files (created automatically) |
| `auto_save` | `true` | Write to disk on every `ChunkModified` event |

On load, the generator pipeline calls `try_load_chunk` before spawning an async generation task. If a saved file exists for a given coordinate, it is loaded directly — generation is skipped.

## LOD System

Chunks have two discrete detail levels driven by distance from the viewer. The `ChunkLod` component on each chunk entity tracks the current level and is updated every frame by `lod_update_system`.

| LOD level | Distance from viewer | Greedy-mesh step | Detail |
|---|---|---|---|
| `0` | < 200 units | 1 | Full |
| `1` | >= 200 units | 2 | Half |

When the level changes, a `ChunkModified` event is emitted so the mesh system regenerates the geometry at the new step size.

**Hysteresis** prevents oscillation at boundaries: a chunk must move clearly past a threshold before the LOD level changes, avoiding rapid mesh rebuilds when the viewer hovers near a distance boundary.

`compute_lod` and `lod_to_step` are available from the prelude if you need to query LOD directly:

```rust
use voxel_framework::prelude::*;

// chunk_pos is the world-space position of the chunk (e.g. from chunk_center())
let lod: u8 = voxel_framework::mesh::lod::compute_lod(chunk_pos, viewer_pos);
let step: usize = voxel_framework::mesh::lod::lod_to_step(lod);
```

## Occlusion Culling

`OcclusionPlugin` hides any chunk whose six face-adjacent neighbors are all fully solid. Because none of that chunk's border faces are visible, its mesh is set to `Visibility::Hidden` automatically and restored when a neighbor changes.

```rust
use voxel_framework::prelude::*;

App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
    .add_plugins(OcclusionPlugin::<DefaultVoxel>::new())
    .run();
```

You can also query occlusion directly without the plugin:

```rust
use voxel_framework::prelude::*;

let occluded = is_chunk_occluded::<DefaultVoxel>(coord, &manager, &chunks);
```

## Mining & Placement

Both systems are pure-function APIs — no hidden Bevy systems are injected. Register the plugins to initialize resources and messages, then call the free functions from your own systems.

```rust
App::new()
    .add_plugins(MiningPlugin)
    .add_plugins(PlacementPlugin);
```

### Mining

`tick_mining` is called every frame while the mine button is held. It accumulates elapsed time against `MiningConfig::break_time`, emits `MiningProgress` each frame, and emits `VoxelBroken` (plus `ChunkModified` to trigger remeshing) when the voxel breaks.

```rust
fn my_mining_system(
    time: Res<Time>,
    mut state: ResMut<MiningState>,
    config: Res<MiningConfig>,
    manager: Res<ChunkManager>,
    mut chunks: Query<&mut Chunk<DefaultVoxel>>,
    mut broken: MessageWriter<VoxelBroken>,
    mut progress: MessageWriter<MiningProgress>,
    mut modified: MessageWriter<ChunkModified>,
) {
    // Call when mine button held:
    tick_mining(target_chunk, target_local, time.delta_secs(),
        &mut state, &config, &manager, &mut chunks,
        &mut broken, &mut progress, &mut modified);
}
```

Call `stop_mining` when the button is released to reset progress:

```rust
stop_mining(&mut state);
```

Override break time via `MiningConfig`:

```rust
app.insert_resource(MiningConfig { break_time: 2.0 });
```

### Placement

`try_place_voxel` is called once when the place button is pressed. It returns `false` if the cooldown has not expired or the target voxel is already solid. `tick_placement` advances the cooldown timer and must be called every frame.

```rust
fn my_placement_system(
    time: Res<Time>,
    mut state: ResMut<PlacementState>,
    config: Res<PlacementConfig>,
    manager: Res<ChunkManager>,
    mut chunks: Query<&mut Chunk<DefaultVoxel>>,
    mut placed: MessageWriter<VoxelPlaced>,
    mut modified: MessageWriter<ChunkModified>,
) {
    // Advance cooldown every frame:
    tick_placement(time.delta_secs(), &mut state, &config);

    // Call once on button press:
    try_place_voxel(target_chunk, target_local, DefaultVoxel::Rock, face_normal,
        &mut state, &config, &manager, &mut chunks,
        &mut placed, &mut modified);
}
```

Override cooldown via `PlacementConfig`:

```rust
app.insert_resource(PlacementConfig { cooldown: 0.5 });
```

## Loading Screen

`LoadingPlugin` tracks the initial chunk load and emits two message types:

- `LoadingProgress` — fired every frame while loading, carries `loaded`, `total`, and `progress` (0.0–1.0).
- `WorldReady` — fired exactly once when all chunks within `load_distance` are loaded for the first time.

```rust
App::new()
    .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
    .add_plugins(LoadingPlugin);
```

Detect `WorldReady` to transition out of a loading screen:

```rust
fn loading_system(mut ready: MessageReader<WorldReady>) {
    for _ in ready.read() {
        // Transition out of loading screen
    }
}
```

Drive a progress bar with `LoadingProgress`:

```rust
fn progress_system(mut progress_events: MessageReader<LoadingProgress>) {
    for ev in progress_events.read() {
        println!("Loading: {}/{} ({:.0}%)", ev.loaded, ev.total, ev.progress * 100.0);
    }
}
```

`WorldLoadingState` is also available as a resource if you prefer polling over event handling:

```rust
fn check_ready(state: Res<WorldLoadingState>) {
    if state.ready {
        // world is loaded
    }
}
```

## Player

`PlayerPlugin<V>` adds a first-person player with capsule collision, mouse look, WASD movement, and selectable movement/look modes. The default is zero-g 6DOF flight; anchored free flight and grounded gravity movement are also available through `PlayerConfig::mode`.

```rust
use voxel_framework::player::PlayerPlugin;

App::new()
    .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
    .add_plugins(PlayerPlugin::<DefaultVoxel>::new())
    .insert_resource(PlayerConfig {
        spawn_position: Vec3::new(0.0, 50.0, 150.0),
        speed: 16.0,
        ..default()
    });
```

| Input | Action |
|---|---|
| Mouse | Depends on `PlayerConfig::mode`: `FreeFlightSixDof` gives yaw + pitch with no clamp; `FreeFlightAnchored` gives FPS-style yaw around a world-axis anchor with pitch clamped to ±89° and a level horizon |
| Q / E | In `FreeFlightSixDof`, roll left/right (continuous or snap, configured in the mode); in `FreeFlightAnchored`, switch the anchor to the next signed world axis (±X/±Y/±Z) with a snap animation; disabled in `Grounded` |
| W / S | Forward / backward |
| A / D | Strafe left / right |
| Space | Move along local up |
| Shift | Move along local down |
| Left click | Lock cursor |
| Escape | Release cursor (second: close window) |

If your game uses an `AppState`, skip `PlayerPlugin` and add the systems manually gated to your playing state:

```rust
use voxel_framework::prelude::{player_look, player_move, player_resolve_collision};

App::new()
    .add_systems(Startup, spawn_player)
    .add_systems(Update,
        (player_look, player_move, player_resolve_collision::<DefaultVoxel>)
            .chain()
            .run_if(in_state(MyState::Playing)),
    );
```

## Pause Menu

`PauseMenuPlugin` is an optional plugin that provides a full pause menu with a settings submenu. It manages its own `PauseMenuState` — use it to gate your gameplay systems:

```rust
use voxel_framework::prelude::{PauseMenuPlugin, PauseMenuState};

App::new()
    .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
    .add_plugins(PauseMenuPlugin)
    .add_systems(Startup, spawn_player)
    .add_systems(Update,
        (player_look, player_move, player_resolve_collision::<DefaultVoxel>)
            .chain()
            .run_if(in_state(PauseMenuState::Playing)),
    );
```

| State | Description |
|---|---|
| `PauseMenuState::Playing` | Normal gameplay — menus hidden |
| `PauseMenuState::Paused` | Pause overlay visible (Resume / Settings / Quit) |
| `PauseMenuState::Settings` | Settings submenu (FOV, SFX volume) |

The Settings submenu writes directly to the camera's `Projection` (FOV) and to `SfxConfig::volume` (SFX volume).

### Customizing the font

Override `PauseMenuConfig` before adding the plugin:

```rust
use voxel_framework::prelude::PauseMenuConfig;

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(PauseMenuConfig {
        font: asset_server.load("fonts/MyFont.ttf"),
    });
}
```

## Coordinate Utilities

Four free functions convert between world space, chunk grid, and local voxel space. All are available via `use voxel_framework::prelude::*`.

```rust
use bevy::math::Vec3;
use voxel_framework::prelude::*;

let world_pos = Vec3::new(18.5, 0.0, 0.0);

// Which chunk contains this world position?
let coord = world_to_chunk_coord(world_pos);
// → IVec3(1, 0, 0)  (chunk boundary at x=16.0)

// Which voxel within that chunk?
let (lx, ly, lz) = world_to_local_coord(world_pos);
// → (2, 0, 0)

// World-space minimum corner of a chunk.
let origin = chunk_origin(coord);
// → Vec3(16.0, 0.0, 0.0)

// World-space center of a chunk.
let center = chunk_center(coord);
// → Vec3(24.0, 8.0, 8.0)
```

All functions use Euclidean division so negative coordinates map to the correct chunk (e.g. `world_pos.x = -1.0` maps to chunk `-1`, not `0`).

## Implementing a Custom Voxel Type

```rust
use voxel_framework::voxel::VoxelType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MyVoxel {
    #[default]
    Air,
    Dirt,
    Stone,
}

impl VoxelType for MyVoxel {
    fn name(&self) -> &'static str {
        match self {
            MyVoxel::Air => "Air",
            MyVoxel::Dirt => "Dirt",
            MyVoxel::Stone => "Stone",
        }
    }

    fn is_solid(&self) -> bool {
        !matches!(self, MyVoxel::Air)
    }

    fn color(&self) -> [f32; 4] {
        match self {
            MyVoxel::Air   => [0.0, 0.0, 0.0, 0.0],
            MyVoxel::Dirt  => [0.45, 0.30, 0.15, 1.0],
            MyVoxel::Stone => [0.50, 0.50, 0.50, 1.0],
        }
    }

    fn to_byte(&self) -> u8 {
        match self {
            MyVoxel::Air   => 0,
            MyVoxel::Dirt  => 1,
            MyVoxel::Stone => 2,
        }
    }

    fn from_byte(byte: u8) -> Self {
        match byte {
            1 => MyVoxel::Dirt,
            2 => MyVoxel::Stone,
            _ => MyVoxel::Air,
        }
    }
}
```

## Implementing a Custom Generator

```rust
use bevy::math::IVec3;
use voxel_framework::chunk::Chunk;
use voxel_framework::generator::ChunkGenerator;

pub struct MyGenerator;

impl ChunkGenerator<MyVoxel> for MyGenerator {
    /// Called on a background thread — must not access the Bevy World.
    fn generate(&self, coord: IVec3, chunk: &mut Chunk<MyVoxel>) {
        use voxel_framework::chunk::CHUNK_SIZE;

        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    let world_y = coord.y * CHUNK_SIZE as i32 + y as i32;
                    let voxel = if world_y < 0 {
                        MyVoxel::Stone
                    } else if world_y < 4 {
                        MyVoxel::Dirt
                    } else {
                        MyVoxel::Air
                    };
                    chunk.set(x, y, z, voxel);
                }
            }
        }
    }
}
```

Register it the same way as `AsteroidGenerator`:

```rust
use std::sync::Arc;
use voxel_framework::generator::{GeneratorPlugin, GeneratorResource};

app.add_plugins(GeneratorPlugin::<MyVoxel>::new())
   .insert_resource(GeneratorResource::<MyVoxel> {
       generator: Arc::new(MyGenerator),
   });
```

## Voxel Palette (DefaultVoxel)

| Variant | Byte | RGBA color |
|---|---|---|
| `Air` | 0 | `(0.00, 0.00, 0.00, 0.0)` — transparent |
| `Rock` | 1 | `(0.35, 0.30, 0.28, 1.0)` — warm grey-brown |
| `IronOre` | 2 | `(0.55, 0.35, 0.20, 1.0)` — rusty orange |
| `SiliconCrystal` | 3 | `(0.40, 0.50, 0.80, 1.0)` — blue-violet |
| `RareMineral` | 4 | `(0.20, 0.85, 0.60, 1.0)` — vivid teal-green |

## Profiling

Compile with `--features tracing` to enable internal spans for hot paths
(mesh generation, chunk load/unload, async generation dispatch, occlusion,
collision, raycast). With the feature off the spans expand to nothing —
zero runtime overhead in default builds.

Add `--features tracy` to route spans to a [Tracy](https://github.com/wolfpld/tracy)
profiler client. Call `voxel_framework::profile::init_tracy()` from your `main()`
**before** constructing the Bevy `App` so the subscriber is installed first
(only one global tracing subscriber may be active at a time):

```rust,no_run
fn main() {
    #[cfg(feature = "tracy")]
    voxel_framework::profile::init_tracy();

    bevy::prelude::App::new()/* ... */.run();
}
```

## License

LGPL-3.0-or-later. See [`LICENSE`](LICENSE).
