# Proposal: Player Anchored Look Mode (Switchable World-Axis FPS Camera)

## Intent

Playtest feedback: 6DOF look in `PlayerMode::FreeFlight` "spins out of control when turning" — unbounded rotation causes disorientation. Users want a Minecraft-creative-style FPS camera: horizon always level, pitch clamped — but with the player's "up" switchable among the 6 signed world axes (±X/±Y/±Z, Super Mario Galaxy-style), manually via Q/E with the existing snap-roll animation. An angular-rate cap was explicitly rejected ("space sim" feel — "this isn't the ship, it's the player").

## Scope

### In Scope
- `LookMode { #[default] SixDof, Anchored }` on `PlayerConfig` + `Player`, copied in `spawn_player`, re-exported in prelude.
- `Player::anchor_up: Vec3` (signed world axis, init via `nearest_world_axis` at spawn) + `pending_anchor: Option<Vec3>`.
- Anchored FreeFlight look: world-space yaw around `anchor_up`; `rotate_local_x` pitch clamped so `asin(forward · anchor_up)` stays within ±89° (`clamp_pitch_delta` helper); no free roll — zero horizon drift by construction.
- Q/E `just_pressed`: next lattice candidate CCW/CW via `world_lattice_candidates` / `next_lattice_candidate` (extensions of `world_lattice_angles` / `next_lattice_step`), animated with `start_or_retarget_snap`; candidate's source axis → `pending_anchor`, committed to `anchor_up` on finish.
- `RollMode` ignored in Anchored (rustdoc on both enums).
- Delta spec: new requirement "Anchored Look Mode" + RollMode interaction note.
- Pure-helper unit tests; 169 existing lib tests untouched.

### Out of Scope
- Automatic anchor switching at pole crossing (rejected — manual Q/E confirmed).
- Grounded arm changes (already Y-anchored FPS, `src/player/mod.rs:859-877`).
- Angular rate limiting; gravity/movement changes; non-axis (diagonal) anchors.

## Approach

Generalize the Grounded arm's Y-anchored look to an arbitrary axis anchor in FreeFlight, reusing the archived `player-world-aligned-snap` machinery (lattice candidates ≥~44° from forward → post-switch pitch ≤~46°, never violates clamp; `SNAP_MIN_PROJ_SQ = 0.49`). `player_look` FreeFlight arm gains `match player.look_mode`: `SixDof` wraps current code unchanged (re-indent only); `Anchored` is new. `world_lattice_angles` becomes a thin wrapper over `world_lattice_candidates`.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `src/player/mod.rs` | Modified | Enum, fields, 4 pure helpers, Anchored arm, tests |
| `src/prelude.rs` | Modified | Re-export `LookMode` |
| `examples/basic_world.rs` | Modified | `look_mode: Anchored` for playtest (showcase vs revert TBD) |
| `openspec/specs/player/spec.md` | Delta | New requirement (merged at archive) |

**Consumer impact**: purely additive — `LookMode` defaults to `SixDof`, existing downstreams (Starframe) are byte-compatible with no code changes.

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| SixDof regression when re-indenting `player_look` | Low | Logic moved verbatim; existing pure tests + playtest regression check |
| Anchor/pitch edge cases (near-pole yaw, mid-anim look) | Med | Pure helpers unit-tested; mouse look stays on old anchor during 0.5s anim |
| Q/E semantic clash with `RollMode` | Low | Anchored ignores RollMode; documented in both rustdocs + spec note |

## Rollback Plan

Additive, no persistence/trait changes: revert the commits (or delete the `Anchored` arm + fields). Default `SixDof` path is untouched, so partial rollback leaves current behavior intact.

## Dependencies

- Archived change `openspec/changes/archive/2026-07-12-player-world-aligned-snap/` (machinery already on `dev`).
- Canonical spec `openspec/specs/player/spec.md` (v2).

## Success Criteria

- [ ] `cargo check`, `cargo test --lib`, `cargo test --doc`, `cargo check --example basic_world` green; 169 baseline tests unedited.
- [ ] Playtest: violent diagonal mouse → horizon always level; pitch stops at ±89°; Q/E animates 90° anchor switch; after repeated Q/E anchor is always a pure world axis.
- [ ] Regression: without `look_mode` set, 6DOF behavior and prior snap-roll are exactly preserved.
