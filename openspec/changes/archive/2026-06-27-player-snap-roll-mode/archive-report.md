# Archive Report — `player-snap-roll-mode`

**Change**: player-snap-roll-mode
**Phase**: sdd-archive
**Backend mode**: hybrid (engram + openspec)
**Archive date**: 2026-06-27
**Verify verdict**: PASS

---

## Summary

Archived the `player-snap-roll-mode` change after successful implementation and
verification. This change introduced the `player` capability as a NEW openspec
domain — the first spec under `openspec/specs/player/`. The implementation adds
an opt-in `RollMode::Snap` to the existing 6DOF player controller with easing
animation, configurable duration, and lattice-aware cancel-and-restart semantics.
Default behavior (`RollMode::Continuous`) is preserved byte-for-byte.

---

## Lineage — Engram Observations

All artifacts persisted to engram with stable topic keys. IDs preserved for
audit-trail traceability.

| Topic Key | Observation ID | Type | Saved |
|-----------|---------------:|------|-------|
| `sdd/player-snap-roll-mode/proposal` | 121 | architecture | 2026-05-08 21:20 |
| `sdd/player-snap-roll-mode/spec` | 122 | architecture | 2026-05-08 21:23 |
| `sdd/player-snap-roll-mode/design` | 123 | architecture | 2026-05-08 21:24 |
| `sdd/player-snap-roll-mode/tasks` | 124 | architecture | 2026-05-08 21:27 |
| `sdd/player-snap-roll-mode/verify-report` | 138 | architecture | 2026-06-27 19:42 |
| `sdd/player-snap-roll-mode/archive-report` | (this report) | architecture | 2026-06-27 |

---

## Files Archived (before -> after)

The entire change folder was moved to the archive with an ISO-date prefix.
All five artifacts and the `specs/player/` subdirectory were preserved verbatim.

| Before | After |
|--------|-------|
| `openspec/changes/player-snap-roll-mode/proposal.md` | `openspec/changes/archive/2026-06-27-player-snap-roll-mode/proposal.md` |
| `openspec/changes/player-snap-roll-mode/specs/player/spec.md` | `openspec/changes/archive/2026-06-27-player-snap-roll-mode/specs/player/spec.md` |
| `openspec/changes/player-snap-roll-mode/design.md` | `openspec/changes/archive/2026-06-27-player-snap-roll-mode/design.md` |
| `openspec/changes/player-snap-roll-mode/tasks.md` | `openspec/changes/archive/2026-06-27-player-snap-roll-mode/tasks.md` |
| `openspec/changes/player-snap-roll-mode/verify-report.md` | `openspec/changes/archive/2026-06-27-player-snap-roll-mode/verify-report.md` |

---

## Files Synced

### `openspec/specs/player/spec.md` (CREATED)

**Action**: New capability spec — no prior `openspec/specs/player/spec.md`
existed. The delta spec from the change was promoted to the canonical spec.

**Content preserved verbatim**:
- 5 `ADDED Requirements` (Continuous Roll Is Default and Preserved, Snap Mode
  Is Opt-In via PlayerConfig, Cancel-and-Restart Is Lattice-Aware, Easing and
  Duration Are Configurable, Roll Is Disabled in Grounded Mode).
- 10 scenarios (every Given/When/Then bullet retained word-for-word).
- All RFC 2119 keywords (MUST, SHALL, SHOULD, MAY) preserved.

**Wording adjustments during promotion** (non-substantive):
- Replaced the "Delta for Player Capability" preamble with a canonical
  capability header that names this file as the source of truth and references
  the originating change. The 5 requirements and 10 scenarios were NOT
  renumbered, NOT reordered, and NOT reworded — only the framing prose changed.
- Removed the `## ADDED Requirements` heading in favor of `## Requirements`
  per the canonical openspec convention (matching `openspec/specs/items/spec.md`).

---

## Doc Deltas Applied

| File | Action | Details |
|------|--------|---------|
| `CHANGELOG.md` | Added | `[Unreleased] -> Added` entry for `RollMode::Snap` |
| `README.md` | Updated | Player controls table: Q/E row now mentions snap mode opt-in |
| `docs/multiplayer-readiness.md` | Added | New section: `Player::snap_anim` is local-only, replicate `Transform` only |
| `examples/basic_world.rs` | Updated | Controls comment mentions snap mode via `PlayerConfig::roll_mode` |

---

## Verification — Archive Integrity

| Check | Result |
|-------|--------|
| Main spec `openspec/specs/player/spec.md` created with all 5 requirements + 10 scenarios verbatim | PASS |
| Change folder moved to `openspec/changes/archive/2026-06-27-player-snap-roll-mode/` | PASS |
| Archive contains all 5 artifacts (`proposal.md`, `specs/player/spec.md`, `design.md`, `tasks.md`, `verify-report.md`) | PASS |
| `specs/player/` subdirectory structure preserved in archive | PASS |
| Active changes directory no longer contains `player-snap-roll-mode/` | PASS |
| All engram observations preserved (no deletes) | PASS |
| Doc deltas applied (CHANGELOG, README, multiplayer-readiness, example) | PASS |

---

## Design Deviations Noted During Verify (non-blocking)

From the verify report — recorded here for completeness:

1. `SnapRollAnim` omits `direction` field (inferred from sign of
   `target_angle - start_angle` instead). Valid simplification — no behavioral
   divergence from spec.
2. `start_or_retarget_snap` omits `current_local_z` parameter (delta-based
   approach makes it unnecessary). Valid simplification — lattice invariant
   preserved.

---

## Outstanding Follow-ups

### 1. PARTIAL scenario coverage

5 of 10 spec scenarios have structural evidence only (no behavioral test):
held key stacking, spawn config copy, grounded-mode suppression. These would
benefit from integration tests with a minimal Bevy `App`.
**Recommendation**: fold into a future `player-test-coverage` change. Not
blocking.

---

## SDD Cycle Complete

The change `player-snap-roll-mode` has been:

1. **Proposed** (engram #121, openspec): intent, scope, approach, rollback plan.
2. **Specced** (engram #122, openspec): 5 requirements, 10 scenarios, RFC 2119.
3. **Designed** (engram #123, openspec): 4 architecture decisions, data flow, interfaces.
4. **Task-broken** (engram #124, openspec): 28 tasks across 9 phases.
5. **Applied** (source code): `RollMode`, `SnapRollEasing`, `SnapRollAnim`, 3 helpers, `Player`/`PlayerConfig` fields, 5 unit tests.
6. **Verified** (engram #138, openspec): PASS — 161 lib tests, 107 doc tests, all 28 tasks complete.
7. **Archived** (this report, openspec + engram): canonical spec promoted to `openspec/specs/player/spec.md`; change folder archived under `2026-06-27-` prefix; doc deltas applied.

The next change can begin without waiting for any cleanup. The user owns the
review-and-commit step on the working tree.

---

## Artifacts

| Path | Source |
|------|--------|
| `openspec/changes/archive/2026-06-27-player-snap-roll-mode/proposal.md` | moved from active |
| `openspec/changes/archive/2026-06-27-player-snap-roll-mode/specs/player/spec.md` | moved from active |
| `openspec/changes/archive/2026-06-27-player-snap-roll-mode/design.md` | moved from active |
| `openspec/changes/archive/2026-06-27-player-snap-roll-mode/tasks.md` | moved from active |
| `openspec/changes/archive/2026-06-27-player-snap-roll-mode/verify-report.md` | moved from active |
| `openspec/changes/archive/2026-06-27-player-snap-roll-mode/archive-report.md` | this file |
| `openspec/specs/player/spec.md` | promoted from delta |
| `CHANGELOG.md` | added `[Unreleased] -> Added` entry |
| `README.md` | updated Q/E controls row |
| `docs/multiplayer-readiness.md` | added snap-roll replication note |
| `examples/basic_world.rs` | updated controls comment |
| Engram `sdd/player-snap-roll-mode/archive-report` | upserted by this phase |
