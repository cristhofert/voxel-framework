# Delta for Player — World-Aligned Snap Roll

Change: `player-world-aligned-snap`. Base spec: `openspec/specs/player/spec.md` (v1).
Untouched requirements (NOT duplicated here): Continuous Roll Is Default and Preserved
(code path at `src/player/mod.rs:727-735` stays byte-for-byte locked), Easing and
Duration Are Configurable, Roll Is Disabled in Grounded Mode.

## ADDED Requirements

### Requirement: World-Aligned Lattice Definition

At Q/E `just_pressed` in snap mode, the system MUST build the candidate up set as
the 6 signed world axes (`±X, ±Y, ±Z`) projected onto the plane perpendicular to the
player's local Z (roll axis), normalized. Axes lying within ~45° of local Z
(projection length² below ~0.5) MUST be excluded — the axis family the player is
roughly looking along never yields up candidates, so a mostly-axis-aligned view
produces exactly 4 stops. At least 4 candidates SHALL always survive: the exact
threshold constant is implementation-defined but MUST keep the exact-45° tie
inclusive (float rounding at an edge-on 45° view must not exclude both axis
families). For each candidate `c`, the signed roll angle from current up `u` MUST
use the `rotate_local_z` convention: `δ = atan2(cross(u, c) · z, u · c)`.

The target MUST be the next candidate in the press direction — Q = smallest positive
δ (counterclockwise around forward), E = largest negative δ (clockwise) — wrapping
past ±π when no candidate remains ahead.

EPS_ON RULE: if `u` already lies on a candidate within floating-point tolerance,
that candidate MUST be skipped and the next one in the press direction selected —
a press SHALL always produce visible motion. The tolerance is an implementation
constant, not fixed by this spec.

The target MUST be fixed at press time and MUST NOT re-resolve mid-animation.

#### Scenario: Forward on a world axis yields a perfect 4-lattice

- GIVEN local Z parallel to a world axis (e.g. looking straight down)
- WHEN candidates are built
- THEN that axis pair is excluded, exactly 4 candidates remain at exactly 90° spacing

#### Scenario: Near-axis forward still yields a 4-lattice

- GIVEN local Z drifted a few degrees off a world axis (typical mouse-look drift)
- WHEN candidates are built
- THEN the near-forward axis pair is excluded and exactly 4 candidates remain
- AND no press can target a stop derived from the near-forward axis
(Playtest regression: with a pure epsilon guard the drifted ±Z pair survived as
two perceptually arbitrary stops — 6 positions where the player expects 4.)

#### Scenario: Generic diagonal forward yields 6 candidates with non-90° steps

- GIVEN forward `(1,1,1)/√3` (all components nonzero)
- WHEN candidates are built and Q is pressed repeatedly from a candidate
- THEN 6 candidates exist ~60° apart, each step is non-90°
- AND six presses cycle back to the starting candidate

#### Scenario: EPS_ON skip on an exact candidate

- GIVEN `transform.up()` exactly on a candidate (within tolerance)
- WHEN Q is pressed
- THEN the target MUST be the next candidate beyond it, never a ~0° no-op

## MODIFIED Requirements

### Requirement: Snap Mode Is Opt-In via PlayerConfig

Setting `PlayerConfig::roll_mode = RollMode::Snap` before `spawn_player` SHALL make
the spawned `Player` use snap mode. In `PlayerMode::FreeFlight`, `KeyCode::KeyQ`
`just_pressed` SHALL start an animation rotating local Z by the signed angle to the
next world-aligned up candidate counterclockwise around forward; `KeyCode::KeyE`
`just_pressed` SHALL do the same clockwise (per World-Aligned Lattice Definition).
Duration and easing follow the existing configurable requirement. While the animation
runs, `keys.pressed` (held) MUST NOT trigger additional rotations — only
`just_pressed` events count.
(Previously: fixed `±PI/2` relative to orientation at press time.)

#### Scenario: Q press from a world-aligned orientation targets +90°

- GIVEN a snap-mode `Player` in FreeFlight with `local_z = Vec3::Z`, `up = Vec3::Y`
- WHEN `KeyCode::KeyQ` `just_pressed` fires
- THEN `snap_anim` target MUST equal `+PI/2` (reproduces prior behavior)

#### Scenario: Q press from a drifted orientation travels the residual

- GIVEN a snap-mode `Player` whose up is rolled 10° off the nearest candidate
- WHEN `KeyCode::KeyQ` `just_pressed` fires
- THEN the target MUST be the next candidate CCW (≈ +80°), not `+PI/2`
- AND on `finished`, `transform.up()` lands within ε of that candidate (absent mouse motion)

#### Scenario: Held key does not stack rotations

- GIVEN a snap-mode `Player` with an in-progress `snap_anim`
- WHEN the key is reported by `keys.pressed` (held) but NOT `just_pressed`
- THEN no new animation SHALL start and the target SHALL remain unchanged

### Requirement: Cancel-and-Restart Is Lattice-Aware

All presses SHALL resolve via one primitive: "next candidate in direction `d` from a
reference up" (with the EPS_ON rule). Fresh press: reference = current
`transform.up()`. Same-direction press mid-animation: reference = the in-flight
target up; the animation SHALL retarget one candidate further, resuming from the
current applied angle. Opposite-direction press mid-animation: reference = current
`transform.up()` in the new direction (nearest stop behind). Once `finished == true`,
`transform.up()` MUST be within ε of a lattice candidate, provided no mouse motion
occurred during the animation.
(Previously: lattice = multiples of `PI/2` from the press-time anchor.)

#### Scenario: Same-direction press extends one candidate further

- GIVEN a world-aligned start, `snap_anim` at `applied = 0.40 rad` toward `PI/2`
- WHEN `KeyCode::KeyQ` `just_pressed` fires again
- THEN the new target MUST be the next candidate past `PI/2` (here `PI`)
- AND the animation MUST resume from `0.40 rad`, not restart

#### Scenario: Opposite-direction press reverses to nearest stop behind

- GIVEN a world-aligned start, `snap_anim` at `applied ≈ 0.47 rad` toward `PI/2`
- WHEN `KeyCode::KeyE` `just_pressed` fires
- THEN the animation MUST retarget the next candidate clockwise from current up (here `0.0`)
- AND on `finished`, `transform.up()` MUST be within ε of that candidate
