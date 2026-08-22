# Player Capability — Movement, Look & Roll Modes (v4)

This spec defines the player look-and-roll behavior spanning `Player` (`src/player/mod.rs`),
`PlayerConfig` (`src/player/mod.rs`), and re-exports in `src/prelude.rs`. It locks
the continuous-roll default, the opt-in snap-roll mode with easing animation
targeting world-aligned up candidates, the world-aligned lattice definition, the
lattice-aware cancel-and-restart semantics, configurable duration/easing,
grounded-mode roll suppression, and the opt-in anchored look mode (world-axis
FPS camera with Q/E anchor switching, in which `RollMode` is inert). Behavior
described here MUST hold for any player spawned via `spawn_player`.

Source of truth: this file. Last updated by archive of `player-anchored-look` on 2026-07-21.

## Requirements

### Requirement: Continuous Roll Is Default and Preserved

`Player::default()` MUST set `mode` to `FreeFlightSixDof { roll_mode: RollMode::Continuous }`. When
`RollMode::Continuous` is active in `PlayerMode::FreeFlightSixDof`, holding `KeyCode::KeyQ`
or `KeyCode::KeyE` SHALL rotate the player around local Z at `Player::roll_speed`
rad/s, scaled by frame delta (frame-rate independent). The continuous-mode code
path MUST be byte-for-byte unchanged so existing consumers observe no delta.

#### Scenario: Default mode is continuous

- GIVEN `let p = Player::default();`
- WHEN `p.mode` is read
- THEN it MUST equal `PlayerMode::FreeFlightSixDof { roll_mode: RollMode::Continuous }`

#### Scenario: Holding Q rolls continuously in free flight

- GIVEN a `Player` in `PlayerMode::FreeFlightSixDof` with `roll_speed = 1.5` and `RollMode::Continuous`
- WHEN `KeyCode::KeyQ` is held for `0.1s` of accumulated `time.delta_secs()`
- THEN local Z rotation MUST advance by `0.15` radians (`1.5 * 0.1`)
- AND no `SnapRollAnim` SHALL be created on `Player::snap_anim`

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

### Requirement: Snap Mode Is Opt-In via PlayerConfig

Setting `PlayerConfig::mode = PlayerMode::FreeFlightSixDof { roll_mode: RollMode::Snap }` before `spawn_player` SHALL make
the spawned `Player` use snap mode. In `PlayerMode::FreeFlightSixDof`, `KeyCode::KeyQ`
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

### Requirement: Easing and Duration Are Configurable

`PlayerConfig::snap_roll_duration_secs` (`f32`, default `0.5`) SHALL control the
animation duration in seconds. `PlayerConfig::snap_roll_easing` (`SnapRollEasing`:
`EaseOutCubic`, `EaseOutQuart`, `Smoothstep`, `Linear`; default `EaseOutCubic`)
SHALL control the curve. Both SHALL be copied from `PlayerConfig` to `Player`
during `spawn_player` and SHALL be readable from `player_look` without an
additional `Res<PlayerConfig>`. The default `0.5s` SHOULD feel snappy yet smooth;
values outside `(0.0, 1.0)` MAY be supported but stability is unspecified.

#### Scenario: Defaults are documented values

- GIVEN `let cfg = PlayerConfig::default();`
- WHEN `cfg.snap_roll_duration_secs` and `cfg.snap_roll_easing` are read
- THEN they MUST equal `0.5` and `SnapRollEasing::EaseOutCubic` respectively

#### Scenario: Spawn copies easing config onto Player

- GIVEN `PlayerConfig { snap_roll_easing: SnapRollEasing::Smoothstep, snap_roll_duration_secs: 0.3, ..default() }`
- WHEN `spawn_player` produces a `Player` component
- THEN `Player::snap_roll_easing == SnapRollEasing::Smoothstep`
- AND `Player::snap_roll_duration == 0.3`

### Requirement: Roll Is Disabled in Grounded Mode

When `Player::mode == PlayerMode::Grounded`, neither `RollMode::Continuous` nor
`RollMode::Snap` SHALL produce any roll rotation, regardless of `KeyCode::KeyQ` or
`KeyCode::KeyE` input. This preserves parity with the current Grounded-mode
behavior at `src/player/mod.rs:585`.

#### Scenario: Grounded continuous ignores Q/E

- GIVEN a `Player` with `mode = PlayerMode::Grounded`
- WHEN `KeyCode::KeyQ` is held for any duration
- THEN local Z rotation MUST NOT change

#### Scenario: Grounded snap ignores Q/E

- GIVEN a `Player` with `mode = PlayerMode::Grounded`
- WHEN `KeyCode::KeyQ` `just_pressed` fires
- THEN `Player::snap_anim` MUST remain `None`
- AND local Z rotation MUST NOT change

### Requirement: Explicit Free-Flight Modes

`PlayerMode` SHALL expose complete movement experiences: `FreeFlightSixDof { roll_mode }`, `FreeFlightAnchored`, and `Grounded { gravity, jump_speed }`. A separate look-mode configuration SHALL NOT exist. `FreeFlightSixDof` is the default and owns its Q/E `RollMode`; `FreeFlightAnchored` uses Q/E for anchor switching.

#### Scenario: Default selects six-degree-of-freedom flight

- GIVEN `PlayerConfig::default()`
- WHEN a player is spawned
- THEN its mode MUST be `PlayerMode::FreeFlightSixDof { roll_mode: RollMode::Continuous }`

#### Scenario: Grounded has no look-mode cross configuration

- GIVEN `PlayerConfig { mode: PlayerMode::Grounded { gravity: 20.0, jump_speed: 8.0 }, ..default() }`
- WHEN a player is spawned
- THEN no look-mode setting is required or consulted

### Requirement: Anchored FPS Look in FreeFlight

In `PlayerMode::FreeFlightAnchored`: yaw MUST be a world-space
rotation around `Player::anchor_up`; pitch MUST be a local-X rotation whose delta
is clamped so `asin(forward · anchor_up)` stays within [−89°, +89°]; free roll
MUST NOT occur — screen-up is the projection of `anchor_up` ⊥ forward by
construction, giving zero horizon drift.

#### Scenario: Pitch clamps at the +89° pole

- GIVEN Anchored FreeFlight with forward elevated +88° toward `anchor_up`
- WHEN a mouse delta requesting +5° more pitch arrives
- THEN the applied delta is clamped so final elevation equals +89°, never beyond

#### Scenario: Pitch clamps at the −89° pole

- GIVEN forward at −88° elevation
- WHEN a delta requesting −5° arrives
- THEN final elevation equals −89°
- AND a subsequent delta AWAY from the pole passes through unclamped

#### Scenario: Horizon stays level under diagonal input

- GIVEN violent simultaneous yaw+pitch mouse deltas over many frames
- WHEN they accumulate
- THEN `camera_right · anchor_up == 0` within ε every frame (no roll accumulates)

### Requirement: Manual Anchor Switching via Q/E

In Anchored FreeFlight, `KeyQ`/`KeyE` `just_pressed` SHALL target the next lattice
candidate CCW/CW from current up per the unmodified World-Aligned Lattice
Definition (45° dominance rule, EPS_ON). The switch MUST animate via the existing
snap machinery, honoring `snap_roll_duration_secs`/`snap_roll_easing`. The
candidate's SOURCE signed world axis MUST be stored in `Player::pending_anchor`
and committed to `anchor_up` only when the animation finishes; until then mouse
look MUST stay active on the OLD anchor. Mid-animation presses follow
Cancel-and-Restart Is Lattice-Aware (extend/reverse), updating `pending_anchor`
to the new target's source axis. At spawn, `anchor_up` MUST initialize to the
nearest signed world axis to the spawn up, with deterministic tie-breaking.

#### Scenario: Q switches anchor 90° from an axial view

- GIVEN Anchored, `forward = -Z`, `anchor_up = +Y`, no animation running
- WHEN `KeyQ` `just_pressed` fires
- THEN an animation starts toward the next candidate CCW
- AND `pending_anchor` holds that candidate's source axis (a pure signed axis)
- AND on `finished`, `anchor_up == pending_anchor` and yaw now orbits it

#### Scenario: Mid-animation extend retargets pending anchor

- GIVEN an in-flight anchor-switch animation with `pending_anchor = A`
- WHEN the same key fires `just_pressed` again
- THEN the animation retargets one candidate further (per Cancel-and-Restart)
- AND `pending_anchor` becomes the new target's source axis, not `A`

#### Scenario: Mouse look during animation uses old anchor

- GIVEN an in-flight anchor-switch animation
- WHEN mouse deltas arrive
- THEN yaw rotates around the OLD `anchor_up`, not `pending_anchor`

#### Scenario: Repeated switches never denormalize the anchor

- GIVEN any sequence of completed Q/E switches
- WHEN `anchor_up` is read after each `finished`
- THEN it is always exactly one of the 6 signed world axes

### Requirement: Anchored Mode Owns Anchor Switching

When `PlayerMode::FreeFlightAnchored` is active, Q/E MUST mean anchor switching;
no roll configuration exists for this mode.

#### Scenario: Continuous roll suppressed in Anchored

- GIVEN a `Player` in `PlayerMode::FreeFlightAnchored`
- WHEN `KeyQ` is held across frames
- THEN no continuous roll occurs
- AND only the initial `just_pressed` starts one anchor switch

#### Scenario: Grounded ignores PlayerMode

- GIVEN `mode = PlayerMode::Grounded`
- WHEN look and Q/E input arrive
- THEN behavior is identical to spec v2 Grounded (Y-anchored look, no roll)
