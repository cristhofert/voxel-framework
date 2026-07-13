# Tasks: Player World-Aligned Snap Roll

Change: `player-world-aligned-snap`. Single file: `src/player/mod.rs`.
Baseline: `cargo test --lib` = 163 pass. Locked regions (must show empty diff):
Continuous arm (lines 727–735), `advance_snap`, `apply_easing`, Grounded arm.

## Phase 1: Foundation — lattice helpers

- [x] 1.1 Add consts `SNAP_EPS_PROJ: f32 = 1e-4` and `SNAP_EPS_ON: f32 = 1e-3` near `SnapRollAnim` in `src/player/mod.rs`, with rustdoc stating what each guards (projection exclusion / on-candidate skip).
- [x] 1.2 Add `pub(crate) fn world_lattice_angles(local_z: Vec3, up: Vec3) -> Vec<f32>`: loop 6 signed world axes, project onto plane ⊥ `local_z`, exclude when projection length² < `SNAP_EPS_PROJ`, return `δ = atan2(cross(up, c) · local_z, up · c)` per survivor.
- [x] 1.3 Add `pub(crate) fn next_lattice_step(angles: &[f32], dir: i32) -> f32`: dir>0 → min over `{δ > SNAP_EPS_ON ? δ : δ + TAU}`; dir<0 → max over `{δ < −SNAP_EPS_ON ? δ : δ − TAU}` (implements EPS_ON RULE + wrap).

## Phase 2: Core — retarget rewrite

- [x] 2.1 Change `start_or_retarget_snap` signature to `(existing, local_z: Vec3, up: Vec3, dir, duration, easing)`; keep the existing same-dir detection formula.
- [x] 2.2 Implement the 3 branches per design table: None → ref `up`, target = step, start 0.0; same dir → ref `Quat::from_axis_angle(local_z, target_angle − current_applied) * up`, target = `target_angle + step`, start = `current_applied`; opposite → ref `up`, target = `current_applied + step`, start = `current_applied`.

## Phase 3: Wiring + docs

- [x] 3.1 Update both Snap-arm call sites (lines ~738, ~746) to pass `*transform.local_z(), *transform.up()`. Verify `git diff` leaves lines 727–735 (Continuous arm) byte-for-byte untouched.
- [x] 3.2 Sync rustdoc: `RollMode::Snap` (lines ~267/275), `snap_roll_duration`, and the `SnapRollAnim`/`start_or_retarget_snap` doc at line ~299 — describe world-aligned candidates, not fixed ±π/2. Run `cargo check`.

## Phase 4: Testing

- [x] 4.1 Adapt the 5 existing tests (lines 1491–1572) by inserting `Vec3::Z, Vec3::Y` args; expectations unchanged. Covers scenarios "Q press from a world-aligned orientation targets +90°", "Same-direction press extends one candidate further", "Opposite-direction press reverses to nearest stop behind".
- [x] 4.2 Test `world_lattice_angles`: axis-parallel `local_z` → exactly 4 candidates at 90° spacing (scenario "Forward on a world axis yields a perfect 4-lattice").
- [x] 4.3 Test `world_lattice_angles` + Q-cycle with forward `(1,1,1)/√3` → 6 candidates ~60° apart; six `next_lattice_step` presses sum to 2π (scenario "Generic diagonal forward yields 6 candidates with non-90° steps").
- [x] 4.4 Test drifted up 10° off candidate: Q → ≈ +80°, E → ≈ −10°; both land within ε of a candidate (scenario "Q press from a drifted orientation travels the residual" — catches Q/E sign swap).
- [x] 4.5 Test EPS_ON: up exactly on a candidate → full step to next, never ~0 (scenario "EPS_ON skip on an exact candidate").
- [x] 4.6 Test extend and reverse mid-animation with drifted frame land on world candidates (lattice-aware retarget requirement).
- [x] 4.7 Confirm `advance_snap`/`apply_easing`/config tests need zero edits.

## Phase 5: Verification gates

- [x] 5.1 `cargo check`, `cargo test --lib` (≥163 + new, all green), `cargo test --doc`, `cargo check --example basic_world`.
- [x] 5.2 `git diff src/player/mod.rs` — confirm Continuous arm, `advance_snap`, `apply_easing`, Grounded arm unchanged.

## Phase 6: Playtest fix — 45° dominance exclusion

Playtest found 6 stops (2 junk) on a near-axis view: `SNAP_EPS_PROJ = 1e-4` only
excluded axes within ~0.6° of forward, so any mouse-look drift resurrected the
near-forward axis pair as perceptually arbitrary candidates.

- [x] 6.1 Rename `SNAP_EPS_PROJ` → `SNAP_MIN_PROJ_SQ = 0.49`: exclude axes within ~45° of local Z. Diagonal `(1,1,1)/√3` scenario unaffected (proj² = 2/3). `0.49` not `0.5`: at an exact-45° edge-on view float rounding put BOTH families below 0.5 → 2 candidates, breaking the ≥4 invariant (caught by the new boundary assertion).
- [x] 6.2 Regression test `lattice_near_axis_forward_still_yields_four_candidates` (drifted ~8° view → 4 candidates; exact 45° boundary keeps 6).
- [x] 6.3 Spec delta + design updated (45° rule + new scenario). Re-run gates.
