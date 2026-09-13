# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```sh
# Check for compile errors (use this instead of build)
cargo check
cargo check --example basic_world

# Run tests (no display required — tests use pure logic)
cargo test
cargo test <test_name>         # run a single test

# Run the bundled example (requires a display)
cargo run --example basic_world
```

**Never run `cargo build` after changes** — `cargo check` is sufficient and much faster.

## Architecture

`voxel-framework` is a reusable **Bevy 0.18** library crate. Games depend on it and compose its plugins. It is **not** a game — there is no `main.rs`.

The 0.8 line has **Rust MSRV 1.89**: Bevy 0.18.1 and `bevy_ecs` declare
1.89.0. Verify with `cargo +1.89.0 check --locked --lib` and
`cargo +1.89.0 check --locked --lib --no-default-features`, not latest stable.
The dedicated Linux CI lane resolves its own lockfile because `Cargo.lock` is
ignored; a local locked check proves only that local dependency resolution.

### Plugin composition

The public architecture is a **flat Bevy plugin catalog**, not a hierarchy of
hidden super-plugins. Every plugin has one primary category:

| Category | Purpose |
|---|---|
| Engine | Foundation for streamable/renderable voxel worlds |
| World Data | Chunk generation, persistence, sources, manifests |
| Gameplay | Reusable rules/interactions: mining, placement, player, items, drops, crafting, mobs |
| Default Implementation | Replaceable UI/audio/visual defaults |
| Convenience | Compatibility bundles, input adapters, examples/prototyping helpers |

`src/plugin_category.rs` defines marker traits for those categories. They are
documentation/API classification markers; Bevy still composes everything
through the normal `Plugin` trait.

`VoxelCorePlugin` and `VoxelPlugin` are **Convenience** bundles kept for
compatibility and quick starts:

- `VoxelCorePlugin` composes Engine plugins plus the lightweight Gameplay
  primitives `MiningPlugin` and `PlacementPlugin`.
- `VoxelPlugin` composes `VoxelCorePlugin` plus selected defaults:
  `FragmentPlugin`, `SkyboxPlugin`, `SfxPlugin`, `HotbarPlugin` when `items` is
  enabled, and `UiPlugin`.

New domain behavior should not start in Convenience. Put it in Engine, World
Data, Gameplay, or Default Implementation first; use Convenience only to
compose/adapt existing pieces.

Games pick explicit plugins or use a convenience bundle:

```rust
// Historical core runtime convenience bundle
.add_plugins(VoxelCorePlugin::<DefaultVoxel>::new())
.add_plugins(GeneratorPlugin::<MyVoxel>::new())

// Quick-start convenience bundle for examples/prototypes
.add_plugins(VoxelPlugin::<DefaultVoxel>::new())
```

`GeneratorPlugin<V>` and `PersistencePlugin<V>` are always opt-in — they require the user to provide a `GeneratorResource<V>`.

### Generic voxel type `V: VoxelType`

The core data structures (`Chunk<V>`, `GeneratorPlugin<V>`, `MeshPlugin<V>`, etc.) are generic over any type implementing `VoxelType` (trait in `src/voxel.rs`). `DefaultVoxel` is the built-in enum (Air, Rock, IronOre, SiliconCrystal, RareMineral).

### Module map

| Path | Purpose |
|---|---|
| `src/voxel.rs` | `VoxelType` trait, `DefaultVoxel`, `VoxelPalette` |
| `src/chunk/` | `Chunk<V>` (16³ flat-array), `ChunkCoord`, coordinate utilities, binary persistence |
| `src/noise/` | Seed-based value noise, FBM, `hash3` — no external noise crates |
| `src/generator/` | `ChunkGenerator<V>` trait + `GeneratorPlugin`; `asteroid/` = built-in generator |
| `src/mesh/` | Greedy meshing (`generate_chunk_mesh`), `MeshPlugin`, LOD (`lod.rs`), neighbor-aware meshing |
| `src/world/` | Streaming (`loading.rs`), `ChunkManager`, `ViewerPosition`, `WorldConfig`, raycast, collision, mining, placement, occlusion, persistence |
| `src/render/` | `RenderPlugin`, shared `StandardMaterial`, `ChunkMaterial` |
| `src/player/` | 6DOF jetpack player (`PlayerPlugin`, `spawn_player`, capsule collision helpers) |
| `src/items/` | `Inventory`, `HotbarState`, `DroppedItem`, `DropTable`, item persistence |
| `src/ui/` | `CrosshairPlugin`, `LoadingScreenPlugin`, `HotbarUiPlugin`, `PickupNotificationPlugin` |
| `src/skybox/` | `SkyboxPlugin` with `SkyboxMode` (e.g. starfield) |
| `src/audio/` | `SfxPlugin`, `SfxEvent`, `SfxRequest` |
| `src/fragments/` | `FragmentPlugin` for voxel fragment particles |
| `src/prelude.rs` | Convenience re-export of all public types |

### Key design decisions

- **Chunks are 16×16×16** (`CHUNK_SIZE = 16`), stored as a flat `Vec<V>` with index `x*256 + y*16 + z`.
- **Greedy meshing** merges coplanar faces of the same voxel type. LOD reduces detail by sampling at a larger step size. There are **2 LOD levels** (`src/mesh/lod.rs`): LOD 0 (step 1) for distance < 200 units, LOD 1 (step 2) for distance ≥ 200 units (`LOD0_MAX_DISTANCE = 200.0`).
- **World streaming** uses `ViewerPosition` (must be updated every frame by the game) to load/unload chunks via `ChunkLoadRequest` / `ChunkUnloadRequest` events.
- **Generators run on background threads** — `ChunkGenerator::generate` must not access the Bevy `World`.
- **Mining/Placement are pure functions** — `tick_mining`, `stop_mining`, `try_place_voxel`, `tick_placement` are called from the game's own systems; the plugins only initialize resources and message types.
- **TOML config** is loaded from `world.toml` in the working directory at startup. Missing file falls back to defaults.
- **`examples/` and `assets/` are tracked** — example sources and their demo assets live in the repo so `cargo run --example basic_world` works after a clean clone. The `assets/` entry in `.gitignore` only prevents new additions; existing files remain tracked.

### Bevy features used

The library depends on bevy with `default-features = false` and only enables: `bevy_asset`, `bevy_render`, `bevy_pbr`, `bevy_core_pipeline`, `multi_threaded`, `bevy_ui`, `bevy_text`, `bevy_sprite`, `bevy_state`, `bevy_audio`, `vorbis`. Games add `bevy_winit` and platform features themselves.

### Cargo features

| Feature | Default | Implies | Purpose |
|---|---|---|---|
| `items` | yes | — | Inventory, hotbar, drops, pickup UI |
| `crafting` | yes | `items` | Recipe registry, crafting consumption |
| `mobs` | yes | — | Hostile/passive mob AI |
| `tracing` | no | — | Internal profiling spans on hot paths |
| `tracy` | no | `tracing` | Routes spans to a Tracy profiler client |

Examples gate on features: `basic_world` requires `items`; `minetest` requires `items,crafting,mobs`.

## Spec-Driven Development (SDD)

This repo uses **hybrid SDD** for substantial changes. Files in `openspec/`, observations in engram. See [`docs/sdd/workflow.md`](docs/sdd/workflow.md) for the full workflow and [`docs/sdd/conventions.md`](docs/sdd/conventions.md) for what counts as substantial.

**Quick reference**:
- Substantial change → run `/sdd-new <change-name>` then `/sdd-ff` (proposal → spec → design → tasks) → `/sdd-apply` → `/sdd-verify` → `/sdd-archive`.
- Trivial change (typo, internal refactor, doc tweak that tracks an accepted spec) → straight to PR, no SDD.
- Engram topic keys: `sdd-init/voxel-framework` for project context; `sdd/<change>/<artifact>` for change artifacts.
- On `/sdd-archive`, public-API changes propagate to `README.md` / `CHANGELOG.md` so docs don't drift from the spec.

When uncertain, default to substantial and start with `/sdd-explore <topic>` — exploration is cheap.
