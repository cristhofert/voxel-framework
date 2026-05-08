# Verification Report — `tool-material-system`

**Change**: tool-material-system
**Phase**: sdd-verify
**Backend mode**: hybrid (engram + openspec)
**Date**: 2026-05-08
**Spec version**: v1 (delta in `openspec/changes/tool-material-system/specs/items/spec.md`)

---

## Completeness

| Metric | Value |
|--------|-------|
| Tasks total | 22 |
| Tasks complete (`[x]`) | 21 |
| Tasks incomplete (`[ ]`) | 1 |

**Incomplete task**: `5.4 Hand off to sdd-verify` — this is the current phase itself, so it is correctly left unchecked until sdd-archive runs. **Not a blocker.**

All implementation, testing, and documentation tasks (1.1 → 5.3) are complete.

---

## Build & Tests Execution

### Build (`cargo check`)

```
Finished `dev` profile [optimized + debuginfo] target(s) in 0.18s
```

**Result**: PASS — clean build, exit code 0.

### Library tests (`cargo test --lib`)

```
running 155 tests
...
test result: ok. 155 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```

**Result**: PASS — 155 passed, 0 failed (matches apply-phase claim).

The 8 new tests added by this change all pass:

| Test | File |
|------|------|
| `voxel::tests::default_voxel_hardness_overrides` | `src/voxel.rs` |
| `voxel::tests::default_voxel_mining_level_overrides` | `src/voxel.rs` |
| `voxel::tests::voxel_type_trait_defaults` | `src/voxel.rs` |
| `world::mining::tests::hardness_zero_is_unbreakable` | `src/world/mining.rs` |
| `world::mining::tests::hardness_scales_break_time` | `src/world/mining.rs` |
| `world::mining::tests::tool_level_below_mining_level_blocks_progress` | `src/world/mining.rs` |
| `world::mining::tests::tool_level_meets_requirement_progresses` | `src/world/mining.rs` |
| `world::mining::tests::tool_speed_scales_delta` | `src/world/mining.rs` |

### Integration tests (`cargo test --tests`)

```
running 14 tests
...
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**Result**: PASS — 14 passed, 0 failed.

### Example check (`cargo check --example basic_world`)

```
Finished `dev` profile [optimized + debuginfo] target(s) in 0.19s
```

**Result**: PASS — clean build, exit code 0.

### Coverage

`coverage_threshold` in `openspec/config.yaml` is `0` → **not configured**, skipped per skill rules.

---

## Spec Compliance Matrix

Every scenario from `specs/items/spec.md` mapped to the implementation site and the test that proves it at runtime.

| Requirement | Scenario | Implementation | Test | Result |
|-------------|----------|----------------|------|--------|
| Hardness Multiplier | Standard hardness scales break time | `src/world/mining.rs:376-385` (`tick_mining` sets `effective_break_time = break_time * hardness`) | `world::mining::tests::hardness_scales_break_time` | COMPLIANT |
| Hardness Multiplier | Hardness 0.0 is unbreakable | `src/world/mining.rs:377-381` (`if hardness > 0.0 { ... } else { f32::MAX }`) and `:397` (`if state.progress >= 1.0 && state.effective_break_time < f32::MAX`) | `world::mining::tests::hardness_zero_is_unbreakable` | COMPLIANT |
| Mining-Level Gating | Tool level below voxel mining level blocks progress | `src/world/mining.rs:615-618` (`if tool_stats.tool_level < voxel_mining_level { stop_mining(&mut state); return; }`) | `world::mining::tests::tool_level_below_mining_level_blocks_progress` | COMPLIANT |
| Mining-Level Gating | Tool level meets or exceeds requirement | `src/world/mining.rs:615-637` (gate falls through when `tool_level >= mining_level`, then `tick_mining` is invoked) | `world::mining::tests::tool_level_meets_requirement_progresses` | COMPLIANT |
| Tool Speed Multiplier | Faster tool reduces effective break time | `src/world/mining.rs:621` (`let effective_delta = time.delta_secs() * tool_stats.tool_speed`) | `world::mining::tests::tool_speed_scales_delta` | COMPLIANT |
| Drop Table Resolution | Explicit registration wins | `src/items/drops.rs:105-115` (`if let Some(&(item, min, max)) = self.entries.get(&voxel_byte)` returns first) | `items::drops::tests::test_drop_table_get_drop` (pre-existing) | COMPLIANT (PRE-EXISTING TEST) |
| Drop Table Resolution | Identity fallback for unregistered byte | `src/items/drops.rs:117-122` (`I::from_byte(voxel_byte)` then `if item.to_byte() != 0 { Some((item, 1)) }`) | (no direct test) | UNTESTED — see Issues |
| Drop Table Resolution | Empty fallback yields no drop | `src/items/drops.rs:117-123` (`else { None }`) | `items::drops::tests::test_drop_table_no_entry_returns_none` (pre-existing) | COMPLIANT (PRE-EXISTING TEST) |
| DefaultVoxel Material Overrides | Air is unbreakable | `src/voxel.rs:370-379` (`DefaultVoxel::Air => 0.0`) | `voxel::tests::default_voxel_hardness_overrides` | COMPLIANT |
| DefaultVoxel Material Overrides | Iron ore requires level-2 tool | `src/voxel.rs:381-390` (`DefaultVoxel::IronOre => 2`) plus the gating site at `mining.rs:615-618` | `voxel::tests::default_voxel_mining_level_overrides` (asserts `IronOre.mining_level() == 2`); `world::mining::tests::tool_level_below_mining_level_blocks_progress` (asserts the gate blocks tool_level=1 vs level=2) | COMPLIANT |
| (Trait defaults clauses across requirements) | `hardness()` defaults to `1.0`, `mining_level()` defaults to `0` | `src/voxel.rs:143-145` and `:160-162` | `voxel::tests::voxel_type_trait_defaults` | COMPLIANT |

**Compliance summary**: 10 of 10 spec scenarios COMPLIANT (1 of those — the identity-fallback drop scenario — relies on a pre-existing positive test for the registered path plus the empty-fallback test, but lacks a direct test for the non-empty identity-fallback case; flagged as a SUGGESTION below, not a blocker).

---

## Correctness (Static — Structural Evidence)

| Requirement | Status | Notes |
|------------|--------|-------|
| Hardness Multiplier | Implemented | Formula at `mining.rs:376-385`; default `1.0` at `voxel.rs:143-145`. |
| Mining-Level Gating | Implemented | Strict-less-than gate at `mining.rs:615-618`; default `0` at `voxel.rs:160-162`. |
| Tool Speed Multiplier | Implemented | Multiplier at `mining.rs:621`; `ToolStats::default()` returns `tool_speed: 1.0` at `mining.rs:145-149`; reset path at `mining.rs:139-142`. |
| Drop Table Resolution | Implemented | Three-branch resolution at `drops.rs:105-124`. |
| DefaultVoxel Material Overrides | Implemented | Per-variant overrides at `voxel.rs:370-390` matching the design table exactly (`Air=0.0/0`, `Rock=3.0/1`, `IronOre=4.0/2`, `SiliconCrystal=2.5/1`, `RareMineral=5.0/2`). |

---

## Coherence (Design)

| Decision | Followed? | Notes |
|----------|-----------|-------|
| Override `DefaultVoxel` rather than redesign trait | Yes | No new traits, no new plugins. |
| Override values per design table | Yes | Exact values present at `voxel.rs:370-390`. |
| Lock `hardness == 0.0` as unbreakable sentinel | Yes | Spec wording matches `tick_mining` behaviour at `mining.rs:377-381` and `:397`. |
| File Changes table | Followed (with caveat) | `src/voxel.rs`, `src/world/mining.rs` (tests only), and `CHANGELOG.md` all modified per table. **Caveat**: design table says `src/voxel.rs` would be "Modified — add `DefaultVoxel::hardness()` and `mining_level()` overrides" — but apply discovered the overrides were ALREADY present (see SDD-pilot finding below). This is a coherence note about reality vs. design assumptions, not a deviation from the design intent. |

---

## Out-of-Scope Items (Not Blockers)

### Pre-existing doctest failures on `dev` baseline

Per the apply-phase report, `cargo test --doc` (full doctest run) reports failures in unrelated modules (`audio`, `generator/asteroid`, `items`, `render/material`, `ui/...`, `world/loading`). These exist on the `dev` baseline before this change and are explicitly out of scope for `tool-material-system`.

This phase did not run `cargo test --doc` because the apply phase already evidenced these failures predate this change, the project's `rules.verify.test_command` is `cargo test` (not `cargo test --doc`), and re-running with stash/restore would offer no incremental value.

**Recommendation**: track baseline doctest health as a separate change.

---

## SDD-Pilot Observation: Code Preceded Spec

During apply, the implementer found that the `DefaultVoxel` `hardness()` and `mining_level()` overrides at `src/voxel.rs:370-390` were **already present** in the codebase before any `sdd-apply` edit was made. The values matched the design table exactly. The spec, design, and tasks for Phase 2 (implementation) therefore documented behavior that already existed in code.

**Why this matters for the SDD pilot**: This is the exact drift case SDD is meant to prevent. The contract had been silently implemented but never specified, tested, or documented. Without SDD it would have remained tribal knowledge — vulnerable to silent regressions whenever someone "fixed" the per-variant values back to the trait defaults.

**What SDD recovered**:
- A versioned `specs/items/spec.md` delta locking the contract (10 scenarios across 5 requirements).
- 8 new unit tests pinning each requirement at runtime (3 in `voxel.rs`, 5 in `world/mining.rs`).
- A CHANGELOG entry framing the per-variant overrides as a documented behavior.
- An explicit "Acceptance" line on the proposal.

**Going-forward signal for the pilot**: the workflow proved its worth even on a "code-light" change — the value was the audit trail and the test pinning, not new functionality. Future pilots should treat undocumented existing behavior as a **first-class candidate** for SDD, not as an edge case.

This finding has been saved to engram (topic: `discovery/sdd-pilot-code-preceded-spec`).

---

## Issues Found

### CRITICAL (must fix before archive)
None.

### WARNING (should fix)
None.

### SUGGESTION (nice to have)

1. **Identity-fallback drop scenario lacks a direct unit test.** The "Identity fallback for unregistered byte" scenario in `specs/items/spec.md` is implemented at `src/items/drops.rs:117-122`, and the `test_drop_table_no_entry_returns_none` test covers the empty-byte branch. There is no test that explicitly asserts a non-empty `from_byte(b)` returns `Some((variant, 1))` for an unregistered byte. The behavior is structurally clear, but a 4-line test would close the matrix. **Not a blocker** — the requirement was pre-existing implementation reaffirmed by this change, not new functionality, and the structural evidence is unambiguous.

2. **Doctest baseline.** The 14 pre-existing doctest failures on `dev` should be tracked as a separate change (out of scope here).

---

## Verdict

**PASS WITH NOTES**

All 10 spec scenarios are compliant by static structural evidence; 9 of 10 are also covered by tests that passed in this verification run. The remaining scenario (identity-fallback non-empty branch in `DropTable::get_drop`) has clear structural evidence and adjacent tests covering both ends of the same conditional. The change is ready for `sdd-archive`.

`cargo check`, `cargo test --lib` (155 passed), `cargo test --tests` (14 passed), and `cargo check --example basic_world` all pass cleanly. No production code or tests were modified during verification.

---

## Artifacts

- Spec: `openspec/changes/tool-material-system/specs/items/spec.md` + engram `sdd/tool-material-system/spec`
- Design: `openspec/changes/tool-material-system/design.md`
- Tasks: `openspec/changes/tool-material-system/tasks.md` + engram `sdd/tool-material-system/tasks`
- Apply progress: engram `sdd/tool-material-system/apply-progress`
- Verify report (this file): `openspec/changes/tool-material-system/verify-report.md` + engram `sdd/tool-material-system/verify-report`
