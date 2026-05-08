# Proposal: Formalize Tool/Material System Contract

## Intent

The tool/material contract (hardness, mining level, tool level, tool speed) is implemented across `VoxelType`, `ItemType`, and the mining engine, but it is **undocumented as a versioned spec**. Downstream consumers (Starframe) need a stable written contract. This change locks the contract, fills one small `DefaultVoxel` code gap, and adds edge-case tests.

## Scope

### In Scope
- Versioned spec: break-time formula, `hardness`/`mining_level` semantics, `tool_level`/`tool_speed` semantics, level gating, `hardness = 0.0` unbreakable convention.
- Override `DefaultVoxel::hardness()` and `mining_level()` (currently uses trait defaults — every block breaks identically).
- Tests: `hardness = 0.0`, `tool_level < mining_level` gating, `tool_speed` multiplier, `DefaultVoxel` per-variant values.

### Out of Scope
- Tool durability, enchantments, multi-tool requirements, mining XP.
- Changes to `ItemType` or `VoxelType` trait signatures.
- Per-face hardness, biome modifiers.

## Approach

Documentation-first, code-light. Author the spec from existing behavior (no redesign). Add missing `DefaultVoxel` overrides mirroring `DefaultTerrainVoxel`. Add unit tests for the formula and edge cases. No public API breaks.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `openspec/changes/tool-material-system/specs/items/spec.md` | New | Versioned tool/material delta spec |
| `src/voxel.rs` | Modified | `DefaultVoxel` overrides `hardness()` / `mining_level()` |
| `src/voxel.rs` (tests) | New | `DefaultVoxel` overrides + trait defaults |
| `src/world/mining.rs` (tests) | New | Formula, gating, `hardness = 0.0` |
| `src/items/inventory.rs` | Unchanged | Referenced by spec only |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| `DefaultVoxel` behavior change surprises consumers | Medium | CHANGELOG entry; not a breaking API change |
| Spec drifts from implementation | Medium | Verify phase enforces parity |
| Starframe couples to internals outside the spec | Low | Spec lists exact public surface |

## Rollback Plan

Revert commits; delete `openspec/changes/tool-material-system/` (or its archive folder) and the new `openspec/specs/items/spec.md` once merged. Reverting the `DefaultVoxel` override restores trait defaults — no persistence migration, no API change.

## Dependencies

- None. All trait methods already exist.

## Success Criteria

- [ ] `openspec/specs/items/spec.md` exists post-archive and matches current behavior.
- [x] `DefaultVoxel` overrides `hardness()` and `mining_level()` for Rock, IronOre, SiliconCrystal, RareMineral; Air returns `0.0`.
- [x] Tests cover formula, `hardness = 0.0`, level gating, `tool_speed`.
- [x] `cargo check` and `cargo test` pass.
- [x] CHANGELOG entry notes the `DefaultVoxel` behavior change.

## Acceptance

Acceptance: tasks 2.1–2.3 (overrides) and 3.1–3.9 (tests) completed by `sdd-apply` on 2026-05-08; `cargo check` and `cargo test --lib` (155 passed, 0 failed) green; `cargo check --example basic_world` clean. Pre-existing doctest failures unrelated to this change.
