# Design: Formalize Tool/Material System Contract

## Technical Approach

This change is **documentation-first, code-light**. The tool/material contract is already
implemented across `VoxelType` (`hardness`, `mining_level`), `ItemType` (`tool_level`,
`tool_speed`), and `world::mining` (formula, gating, unbreakable handling). The bulk of
the work is the versioned delta spec in `openspec/changes/tool-material-system/specs/items/spec.md`;
the only Rust change is filling one gap: `DefaultVoxel` currently uses the trait defaults
(`hardness=1.0`, `mining_level=0` for every variant), making every block break in `1.5 s`
with bare hands. We override these to mirror `DefaultTerrainVoxel`'s tuning. New tests pin
the contract.

No new traits, no new plugins, no schedule changes. The trait surface is already correct.

## Architecture Decisions

### Decision: Override `DefaultVoxel` rather than redesign the trait

| Option | Tradeoff | Decision |
|---|---|---|
| Override `DefaultVoxel::hardness/mining_level` | Mirrors `DefaultTerrainVoxel` shape, zero API change | Chosen |
| Add a new `MaterialProperties` resource | New plugin wiring, breaks symmetry with existing trait | Rejected — over-engineered |
| Leave `DefaultVoxel` on trait defaults | Spec would document a degenerate baseline (everything breaks identically) | Rejected — defeats the point |

**Rationale**: `VoxelType` already exposes the per-variant hooks. Other consumers
(`DefaultTerrainVoxel`, downstream voxel types in Starframe) override them. `DefaultVoxel`
not overriding them is a code gap, not a design decision.

### Decision: Override values for `DefaultVoxel`

| Variant       | hardness | mining_level | Rationale |
|---------------|----------|--------------|-----------|
| `Air`         | `0.0`    | `0`          | Unbreakable convention (per spec) — also matches existing `tick_mining` `f32::MAX` branch. |
| `Rock`        | `3.0`    | `1`          | Bulk material, mirrors `DefaultTerrainVoxel::Stone`. |
| `IronOre`     | `4.0`    | `2`          | Mirrors `DefaultTerrainVoxel::IronOre`. Requires iron-tier tool. |
| `SiliconCrystal` | `2.5` | `1`          | Softer than rock (crystalline), stone-tier tool. |
| `RareMineral` | `5.0`    | `2`          | Hardest, iron-tier tool — tracks "rarest material" intent in existing doc. |

**Rationale**: Values mirror `DefaultTerrainVoxel` where variants overlap (Stone↔Rock,
IronOre↔IronOre). The two novel variants follow the same gating as `DefaultTerrainVoxel`:
level 1 for common, level 2 for premium ores. Numbers are conventional (multiples of `0.5`)
and match the contract's break-time formula at the default `1.5 s` config.

### Decision: Lock `hardness == 0.0` as the unbreakable sentinel

**Choice**: Document that `hardness() == 0.0` MUST mean unbreakable; document the
existing `tick_mining` behaviour that maps it to `f32::MAX` effective break time.
**Alternatives considered**: Adding a separate `is_breakable() -> bool` method.
**Rationale**: The convention is already coded and works; introducing a second method
would split the contract for no gain.

## Data Flow

The existing flow is unchanged — the spec just names it:

    voxel.hardness()  ──┐
                        ├──→ tick_mining: effective_break_time =
    config.break_time ──┘     break_time * hardness  (or f32::MAX if hardness == 0)
                                       │
    item.tool_speed() ──→ effective_delta = delta * tool_speed
                                       │
    voxel.mining_level() ──→ if tool_level < mining_level → no progress
                                       │
                                       ▼
                              elapsed += effective_delta
                              progress = elapsed / effective_break_time
                              progress >= 1.0 → break voxel

Reference paths: `src/world/mining.rs:366-388` (effective break time and unbreakable
branch); `src/world/mining.rs:615-621` (level gate + tool-speed multiplier in
`mining_input_system`).

## File Changes

| File | Action | Description |
|------|--------|-------------|
| `openspec/changes/tool-material-system/specs/items/spec.md` | Create (sdd-spec) | Versioned delta spec — formula, semantics, RFC 2119 requirements |
| `openspec/changes/tool-material-system/design.md` | Create | This file |
| `src/voxel.rs` | Modify | Add `DefaultVoxel::hardness()` and `mining_level()` overrides per the table above |
| `src/voxel.rs` (tests) | Add | Unit tests asserting the override values; trait-default coverage for a minimal `VoxelType` impl |
| `src/world/mining.rs` (tests) | Add | `hardness == 0.0` → infinite break time; `tool_level < mining_level` → no progress; `tool_speed` halves/quarters elapsed time |
| `CHANGELOG.md` | Modify (in archive phase) | Note `DefaultVoxel` behaviour change — not a breaking API change |

No `src/items/inventory.rs` changes — `tool_level` / `tool_speed` already match the spec.

## Interfaces / Contracts

No new interfaces. The existing trait methods (`VoxelType::hardness`, `mining_level`,
`ItemType::tool_level`, `tool_speed`) and the `MiningConfig::break_time` field constitute
the public surface the spec locks.

## Testing Strategy

| Layer | What to Test | Approach |
|-------|-------------|----------|
| Unit (`voxel.rs`) | `DefaultVoxel` per-variant `hardness()` / `mining_level()` values; `Air` unbreakable | Direct trait calls — no Bevy context |
| Unit (`mining.rs`) | Formula: `effective_break_time = break_time * hardness`; unbreakable maps to `f32::MAX`; `tool_speed` scales delta; `tool_level` gating | Manipulate `MiningState` directly (pattern from existing tests in `mining.rs:659-773`) |
| Integration | Not required — pure-function APIs covered by unit tests | — |

`cargo test` only — no display, no Bevy `App`. Follows the project rule: "never run
`cargo build`; use `cargo check`".

## Migration / Rollout

No migration. Persistence format unchanged (byte mapping untouched). No public API change.
`DefaultVoxel` consumers see different break times after the override — documented in
CHANGELOG, not flagged as breaking. Rollback = revert commits.

## Risks for Downstream Consumers

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Starframe (or other game) hardcoded `1.5 s` break time for all `DefaultVoxel` variants | Low — they would have hit the trait default by accident, not by design | CHANGELOG entry; spec makes per-variant timing explicit |
| Game uses `DefaultVoxel::Air` in places where `hardness() == 0.0` now matters (e.g. tries to mine air) | Very low — `is_solid()` filters air earlier in the mining path | Test `mining_input_system` short-circuits on non-solid hits (already covered by raycast) |
| Future `DefaultVoxel` variant added without overrides | Drifts back to defaults silently | Spec MUST require any new built-in variant to define both `hardness()` and `mining_level()`; covered in `sdd-spec` |

## Open Questions

- [ ] None. Override values are conventional and mirror the existing `DefaultTerrainVoxel` choices; the spec phase will lock the contract verbatim.
