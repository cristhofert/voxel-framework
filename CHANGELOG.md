# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- `world_to_local_coord` no longer returns an out-of-range local index (`16`)
  for positions a tiny distance below a chunk edge (for example `x = -1e-7`).
  In release builds this could panic or read/write the wrong voxel through
  `get_voxel_at` / `set_voxel_at`.
- `resolve_sphere_collision` and `resolve_capsule_collision` no longer push a
  body too far when it rests on the edge or corner between voxels (the push
  was counted once per overlapped voxel: 2x on edges, 4x on corners). Mobs
  no longer pop upward when walking along block edges.
- The pause menu's mouse sensitivity buttons now apply to the live player
  immediately. They only changed `PlayerConfig`, which is copied into
  `Player::sensitivity` at spawn, so the new value took effect only after a
  restart.

### Changed

- `lod_update_system` looks up a chunk's coordinate through its
  `ChunkCoordComp` instead of scanning every loaded chunk each time a chunk
  changes LOD level (falls back to the scan for entities without one).

## [0.10.5] - 2026-11-23

### Added

- Ship the SIL Open Font License 1.1 text (`src/ui/OFL-NotoSans.txt`) for the
  embedded Noto Sans fonts.
- Add `assets/CREDITS.md` documenting the provenance of example assets.

### Changed

- Replace the example block textures with original, procedurally generated
  ones (`scripts/generate-example-textures.py`). The previous textures were
  third-party files without a license and are no longer distributed. Example
  texture files were renamed: `grass_block_top` → `grass_top`, `oak_log` →
  `wood_log`, `oak_leaves` → `leaves`, `water_still` → `water`,
  `amethyst_block` → `crystal`, `emerald_ore` → `rare_ore`.

## [0.10.4] - 2026-11-22

### Added

- Optional HotbarContentPlugin<I> renders inventory slot icons, every stack count,
  and the selected item name. ItemIconPalette<I> supports explicit material-color
  associations through VoxelType::color(), with a generic fallback symbol.
- Native hotbar_icons example and consumer guide, including 14-slot navigation.

### Changed

- Hotbar UI reserves horizontal margins and shrinks square slots to fit one row.
  Content mode caps slots at 50px. Selection renders from HotbarState, including
  direct programmatic changes. Legacy slot-only use and inventory rules remain.

## [0.10.3] - 2026-11-19

### Added

- Optional `DropVisualPlugin<I>` renders positioned collectible drops as shared,
  unlit item-colored cubes. Added a graphical drops example and consumer guide.

### Fixed

- Drop lifetime advances with zero or multiple collectors and without Transform.
  Pickup and expiry remove the visual with the drop; partial pickup retains it.

## [0.10.2] - 2026-11-19

### Fixed

- Every neighbor in the mining input adapter's 2×2 face patch now respects its
  own mining level. Insufficient tools leave protected neighbors intact without
  VoxelBroken messages or item drops. Reach, patch shape, timing, and other
  mining rules remain unchanged.

## [0.10.0] - 2026-11-16

### Fixed

- Vertex-colored terrain now uses an opaque fallback material, restoring depth
  writes for solid voxels instead of forcing all chunks through alpha blending.
  Air remains absent geometry; texture-array rendering is unchanged.

### Migration

- `ChunkMaterialConfig` now has an `alpha_mode` field defaulting to
  `AlphaMode::Opaque`. Fully specified literals must add the field or use
  `..Default::default()`.
- Custom palettes requiring the previous blended fallback must explicitly select
  `AlphaMode::Blend`. This remains a shared chunk policy without separate
  opaque/translucent geometry or per-triangle sorting.

## [0.9.1] - 2026-11-11

### Fixed

- Spawn item drops at the broken voxel's world position and integrate their
  velocity so nearby drops move toward the collector.
- Keep unaccepted drops when the inventory is full, preserve partial-pickup
  remainders, and report only the quantity actually collected.

## [0.9.0] - 2026-11-09

### Migration

- Native players start with a visible, free cursor and require a focused click
  to activate control. Focus return alone does not recapture the cursor.
- `spawn_player` no longer takes a cursor query. `player_look` and `player_move`
  now take a primary-window query; `player_cursor_system` queries the primary
  window and its focus. Registration through Bevy `add_systems` is unchanged;
  consumers invoking the functions directly must update their arguments.

### Fixed

- Player spawning leaves the cursor free until a focused click. Focus loss
  releases capture; native look and movement require a focused, captured primary
  window. Inactive mouse motion is discarded and residual movement is stopped.
  Headless synthetic input remains supported. `player_input_active` exposes the
  ownership condition, and `player_focus` demonstrates native diagnostics.

## [0.8.1] - 2026-10-22

Candidate preparation only; this entry does not announce publication or a tag.
Readiness requires the exact-candidate feature matrix, Rust 1.89.0, targeted
Windows inventory-persistence, documentation, package, and clean-preflight gates.

### Added

- Optional `Container::try_exchange_atomic` and neutral, non-exhaustive
  `ContainerExchangeError`, re-exported through items and the prelude, preserve
  legacy implementors without new bounds or mandatory methods.
- Pinned optional `tempfile =3.27.0`, enabled only by `items`, supports sibling
  temporary-file inventory saves. Exact-toolchain compatibility remains part
  of candidate verification, not an inference from the pin.
- Targeted Windows inventory-persistence CI and expanded exact Rust 1.89.0
  checks complement the existing Linux feature matrix.

### Fixed

- Recipe inputs/outputs now aggregate equal items in first-occurrence order
  with checked quantities; zero entries and empty recipes are supported,
  sentinel items and overflowing recipes are rejected, and `can_craft`
  remains ingredients-only.
- Built-in inventory crafting stages consumption before production, using
  freed capacity and committing only complete exchanges. Inventory validation,
  bounded `add` leftovers, and defensive saturated counting prevent invalid
  layouts and per-item quantity overflow without adding gameplay stack limits.
- Legacy custom-container crafting checks actual operation returns and stops
  on failure. Its conservative fallback remains non-atomic and may partially
  mutate; all-or-nothing behavior requires the optional atomic hook.
- Inventory JSON saves validate the source and replace through a synced sibling
  temporary file without truncating the previous save on pre-replacement
  failure. Existing permissions and followed symlinks are preserved; dangling,
  non-regular, read-only, and Unix multiply hard-linked targets are rejected.
  This is whole-file atomic visibility, not universal power-cut durability or
  preservation of ownership, ACLs, and extended attributes.
- Inventory loads reject capacity/slot-length mismatches and aggregate overflow,
  normalize zero/unknown/empty entries at their original indexes, and preserve
  holes and valid duplicate stacks. Signatures and JSON schema are unchanged.

Only crafting, inventory, and inventory persistence are changed; there are no
mobs/navigation, chunk/world/spawn, HUD/stats/effects saving, or other game-feature
changes, and no automatic integration or publication.

## [0.8.0] - 2026-09-11

### Added

- `VoxelHudPlugin` adds an explicit, non-generic retained HUD renderer backed by
  author-defined `HudRegistry` definitions and screen-ready `HudValueSet`
  values.
- Text, continuous bar, icon, and bounded repeated-segment presentations support
  nine screen anchors, deterministic ordering, a default theme, and narrow
  per-element style overrides.
- `HudSystemSet::Publish` provides a same-frame publication boundary, while
  compatible fallbacks and transition-deduplicated `HudDiagnostic` messages
  keep missing, mismatched, invalid, or unavailable-asset inputs non-panicking.
- Deterministic registry/value snapshots, a feature-independent headless `hud`
  example, and migration guidance document inspection, coexistence with
  `UiPlugin`, and element-by-element replacement of custom overlays.
- The HUD remains an additive, explicit Default Implementation choice: it is
  not included in `UiPlugin`, `VoxelCorePlugin`, or `VoxelPlugin`, requires no
  new Cargo feature, and adds no vertical presets, automatic domain bridges or
  player discovery, gameplay rules, persistence, or JSON/config DSL.

### Fixed

- Corrected the advertised Rust MSRV from 1.85 to **1.89**, required by Bevy
  and `bevy_ecs` 0.18.1; default and no-default library checks pass on Rust 1.89.0.
- HUD images now recover after successful same-path reloads and enter a safe
  state after failed reloads without requiring values to be republished.

## [0.7.0] - 2026-09-06

### Added

- `VoxelEffectsPlugin`, `EffectRegistry`, `EffectDefinition`,
  `ActiveEffectSet`, and `EffectInstance` add a horizontal author-defined
  effects layer without framework presets, potion semantics, or player-specific
  behavior.
- `EffectApplied`, `EffectExpired`, `apply_effect`, `apply_effect_instance`,
  and `tick_effects` provide optional Bevy-message helpers for systems that need
  to react to effect lifecycle changes.
- Effect definitions and active effect sets expose deterministic serializable
  snapshots for documentation, tooling, manifests, and AI inspection.
- Active effects now expose read-side helpers for active state, stacks,
  remaining seconds, and remaining duration ratio.
- `examples/effect_definitions.rs` demonstrates a headless author-defined
  effects setup using `MinimalPlugins`.

## [0.6.0] - 2026-09-05

### Added

- `VoxelStatsPlugin`, `StatRegistry`, `StatDefinition`, and `StatSet` add a
  horizontal stats layer where game authors define runtime values such as
  oxygen, mana, hunger, hull integrity, or any other stat without framework
  presets or fixed body schemas.
- `StatChanged` plus `set_stat`, `add_stat`, `drain_stat`, and `fill_stat`
  provide optional Bevy-message helpers for systems that need to react to
  author-defined stat value changes.
- `StatSet` can now be attached to entities as a Bevy component, while still
  working as the global stats resource initialized by `VoxelStatsPlugin`.
- `StatSet::from_registry(&registry)` creates a runtime set initialized from
  registered defaults for entity-local or global stat values.
- `StatDefinition::span()` plus `StatSet::normalized`,
  `is_at_or_below`, `is_at_or_above`, `is_at_or_below_percent`, and
  `is_at_or_above_percent` add generic read-side ergonomics for UI, rules, and
  effects authored outside the framework.
- Stat definitions and runtime values expose deterministic serializable
  snapshots for documentation, tooling, manifests, and AI inspection.
- `examples/stats_definitions.rs` demonstrates a headless author-defined stats
  setup using `MinimalPlugins` and `StatChanged` readers.

## [0.5.0] - 2026-09-05

### Added

- `VoxelContentPlugin<V>` and the `content` module: an author-defined block
  registry with `BlockDefinition`, `BlockRegistry`, `ContentId`,
  `ContentValue`, and `BlockEngineProperties`. This starts the 0.5 gameplay
  definition layer without shipping game presets or fixed gameplay categories.
- With the `items` feature, `ItemContentPlugin<I>`,
  `BlockDropContentPlugin<I>`, `ItemDefinition`, `ItemRegistry`,
  `BlockDropDefinition`, `BlockDropRegistry`, and `DropCount` extend the same
  author-defined content model to items and block drops.
- Content definitions now include typed attribute helpers, registry
  `require(...)` methods, and block-drop validation helpers so setup code can
  fail loudly when author-defined ids or item references are wrong.
- `DropTable` now supports multiple runtime drop entries per voxel byte and
  probabilistic drop entries, plus a bridge from author-defined
  `BlockDropRegistry` definitions into the runtime table consumed by
  `DropsPlugin`.
- Block, item, and drop registries now expose deterministic serializable
  `snapshot()` outputs for documentation, tooling, manifests, and AI
  inspection.

### Changed

- `VoxelBroken` now carries the destroyed voxel's real `voxel_byte`, and
  `DropsPlugin` uses that value instead of deriving a pseudo-byte from local
  coordinates.
- `world.json` drop override entries now include a `chance` field while
  retaining backward-compatible deserialization for older entries that omit it.

## [0.4.0] - 2026-09-04

### Added

- Documented 0.4 stability expectations, known limitations, and AI-assistant entrypoints.
- `RollMode::Snap` for animated 90° Q/E roll, configurable duration and easing
  via `PlayerConfig`. Default remains `RollMode::Continuous` (no behavior change
  for existing consumers).
- `PlayerMode::FreeFlightAnchored` for Minecraft-creative-style FPS camera
  movement: world-space yaw around a switchable world-axis anchor
  (`Player::anchor_up`, initialized to the nearest signed world axis at spawn),
  pitch clamped to ±89°, and a level horizon. Q/E switch the anchor using the
  snap-roll animation (`snap_roll_duration_secs` / `snap_roll_easing`).

### Changed
- Player movement configuration now uses explicit complete modes:
  `FreeFlightSixDof { roll_mode }`, `FreeFlightAnchored`, and `Grounded`.
  The separate `LookMode` and `PlayerConfig::roll_mode` settings were removed;
  roll behavior is part of the 6DOF mode.
- `RollMode::Snap` semantics: Q/E now animates to the next **world-aligned**
  up candidate (Starframe-style click-to-orient) instead of a fixed ±90°
  relative to the orientation at press time — correcting the horizon drift
  that 6DOF mouse look accumulates. Candidates are the 6 signed world axes
  projected perpendicular to the view: near-axis views give exactly 4 stops,
  diagonal views up to 6. Cancel-and-restart retargeting is lattice-aware.
  No API change; `RollMode::Continuous` untouched. Locked by
  `openspec/specs/player/spec.md` (v2). `examples/basic_world` now showcases
  `RollMode::Snap`.
- Default `PlayerConfig::snap_roll_duration_secs` raised from `0.18` to `0.5`
  after playtest feedback (snappier felt too abrupt).
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
