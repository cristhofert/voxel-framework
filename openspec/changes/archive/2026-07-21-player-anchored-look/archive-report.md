# Archive Report — `player-anchored-look`

**Change**: player-anchored-look
**Phase**: sdd-archive
**Backend mode**: hybrid (engram + openspec)
**Archive date**: 2026-07-21
**Verify verdict**: PASS (0 critical, 0 warning, 2 suggestions)

---

## Summary

Archived the `player-anchored-look` change after implementation, a user-approved
manual playtest (2026-07-21), and verification (184/184 lib tests = 169 baseline
+ 15 new, 107 doc tests, locked regions regression-free via
whitespace-insensitive diff). The change adds an opt-in `LookMode::Anchored`
FPS camera to FreeFlight: world-space yaw around a switchable world-axis anchor
(`Player::anchor_up`), pitch clamped to ±89°, zero horizon roll by
construction — with Q/E switching the anchor among the 6 signed world axes
(Super Mario Galaxy-style) via the existing snap-roll animation. `RollMode` is
inert while Anchored. Purely additive: `LookMode::SixDof` default keeps
existing consumers (Starframe) byte-compatible.

---

## Lineage — Engram Observations

| Topic Key | Observation ID | Type | Saved |
|-----------|---------------:|------|-------|
| (exploration/decision) "Decided anchored FPS look mode" | 153 | decision | 2026-07-13 |
| `sdd/player-anchored-look/proposal` | 154 | decision | 2026-07-13 |
| `sdd/player-anchored-look/spec` | 155 | architecture | 2026-07-13 |
| `sdd/player-anchored-look/design` | 156 | architecture | 2026-07-13 |
| `sdd/player-anchored-look/tasks` | 161 | architecture | 2026-07-21 |
| `sdd/player-anchored-look/apply-progress` | 164 | architecture | 2026-07-21 |
| `sdd/player-anchored-look/verify-report` | 166 | architecture | 2026-07-21 |
| `sdd/player-anchored-look/state` | 162 | discovery | 2026-07-21 (upserted ARCHIVED at archive) |
| `sdd/player-anchored-look/archive-report` | (this report, upserted) | architecture | 2026-07-21 |

Note: per hybrid-mode convention the filesystem delta spec
(`specs/player/spec.md` in this folder) was the merge source; engram copies are
the cross-session backup.

---

## Files Archived (before -> after)

| Before | After |
|--------|-------|
| `openspec/changes/player-anchored-look/proposal.md` | `openspec/changes/archive/2026-07-21-player-anchored-look/proposal.md` |
| `openspec/changes/player-anchored-look/specs/player/spec.md` | `openspec/changes/archive/2026-07-21-player-anchored-look/specs/player/spec.md` |
| `openspec/changes/player-anchored-look/design.md` | `openspec/changes/archive/2026-07-21-player-anchored-look/design.md` |
| `openspec/changes/player-anchored-look/tasks.md` | `openspec/changes/archive/2026-07-21-player-anchored-look/tasks.md` |
| `openspec/changes/player-anchored-look/verify-report.md` | `openspec/changes/archive/2026-07-21-player-anchored-look/verify-report.md` |

(No `exploration.md` — this change started from decision #153 instead of a
filesystem exploration artifact.)

Housekeeping applied before the move: removed the stale
"PENDIENTE — requiere display" fragment from `tasks.md` 5.3 (redundant next to
the APPROVED 2026-07-21 marker; verify-report Suggestion 1).

---

## Files Synced

### `openspec/specs/player/spec.md` (UPDATED, v2 -> v3)

- **ADDED**: `Anchored Look Mode Is Opt-In` (2 scenarios: default preserves
  6DOF, spawn copies non-default mode).
- **ADDED**: `Anchored FPS Look in FreeFlight` (3 scenarios: +89° pole clamp,
  −89° pole clamp with inward passthrough, level horizon under diagonal input).
- **ADDED**: `Manual Anchor Switching via Q/E` (4 scenarios: axial 90° switch,
  mid-animation extend retarget, old-anchor mouse look during animation,
  anchor never denormalizes).
- **ADDED**: `RollMode Is Inert in Anchored Look` (2 scenarios: continuous
  roll suppressed, Grounded ignores LookMode).
- **UNTOUCHED (append-only merge, 6 requirements preserved verbatim)**:
  `Continuous Roll Is Default and Preserved`, `World-Aligned Lattice
  Definition`, `Snap Mode Is Opt-In via PlayerConfig`, `Cancel-and-Restart Is
  Lattice-Aware`, `Easing and Duration Are Configurable`, `Roll Is Disabled in
  Grounded Mode`.
- Header: title now "Player Capability — Look & Roll Modes (v3)"; intro prose
  names the anchored look mode; "Last updated by archive of
  `player-anchored-look` on 2026-07-21".
- Total: 10 requirements, in-spec "spec v2" references in the new scenarios
  denote the preserved v2 baseline behavior (accepted delta wording kept
  verbatim).

---

## Doc Deltas Applied

| File | Action | Details |
|------|--------|---------|
| `README.md` | Updated | Player controls table: Mouse row documents `PlayerConfig::look_mode = LookMode::Anchored` (world-axis anchored yaw, ±89° pitch clamp, no horizon roll); Q/E row documents anchor switching among the 6 signed world axes with the snap animation and `RollMode` inertness |
| `CHANGELOG.md` | Added | `[Unreleased] -> Added`: `LookMode` entry (enum + `PlayerConfig`/`Player` fields + prelude re-export, anchored FPS behavior, Q/E anchor switching reusing snap easing/duration, RollMode inert, additive default, spec v3 reference, basic_world showcase) |

Not touched: `docs/multiplayer-readiness.md` (`SnapRollAnim` struct unchanged;
new `anchor_up`/`pending_anchor` are plain `Player` fields covered by the
existing component-replication note), `src/`, `examples/` (implementation
already on the working tree from apply; `examples/basic_world.rs` carries the
`LookMode::Anchored` showcase, recorded in the changelog entry).

---

## Verification — Archive Integrity

| Check | Result |
|-------|--------|
| Main spec updated: 4 added requirements (11 scenarios), header v3 | PASS |
| Untouched requirements preserved (append-only merge; `git diff` touches header + tail additions only) | PASS |
| Change folder moved to `openspec/changes/archive/2026-07-21-player-anchored-look/` | PASS |
| Archive contains all 5 artifacts + `specs/player/` subtree | PASS |
| Active changes directory no longer contains `player-anchored-look/` | PASS |
| Doc deltas applied (README, CHANGELOG) per `rules.archive` in `openspec/config.yaml` | PASS |
| Non-destructive merge (no removals, no signature breaks) — no warning needed | PASS |
| Engram lineage IDs recorded; archive-report + state upserted | PASS |

---

## Outstanding Follow-ups

1. **3 frame-loop scenarios remain PARTIAL (manual)** (level horizon under
   diagonal input, old-anchor look mid-animation, continuous-roll suppression):
   structural evidence + approved playtest only. Candidate for a future minimal
   Bevy `App` harness driving `player_look` (verify-report Suggestion 2). Not
   blocking.
2. **Commit** is owned by the orchestrator/user — nothing was committed by this
   phase. The `src/player/mod.rs`, `src/prelude.rs`, `examples/basic_world.rs`
   working-tree changes belong to the same commit as this archive.

---

## SDD Cycle Complete

1. **Explored/Decided** (engram #153): 6DOF disorientation → switchable
   world-axis anchored FPS camera; angular-rate cap rejected.
2. **Proposed** (engram #154, `proposal.md`): additive `LookMode`, reuse of
   `player-world-aligned-snap` machinery, Starframe byte-compatibility,
   rollback plan.
3. **Specced** (engram #155 + delta file): 4 ADDED requirements, 11 scenarios,
   RFC 2119; 6 base requirements explicitly untouched.
4. **Designed** (engram #156): 4 pure helpers (`world_lattice_candidates`,
   `next_lattice_candidate`, `snap_reference_up`, `clamp_pitch_delta` +
   `nearest_world_axis`); `match player.look_mode` wrapping the FreeFlight arm;
   SixDof verbatim.
5. **Applied** (engram #164): 3 files; 15 new tests (184 total); Q/E anchor
   switch wired through `start_or_retarget_snap` unchanged.
6. **Verified** (engram #166, `verify-report.md`): PASS — 25/25 tasks, gates
   re-run green, 8/11 scenarios test-backed + 3 playtest-backed, locked regions
   regression-free.
7. **Archived** (this report): canonical spec at v3; folder under `2026-07-21-`
   prefix; README/CHANGELOG synced.
