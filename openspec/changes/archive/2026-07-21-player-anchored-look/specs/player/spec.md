# Delta for Player — Anchored Look Mode

Change: `player-anchored-look`. Base spec: `openspec/specs/player/spec.md` (v2).
Untouched requirements (NOT duplicated here): Continuous Roll Is Default and
Preserved, World-Aligned Lattice Definition, Snap Mode Is Opt-In via PlayerConfig,
Cancel-and-Restart Is Lattice-Aware, Easing and Duration Are Configurable, Roll Is
Disabled in Grounded Mode.

Baseline invariants: the 169 existing lib tests MUST stay green unedited. The
`LookMode::SixDof` FreeFlight path MUST be semantically unchanged (re-indentation
allowed). `PlayerMode::Grounded` MUST be completely unaffected by `LookMode`.

## ADDED Requirements

### Requirement: Anchored Look Mode Is Opt-In

A `LookMode` enum (`SixDof` = default, `Anchored`) SHALL exist on `PlayerConfig`
and `Player`, MUST be copied config → component by `spawn_player`, and MUST be
re-exported from the prelude. Defaults MUST yield `LookMode::SixDof`, so existing
consumers are byte-compatible with zero code changes.

#### Scenario: Default preserves 6DOF behavior

- GIVEN a game that never sets `look_mode`
- WHEN a player is spawned and FreeFlight look runs
- THEN `look_mode == LookMode::SixDof`
- AND mouse look, roll, and snap roll behave exactly per spec v2

#### Scenario: Spawn copies non-default look mode

- GIVEN `PlayerConfig { look_mode: LookMode::Anchored, ..default() }`
- WHEN `spawn_player` produces a `Player`
- THEN `Player::look_mode == LookMode::Anchored`

### Requirement: Anchored FPS Look in FreeFlight

In `PlayerMode::FreeFlight` with `LookMode::Anchored`: yaw MUST be a world-space
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

### Requirement: RollMode Is Inert in Anchored Look

When `LookMode::Anchored` is active in FreeFlight, `RollMode` MUST have no
effect: Q/E mean anchor switch regardless of `roll_mode`; neither continuous roll
nor roll-only snap SHALL occur. This interaction SHALL be documented in the
rustdoc of both enums.

#### Scenario: Continuous roll suppressed in Anchored

- GIVEN Anchored with `roll_mode = RollMode::Continuous`
- WHEN `KeyQ` is held across frames
- THEN no continuous roll occurs
- AND only the initial `just_pressed` starts one anchor switch

#### Scenario: Grounded ignores LookMode

- GIVEN `mode = PlayerMode::Grounded` with `look_mode = LookMode::Anchored`
- WHEN look and Q/E input arrive
- THEN behavior is identical to spec v2 Grounded (Y-anchored look, no roll)
