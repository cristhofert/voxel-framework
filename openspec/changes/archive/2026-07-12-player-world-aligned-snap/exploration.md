# Exploration: player-world-aligned-snap

Change: `player-world-aligned-snap` · Project: `voxel-framework` · Phase: sdd-explore
Date: 2026-07-05 · Mode: hybrid (openspec + engram)

## Settled Decisions (user-confirmed — not open questions)

1. **Q/E semantics**: Q = rotate to the NEXT world-lattice point counterclockwise around forward; E = clockwise. First press from a drifted orientation travels less than 90°.
2. **Mid-animation mouse look**: the snap target is FIXED at press time (computed from the transform at `just_pressed`). It does NOT re-resolve while the animation runs.
3. **No third variant**: modify `RollMode::Snap` semantics directly. Repo is private, feature just shipped, no downstream consumers of anchor-relative snap.

Exploration validated all three against the code (see analysis below); no evidence was found against any of them.

## Current State

- `RollMode::Snap` (shipped in `player-snap-roll-mode`, archived 2026-06-27): Q/E `just_pressed` starts an eased roll of ±90° **relative to the orientation at press time** (the "anchor"). State machine is three pure helpers in `src/player/mod.rs`:
  - `apply_easing(t, easing)` (line 599) — curve evaluation.
  - `start_or_retarget_snap(existing, dir, duration, easing)` (line 608) — builds/retargets `SnapRollAnim` purely in **relative angles** (anchor = 0, lattice = multiples of `FRAC_PI_2`). Same-direction press extends target by another quadrant; opposite-direction press reverses to the nearest lattice point at-or-behind `current_applied`.
  - `advance_snap(&mut anim, dt)` (line 667) — returns per-frame `(delta, finished)`; caller applies via `transform.rotate_local_z(delta)`.
- `SnapRollAnim` (line 302): `start_angle`, `target_angle`, `elapsed`, `duration`, `easing`, `current_applied` — all relative angles, no world data.
- Call site: `player_look`, `RollMode::Snap` arm (lines 736–760). The transform is already in scope (`transform.up()`, `transform.local_z()` available at zero extra cost).
- **Root cause of the drift**: FreeFlight mouse look composes `rotate_local_y` + `rotate_local_x` (lines 722–723). Local yaw∘pitch don't commute, so diagonal mouse paths accumulate roll relative to the world (standard 6DOF horizon drift). The snap lattice is anchored at press time, so snaps stay internally consistent but inherit the drift — the player lands 90°-consistent with *themselves*, not with the voxel grid.

## The Math (question 1)

### Lattice definition

Given the camera's world-space **back** vector `z = transform.local_z()` (Bevy camera looks along −Z, so local +Z is the roll axis that `rotate_local_z` uses) and **up** `u = transform.up()`:

- Candidate up vectors = the 6 signed world axes `a ∈ {±X, ±Y, ±Z}` projected onto the plane ⊥ `z`:
  `c = normalize(a − (a·z) z)`
- Exclude candidates where the projection is degenerate: `1 − (a·z)² < EPS_PROJ` (axis near-parallel to the view axis). Only one axis *pair* can ever be near-parallel to `z`, so **at least 4 candidates always survive**. Suggested `EPS_PROJ ≈ 1e-4` (guards `normalize` against NaN; below ~0.6° tilt the axis is visually "the one you're looking down" anyway).

### Signed angle to a candidate

For each surviving candidate `c`, the signed roll angle that moves `u` onto `c` when passed to `rotate_local_z`:

```
δ(c) = atan2(cross(u, c) · z, u · c)      // δ ∈ (−π, π]
```

Verification of the sign convention: rotating `u` around axis `z` by `δ` gives `u' = u·cosδ + (z × u)·sinδ`; requiring `u' = c` yields exactly `cosδ = u·c`, `sinδ = cross(u, c)·z`. So the returned `δ` is directly usable as the total angle for the existing `rotate_local_z`-based animation — **Q/E keep the exact same visual rotation direction as today** (Q → positive `δ`, E → negative, matching current `dir = +1/−1`).

### Target selection

- **Q (dir = +1)**: candidate with the smallest `δ > EPS_ON`; if none (all candidates behind), wrap: smallest `δ + 2π`. Equivalently: sort angles, pick next strictly-CCW.
- **E (dir = −1)**: mirror — largest `δ < −EPS_ON`.
- `EPS_ON` (~1e-3 rad): if the player is already *on* a candidate within FP error, skip it and go one step further — preserves today's "if already on a lattice point, go one further" rule and guarantees a press always visibly moves.

### Lattice geometry facts (verified analytically)

- **Forward in a coordinate plane** (level flight, looking straight up/down — the overwhelmingly common cases): the two axes spanning that plane project onto a *single line* (the plane ⊥ z intersects a coordinate plane in one line), so the candidate set collapses to **exactly 4 directions at exactly 90° spacing**. E.g. forward = 45° between X and Z: ±X and ∓Z project to the same directions; candidates = {±Y, ±(1,0,−1)/√2}-style, uniform 90°.
- **Forward exactly on a world axis** (e.g. looking straight down): that axis pair is excluded; the other 4 candidates project to themselves unchanged → perfect 4-point lattice, zero distortion.
- **Fully generic forward** (all components nonzero, e.g. (1,1,1)/√3): the three axis pairs project to three distinct lines → **6 candidates**, non-uniform spacing (60° apart for the (1,1,1) case). Steps are then <90°, and cycling Q six times returns to start (angles sum to 360°). This is the maximally-grid-aligned behavior at every stop and is self-consistent (you always cycle the same stop set); an alternative "nearest-anchor + uniform 90°" lattice was considered and rejected because only its first stop is grid-aligned — subsequent stops drift off-candidate and the next press produces surprising corrective micro-steps.
- **Coincident candidates** (45°-in-plane case produces duplicate directions from two axes): no dedup needed — "smallest angle strictly greater than EPS_ON" treats duplicates as one stop.
- **Up equidistant between two candidates**: no tie-breaking needed — Q and E each pick their own side deterministically; a tie within one rotation direction is impossible for distinct candidate angles.

## Q/E Semantics & Cancel-and-Restart (question 2) — decision 1 confirmed coherent

The three press cases unify under one primitive, `next candidate in direction d from a reference up`:

| Case | Reference up | Rule |
|---|---|---|
| Fresh press (no anim) | current `transform.up()` | next candidate in `d` (EPS_ON-skip) |
| Same-direction press mid-anim | **in-flight target up** = current up rotated by the residual `target_angle − current_applied` around `z` | next candidate strictly past the old target → "extend by one more stop" |
| Opposite-direction press mid-anim | current `transform.up()` | next candidate in the new `d` (EPS_ON-skip) → "reverse to nearest stop behind" |

- This exactly generalizes the shipped anchor-relative rules: with forward in a coordinate plane and no drift, it reproduces the old `anchor + k·π/2` behavior bit-for-bit in intent (extend → +90° more; reverse from 27° → back to the 0° stop, i.e. travel −27°).
- The old invariant "roll lands on a multiple of π/2 once finished" becomes: **"once finished, `transform.up()` is within ε of a lattice candidate, provided no mouse motion occurred during the animation"** (the proviso follows from decision 2).
- First press from a drifted orientation travels the residual angle — **strictly less than the local candidate spacing** (≤90°, typically a few degrees when correcting drift). Matches decision 1. Note the fine print: "less than 90°" holds for spacing-90° configurations; in the generic 6-candidate case a step can be up to ~120° in pathological orientations — the spec should say "to the next candidate" rather than hard-code an upper bound.

## Impact on Pure Helpers & Testing (question 3)

Key insight: **`SnapRollAnim` can stay in relative angles, unchanged.** The world-alignment only affects how the *target angle is chosen at press time*. Consequences:

- `apply_easing` — untouched.
- `advance_snap` — untouched (relative-angle interpolation + FP-residue correction still correct). Its 2 tests untouched.
- `start_or_retarget_snap` — replaced by a world-aware version. Proposed shape (all plain glam math, no Bevy `App`):

```rust
/// Signed angles (rotate_local_z convention) from `up` to every valid
/// world-axis candidate around the roll axis `local_z`.
pub(crate) fn world_lattice_angles(local_z: Vec3, up: Vec3) -> ArrayVec/SmallVec/Vec<f32>;

/// Angle to the next candidate in `dir` from `from_angle_offset`, with EPS_ON skip.
pub(crate) fn next_lattice_angle(angles: &[f32], dir: i32, /* … */) -> f32;

pub(crate) fn start_or_retarget_snap(
    existing: Option<SnapRollAnim>,
    local_z: Vec3,          // transform.local_z() at press time
    up: Vec3,               // transform.up() at press time
    dir: i32,
    duration: f32,
    easing: SnapRollEasing,
) -> SnapRollAnim
```

  (Exact decomposition into sub-helpers is a design-phase call; the signature change — adding `local_z`, `up` — is the essential part.)
- Call site `player_look` (lines 738, 746): pass `*transform.local_z()`, `*transform.up()` (Dir3 → Vec3 deref). Transform is already mutably borrowed in scope; no query changes, no new system params, no Bevy plumbing.
- **Testing pattern preserved**: `Vec3` comes from glam via `bevy::prelude` — pure math, no App/input mocking. Existing `start_or_retarget_snap` tests adapt by passing `Vec3::Z, Vec3::Y` (canonical aligned orientation), under which the new function reproduces the old expectations exactly (candidates at 0/±90/180 → Q targets +π/2, extend targets π, reverse targets 0). New tests to add:
  - drifted up (e.g. 10° roll): Q → δ ≈ +80°, E → δ ≈ −10°, both landing on candidates;
  - forward = (1,1,1)/√3: 6 candidates, 60° steps;
  - forward straight down: ±Y excluded, 4 candidates, 90° steps;
  - EPS_ON skip: up exactly on a candidate → full step, not no-op;
  - same-direction extend and opposite-direction reverse in world terms.

## Mouse Look During Animation (question 4) — decision 2 confirmed sound

Target fixed at press time (user-confirmed). Validation of why this is the right call for this codebase:

- `advance_snap` stays byte-identical; easing over a fixed angular distance stays trivial and monotonic.
- Re-resolving each frame would require the animation to chase a moving target (variable total distance under a t-normalized easing → velocity discontinuities), would drag the transform into `advance_snap` (breaking the pure-helper test pattern), and buys almost nothing: mid-animation mouse motion introduces error of the same class the feature already tolerates *between* presses, and the very next Q/E press re-anchors to the world lattice.
- Spec consequence: the "lands aligned" guarantee must carry the proviso "absent mouse motion during the animation".

## Spec Changes Required (question 5)

Canonical spec: `openspec/specs/player/spec.md`. Delta spec (change phase) touches:

| Requirement | Change |
|---|---|
| **Snap Mode Is Opt-In via PlayerConfig** (line 35) | MODIFIED. "rotating local Z by `+PI/2`" → "rotating local Z by the signed angle to the next world-aligned up candidate, CCW for Q / CW for E". Scenario "Q press starts +90 animation" splits into two: (a) from an exactly world-aligned orientation → target `+PI/2`; (b) from a drifted orientation → target = residual angle < spacing, landing on a candidate. "Held key does not stack" scenario survives as-is. |
| **Cancel-and-Restart Is Lattice-Aware** (line 58) | MODIFIED. Lattice redefined from "multiples of `PI/2` from anchor" to the world-candidate set (6 signed world axes projected ⊥ forward, near-parallel excluded). Same-direction → next candidate past current target; opposite → next candidate from current up. Closing invariant reworded: on `finished`, `transform.up()` is within ε of a candidate, provided no mouse motion during the animation. Both scenarios rewritten in world terms (the 0.40 rad / 0.47 rad numbers still work when starting from an aligned orientation). |
| **NEW: World-Aligned Lattice Definition** | ADDED. Candidate construction, near-parallel exclusion (≥4 candidates always), signed-angle formula/convention, EPS_ON skip rule, target-fixed-at-press-time rule. Scenarios: forward on axis → perfect 4-lattice; generic forward → 6 candidates; drifted first press < spacing. |
| Continuous Roll Is Default and Preserved | UNTOUCHED (spec locks it; code path not modified). |
| Easing and Duration Are Configurable | UNTOUCHED (duration/easing orthogonal to target selection). |
| Roll Is Disabled in Grounded Mode | UNTOUCHED. |

Doc comments needing sync (non-spec): `RollMode::Snap` rustdoc (lines 267, 275), `snap_roll_duration` docs (lines 407, 505) — "90° snap rotation" wording becomes "snap to the nearest world-aligned orientation". No `prelude.rs` changes (`RollMode`, `SnapRollEasing` already exported; helpers stay `pub(crate)`). No README/CHANGELOG API-surface change — behavior description of Snap mode only.

## Affected Areas

- `src/player/mod.rs` — `start_or_retarget_snap` rewrite (+ new lattice helpers), call-site arg additions in `player_look` Snap arm, rustdoc updates, test adaptations + new tests. Everything else (types, `advance_snap`, `apply_easing`, Continuous arm, Grounded arm) untouched.
- `openspec/specs/player/spec.md` — via delta spec then archive merge (see table above).

## Approaches (target-selection lattice)

1. **Full candidate set (6 projected signed axes)** — every stop is maximally grid-aligned; collapses to a uniform 4-point/90° lattice in all common orientations (forward in a coordinate plane or on an axis); self-consistent cycling.
   - Pros: solves the exact playtest complaint at every stop; degenerate cases fall out naturally; one primitive covers fresh/extend/reverse.
   - Cons: generic diagonal orientations give 6 stops at non-90° spacing (novel but predictable).
   - Effort: **Low-Medium**
2. **Nearest-candidate anchor + uniform 90° steps** — first press aligns, subsequent presses add exact quadrants from there.
   - Pros: constant step size.
   - Cons: only the first stop is grid-aligned in generic orientations; later presses re-resolve nearest → surprising corrective micro-steps; incoherent stop set.
   - Effort: Low-Medium (same code, worse feel)
3. *(Rejected upfront by decision 3)* third `RollMode` variant — dead spec/API surface for an unwanted behavior.

## Recommendation

Approach 1 (full candidate set), implementing the settled decisions: press-time-fixed target, Q=CCW/E=CW next-candidate, `RollMode::Snap` modified in place. Keep `SnapRollAnim`/`advance_snap`/`apply_easing` untouched; confine the change to target selection at press time. This is small enough that spec + implementation land in one apply batch.

## Risks

- **Sign-convention flip** (local +Z is *back*, not forward; `cross(u,c)·z` ordering): a wrong sign makes Q/E swap or overshoot. Mitigation: unit test the δ formula against known rotations + one manual playtest (`cargo run --example basic_world` needs display).
- **EPS_ON tuning**: too small → post-snap FP residue makes the next press a ~0° no-op-looking step; too large → small intentional corrections get skipped. Start at ~1e-3 rad; spec should state the rule, not the constant.
- **Non-uniform steps in generic orientations** may read as "wrong" to a player expecting always-90°; acceptable because those orientations are exactly the ones where no 90° step could be grid-aligned anyway. Document in spec scenario.
- **Mid-anim mouse motion** leaves final up slightly off-candidate (by design, decision 2); spec must carry the proviso or verify will flag it.
- Spec lock on Continuous mode: implementation must not touch lines 727–735; verify phase should diff that arm.

## Ready for Proposal

**Yes.** Direction, semantics, and scope are settled; the math is validated against the code's conventions; helper-signature impact is contained. Next: `sdd-propose` (can fold this exploration in directly).
