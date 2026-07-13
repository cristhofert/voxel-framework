# Proposal: Player World-Aligned Snap Roll

## Intent

`RollMode::Snap` rolls ±90° relative to the orientation at press time. FreeFlight mouse look (local yaw∘pitch, non-commutative) accumulates world-relative roll drift, so snaps stay consistent with the player but not with the voxel grid — the playtest complaint. Redefine Snap targets as world-aligned up candidates so every finished snap lands grid-aligned.

## Scope

### In Scope
- Target selection at press time: candidates = 6 signed world axes projected onto plane ⊥ `local_z` (near-parallel axes excluded, ≥4 always survive); signed angle `δ = atan2(cross(u,c)·z, u·c)` fed to `rotate_local_z`.
- Q = next candidate CCW around forward, E = CW; first press from drift travels the residual (<90° in uniform lattices). EPS_ON skip: already on a candidate → one step further.
- One primitive covers fresh / same-dir extend / opposite-dir reverse: "next candidate in direction d from a reference up".
- `start_or_retarget_snap` gains `local_z: Vec3, up: Vec3` params (+ new pure lattice helpers); call site in `player_look` passes the in-scope transform.
- Snap target fixed at press time; no mid-animation re-resolution.
- Delta spec: modify "Snap Mode Is Opt-In" and "Cancel-and-Restart Is Lattice-Aware"; add "World-Aligned Lattice Definition"; rustdoc sync.
- Tests: adapt existing (Vec3::Z/Vec3::Y reproduce old expectations); add drifted-up, diagonal-forward (6 candidates), axis-forward (4 candidates), EPS_ON-skip, extend/reverse cases.

### Out of Scope
- Third `RollMode` variant (rejected — modify Snap in place).
- Mid-animation target re-resolution.
- `SnapRollAnim`, `advance_snap`, `apply_easing`, Continuous arm, Grounded arm — untouched.
- Aligning yaw/pitch to the grid; only roll/up.

## Approach

Exploration Approach 1 (full candidate set). `SnapRollAnim` stays in relative angles; world-alignment lives entirely in press-time target selection as pure glam math — no Bevy plumbing, no query changes.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `src/player/mod.rs` | Modified | `start_or_retarget_snap` rewrite + lattice helpers, `player_look` Snap-arm args, rustdoc, tests |
| `openspec/specs/player/spec.md` | Modified | Via delta spec + archive merge |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Sign flip (local +Z is BACK) | Med | Unit-test δ formula against known rotations; manual playtest |
| EPS_ON mistuned | Med | Spec fixes the rule, not the constant; start ~1e-3 rad |
| Non-90° steps at diagonal orientations feel wrong | Low | Spec scenario documents 6-candidate case |
| Mid-anim mouse leaves up off-candidate | By design | Spec carries "absent mouse motion" proviso |
| Touching Continuous arm (spec-locked) | Low | Verify phase diffs lines 727–735 byte-for-byte |

## Rollback Plan

Single-file code change: revert the commit touching `src/player/mod.rs` restores anchor-relative snap; delta spec is not merged to canonical spec until archive, so pre-archive rollback is a `git revert` with no spec cleanup.

## Dependencies

- None (pure glam math; no new crates, no API surface change).

## Success Criteria

- [ ] From a drifted orientation, one Q/E press lands `transform.up()` within ε of a world candidate (no mouse motion during anim).
- [ ] Aligned orientation reproduces old behavior: Q → +π/2, extend → π, reverse → nearest stop behind.
- [ ] `advance_snap`/`apply_easing` and their tests unchanged; Continuous arm byte-identical.
- [ ] `cargo test` green with new lattice tests; `cargo check` clean.
