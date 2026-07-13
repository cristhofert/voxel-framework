# Design: Player World-Aligned Snap Roll

Change: `player-world-aligned-snap` · Base: proposal + delta spec (`specs/player/spec.md`) · Date: 2026-07-05

## Technical Approach

World-alignment lives entirely in press-time target selection. `SnapRollAnim` stays in relative angles; two new pure glam helpers compute the candidate lattice and the next step, and `start_or_retarget_snap` orchestrates them. `advance_snap`, `apply_easing`, the Continuous arm (lines 727–735), and the Grounded arm are byte-for-byte untouched. No schedule/system-order changes; no generic-type implications; no new dependencies.

Key invariant making this safe: the animation applies only angle *differences* via `rotate_local_z(delta)`, so the absolute meaning of the relative frame (the old "anchor") is irrelevant — new targets are expressed as `current_applied + step` (or `target_angle + step`) and land exactly on the world candidate computed at press time.

## Architecture Decisions

| Decision | Choice | Alternative rejected | Rationale |
|---|---|---|---|
| Anim representation | `SnapRollAnim` stays relative-angle | Store world target quat/up in anim | `advance_snap`/`apply_easing` + tests untouched (exploration finding); differences-only application makes anchor drift harmless |
| Helper decomposition | `world_lattice_angles` + `next_lattice_step` + orchestration in `start_or_retarget_snap` | One monolithic function | Each piece unit-testable with plain glam, no Bevy App (project pattern) |
| Extend reference up | Derive in-flight target up: `Quat::from_axis_angle(local_z, target_angle − current_applied) * up` | Add `target_up` field to `SnapRollAnim` | Struct change is out of scope; formula is exact because `up ⊥ local_z` (orthonormal frame) |
| Candidate container | `Vec<f32>` | `[f32; 6]`+len, smallvec | Cold path (once per key press); no new deps; ≤6 elems |
| Candidate iteration | Loop 6 signed axes, project each | 3 axes + π offsets | Trivial cost, matches spec wording directly |
| Thresholds | `SNAP_MIN_PROJ_SQ = 0.49` (projection length² — ~45° dominance exclusion; was `SNAP_EPS_PROJ = 1e-4`, revised after playtest: an epsilon guard let the near-forward axis pair survive as junk candidates. `0.49` not `0.5` so the exact-45° tie stays inclusive under float rounding — `0.5` excluded BOTH families on an edge-on diagonal, breaking the ≥4 invariant), `SNAP_EPS_ON = 1e-3` rad (on-candidate skip) — internal consts | Config fields | Spec fixes the rule, not the constant (proposal risk mitigation) |

## Data Flow

    Q/E just_pressed (player_look, Snap arm)
        │  *transform.local_z(), *transform.up()   (Dir3 → Vec3 deref)
        ▼
    start_or_retarget_snap(existing, local_z, up, dir, duration, easing)
        │  world_lattice_angles(z, ref_up) → Vec<δ>
        │  next_lattice_step(&angles, dir) → step
        ▼
    SnapRollAnim (relative angles) ──per frame──▶ advance_snap ──▶ rotate_local_z(delta)

## Interfaces / Contracts

```rust
/// Signed angles δ ∈ (−π, π] (rotate_local_z convention) from `up` to each
/// surviving world-axis candidate around roll axis `local_z` (Bevy BACK vector).
/// δ = atan2(cross(up, c) · local_z, up · c); axes with projection
/// length² < SNAP_MIN_PROJ_SQ (0.49 → within ~45° of local_z) excluded
/// (≥4 always survive; near-axis views give exactly 4).
pub(crate) fn world_lattice_angles(local_z: Vec3, up: Vec3) -> Vec<f32>;

/// Next step in `dir`: dir>0 → min over {δ > SNAP_EPS_ON ? δ : δ + TAU};
/// dir<0 → max over {δ < −SNAP_EPS_ON ? δ : δ − TAU}. Duplicates collapse
/// naturally; result is always a nonzero signed angle.
pub(crate) fn next_lattice_step(angles: &[f32], dir: i32) -> f32;

pub(crate) fn start_or_retarget_snap(
    existing: Option<SnapRollAnim>,
    local_z: Vec3,   // *transform.local_z() at press time
    up: Vec3,        // *transform.up() at press time
    dir: i32,        // +1 = Q (CCW), −1 = E (CW) — unchanged convention
    duration: f32,
    easing: SnapRollEasing,
) -> SnapRollAnim;
```

`start_or_retarget_snap` branches (same-dir detection formula kept from current code):

| Case | Reference up | New target_angle | start_angle |
|---|---|---|---|
| `None` | `up` | `next_lattice_step(...)` | `0.0` |
| Same dir | `Quat::from_axis_angle(local_z, residual) * up` | `anim.target_angle + step` | `anim.current_applied` |
| Opposite dir | `up` | `anim.current_applied + step` | `anim.current_applied` |

Sign convention (proposal risk #1): `local_z` is Bevy's BACK vector — exactly the axis `rotate_local_z` uses; exploration verified δ from this formula feeds `rotate_local_z` unmodified, Q stays visually CCW.

## File Changes

| File | Action | Description |
|---|---|---|
| `src/player/mod.rs` | Modify | Replace `start_or_retarget_snap` body + signature; add 2 helpers + 2 consts; pass `*transform.local_z()`, `*transform.up()` at both call sites (lines ~738, ~746); rustdoc sync (`RollMode::Snap` line 267/275, `snap_roll_duration` docs); tests |

Single file. `openspec/specs/player/spec.md` changes only at archive via the delta spec.

## Testing Strategy

| Layer | What | Approach |
|---|---|---|
| Unit (adapted) | 5 existing `start_or_retarget_snap` tests | Add `Vec3::Z, Vec3::Y` args; expectations unchanged (aligned frame reproduces +π/2 / π / 0) |
| Unit (untouched) | `advance_snap`, `apply_easing`, config tests | Zero edits — success criterion |
| Unit (new) | Lattice math | `world_lattice_angles`: axis-forward → 4 angles @90°; `(1,1,1)/√3` forward → 6 angles @~60°; drifted up 10° → Q≈+80°, E≈−10°, both land on candidates; EPS_ON skip → full step, never ~0; extend/reverse in world terms; Q-cycle on diagonal sums to 2π |
| Manual | Feel + sign check | `cargo run --example basic_world` (needs display) |

Verify gate: `cargo test` green, `cargo check` clean, `git diff` shows lines 727–735 untouched.

## Migration / Rollout

No migration. Behavior change only in `RollMode::Snap` (opt-in, non-default); revert = single-commit `git revert`.

## Open Questions

None — all press-semantics questions settled in exploration (decisions 1–3).
