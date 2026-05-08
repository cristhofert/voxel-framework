# Tasks: Formalize Tool/Material System Contract

## Phase 1: Scaffolding

- [x] 1.1 Confirm `openspec/changes/tool-material-system/` contains `proposal.md`, `design.md`, and `specs/items/spec.md` (already authored). No new files in this phase.
- [x] 1.2 Run `cargo check` on a clean tree to record a green baseline before edits.
- [x] 1.3 Run `cargo test` to record the current passing test set (so new failures are attributable to this change).

## Phase 2: Implementation

- [x] 2.1 In `src/voxel.rs`, add `impl VoxelType for DefaultVoxel` overrides for `hardness()` returning per-variant values: `Air = 0.0`, `Rock = 3.0`, `IronOre = 4.0`, `SiliconCrystal = 2.5`, `RareMineral = 5.0` (per design table; satisfies "DefaultVoxel Material Overrides" + "Air is unbreakable" scenarios).
- [x] 2.2 In `src/voxel.rs`, add `mining_level()` overrides: `Air = 0`, `Rock = 1`, `IronOre = 2`, `SiliconCrystal = 1`, `RareMineral = 2` (satisfies "Iron ore requires level-2 tool" scenario).
- [x] 2.3 Verify the overrides compile: `cargo check`. No other production code touched.

## Phase 3: Testing

Tests in this phase MUST be written AFTER the overrides from Phase 2 land — assertions on
`DefaultVoxel::IronOre.mining_level() == 2` will fail against the trait default.

- [x] 3.1 In `src/voxel.rs` `#[cfg(test)] mod tests`, add `default_voxel_hardness_overrides` asserting each variant returns the value from task 2.1 (covers "Air is unbreakable" + per-variant table).
- [x] 3.2 In `src/voxel.rs` tests, add `default_voxel_mining_level_overrides` asserting each variant returns the value from task 2.2 (covers "Iron ore requires level-2 tool").
- [x] 3.3 In `src/voxel.rs` tests, add `voxel_type_trait_defaults` using a minimal local `VoxelType` impl to assert the trait defaults are still `hardness() == 1.0` and `mining_level() == 0` (locks "Hardness Multiplier" + "Mining-Level Gating" default clauses).
- [x] 3.4 In `src/world/mining.rs` `#[cfg(test)] mod tests`, add `hardness_zero_is_unbreakable` constructing a `MiningState` for a voxel with `hardness() == 0.0` and asserting `effective_break_time == f32::MAX` and that `tick_mining` over a long delta does NOT emit `VoxelBroken` (covers "Hardness 0.0 is unbreakable").
- [x] 3.5 In `src/world/mining.rs` tests, add `hardness_scales_break_time` asserting `effective_break_time == break_time * hardness` for `break_time = 1.5`, `hardness = 3.0` → `4.5` (covers "Standard hardness scales break time").
- [x] 3.6 In `src/world/mining.rs` tests, add `tool_level_below_mining_level_blocks_progress` asserting that with `tool_level = 1` vs `mining_level = 2`, no `MiningProgress`/`VoxelBroken` is emitted and `stop_mining` semantics hold (covers "Tool level below voxel mining level blocks progress"; mirrors gating at `src/world/mining.rs:615-621`).
- [x] 3.7 In `src/world/mining.rs` tests, add `tool_level_meets_requirement_progresses` asserting progress accumulates when `tool_level == mining_level` (covers "Tool level meets or exceeds requirement").
- [x] 3.8 In `src/world/mining.rs` tests, add `tool_speed_scales_delta` asserting that with `tool_speed = 2.0`, `delta = 0.1`, `break_time = 1.5`, `hardness = 1.0`, the per-call delta passed to `tick_mining` is `0.2` and `progress >= 1.0` after ~`0.75 s` accumulated real time (covers "Faster tool reduces effective break time"; multiplier site at `src/world/mining.rs:615-621`).
- [x] 3.9 Run `cargo test` and confirm all new tests pass.

## Phase 4: Documentation

- [x] 4.1 Add a `CHANGELOG.md` entry under the next unreleased section noting: `DefaultVoxel` now overrides `hardness()` and `mining_level()` per-variant (was trait defaults). Flag as behavior change, not API break.
- [x] 4.2 Verify `README.md` Quick Start needs no change (DefaultVoxel is the internal default; new behavior is invisible to the documented API surface). Note the verification result in the verify report.
- [x] 4.3 Append an "Acceptance" line to `openspec/changes/tool-material-system/proposal.md` Success Criteria once items 2.x and 3.x are checked off (do this in `sdd-apply`, not now).

## Phase 5: Verification

- [x] 5.1 Run `cargo check` — must be clean.
- [x] 5.2 Run `cargo test` — all tests including the 8 new ones must pass.
- [x] 5.3 Run `cargo check --example basic_world` — must be clean (DefaultVoxel is what the example uses).
- [ ] 5.4 Hand off to `sdd-verify` with: spec at `openspec/changes/tool-material-system/specs/items/spec.md`, tasks at this file, evidence = test output + `cargo check` exit codes.
