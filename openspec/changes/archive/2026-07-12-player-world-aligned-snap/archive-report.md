# Archive Report — `player-world-aligned-snap`

**Change**: player-world-aligned-snap
**Phase**: sdd-archive
**Backend mode**: hybrid (engram + openspec)
**Archive date**: 2026-07-12
**Verify verdict**: PASS WITH WARNINGS (1 minor, non-blocking)

---

## Summary

Archived the `player-world-aligned-snap` change after successful implementation,
two rounds of user playtesting, and verification (169/169 lib tests, 107/107 doc
tests, locked regions byte-for-byte unchanged). The change redefines
`RollMode::Snap` from fixed ±90° relative rolls to world-aligned lattice
targeting (Starframe-style click-to-orient): Q/E animates to the next
world-aligned up candidate, correcting the horizon drift that 6DOF local
yaw∘pitch mouse look accumulates. Near-axis views yield exactly 4 stops
(45° dominance exclusion, `SNAP_MIN_PROJ_SQ = 0.49`); diagonal views up to 6.

---

## Lineage — Engram Observations

| Topic Key | Observation ID | Type | Saved |
|-----------|---------------:|------|-------|
| `sdd/player-world-aligned-snap/explore` | 142 | architecture | 2026-07-05 |
| `sdd/player-world-aligned-snap/proposal` | 143 | architecture | 2026-07-05 |
| `sdd/player-world-aligned-snap/spec` | 144 | architecture | 2026-07-05 |
| `sdd/player-world-aligned-snap/design` | 145 | architecture | 2026-07-05 |
| `sdd/player-world-aligned-snap/tasks` | 146 | architecture | 2026-07-05 |
| `sdd/player-world-aligned-snap/verify-report` | 149 | discovery | 2026-07-13 |
| `sdd/player-world-aligned-snap/archive-report` | (this report, upserted) | discovery | 2026-07-12 |

Note: the filesystem delta spec (updated 2026-07-12, includes the Phase 6
playtest regression scenario) is newer than engram #144 (2026-07-05); the
filesystem copy was the merge source, per hybrid-mode convention.

---

## Files Archived (before -> after)

| Before | After |
|--------|-------|
| `openspec/changes/player-world-aligned-snap/exploration.md` | `openspec/changes/archive/2026-07-12-player-world-aligned-snap/exploration.md` |
| `openspec/changes/player-world-aligned-snap/proposal.md` | `openspec/changes/archive/2026-07-12-player-world-aligned-snap/proposal.md` |
| `openspec/changes/player-world-aligned-snap/specs/player/spec.md` | `openspec/changes/archive/2026-07-12-player-world-aligned-snap/specs/player/spec.md` |
| `openspec/changes/player-world-aligned-snap/design.md` | `openspec/changes/archive/2026-07-12-player-world-aligned-snap/design.md` |
| `openspec/changes/player-world-aligned-snap/tasks.md` | `openspec/changes/archive/2026-07-12-player-world-aligned-snap/tasks.md` |
| `openspec/changes/player-world-aligned-snap/verify-report.md` | `openspec/changes/archive/2026-07-12-player-world-aligned-snap/verify-report.md` |

---

## Files Synced

### `openspec/specs/player/spec.md` (UPDATED, v1 -> v2)

- **ADDED**: `World-Aligned Lattice Definition` (4 scenarios: axis-forward
  4-lattice, near-axis 4-lattice playtest regression, diagonal 6-candidate
  cycle, EPS_ON skip). Placed before `Snap Mode Is Opt-In via PlayerConfig`,
  which references it — mirrors the delta's own ordering.
- **MODIFIED**: `Snap Mode Is Opt-In via PlayerConfig` — replaced fixed
  `±PI/2` text with world-aligned candidate targeting; 3 scenarios (aligned
  +90° back-compat, drifted residual travel, held-key guard).
- **MODIFIED**: `Cancel-and-Restart Is Lattice-Aware` — replaced anchor-based
  `PI/2`-multiple lattice with the single "next candidate in direction `d`
  from a reference up" primitive; 2 scenarios (extend, reverse).
- **UNTOUCHED (byte-identical, diff-audited)**: `Continuous Roll Is Default
  and Preserved`, `Easing and Duration Are Configurable`, `Roll Is Disabled
  in Grounded Mode`.
- Header: version bumped to v2; "Last updated by archive of
  `player-world-aligned-snap` on 2026-07-12"; intro prose now names the
  world-aligned lattice.

---

## Doc Deltas Applied

| File | Action | Details |
|------|--------|---------|
| `README.md` | Updated | Q/E controls row: snap now targets the next world-aligned up direction; 4 stops near-axis, up to 6 diagonal; corrects 6DOF horizon drift |
| `CHANGELOG.md` | Added | `[Unreleased] -> Changed`: world-aligned `RollMode::Snap` semantics (spec v2 reference, basic_world showcase) + `snap_roll_duration_secs` default `0.18 -> 0.5` bump (previously unrecorded) |

Not touched: `docs/multiplayer-readiness.md` (replication note from
`player-snap-roll-mode` still accurate — `SnapRollAnim` struct unchanged),
`src/`, `examples/` (per phase constraints; `examples/basic_world.rs` already
carries the `RollMode::Snap` showcase in the working tree, recorded in the
changelog entry).

---

## Verification — Archive Integrity

| Check | Result |
|-------|--------|
| Main spec updated: 1 added + 2 modified requirements, header v2 | PASS |
| Untouched requirements byte-identical (`git diff` audit: no +/- line touches them) | PASS |
| Change folder moved to `openspec/changes/archive/2026-07-12-player-world-aligned-snap/` | PASS |
| Archive contains all 6 artifacts + `specs/player/` subtree | PASS |
| Active changes directory no longer contains `player-world-aligned-snap/` | PASS |
| Doc deltas applied (README, CHANGELOG) per `rules.archive` in `openspec/config.yaml` | PASS |
| Engram lineage IDs recorded; archive-report + state upserted | PASS |

---

## Outstanding Follow-ups

1. **Held-key scenario remains static-evidence-only** (carried over from
   `player-snap-roll-mode`): no runtime test simulating held input. Candidate
   for a future `player-test-coverage` change. Not blocking.
2. **Commit** is owned by the orchestrator/user — nothing was committed by
   this phase. `examples/basic_world.rs` and `src/player/mod.rs` working-tree
   changes belong to the same commit as this archive.

---

## SDD Cycle Complete

1. **Explored** (engram #142, `exploration.md`): drift root cause — local yaw∘pitch accumulates world-relative roll.
2. **Proposed** (engram #143): world-aligned candidate targeting, consumer impact (Starframe), rollback plan.
3. **Specced** (engram #144 + delta file): 1 ADDED + 2 MODIFIED requirements, 9 scenarios, RFC 2119.
4. **Designed** (engram #145): 2 pure glam helpers (`world_lattice_angles`, `next_lattice_step`) + orchestration in `start_or_retarget_snap`; thresholds as internal consts.
5. **Applied**: single file `src/player/mod.rs`; 6 new tests; Phase 6 playtest fix (`SNAP_MIN_PROJ_SQ = 0.49` dominance exclusion).
6. **Verified** (engram #149, `verify-report.md`): PASS WITH WARNINGS — 19/19 tasks, 169 lib tests, locked regions unchanged.
7. **Archived** (this report): canonical spec at v2; folder under `2026-07-12-` prefix; README/CHANGELOG synced.
