# SDD Conventions — `voxel-framework`

When **must** a change go through SDD, and when can it skip the flow? This document is the rule. It's intentionally biased toward letting trivial work move fast.

## The rule: substantial = SDD

A change is **substantial** (and MUST go through `/sdd-new` → … → `/sdd-archive`) if it touches **any** of:

### Public API surface
- `src/lib.rs`, `src/prelude.rs` (re-exports, top-level types)
- Trait definitions: `VoxelType` (`src/voxel.rs`), `ChunkGenerator<V>` / `RegionGenerator<V>` (`src/generator/`), and any other public trait
- Public struct fields, public enum variants, function signatures called from `examples/` or downstreams (notably Starframe)
- Cargo features (`Cargo.toml [features]`) — any add/remove/rename, or moving items behind a feature

### Plugin and system wiring
- A new `*Plugin` exposed publicly, or removal of an existing one
- Changes to system order (Bevy schedule labels, `add_systems` placement)
- Changes to which plugins `VoxelCorePlugin` vs `VoxelPlugin` compose

### Persistence and on-disk formats
- `src/chunk/persistence.rs` — chunk binary format
- `src/world/persistence.rs` — world save layout
- `src/items/persistence.rs` — item state format
- `world.toml` config schema (`src/world/config.rs`)
- Any change that breaks compatibility with existing saves

### Core algorithms
- Greedy meshing (`src/mesh/`)
- LOD logic and thresholds (`src/mesh/lod.rs`)
- Occlusion (`src/world/occlusion.rs`)
- Raycast / collision / mining / placement (`src/world/`)
- Generator dispatch (`src/generator/region.rs` — multi-generator)

### Render path
- Shaders (any `.wgsl`)
- `ChunkMaterial`, `RenderPlugin`, vertex layout
- Skybox modes (`src/skybox/`)

### Cross-cutting behavior
- Status effects model (`src/player/effects.rs`), damage/regen application
- Event types exposed publicly (e.g., `PlayerDamaged`, `LoadingProgress`, `WorldReady`)

## The exception list: NOT substantial

Skip SDD — go straight to a PR — if the change is **only** any of:

- Typo or doc-only fix (including comments)
- Clippy/rustfmt cleanup with no behavior change
- Internal refactor with no public-API delta and no algorithm change
- Constant tweak that's already a documented config knob (e.g., `LOD0_MAX_DISTANCE` adjustment if the threshold is the only parameter the spec references)
- Test-only additions that don't change source behavior
- Asset-only changes (sprites, audio, fonts under `src/ui/`)
- CI / `.github/workflows/` tweaks (unless adding an SDD gate, which itself is substantial)
- README / CHANGELOG / docs updates that **track** an already-accepted spec (these are part of `/sdd-archive`, not a separate change)

## When unsure

If you can't tell, default to **substantial** and run `/sdd-explore <topic>` first. Exploration is cheap and produces an `exploration.md` you can throw away if the conclusion is "this is trivial."

The PR template has a checkbox for exactly this. Pick one, and if a reviewer disagrees, escalate via comment — don't silently flip flags.

## Examples

| Change | Verdict | Why |
|---|---|---|
| Fix typo in `CLAUDE.md` | not substantial | doc-only |
| Add new variant to `DefaultVoxel` | substantial | public enum, downstream compatibility |
| Tune fall-damage curve constant | not substantial | constant in player module, no API change |
| Add `damage_curve()` to `PlayerHealth` | substantial | new public method |
| Refactor internal `noise::fbm` for perf | not substantial | private, no behavior delta |
| Change FBM signature to take `&Seed` | substantial | public surface (used by generators) |
| Add `tracy-tracing-tracy-only-mode` Cargo feature | substantial | Cargo features rule |
| Update `tracing-tracy` to 0.12 | substantial | dependency bump, can break consumers |
| Update `serde_json` patch version | not substantial | semver-compatible bump |
| Migrate chunk binary format from v2 to v3 | substantial | persistence format |
| Add new mob type behind `mobs` feature | substantial | new public type, new system wiring |

## Gates

Right now there is **no CI gate** enforcing this. The PR template asks for the verdict, and reviewers check it. We'll add a CI gate if drift returns.
