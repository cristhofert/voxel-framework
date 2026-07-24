# AGENTS.md — Building a game **with** voxel-framework

This file orients an AI agent (or a new contributor) who is **consuming**
`voxel-framework` to build a game. It is the fast mental model plus the
non-obvious runtime contracts. For the deep API, read the rustdoc
(`cargo doc --open`) and the [README](./README.md).

> Not building a game *with* the crate but working *on* the crate itself?
> See [`CLAUDE.md`](./CLAUDE.md) instead — that covers the repo's own workflow.

---

## What this is

`voxel-framework` is a **reusable Bevy 0.18 library**, not a game. There is no
`main.rs`. Your game is a separate crate that depends on it and **composes its
plugins**. It provides chunk storage, procedural generation, greedy meshing,
world streaming, PBR rendering, mining/placement, and an optional 6DOF player.

Add it to your `Cargo.toml` (see the README Quick Start for git/tag forms) and:

```rust
use voxel_framework::prelude::*;
```

The prelude re-exports every public type you normally need.

## Core model (three abstractions)

- **`VoxelType`** (trait, `src/voxel.rs`) — every voxel palette implements it:
  is-solid, render color, and byte (de)serialization for persistence. The
  built-in **`DefaultVoxel`** enum (Air, Rock, IronOre, SiliconCrystal,
  RareMineral) works out of the box.
- **`Chunk<V>`** (`src/chunk/`) — the storage unit: a **16×16×16** flat `Vec<V>`
  indexed `x*256 + y*16 + z`. Identified by a `ChunkCoord` (an `IVec3` where one
  unit = `CHUNK_SIZE` = 16 world units).
- **`ChunkGenerator<V>`** (trait, `src/generator/`) — async-safe procedural
  generator. Runs on a background `AsyncComputeTaskPool` thread and returns a
  fully populated `Chunk<V>`.

Everything is generic over `V: VoxelType`. Use `DefaultVoxel` unless you need a
custom palette.

## The one decision you must make: which plugin?

| Plugin | Gives you | Use when |
|--------|-----------|----------|
| `VoxelCorePlugin::<V>` | Pure engine: chunk mgmt, streaming, meshing, rendering, occlusion, mining, placement, loading. **No game content.** | You want full control over UI/audio/hotbar. |
| `VoxelPlugin::<V>` | Everything in Core **plus** skybox, audio, hotbar, fragments, UI. | You want batteries included. |

Both are additive. Then, **always opt-in** (they need a `GeneratorResource<V>`):

- `GeneratorPlugin::<V>` — wires the generator to the world. Without it, chunks come up empty.
- `PersistencePlugin::<V>` — saves/loads modified chunks to disk.

### Minimal: pure engine

```rust
App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins(VoxelCorePlugin::<DefaultVoxel>::new())
    .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())
    .insert_resource(GeneratorResource::<DefaultVoxel> {
        generator: std::sync::Arc::new(AsteroidGenerator::new(AsteroidConfig::default())),
    })
    .run();
```

### Batteries-included

```rust
App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
    .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())
    .add_plugins(PersistencePlugin::<DefaultVoxel>::with_world("world"))
    .insert_resource(GeneratorResource::<DefaultVoxel> {
        generator: std::sync::Arc::new(AsteroidGenerator::new(AsteroidConfig::default())),
    })
    .run();
```

## Runtime contracts & gotchas (the things that bite)

These are not obvious from the type signatures — get them wrong and it silently
does nothing:

1. **`ViewerPosition` must be updated every frame.** Streaming loads/unloads
   chunks around it. If your game never writes it, no chunks stream in. Keep it
   in sync with your camera in an `Update` system.
2. **Generators run on background threads — never touch the Bevy `World`.**
   `ChunkGenerator::generate` runs off the main thread. Everything it needs must
   be owned/`Send + Sync` (that's why generators are wrapped in `Arc`). No
   `Commands`, no `Res`, no `Query` inside a generator.
3. **Mining & Placement are pure functions you call yourself.** `tick_mining`,
   `stop_mining`, `try_place_voxel`, `tick_placement` are plain functions invoked
   from *your* systems. The plugins only initialize resources/messages — they do
   not run the interaction loop for you.
4. **`world.toml` is read from the current working directory at startup.** Missing
   file → defaults. `WorldConfig::from_file(path)` loads an explicit one.
5. **Chunks are 16³.** Index math is `x*256 + y*16 + z`. Coordinates are in chunk
   units (1 unit = 16 world units) via `ChunkCoord`.

## Cargo features

| Feature | Default | Purpose |
|---------|---------|---------|
| `items` | yes | Inventory, hotbar, drops, pickup UI |
| `crafting` | yes (implies `items`) | Recipe registry, crafting |
| `mobs` | yes | Hostile/passive mob AI |
| `tracing` | no | Internal profiling spans on hot paths |
| `tracy` | no (implies `tracing`) | Route spans to a Tracy client |

## Commands

```sh
cargo check                       # verify compile — use this, NOT `cargo build`
cargo check --example basic_world
cargo test                        # pure-logic tests, no display needed
cargo run --example basic_world   # requires a display
```

## Where to look next

- **[README.md](./README.md)** — full guide: world config, persistence, LOD,
  occlusion, mining/placement, player, custom voxel types, custom generators.
- **[examples/basic_world.rs](./examples/basic_world.rs)** — minimal working game
  (asteroid world, texture array, player). Walkthrough in
  [examples/basic_world.md](./examples/basic_world.md).
- **[examples/minetest.rs](./examples/minetest.rs)** — advanced example
  (`items,crafting,mobs`).
- **`src/prelude.rs`** — the full list of re-exported public types.
- **`src/lib.rs`** — crate-level rustdoc with the plugin architecture overview.
