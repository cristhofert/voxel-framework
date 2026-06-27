# Tasks: Player Snap Roll Mode

## Phase 1: Scaffolding

- [x] 1.1 Confirm `openspec/changes/player-snap-roll-mode/` has `proposal.md`, `design.md`, `specs/player/spec.md`.
- [x] 1.2 Baseline `cargo test --lib` (expected: 156 pass).
- [x] 1.3 Baseline `cargo test --doc` (expected: 107 pass).

## Phase 2: New Types

- [x] 2.1 In `src/player/mod.rs:~280` declare `pub enum RollMode { #[default] Continuous, Snap }` with `#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]`.
- [x] 2.2 Same site, declare `pub enum SnapRollEasing { #[default] EaseOutCubic, EaseOutQuart, Smoothstep, Linear }` with same derives.
- [x] 2.3 Same site, declare `pub(crate) struct SnapRollAnim` `#[derive(Copy, Clone, Debug)]` with fields `start_angle: f32`, `target_angle: f32`, `elapsed: f32`, `duration: f32`, `easing: SnapRollEasing`, `current_applied: f32`.

## Phase 3: Field Additions

- [x] 3.1 Add to `Player` (`src/player/mod.rs:282-356`): `pub roll_mode: RollMode`, `pub snap_roll_duration: f32`, `pub snap_roll_easing: SnapRollEasing`, `pub(crate) snap_anim: Option<SnapRollAnim>`.
- [x] 3.2 Update `Default for Player` (`:358-376`): `Continuous`, `0.18`, `EaseOutCubic`, `None`.
- [x] 3.3 Add to `PlayerConfig` (`:411-453`): `pub roll_mode: RollMode`, `pub snap_roll_duration_secs: f32` (default `0.18`), `pub snap_roll_easing: SnapRollEasing` (default `EaseOutCubic`); update `Default for PlayerConfig`.
- [x] 3.4 In `spawn_player` (`:~470-516`), copy the 3 config fields into the spawned `Player`.

## Phase 4: Pure Helpers

- [x] 4.1 Above `player_look`, add `pub(crate) fn apply_easing(t: f32, e: SnapRollEasing) -> f32` implementing 4 curves per design (cubic = `1.0 - (1.0 - t).powi(3)`, etc.).
- [x] 4.2 Add `pub(crate) fn start_or_retarget_snap(existing: Option<SnapRollAnim>, dir: i32, current_local_z: f32, duration: f32, easing: SnapRollEasing) -> SnapRollAnim` with the 3-branch decision tree (no existing / same dir extend / opposite dir lattice-aware).
- [x] 4.3 Add `pub(crate) fn advance_snap(anim: &mut SnapRollAnim, dt: f32) -> (f32, bool)`: advance `elapsed`, `t.clamp(0.0, 1.0)`, ease, compute frame delta, update `current_applied`. On `finished`, write exact `target_angle`.

## Phase 5: Wire into player_look

- [x] 5.1 In `player_look` `FreeFlight` branch (`src/player/mod.rs:534-588`), replace the unconditional roll block at `:560-566` with `match player.roll_mode`.
- [x] 5.2 `Continuous` arm: keep existing `transform.rotate_local_z(roll * roll_speed * dt)` byte-for-byte.
- [x] 5.3 `Snap` arm: on `keys.just_pressed(KeyCode::KeyQ)`/`KeyE`, call `start_or_retarget_snap`; if `snap_anim.is_some()` call `advance_snap`, `transform.rotate_local_z(delta)`, clear on `finished`.
- [x] 5.4 Confirm `Grounded` skips roll for both arms (preserve `:585`).

## Phase 6: Public Exports

- [x] 6.1 Re-export `RollMode` and `SnapRollEasing` in `src/prelude.rs:123-135`.

## Phase 7: Tests

Add 5 tests to `mod tests` in `src/player/mod.rs:1271-1314`. Pure-function only — no Bevy `App` (mirror `src/world/mining.rs:376-388`).

- [x] 7.1 `snap_roll_completes_to_90deg`: loop `advance_snap` for `duration/dt`; assert cumulative ≈ `PI/2`.
- [x] 7.2 `snap_roll_easing_monotonic`: collect deltas under `EaseOutCubic`; assert `deltas[i] >= deltas[i+1]`.
- [x] 7.3 `snap_roll_same_direction_extends_target`: build anim, retarget same dir mid-anim; assert `target_angle == start_angle + PI` and resumes from `current_applied`.
- [x] 7.4 `snap_roll_opposite_direction_reverses_to_lattice`: CCW anim at `current_applied ≈ 0.47`; retarget `dir = -1`; assert `target_angle` is nearest `-PI/2` lattice point below current.
- [x] 7.5 `continuous_mode_unchanged`: assert `Player::default().roll_mode == Continuous` and `snap_anim.is_none()`.

## Phase 8: Build & Test Verification

- [x] 8.1 `cargo check` — clean.
- [x] 8.2 `cargo check --example basic_world` — clean.
- [x] 8.3 `cargo test --lib` — 161 passing (156 + 5).
- [x] 8.4 `cargo test --doc` — 107 passing.

## Phase 9: Pre-Archive Checklist

- [x] 9.1 Verify `Player::default().roll_mode == Continuous` via test 7.5.
- [x] 9.2 Inspect `src/player/persistence.rs`: confirm `PlayerSaveData` does NOT serialize new fields.
- [x] 9.3 Confirm no API regression: `cargo check --no-default-features` and `cargo check --all-features` both pass.
- [x] 9.4 Hand off to `sdd-verify`: spec `openspec/changes/player-snap-roll-mode/specs/player/spec.md`, tasks this file, evidence = test output + check exit codes.
