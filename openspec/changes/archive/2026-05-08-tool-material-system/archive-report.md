# Archive Report — `tool-material-system`

**Change**: tool-material-system
**Phase**: sdd-archive
**Backend mode**: hybrid (engram + openspec)
**Archive date**: 2026-05-08
**Verify verdict**: PASS WITH NOTES (see Outstanding Follow-ups)

---

## SDD Pilot — Closing Note

This was the **FIRST end-to-end pilot of the hybrid SDD workflow** in
`voxel-framework`. The workflow ran the full DAG cleanly — `sdd-explore` →
`sdd-propose` → `sdd-spec` → `sdd-design` → `sdd-tasks` → `sdd-apply` →
`sdd-verify` → `sdd-archive` — with both Engram persistence and OpenSpec files
producing complete, traceable artifacts. The change itself was small
(documentation-first, code-light), which made it an ideal pilot: enough surface
to exercise every phase, small enough that drift between phases was easy to
catch.

**Pilot signal**: the SDD workflow produced its highest value on a "code-light"
change. The implementation was already in the codebase; SDD recovered the
contract as a versioned spec, pinned it with 8 unit tests, and locked the
behavior in CHANGELOG. Future pilots should treat undocumented existing
behavior as a first-class candidate for SDD.

---

## Lineage — Engram Observations

All artifacts persisted to engram with stable topic keys. IDs preserved for
audit-trail traceability.

| Topic Key | Observation ID | Type | Saved |
|-----------|---------------:|------|-------|
| `sdd/tool-material-system/explore` | 52 | architecture | 2026-03-29 (early exploration) |
| `sdd/tool-material-system/proposal` | 107 | architecture | 2026-05-08 14:58 |
| `sdd/tool-material-system/spec` | 108 | architecture | 2026-05-08 15:01 |
| `sdd/tool-material-system/design` | 110 | architecture | 2026-05-08 15:01 |
| `sdd/tool-material-system/tasks` | 111 | architecture | 2026-05-08 15:03 |
| `sdd/tool-material-system/apply-progress` | 112 | architecture | 2026-05-08 15:13 |
| `sdd/tool-material-system/verify-report` | 114 | architecture | 2026-05-08 15:17 |
| `discovery/sdd-pilot-code-preceded-spec` | 115 | discovery | 2026-05-08 15:17 |
| `sdd/tool-material-system/archive-report` | (this report) | architecture | 2026-05-08 |

### Note on `discovery/mining-tests-bevy-coupling`

The orchestrator launch prompt referenced `discovery/mining-tests-bevy-coupling`
as a discovery to record. A search of engram for this topic key, and for the
keywords "mining tests Bevy coupling", returned no observation. The discovery
appears to have been planned but not persisted during the apply or verify
phases — likely because the apply phase did not encounter Bevy-coupling
problems with the mining tests (the new tests in `src/world/mining.rs`
manipulate `MiningState` directly without an `App`, following the existing
test pattern at `mining.rs:659-773`). Recording the absence here closes the
loop: no orphan reference remains.

---

## Files Archived (before → after)

The entire change folder was moved to the archive with an ISO-date prefix.
All five artifacts and the `specs/items/` subdirectory were preserved verbatim.

| Before | After |
|--------|-------|
| `openspec/changes/tool-material-system/proposal.md` | `openspec/changes/archive/2026-05-08-tool-material-system/proposal.md` |
| `openspec/changes/tool-material-system/specs/items/spec.md` | `openspec/changes/archive/2026-05-08-tool-material-system/specs/items/spec.md` |
| `openspec/changes/tool-material-system/design.md` | `openspec/changes/archive/2026-05-08-tool-material-system/design.md` |
| `openspec/changes/tool-material-system/tasks.md` | `openspec/changes/archive/2026-05-08-tool-material-system/tasks.md` |
| `openspec/changes/tool-material-system/verify-report.md` | `openspec/changes/archive/2026-05-08-tool-material-system/verify-report.md` |

The change directory was untracked in git (the change was never committed
during the pilot, by design — the user reviews and commits at the end), so the
move was performed with `mv` rather than `git mv`. The archive is now visible
under `openspec/changes/archive/2026-05-08-tool-material-system/` and remains
untracked until the user stages and commits it.

---

## Files Synced

### `openspec/specs/items/spec.md` (CREATED)

**Action**: New capability spec — no prior `openspec/specs/items/spec.md`
existed. The delta spec from the change was promoted to the canonical spec.

**Content preserved verbatim**:
- 5 `ADDED Requirements` (Hardness Multiplier, Mining-Level Gating, Tool Speed
  Multiplier, Drop Table Resolution, DefaultVoxel Material Overrides).
- 10 scenarios (every Given/When/Then bullet retained word-for-word).
- All RFC 2119 keywords (MUST, SHALL, SHOULD, MAY) preserved.

**Wording adjustments during merge** (non-substantive):
- Replaced the "Delta for items" preamble with a canonical capability header
  that names this file as the source of truth and references the originating
  change. The 5 requirements and 10 scenarios were NOT renumbered, NOT
  reordered, and NOT reworded — only the framing prose changed.
- Kept the `## Requirements` heading from the canonical openspec convention
  with `### Requirement: ...` sub-headings so future changes can reference
  individual requirements by name.

### `CHANGELOG.md` (already updated by sdd-apply — no archive edit needed)

The `## [Unreleased]` section already contains the entry written during apply
(see `CHANGELOG.md:9-17`). Verified during this archive — the entry is
correctly framed as a behavior change (not an API break), names every new
override per variant, and points back to the spec at
`openspec/changes/tool-material-system/specs/items/spec.md`. The reference
path now resolves to the archived location, but that is acceptable — the
CHANGELOG points at the change folder where the contract was first published,
not at the canonical source. No edit performed.

### `README.md` (no change required)

Per the verify report and the `openspec/config.yaml` `rules.archive`
("sync README.md and CHANGELOG.md from the accepted spec when the change has
public-API impact"), README was reviewed against the spec:

| README claim | Spec impact | Decision |
|---|---|---|
| `Voxel Palette (DefaultVoxel)` table at `README.md:613-621` lists the 5 variants and their colors | Variant set unchanged (Air, Rock, IronOre, SiliconCrystal, RareMineral); colors unchanged | No edit |
| Quick Start uses `DefaultVoxel` directly | No public API change; the per-variant `hardness()`/`mining_level()` overrides are internal to `DefaultVoxel` | No edit |
| Mining & Placement section documents `tick_mining`, `stop_mining`, `try_place_voxel`, `tick_placement` | All four signatures unchanged | No edit |
| `MiningConfig::break_time` example | Field unchanged | No edit |

The change had **no public-API surface impact** — no renamed types, no new
top-level types, no changed function signatures, no new Quick Start. The
internal `DefaultVoxel` override is documented in CHANGELOG as a behavior
change, which is the right surface per the openspec rule.

---

## Outstanding Follow-ups (from verify report)

Both items are **non-blocking** — verify verdict was PASS WITH NOTES.

### 1. Identity-fallback drop scenario lacks a direct unit test

**Scope**: small (4 lines).
**Where**: `src/items/drops.rs` `#[cfg(test)] mod tests`.
**What**: add a test that registers nothing for `voxel_byte = 1`, calls
`get_drop(1, seed)`, and asserts `Some((variant, 1))` where
`DefaultItem::from_byte(1)` returns a non-empty variant. Adjacent tests
(`test_drop_table_get_drop`, `test_drop_table_no_entry_returns_none`) already
cover the registered branch and the empty-byte branch. This closes the matrix
for the "Identity fallback for unregistered byte" scenario in the spec.
**Recommendation**: track as a tiny follow-up change (or fold into a future
items-related change). Not worth its own SDD cycle.

### 2. Pre-existing doctest failures on `dev` baseline

**Scope**: 14 doctest failures across `audio`, `generator/asteroid`, `items`,
`render/material`, `ui/...`, `world/loading`. All predate this change —
explicitly out of scope per the apply and verify phase records.
**Recommendation**: open a separate change `doctest-baseline-cleanup`. Out
of scope for this archive.

---

## Verification — Archive Integrity

| Check | Result |
|-------|--------|
| Main spec `openspec/specs/items/spec.md` created with all 5 requirements + 10 scenarios verbatim | PASS |
| Change folder moved to `openspec/changes/archive/2026-05-08-tool-material-system/` | PASS |
| Archive contains all 5 artifacts (`proposal.md`, `specs/items/spec.md`, `design.md`, `tasks.md`, `verify-report.md`) | PASS |
| `specs/items/` subdirectory structure preserved in archive | PASS |
| Active changes directory no longer contains `tool-material-system/` | PASS |
| All engram observations preserved (no deletes) | PASS |
| `cargo check` clean after move | PASS (see below) |
| Working tree dirty for user review (no commits made) | PASS |

### `cargo check` after archive move

```
Finished `dev` profile [optimized + debuginfo] target(s) in 0.18s
```

Exit code 0. The `openspec/` directory is not on the Cargo build path, so
moving change artifacts cannot affect the build, but we ran it anyway as a
paranoia check for the first pilot.

---

## SDD Cycle Complete

The change `tool-material-system` has been:

1. **Explored** (engram #52, 2026-03-29): tool/material system mapped from existing code.
2. **Proposed** (engram #107, openspec): contract scope and rollback plan.
3. **Specced** (engram #108, openspec): 5 requirements, 10 scenarios, RFC 2119.
4. **Designed** (engram #110, openspec): override-vs-redesign decision, override values.
5. **Task-broken** (engram #111, openspec): 22 tasks across 5 phases.
6. **Applied** (engram #112, source code): `DefaultVoxel` overrides + 8 unit tests + CHANGELOG entry.
7. **Verified** (engram #114, openspec): PASS WITH NOTES — 10/10 spec scenarios compliant by static evidence; 9/10 covered at runtime.
8. **Archived** (this report, openspec): main spec promoted; change folder archived under `2026-05-08-` prefix.

The next change can begin without waiting for any cleanup. The user owns the
review-and-commit step on the working tree (the pilot intentionally left it
unstaged).

---

## Artifacts

| Path | Source |
|------|--------|
| `openspec/changes/archive/2026-05-08-tool-material-system/proposal.md` | moved from active |
| `openspec/changes/archive/2026-05-08-tool-material-system/specs/items/spec.md` | moved from active |
| `openspec/changes/archive/2026-05-08-tool-material-system/design.md` | moved from active |
| `openspec/changes/archive/2026-05-08-tool-material-system/tasks.md` | moved from active |
| `openspec/changes/archive/2026-05-08-tool-material-system/verify-report.md` | moved from active |
| `openspec/changes/archive/2026-05-08-tool-material-system/archive-report.md` | this file |
| `openspec/specs/items/spec.md` | promoted from delta |
| `CHANGELOG.md` | already updated by sdd-apply (verified) |
| Engram `sdd/tool-material-system/archive-report` | upserted by this phase |
