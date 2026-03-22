# voxel-framework

> A reusable voxel engine framework built on [Bevy](https://bevyengine.org/).

## Features

- 16×16×16 voxel chunks with efficient flat-array storage
- Greedy meshing with LOD support and neighbor-aware face culling
- Pluggable `ChunkGenerator` trait — bring your own world generation
- Built-in `AsteroidGenerator` with FBM noise and mineral distribution
- World streaming — automatic chunk load/unload based on viewer position
- PBR rendering via Bevy's `StandardMaterial` with vertex colors
- Binary chunk persistence
- Dynamic LOD (3 levels based on viewer distance)
- Auto-save chunk persistence (optional, disk-backed)
- TOML world configuration via `world.toml`
- Coordinate utilities for world/chunk/voxel space conversions

## Quick Start

Add the dependency to your `Cargo.toml`:

```toml
[dependencies]
voxel-framework = { path = "../voxel-framework" }
bevy = "0.18"
```

Minimal `main.rs` that spawns an asteroid world with config and persistence:

```rust
use std::sync::Arc;
use bevy::prelude::*;
use voxel_framework::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        // Registers WorldPlugin, RenderPlugin, MeshPlugin, and WorldConfigPlugin.
        .add_plugins(VoxelPlugin)
        // Registers the async generation system.
        .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())
        // Enables auto-save on chunk modification.
        .add_plugins(PersistencePlugin::<DefaultVoxel>::new())
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
| `render` | Shared `StandardMaterial` for vertex-colored chunks, `RenderPlugin` |
| `prelude` | Convenience re-export of all public types |

## World Configuration

The framework reads `world.toml` from the working directory at startup via `WorldConfigPlugin` (included in `VoxelPlugin`). If the file is absent or malformed, all values fall back to their defaults.

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

`PersistencePlugin` wires chunk auto-save into the Bevy game loop. Every time a `ChunkModified` event fires, the affected chunk is written to disk in binary format.

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

`PersistenceConfig` controls the save directory and whether auto-save is active. Insert it before or after the plugin to override the defaults:

```rust
use voxel_framework::world::PersistenceConfig;

App::new()
    .insert_resource(PersistenceConfig {
        save_dir: "./saves/chunks".to_string(),
        auto_save: true,
    })
    .add_plugins(PersistencePlugin::<DefaultVoxel>::new());
```

| Field | Default | Description |
|---|---|---|
| `save_dir` | `"./chunks"` | Directory for chunk binary files (created automatically) |
| `auto_save` | `true` | Write to disk on every `ChunkModified` event |

On load, the generator pipeline calls `try_load_chunk` before spawning an async generation task. If a saved file exists for a given coordinate, it is loaded directly — generation is skipped.

## LOD System

Chunks have three discrete detail levels driven by distance from the viewer. The `ChunkLod` component on each chunk entity tracks the current level and is updated every frame by `lod_update_system`.

| LOD level | Distance from viewer | Greedy-mesh step | Detail |
|---|---|---|---|
| `0` | < 50 units | 1 | Full |
| `1` | 50–100 units | 2 | Half |
| `2` | > 100 units | 4 | Coarse |

When the level changes, a `ChunkModified` event is emitted so the mesh system regenerates the geometry at the new step size.

**Hysteresis** prevents oscillation at boundaries: a chunk must move clearly past a threshold before the LOD level changes, avoiding rapid mesh rebuilds when the viewer hovers near a distance boundary.

`compute_lod` and `lod_to_step` are available from the prelude if you need to query LOD directly:

```rust
use voxel_framework::prelude::*;

// chunk_pos is the world-space position of the chunk (e.g. from chunk_center())
let lod: u8 = voxel_framework::mesh::lod::compute_lod(chunk_pos, viewer_pos);
let step: usize = voxel_framework::mesh::lod::lod_to_step(lod);
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

## License

MIT
