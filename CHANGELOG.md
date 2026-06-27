# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- Project license changed from MIT to LGPL-3.0-or-later.
- `DefaultVoxel` now overrides `VoxelType::hardness()` and `mining_level()`
  per variant (`Air = 0.0/0`, `Rock = 3.0/1`, `IronOre = 4.0/2`,
  `SiliconCrystal = 2.5/1`, `RareMineral = 5.0/2`) instead of inheriting the
  trait defaults (`1.0/0`). Behavior change for any consumer that mined
  `DefaultVoxel` blocks at the previous uniform `1.5 s` break time. No API
  break, no persistence migration. The tool/material contract is now locked
  by the versioned spec under `openspec/changes/tool-material-system/specs/items/spec.md`.

## [0.3.0] - 2026-04-27

### Added
- `RegionGenerator<V>` trait + `MultiGenerator<V>` + `MultiGeneratorPlugin<V>` —
  composite worlds dispatch chunks to regional generators in insertion order
  with a configurable fallback. A blanket impl for any `ChunkGenerator<V>`
  keeps existing single-generator setups working unchanged.
- Optional `tracing` Cargo feature wraps hot paths (greedy & neighbor meshing,
  world streaming, chunk unload, generator dispatch/poll, occlusion, capsule
  collision, raycast) in `tracing` spans. Zero overhead when disabled.
- Optional `tracy` Cargo feature routes those spans to a Tracy profiler client
  via `tracing-tracy`. Init helper `voxel_framework::profile::init_tracy()`
  for consumers to call before constructing the Bevy `App`.
- `docs/multiplayer-readiness.md` — audit and invariants for any future
  networking work; documents that the framework's simulation path is
  deterministic by construction (no `thread_rng`, no wall-clock reads).

### Changed
- **BREAKING**: `MobConfig::gravity` is now `Option<f32>` instead of `f32`.
  `Some(g)` keeps the previous grounded integration; `None` skips
  vertical-velocity application and `on_ground` snapping for zero-g games.
  `MobConfig::default()` returns `Some(20.0)`, so callers that build via
  `..MobConfig::default()` are unaffected. Explicit `gravity: 20.0` field
  literals must become `gravity: Some(20.0)`.
- `items`, `crafting`, and `mobs` modules are now gated behind same-named
  Cargo features. All three are in `default = ["items", "crafting", "mobs"]`,
  so existing consumers get them without configuration. Games that want only
  the engine layer can set `default-features = false`. Examples declare
  `required-features`: `basic_world` needs `items`; `minetest` needs all three.
- README adds a "Profiling" section documenting the new feature flags.

### Fixed
- Removed unused imports in `src/world/raycast.rs` test module surfaced by
  `cargo check --no-default-features`.

## [0.2.0] - 2026-04-22

### Added
- `VoxelCorePlugin` — pure engine plugin layer (world streaming, render, mesh,
  occlusion, mining, placement, loading) without game-specific systems
- `PauseMenuPlugin` — optional full pause menu with `PauseMenuState`
  (Playing / Paused / Settings), dark overlay, Resume/Settings/Quit buttons,
  FOV and SFX volume sliders, and `SfxRequest` integration
- `PauseMenuConfig` — configurable `Handle<Font>` for all pause menu text
- `PlayerPlugin<V>` — 6DOF zero-gravity jetpack player with capsule collision,
  mouse look, WASD movement, Q/E roll, and cursor management
- `UiPlugin` — `CrosshairPlugin`, `LoadingScreenPlugin`, `HotbarUiPlugin`,
  `PickupNotificationPlugin`
- `SkyboxPlugin` — procedural starfield and solid-color skybox modes
- `SfxPlugin` — two-layer audio architecture (event emitter + game-provided handler)
- `FragmentPlugin` — voxel fragment particles on block break
- `DropsPlugin` — item drop system with `DropTable` weighted random selection
- `HotbarPlugin` — hotbar selection state and scroll input
- Neighbor-aware greedy meshing (`generate_chunk_mesh_with_neighbors`) for
  seam-free chunk borders
- Save-on-unload: chunks written to disk when they stream out of view
- `OcclusionPlugin<V>` — hides chunks fully enclosed by solid neighbors
- `MiningPlugin` — pure-function voxel break API with break-time tracking,
  `VoxelBroken` events, and `MiningProgress` messages
- `PlacementPlugin` — pure-function voxel placement API with cooldown tracking
  and `VoxelPlaced` events
- `LoadingPlugin` — `LoadingProgress` messages and `WorldReady` event
- `PersistencePlugin<V>` — auto-save on `ChunkModified`, save-on-unload
- Dynamic LOD system (2 levels: full detail < 50 units, coarse > 50 units)
  with hysteresis to prevent mesh thrashing
- Sphere and capsule collision detection against solid voxels
- DDA raycast (`raycast`) returning `RaycastHit` with voxel coords and face normal
- Inventory system with `Inventory`, `InventorySlot`, `ItemCollector`, `MagnetConfig`
- Item persistence (`save_inventory` / `load_inventory`) in JSON format
- `WorldConfig` loaded from `world.toml` with TOML asteroid, lighting, and
  skybox configuration sections
- `VoxelPalette` resource for runtime voxel color lookups
- Comprehensive rustdoc coverage for all public modules, including conceptual
  guides, algorithm descriptions, ASCII diagrams, and code examples
- `CLAUDE.md` with architecture reference and development commands

### Changed
- `VoxelPlugin` now delegates to `VoxelCorePlugin` internally; existing code
  using `VoxelPlugin` is unaffected
- LOD simplified from 3 levels to 2 (removed intermediate step that caused
  visual artifacts near the first boundary)
- Default load distance reduced for better out-of-the-box performance
- Dependency build profiles optimized: Bevy and deps compile at opt-level 3
  in debug builds, own crate stays at opt-level 1

### Fixed
- `ClearColor` set to black in starfield skybox mode to prevent grey background
- Player capsule dimensions restored to correct proportions after erroneous resize
- Asteroid generator no longer applies Y-axis flattening to asteroid shapes
- `ItemPickedUp` message now correctly registered in `PickupNotificationPlugin`
- Empty chunks skipped during render to avoid redundant draw calls

## [0.1.0] - 2026-03-22

### Added
- Initial implementation: `VoxelPlugin`, `Chunk<V>`, `ChunkManager`,
  `ViewerPosition`, world streaming, greedy meshing, `AsteroidGenerator`,
  `GeneratorPlugin<V>`, `RenderPlugin<V>`, `MeshPlugin<V>`, FBM noise,
  binary chunk persistence, coordinate utilities
