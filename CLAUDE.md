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

### Plugin composition

`VoxelCorePlugin` (en `src/lib.rs`) es el motor puro — no asume nada del juego:
- `WorldPlugin` + `WorldConfigPlugin` — `ChunkManager`, `ViewerPosition`, streaming, TOML config
- `RenderPlugin<DefaultVoxel>` — shared `StandardMaterial` para chunks con vertex colors
- `MeshPlugin<DefaultVoxel>` — generación de mesh async y chunk despawn
- `OcclusionPlugin<DefaultVoxel>` — oculta chunks rodeados de vecinos sólidos
- `MiningPlugin` / `PlacementPlugin` — APIs de break/place (estilo pure-function)
- `LoadingPlugin` — eventos `LoadingProgress` + `WorldReady`

`VoxelPlugin` es el batteries-included: internamente usa `VoxelCorePlugin` y agrega los plugins game-specific:
- `FragmentPlugin`, `SkyboxPlugin`, `SfxPlugin`, `HotbarPlugin`, `UiPlugin`

Los juegos eligen qué necesitan:

```rust
// Motor voxel puro
.add_plugins(VoxelCorePlugin::<DefaultVoxel>::new())
.add_plugins(GeneratorPlugin::<MyVoxel>::new())

// Todo batteries-included
.add_plugins(VoxelPlugin::<DefaultVoxel>::new())
```

`GeneratorPlugin<V>` y `PersistencePlugin<V>` son siempre opt-in — requieren que el usuario provea `GeneratorResource<V>`.

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
- **Greedy meshing** merges coplanar faces of the same voxel type. LOD reduces detail by sampling at step size 2 (LOD 1) or 4 (LOD 2). LOD thresholds: <50 units → LOD 0, 50–100 → LOD 1, >100 → LOD 2.
- **World streaming** uses `ViewerPosition` (must be updated every frame by the game) to load/unload chunks via `ChunkLoadRequest` / `ChunkUnloadRequest` events.
- **Generators run on background threads** — `ChunkGenerator::generate` must not access the Bevy `World`.
- **Mining/Placement are pure functions** — `tick_mining`, `stop_mining`, `try_place_voxel`, `tick_placement` are called from the game's own systems; the plugins only initialize resources and message types.
- **TOML config** is loaded from `world.toml` in the working directory at startup. Missing file falls back to defaults.
- **`examples/` and `assets/` are tracked** — example sources and their demo assets live in the repo so `cargo run --example basic_world` works after a clean clone. The `assets/` entry in `.gitignore` only prevents new additions; existing files remain tracked.

### Bevy features used

The library depends on bevy with `default-features = false` and only enables: `bevy_asset`, `bevy_render`, `bevy_pbr`, `bevy_core_pipeline`, `multi_threaded`, `bevy_ui`, `bevy_text`, `bevy_sprite`. Games add `bevy_winit` and platform features themselves.
