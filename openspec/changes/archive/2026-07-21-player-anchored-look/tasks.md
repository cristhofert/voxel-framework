# Tasks: Player Anchored Look Mode

Change: `player-anchored-look` · Inputs: proposal.md, specs/player/spec.md, design.md.
Baseline: 169 existing lib tests MUST stay green and unedited. Never `cargo build`.

## Phase 1: Scaffolding — Types and Fields (`src/player/mod.rs`)

- [x] 1.1 Add `LookMode { #[default] SixDof, Anchored }` enum (derive set mirroring `RollMode`) with rustdoc noting RollMode is ignored in Anchored.
- [x] 1.2 Add `look_mode: LookMode` to `PlayerConfig` + its `Default`.
- [x] 1.3 Add `Player` fields: `look_mode: LookMode`, `anchor_up: Vec3` (default `Vec3::Y`), `pending_anchor: Option<Vec3>` (`pub(crate)`, default `None`) + defaults.
- [x] 1.4 In `spawn_player`: build the `Transform` in a local, copy `look_mode`, set `anchor_up = nearest_world_axis(*transform.up())` (helper stub OK until 2.5).
- [x] 1.5 Add rustdoc note on `RollMode` ("ignored when `LookMode::Anchored`").
- [x] 1.6 Re-export `LookMode` in `src/prelude.rs` (player re-export, ~line 127).

## Phase 2: Core — Pure Helpers (`src/player/mod.rs`)

- [x] 2.1 Move `world_lattice_angles` body into `world_lattice_candidates(local_z, up) -> Vec<(f32, Vec3)>` carrying each candidate's source signed axis; same `SNAP_MIN_PROJ_SQ` filter.
- [x] 2.2 Reduce `world_lattice_angles` to a thin `.map(|(a, _)| a)` wrapper — exact signature/behavior preserved.
- [x] 2.3 Add `next_lattice_candidate(&[(f32, Vec3)], dir) -> (f32, Vec3)` mirroring `next_lattice_step` (EPS_ON wrap, min/max by wrapped angle); panics with message on empty slice.
- [x] 2.4 Add `snap_reference_up(existing, local_z, up, dir) -> Vec3` replicating `start_or_retarget_snap`'s reference-up rule (same-dir mid-anim → up advanced by residual; else up).
- [x] 2.5 Add `clamp_pitch_delta(forward, anchor_up, pitch_d) -> f32` (asin form, clamp-then-subtract, ±89°, NaN-free) and `nearest_world_axis(v) -> Vec3` (max dot over `AXES`, strict `>` tie-break).

## Phase 3: Wiring — Anchored Arm in `player_look`

- [x] 3.1 Wrap the FreeFlight arm in `match player.look_mode`; SixDof arm = current code re-indented verbatim (zero logic edits; Grounded arm untouched).
- [x] 3.2 Anchored mouse look: yaw `transform.rotate(Quat::from_axis_angle(player.anchor_up, yaw_d))`; pitch `rotate_local_x(clamp_pitch_delta(..))`; keep `Vec2::ZERO` guard; old anchor while anim runs.
- [x] 3.3 Anchored Q/E `just_pressed` (RollMode ignored): compute `ref_up` via `snap_reference_up`, set `pending_anchor` from `next_lattice_candidate(world_lattice_candidates(local_z, ref_up), dir).1`, call `start_or_retarget_snap` unchanged.
- [x] 3.4 Advance snap in Anchored; on finished: `anchor_up = pending_anchor.take()`, `look_to(*forward(), anchor_up)`, clear `snap_anim`.
- [x] 3.5 Run `cargo check` — clean.

## Phase 4: Testing (new tests only, existing `#[cfg(test)]` module)

- [x] 4.1 Pitch clamp: `clamp_pitch_under_limit_passes_through`, `clamp_pitch_crossing_limit_truncates`, `clamp_pitch_at_limit_blocks_outward_allows_inward`.
- [x] 4.2 Candidates: `candidates_axis_forward_have_source_axes` (Z/Y → 4 axes, X→−π/2, NEG_X→+π/2), `candidates_diagonal_forward_yield_six_axes`, `lattice_angles_equal_candidate_angles`.
- [x] 4.3 Stepping: `next_lattice_candidate_matches_next_lattice_step` (both dirs, drifted up), `next_candidate_q_yields_neg_x_e_yields_x`.
- [x] 4.4 Reference/commit: `snap_reference_up_fresh_and_opposite_use_up`, `snap_reference_up_same_dir_advances_residual`, `anchor_commit_matches_pending_projection` (fresh, extend, reverse).
- [x] 4.5 Spawn/defaults: `nearest_world_axis_snaps_biased_vectors`, `nearest_world_axis_tie_uses_axes_order`, `look_mode_defaults_to_sixdof`, `look_mode_config_copies_to_player` (non-default `Anchored`).
- [x] 4.6 Run `cargo test --lib` — 169 baseline + new tests green (184 total); `git diff` shows test additions only.

## Phase 5: Example, Docs, Verification Gate

- [x] 5.1 Set `look_mode: LookMode::Anchored` in `examples/basic_world.rs` for playtest (showcase vs revert decided post-playtest).
- [x] 5.2 Gate: `cargo test --doc` + `cargo check --example basic_world` green.
- [x] 5.3 Manual playtest per proposal: level horizon under diagonal input, ±89° stop, Q/E animated 90° switch, anchor always a pure axis, 6DOF regression check without `look_mode`. — APPROVED 2026-07-21: usuario confirmó 1–3; basic_world queda como showcase Anchored.
