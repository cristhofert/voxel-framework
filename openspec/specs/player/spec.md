# Player Capability — Snap Roll Mode (v1)

This spec defines the player roll behavior spanning `Player` (`src/player/mod.rs`),
`PlayerConfig` (`src/player/mod.rs`), and re-exports in `src/prelude.rs`. It locks
the continuous-roll default, the opt-in snap-roll mode with easing animation, the
cancel-and-restart lattice semantics, configurable duration/easing, and grounded-mode
roll suppression. Behavior described here MUST hold for any player spawned via
`spawn_player`.

Source of truth: this file. Last updated by archive of `player-snap-roll-mode` on 2026-06-27.

## Requirements

### Requirement: Continuous Roll Is Default and Preserved

`Player::default()` MUST set `roll_mode == RollMode::Continuous`. When
`RollMode::Continuous` is active in `PlayerMode::FreeFlight`, holding `KeyCode::KeyQ`
or `KeyCode::KeyE` SHALL rotate the player around local Z at `Player::roll_speed`
rad/s, scaled by frame delta (frame-rate independent). The continuous-mode code
path MUST be byte-for-byte unchanged so existing consumers observe no delta.

#### Scenario: Default mode is continuous

- GIVEN `let p = Player::default();`
- WHEN `p.roll_mode` is read
- THEN it MUST equal `RollMode::Continuous`

#### Scenario: Holding Q rolls continuously in free flight

- GIVEN a `Player` in `PlayerMode::FreeFlight` with `roll_speed = 1.5` and `RollMode::Continuous`
- WHEN `KeyCode::KeyQ` is held for `0.1s` of accumulated `time.delta_secs()`
- THEN local Z rotation MUST advance by `0.15` radians (`1.5 * 0.1`)
- AND no `SnapRollAnim` SHALL be created on `Player::snap_anim`

### Requirement: Snap Mode Is Opt-In via PlayerConfig

Setting `PlayerConfig::roll_mode = RollMode::Snap` before `spawn_player` SHALL
make the spawned `Player` use snap mode. In `PlayerMode::FreeFlight`,
`KeyCode::KeyQ` `just_pressed` SHALL start an animation rotating local Z by
`+PI/2` over `Player::snap_roll_duration_secs` using the configured
`SnapRollEasing`. `KeyCode::KeyE` `just_pressed` SHALL do the same with `-PI/2`.
While the animation runs, `keys.pressed` (held) MUST NOT trigger additional
rotations — only `just_pressed` events count.

#### Scenario: Q press in snap mode starts +90 animation

- GIVEN `PlayerConfig { roll_mode: RollMode::Snap, ..default() }` and a spawned `Player` in `PlayerMode::FreeFlight`
- WHEN `KeyCode::KeyQ` `just_pressed` fires on a frame
- THEN `Player::snap_anim` MUST be `Some(SnapRollAnim { sign: +1, .. })` with `target_angle == PI/2`

#### Scenario: Held key does not stack rotations

- GIVEN a snap-mode `Player` with an in-progress `snap_anim` toward `+PI/2`
- WHEN `KeyCode::KeyQ` is reported by `keys.pressed` (held) but NOT `just_pressed`
- THEN no new animation SHALL start
- AND `target_angle` SHALL remain `PI/2`

### Requirement: Cancel-and-Restart Is Lattice-Aware

If a snap animation is in progress and a `just_pressed` event fires for the SAME
direction, the animation SHALL retarget so the new target equals
`anchor + (completed_quadrants + 1) * sign * PI/2`, restarting from the current
applied angle (not from anchor). If `just_pressed` fires for the OPPOSITE
direction, it SHALL cancel and snap toward the nearest 90 deg lattice point below
the current applied angle in the new direction. Across any sequence of consecutive
snaps, local Z roll MUST land on a `PI/2` multiple once `finished == true`.

#### Scenario: Same-direction press extends by 90 from anchor

- GIVEN `snap_anim` in progress at `applied = 0.40 rad` toward `target = PI/2` with `sign = +1`
- WHEN `KeyCode::KeyQ` `just_pressed` fires again
- THEN the new `target_angle` MUST equal `PI` (two quadrants from anchor)
- AND the animation MUST resume from `applied = 0.40 rad`, not from `0.0`

#### Scenario: Opposite-direction press snaps to nearest lattice below

- GIVEN `snap_anim` in progress at `applied ≈ 0.47 rad` (~27 deg) heading to `+PI/2`
- WHEN `KeyCode::KeyE` `just_pressed` fires
- THEN the animation MUST retarget to `0.0` from anchor (nearest 90 deg lattice point at-or-below current applied angle in the negative direction)
- AND on `finished`, the local Z rotation delta from anchor MUST be exactly a multiple of `PI/2`

### Requirement: Easing and Duration Are Configurable

`PlayerConfig::snap_roll_duration_secs` (`f32`, default `0.18`) SHALL control the
animation duration in seconds. `PlayerConfig::snap_roll_easing` (`SnapRollEasing`:
`EaseOutCubic`, `EaseOutQuart`, `Smoothstep`, `Linear`; default `EaseOutCubic`)
SHALL control the curve. Both SHALL be copied from `PlayerConfig` to `Player`
during `spawn_player` and SHALL be readable from `player_look` without an
additional `Res<PlayerConfig>`. The default `0.18s` SHOULD feel snappy yet smooth;
values outside `(0.0, 1.0)` MAY be supported but stability is unspecified.

#### Scenario: Defaults are documented values

- GIVEN `let cfg = PlayerConfig::default();`
- WHEN `cfg.snap_roll_duration_secs` and `cfg.snap_roll_easing` are read
- THEN they MUST equal `0.18` and `SnapRollEasing::EaseOutCubic` respectively

#### Scenario: Spawn copies easing config onto Player

- GIVEN `PlayerConfig { snap_roll_easing: SnapRollEasing::Smoothstep, snap_roll_duration_secs: 0.25, ..default() }`
- WHEN `spawn_player` produces a `Player` component
- THEN `Player::snap_roll_easing == SnapRollEasing::Smoothstep`
- AND `Player::snap_roll_duration_secs == 0.25`

### Requirement: Roll Is Disabled in Grounded Mode

When `Player::mode == PlayerMode::Grounded`, neither `RollMode::Continuous` nor
`RollMode::Snap` SHALL produce any roll rotation, regardless of `KeyCode::KeyQ` or
`KeyCode::KeyE` input. This preserves parity with the current Grounded-mode
behavior at `src/player/mod.rs:585`.

#### Scenario: Grounded continuous ignores Q/E

- GIVEN a `Player` with `mode = PlayerMode::Grounded` and `roll_mode = RollMode::Continuous`
- WHEN `KeyCode::KeyQ` is held for any duration
- THEN local Z rotation MUST NOT change

#### Scenario: Grounded snap ignores Q/E

- GIVEN a `Player` with `mode = PlayerMode::Grounded` and `roll_mode = RollMode::Snap`
- WHEN `KeyCode::KeyQ` `just_pressed` fires
- THEN `Player::snap_anim` MUST remain `None`
- AND local Z rotation MUST NOT change
