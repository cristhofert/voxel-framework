# Design: Player Anchored Look Mode

Change: `player-anchored-look` · Base: proposal (+ decisions `sdd/player-anchored-look/decisions`) · Date: 2026-07-13

## Technical Approach

Generalize the Grounded arm's Y-anchored FPS look to an arbitrary signed world axis in FreeFlight. All new logic is pure glam helpers (project pattern); `player_look`'s FreeFlight arm gains `match player.look_mode` — `SixDof` is the current code re-indented verbatim, `Anchored` is new. Q/E anchor switching reuses the archived snap machinery unchanged; only target-axis *bookkeeping* is added around it. No schedule/system-order changes; no generic-type implications; no new dependencies.

## Architecture Decisions

| Decision | Choice | Alternatives rejected | Rationale |
|---|---|---|---|
| Mode selector | `#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)] pub enum LookMode { #[default] SixDof, Anchored }` on `PlayerConfig` + `Player` | bool flag; `Anchored { axis }` variant data | Mirrors `RollMode` pattern exactly (derive set, `#[default]`, config→Player copy, prelude re-export); axis is runtime state, not config |
| Anchor state | `Player::anchor_up: Vec3` (pub) + `Player::pending_anchor: Option<Vec3>` (pub(crate)) | Separate component; field on `SnapRollAnim` | `SnapRollAnim` is locked (precedent design); component split buys nothing for single-entity player; `Vec3` feeds `Quat::from_axis_angle` directly |
| Candidate base | `world_lattice_candidates(local_z, up) -> Vec<(f32, Vec3)>`; `world_lattice_angles` becomes a thin `.map(\|(a,_)\| a)` wrapper | Duplicate loop; change `world_lattice_angles` return type | Single source of truth for lattice rule; wrapper keeps all 169 existing tests byte-untouched |
| Next-candidate | `next_lattice_candidate(&[(f32, Vec3)], dir) -> (f32, Vec3)` mirroring `next_lattice_step` (same wrap/EPS_ON formula, min/max by wrapped angle, axis carried) | Make `next_lattice_step` generic/wrapper | Input types differ (`&[f32]` vs `&[(f32,Vec3)]`); mirroring keeps the locked function untouched; an agreement test pins them together |
| pending_anchor resolution | New pure helper `snap_reference_up(existing, local_z, up, dir) -> Vec3` replicating `start_or_retarget_snap`'s reference-up rule; arm computes `next_lattice_candidate(world_lattice_candidates(local_z, ref_up), dir).1` | Change `start_or_retarget_snap` to return the axis; inline duplication in the system | Same-dir retarget references the *in-flight target up*, not current up — computing the axis from raw `up` would desync `pending_anchor` from where the anim lands. Signature change would edit locked tests; a pure helper is unit-testable where inline arm code is not |
| Pitch clamp | `clamp_pitch_delta(forward, anchor_up, pitch_d)`: `pitch = asin(forward·anchor_up)`, return `clamp(pitch + pitch_d, ±89°) − pitch`; apply via `rotate_local_x` | Euler decompose like Grounded arm | `to_euler(YXZ)` only works for the Y anchor; asin form is anchor-agnostic. Sign checked: positive `rotate_local_x` raises forward toward local up, matching `asin` growth |
| Yaw | `transform.rotate(Quat::from_axis_angle(player.anchor_up, yaw_d))` | `rotate_y` (Y-only); `rotate_local_y` (drifts) | World-space rotation about the anchor is the direct generalization of the Grounded arm's `rotate_y` |
| Spawn anchor | `nearest_world_axis(v)`: iterate `AXES` order `[X, NEG_X, Y, NEG_Y, Z, NEG_Z]`, keep max dot with strict `>` — first-in-order wins ties | `Vec3::signum`-style per-component | Stable, documented tie-break; reuses the existing `AXES` order so there is one canonical axis ordering in the file |
| Anchor commit | On `finished`: `anchor_up = pending_anchor.take()`, then `let f = *transform.forward(); transform.look_to(f, player.anchor_up)` | Raw commit only | Mid-anim yaw rotates the frame about the old anchor while world axis B stays fixed → residual roll vs B at commit. `look_to` preserves forward, snaps up to B's projection: enforces the FPS invariant exactly and absorbs FP residue. Correction ≈ 0 in normal play (bounded by yaw during the 0.5 s anim). Safe: candidate axes are ≥ ~44° from forward, never degenerate |

## Data Flow — Anchored arm (FreeFlight only)

    mouse delta ─────────────────────────────► yaw: rotate(Quat::from_axis_angle(anchor_up, yaw_d))
        │                                      pitch: rotate_local_x(clamp_pitch_delta(*forward(), anchor_up, pitch_d))
        │                                      (anchor_up = OLD anchor while anim runs)
    Q/E just_pressed (dir = +1 / −1; RollMode ignored)
        │  ref_up = snap_reference_up(snap_anim, *local_z(), *up(), dir)
        │  (_, axis) = next_lattice_candidate(&world_lattice_candidates(local_z, ref_up), dir)
        │  pending_anchor = Some(axis)                    // pure signed world axis
        ▼
    snap_anim = start_or_retarget_snap(snap_anim, local_z, up, dir, dur, easing)   // UNCHANGED
        │
    per frame: advance_snap ──► rotate_local_z(delta)
        └─ finished ──► anchor_up = pending_anchor.take(); look_to(forward, anchor_up); snap_anim = None

Because `pending_anchor` is always a pure signed axis, `anchor_up` stays exactly on the lattice forever — zero drift accumulation across any number of switches.

## Interfaces / Contracts

```rust
/// How mouse look is interpreted in [`PlayerMode::FreeFlight`].
/// `Anchored` = FPS look (level horizon, pitch ±89°) around a switchable
/// world-axis anchor; Q/E switch the anchor (RollMode is IGNORED).
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
pub enum LookMode { #[default] SixDof, Anchored }

/// Like [`world_lattice_angles`] but returns (angle δ, SOURCE signed world
/// axis) per surviving candidate. Same SNAP_MIN_PROJ_SQ filter; ≥4 survive.
pub(crate) fn world_lattice_candidates(local_z: Vec3, up: Vec3) -> Vec<(f32, Vec3)>;

/// Thin wrapper: world_lattice_candidates(..).into_iter().map(|(a, _)| a).collect()
pub(crate) fn world_lattice_angles(local_z: Vec3, up: Vec3) -> Vec<f32>;

/// Mirror of [`next_lattice_step`] carrying the axis: same EPS_ON wrap,
/// min (dir>0) / max (dir<0) by wrapped angle. Returns the wrapped angle.
/// Caller guarantees non-empty (SNAP_MIN_PROJ_SQ invariant); panics otherwise.
pub(crate) fn next_lattice_candidate(cands: &[(f32, Vec3)], dir: i32) -> (f32, Vec3);

/// The reference up [`start_or_retarget_snap`] resolves candidates from:
/// same-dir mid-anim → up advanced by the unapplied residual
/// (Quat::from_axis_angle(local_z, target−applied) * up); else → up.
pub(crate) fn snap_reference_up(
    existing: Option<SnapRollAnim>, local_z: Vec3, up: Vec3, dir: i32) -> Vec3;

/// Clamps a pitch delta so asin(forward·anchor_up) stays in ±89°.
/// pitch = asin(clamp(forward·anchor_up, −1, 1)); returns
/// clamp(pitch + pitch_d, ±89°.to_radians()) − pitch. Pure, NaN-free.
pub(crate) fn clamp_pitch_delta(forward: Vec3, anchor_up: Vec3, pitch_d: f32) -> f32;

/// Signed world axis with max dot(v, axis); ties broken by AXES order
/// [X, NEG_X, Y, NEG_Y, Z, NEG_Z] (strict >, first wins). For spawn init.
pub(crate) fn nearest_world_axis(v: Vec3) -> Vec3;
```

Changed items: `Player { look_mode: LookMode, anchor_up: Vec3 /* default Y */, pending_anchor: Option<Vec3> /* None */ }` + same-name `look_mode` on `PlayerConfig`; `spawn_player` copies `look_mode` and sets `anchor_up = nearest_world_axis(*transform.up())` (build the `Transform` in a local first); rustdoc notes on `RollMode` ("ignored when `LookMode::Anchored`") and `LookMode`; `src/prelude.rs` line 127 gains `LookMode`.

## Edge Cases

| Case | Resolution |
|---|---|
| Pitch exactly at ±89° | `clamp_pitch_delta` returns 0 for outward deltas, passthrough inward (clamp-then-subtract form gives this for free) |
| Anchor switch while pitched | Candidates are ≥ ~44° from forward (SNAP_MIN_PROJ_SQ) → post-switch pitch ≤ ~46°, never violates the clamp; no special handling |
| Q/E extend same dir mid-anim | `snap_reference_up` advances up by the residual → `pending_anchor` becomes the axis one candidate PAST the in-flight target, exactly matching the retargeted anim |
| Q/E reverse mid-anim | Reference = current up (matches `start_or_retarget_snap` opposite-dir branch) → `pending_anchor` becomes the nearest axis behind; overwrites the previous pending value |
| Mouse look during anim | Yaw/pitch stay on OLD `anchor_up` until commit; residual roll vs the new axis is absorbed by the `look_to` renormalization at commit |
| dt = 0 / zero mouse delta | `delta == Vec2::ZERO` guard skips the mouse block; `advance_snap(anim, 0.0)` yields delta 0, not finished — harmless. (`duration == 0` → NaN is a pre-existing trait of the locked machinery, out of scope) |
| `nearest_world_axis` ties | Strict `>` over AXES order: e.g. `(1,1,0)` → `X`; documented, deterministic |
| Empty candidate slice | Unreachable by the ≥4 invariant; `next_lattice_candidate` panics with message rather than returning garbage |
| Looking near-parallel to `anchor_up` (pitch ≈ 89°) | The anchor axis itself falls below SNAP_MIN_PROJ_SQ and drops out of candidates — Q/E still yields a valid ~perpendicular axis |
| SixDof/Anchored crosstalk | `look_mode` is fixed at spawn; `snap_anim` is only ever driven by one arm |

## File Changes

| File | Action | Description |
|---|---|---|
| `src/player/mod.rs` | Modify | `LookMode`; 3 `Player` + 1 `PlayerConfig` fields + defaults; spawn copy + anchor init; `world_lattice_candidates` (body moved from `world_lattice_angles`) + wrapper; `next_lattice_candidate`; `snap_reference_up`; `clamp_pitch_delta`; `nearest_world_axis`; Anchored arm in `player_look`; SixDof arm re-indented verbatim; rustdoc sync; tests |
| `src/prelude.rs` | Modify | Add `LookMode` to the player re-export (line 127) |
| `examples/basic_world.rs` | Modify | `look_mode: LookMode::Anchored` for playtest (showcase vs revert decided post-playtest) |

## Locked / Preserved Regions

- **Grounded arm** (`player_look`, ~859–877): zero edits.
- **Snap machinery bodies**: `advance_snap`, `apply_easing`, `start_or_retarget_snap`, `SnapRollAnim` — byte-untouched.
- **Continuous and Snap roll arms** inside `SixDof`: semantically unchanged; the whole SixDof arm may be re-indented inside `match player.look_mode` but logic verbatim.
- `world_lattice_angles` keeps its exact signature and observable behavior (wrapper); its rustdoc may point to `world_lattice_candidates`.
- All 169 existing lib tests: unedited (success criterion).

## Testing Strategy

Pure-function tests only (glam, no Bevy App), in the existing `#[cfg(test)]` module.

| Scenario (proposal/plan) | Test |
|---|---|
| Pitch clamp passthrough under limit | `clamp_pitch_under_limit_passes_through` (pitch 0°, ±30° deltas) |
| Crossing +89°/−89° truncates exactly | `clamp_pitch_crossing_limit_truncates` (from 80°, +20° → +9°; mirror low) |
| At limit: 0 outward, free inward | `clamp_pitch_at_limit_blocks_outward_allows_inward` |
| Candidate source axes, axial view | `candidates_axis_forward_have_source_axes` (local_z = Z, up = Y → 4: ±X/±Y, correct axes/angles: X→−π/2, NEG_X→+π/2) |
| Diagonal view, 6 candidates | `candidates_diagonal_forward_yield_six_axes` |
| Wrapper equivalence | `lattice_angles_equal_candidate_angles` (both views, elementwise) |
| Step/candidate agreement | `next_lattice_candidate_matches_next_lattice_step` (both dirs, drifted up) |
| Q/E from (Z, Y) with expected axis | `next_candidate_q_yields_neg_x_e_yields_x` (Q → (+π/2, NEG_X), E → (−π/2, X)) |
| Reference-up rule | `snap_reference_up_fresh_and_opposite_use_up` + `snap_reference_up_same_dir_advances_residual` |
| Anchor lands on pending projection | `anchor_commit_matches_pending_projection`: compose `Quat::from_axis_angle(local_z, anim.target_angle) * up` from `start_or_retarget_snap` output, assert ≈ normalized projection of the axis from `snap_reference_up`+`next_lattice_candidate` (fresh, extend, reverse) |
| Spawn anchor | `nearest_world_axis_snaps_biased_vectors` + `nearest_world_axis_tie_uses_axes_order` |
| Defaults | `look_mode_defaults_to_sixdof` (Player + PlayerConfig; anchor_up = Y, pending None) |
| Config→Player copy (project lesson: NON-default value) | `look_mode_config_copies_to_player` asserting `LookMode::Anchored` propagates |
| Baseline | 169 existing tests green, unedited (`git diff` on the test module shows additions only) |

Verify gate: `cargo check`, `cargo test --lib`, `cargo test --doc`, `cargo check --example basic_world`; manual playtest per proposal success criteria. Never `cargo build`.

## Migration / Rollout

None — additive, `LookMode::SixDof` default preserves current behavior byte-for-byte for downstreams (Starframe). Rollback = revert commits or delete the Anchored arm + fields.

## Open Questions

None — pole handling (manual Q/E), RollMode interaction, and mid-anim semantics settled in decisions `sdd/player-anchored-look/decisions`; commit renormalization resolved here (decision table, "Anchor commit").
