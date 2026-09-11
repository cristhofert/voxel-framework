# AGENTS.md — Building a game **with** voxel-framework

This file orients an AI agent (or a new contributor) who is **consuming**
`voxel-framework` to build a game. It is the fast mental model plus the
non-obvious runtime contracts. For the deep API, read the rustdoc
(`cargo doc --open`) and the [README](./README.md).

> Not building a game *with* the crate but working *on* the crate itself?
> See [`CLAUDE.md`](./CLAUDE.md) instead — that covers the repo's own workflow.

---

## Stability & scope

`voxel-framework` 0.8 is the current package line intended for real Bevy
consumer crates, examples, and experiments. It targets Bevy 0.18 and Rust MSRV
1.85. Treat it as useful but pre-1.0: public APIs and the local persistence
format may still change across 0.x minor releases. Package metadata may be
prepared before the corresponding crate and tag are published.

It is not a standalone game/editor, and it does not currently include
multiplayer/networking or long-term 1.0 save compatibility guarantees.

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

## Compose the flat plugin catalog

The framework exposes a **flat catalog**, not a hierarchy of increasingly
game-like super-plugins. Pick explicit plugins by responsibility as your game
matures:

| Category | Responsibility | Examples |
|---|---|---|
| Engine | Streamable/renderable voxel-world foundation | `WorldPlugin`, `WorldConfigPlugin`, `RenderPlugin<V>`, `MeshPlugin<V>`, `OcclusionPlugin<V>`, `LoadingPlugin` |
| World Data | Chunk sources, generation, persistence, lifecycle data | `GeneratorPlugin<V>`, `MultiGeneratorPlugin<V>`, `PersistencePlugin<V>` |
| Gameplay | Reusable rules and author-defined horizontal data | `VoxelContentPlugin<V>`, `VoxelStatsPlugin`, `VoxelEffectsPlugin`, `MiningPlugin`, `PlacementPlugin`, `PlayerPlugin<V>` |
| Default Implementation | Replaceable presentation defaults | `UiPlugin`, `VoxelHudPlugin`, `SkyboxPlugin`, `FragmentPlugin`, `DefaultAudioPlugin` |
| Convenience | Compatibility bundles and adapters | `VoxelCorePlugin<V>`, `VoxelPlugin<V>`, `MiningInputPlugin<V, I>`, `PlacementInputPlugin<V>` |

`VoxelCorePlugin<V>` and `VoxelPlugin<V>` remain useful starting points, but
they are convenience bundles rather than the primary architecture:

| Bundle | Gives you | Use when |
|---|---|---|
| `VoxelCorePlugin<V>` | Engine foundation plus mining and placement primitives | You want to own UI, audio, and other presentation choices |
| `VoxelPlugin<V>` | Core plus selected defaults: skybox, SFX events, hotbar with `items`, fragments, and UI | You want a quick prototype or example |

Generation and persistence remain explicit World Data choices regardless of
the bundle:

- `GeneratorPlugin<V>` wires a generator into streaming and requires a
  `GeneratorResource<V>`. Without it, requested chunks are not procedurally
  generated.
- `PersistencePlugin<V>` optionally saves and loads modified chunks from disk.

### Core convenience bundle

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

### Quick-start bundle

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

## Author-defined content, stats, and effects

These Gameplay plugins are independent horizontal building blocks:

- `VoxelContentPlugin<V>` initializes the block registry; with `items`,
  `ItemContentPlugin<I>` and `BlockDropContentPlugin<I>` initialize item and
  drop registries.
- `VoxelStatsPlugin` initializes an empty stat catalog and global value set;
  `StatSet` can also be attached to entities.
- `VoxelEffectsPlugin` initializes an empty effect catalog and active-effect
  set; `ActiveEffectSet` can also be attached to entities.

The framework does **not** define health/oxygen/mana, poison/buffs, genre
categories, or other vertical presets. The game author registers definitions in
ordinary Rust setup systems and decides what ids, tags, and attributes mean.
Snapshots are deterministic and serializable for tooling, but there is no JSON
configuration schema competing with the Rust API.

## Author-defined retained HUD

`VoxelHudPlugin` is an explicit, non-generic Default Implementation plugin. It
initializes an empty `HudRegistry`, `HudValueSet`, and retained renderer; it is
not included in `UiPlugin`, `VoxelCorePlugin<V>`, or `VoxelPlugin<V>`. Register
neutral text, bar, icon, or bounded-segment definitions during `Startup`, then
publish screen-ready values from game-owned systems.

Put publishers in `HudSystemSet::Publish` when updates must render in the same
frame. Missing or mismatched values resolve safely to hidden state or a
compatible author fallback and emit transition-deduplicated `HudDiagnostic`
messages. Snapshot APIs are deterministic inspection tools, not persistence or
a JSON/config DSL.

The plugin can coexist with `UiPlugin` and custom Bevy UI. Migrate a custom
overlay element by element: preserve the game-owned query, publish its neutral
view value explicitly, verify the retained replacement, then remove the old UI
entity. Do not invent automatic stats/effects bridges, player discovery,
gameplay rules, or genre presets. See [`docs/hud.md`](./docs/hud.md) and the
feature-independent [`examples/hud.rs`](./examples/hud.rs).

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
4. **HUD values are author-published view data.** The framework never discovers
   players or mirrors stats/effects automatically. Use `HudSystemSet::Publish`
   for same-frame retained updates.
5. **`world.toml` is read from the current working directory at startup.** Missing
   file → defaults. `WorldConfig::from_file(path)` loads an explicit one.
6. **Chunks are 16³.** Index math is `x*256 + y*16 + z`. Coordinates are in chunk
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

| Command | Environment | Purpose |
|---|---|---|
| `cargo check` | Headless | Verify compilation; use this, never `cargo build` |
| `cargo test` | Headless | Run pure-logic and selected runtime tests |
| `cargo run --example content_definitions` | Headless; requires `items` (enabled by default) | Inspect author-defined blocks, items, and drops |
| `cargo run --example stats_definitions` | Headless | Inspect author-defined global/entity stats |
| `cargo run --example effect_definitions` | Headless | Inspect author-defined global/entity effects |
| `cargo run --example hud` | Headless; feature-independent | Inspect neutral retained HUD definitions, same-frame publication, and updates |
| `cargo run --example basic_world` | Graphical; display required; requires `items` (enabled by default) | Run the small playable asteroid example |
| `cargo run --example minetest` | Graphical; display required; requires `items`, `crafting`, and `mobs` (enabled by default) | Run the larger integration example |

## Where to look next

- **[README.md](./README.md)** — full guide: world config, persistence, LOD,
  occlusion, mining/placement, player, custom voxel types, custom generators.
- **[examples/basic_world.rs](./examples/basic_world.rs)** — minimal working game
  (asteroid world, texture array, player). Walkthrough in
  [examples/basic_world.md](./examples/basic_world.md).
- **[examples/minetest.rs](./examples/minetest.rs)** — advanced example
  (`items,crafting,mobs`).
- **[examples/content_definitions.rs](./examples/content_definitions.rs)**,
  **[examples/stats_definitions.rs](./examples/stats_definitions.rs)**, and
  **[examples/effect_definitions.rs](./examples/effect_definitions.rs)** —
  headless examples of the author-defined horizontal APIs.
- **[docs/hud.md](./docs/hud.md)** and
  **[examples/hud.rs](./examples/hud.rs)** — retained HUD publication,
  scheduling, migration, fallbacks, inspection, and a neutral headless example.
- **`src/prelude.rs`** — the full list of re-exported public types.
- **`src/lib.rs`** — crate-level rustdoc with the plugin architecture overview.
