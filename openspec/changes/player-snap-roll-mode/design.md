# Design: Player Snap Roll Mode

## Technical Approach

Add an OPT-IN snap-roll mode to the existing 6DOF player controller. The current
`player_look` system (`src/player/mod.rs:534-588`) integrates Q/E roll continuously
while keys are held; the change introduces an alternate branch driven by
`just_pressed` events that animates the local-Z rotation toward the next 90 deg
lattice point with an easing curve.

All animation state lives on `Player` (runtime-only field, not persisted). All
math lives in three pure helpers (`apply_easing`, `start_or_retarget_snap`,
`advance_snap`) — mirrors the `compute_effective_break_time` pattern from the
archived `tool-material-system` change. No new crates, no new plugins, no
schedule changes, no `PlayerMode` variants. `PlayerConfig` gains three additive
fields wired into `Player` on spawn.

## Architecture Decisions

### Decision: Animation state on `Player`, not a separate component

| Option | Tradeoff | Decision |
|---|---|---|
| `pub(crate) snap_anim: Option<SnapRollAnim>` field on `Player` | Single `Query<(&Player, &mut Transform)>` already used in `player_look`; no extra mutable borrow | Chosen |
| New `SnapRollState` component | Forces second `Query` and a spawn-time insert; breaks symmetry with `velocity` / `vertical_velocity` runtime fields | Rejected — over-engineered |
| `Resource` keyed by entity | Wrong scope (single-player); resource churn for an inherent player property | Rejected |

**Rationale**: `Player` already carries runtime fields (`velocity`, `vertical_velocity`,
`impact_speed`, `footstep_timer`). `snap_anim` is the same shape — per-entity transient
state. `pub(crate)` prevents accidental external mutation.

### Decision: Inline easing, no `bevy_easings` dependency

| Option | Tradeoff | Decision |
|---|---|---|
| Inline `match` over `SnapRollEasing` returning `f32` | Zero deps; trivial code; matches existing inline lerp at `mod.rs:654,704` | Chosen |
| `bevy_easings` crate | New transitive dep; Cargo.toml has `default-features = false` + a curated feature list — adding for 4 trivial curves is over-budget | Rejected |

**Rationale**: Each curve is one expression (`1.0 - (1.0 - t).powi(3)` for ease-out cubic).
Project already uses inline lerp for translation smoothing.

### Decision: Opt-in via runtime `RollMode` enum, not a Cargo feature

| Option | Tradeoff | Decision |
|---|---|---|
| `pub enum RollMode { Continuous, Snap }` field on `PlayerConfig` / `Player` | Single `match` arm cost in the `Continuous` branch; downstreams can choose per-instance | Chosen |
| `#[cfg(feature = "snap-roll")]` | Fragments build matrix; recompile required to switch; no per-game flexibility | Rejected |

**Rationale**: Cost in the default path is one branch. Lets a single binary
support both control schemes (e.g., menu toggle).

### Decision: `just_pressed` for snap, lattice-aware reverse for opposite direction

| Option | Tradeoff | Decision |
|---|---|---|
| `just_pressed` discrete trigger | Standard for non-repeating UX; one snap per key press | Chosen |
| `pressed` (held) | Auto-fires every frame → chain-snap with no human gating | Rejected |

For mid-animation opposite-direction press (e.g., snapping CCW, user presses E):

| Option | Tradeoff | Decision |
|---|---|---|
| Snap to nearest 90 deg lattice point BELOW `current_applied` in new direction | Preserves invariant "completed snaps land on multiples of PI/2 from anchor" | Chosen |
| Reverse to `start_angle` (undo) | Loses progress — feels like a glitch | Rejected |
| Stack -PI/2 from `current_applied` raw | Final angles drift off-grid; breaks lattice | Rejected |

**Rationale**: User selected lattice-aware reverse during planning. Tests #3 and #4
enforce it.

## Data Flow

Per frame, in `RollMode::Snap`:

```
keys.just_pressed(Q/E)
  -> start_or_retarget_snap(snap_anim, dir, current_z, duration, easing)
     -> SnapRollAnim { start_angle, target_angle, current_applied, elapsed=0, easing, duration }
  -> player.snap_anim = Some(anim)

every Update tick (when snap_anim.is_some()):
  -> advance_snap(&mut anim, dt)
     -> (delta_radians_this_frame, finished_bool)
  -> transform.rotate_local_z(delta_radians)
  -> if finished: snap exact target, then snap_anim = None
```

`Continuous` branch is unchanged — same `transform.rotate_local_z(roll * roll_speed * dt)`.

## File Changes

| File | Action | Description |
|------|--------|-------------|
| `src/player/mod.rs` | Modify | Add `RollMode`, `SnapRollEasing`, `SnapRollAnim`, three helpers, `Player` field `snap_anim`, three `PlayerConfig` fields, branch in `player_look`, 5 unit tests |
| `src/prelude.rs` | Modify | Re-export `RollMode`, `SnapRollEasing` alongside `PlayerConfig` |
| `src/player/persistence.rs` | Verify | No change — `PlayerSaveData` only persists spawn position; new fields stay runtime-only |
| `examples/basic_world.rs` | Modify (archive phase) | Update controls comment to mention snap-mode availability |
| `README.md`, `CHANGELOG.md`, `docs/multiplayer-readiness.md` | Modify (archive phase) | Doc deltas: feature mention, changelog entry, multiplayer note that only `target_angle` is authoritative |

## Interfaces / Contracts

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RollMode {
    #[default]
    Continuous,
    Snap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SnapRollEasing {
    #[default]
    EaseOutCubic,
    EaseOutQuart,
    Smoothstep,
    Linear,
}

#[derive(Debug, Clone)]
pub(crate) struct SnapRollAnim {
    pub start_angle: f32,      // cumulative roll at anim start (rad)
    pub target_angle: f32,     // cumulative roll at anim end (rad)
    pub current_applied: f32,  // cumulative roll already written to Transform
    pub elapsed: f32,          // seconds since anim start
    pub duration: f32,         // total seconds
    pub easing: SnapRollEasing,
    pub direction: i32,        // +1 (CCW) | -1 (CW); for retarget logic
}

pub(crate) fn apply_easing(t: f32, e: SnapRollEasing) -> f32;
pub(crate) fn start_or_retarget_snap(
    existing: Option<SnapRollAnim>,
    dir: i32,
    current_local_z: f32,
    duration: f32,
    easing: SnapRollEasing,
) -> SnapRollAnim;
pub(crate) fn advance_snap(anim: &mut SnapRollAnim, dt: f32) -> (f32, bool);
```

`PlayerConfig` gains `roll_mode: RollMode`, `snap_roll_duration_secs: f32` (default `0.18`),
`snap_roll_easing: SnapRollEasing`. `Player` gains the same three plus
`pub(crate) snap_anim: Option<SnapRollAnim>` (default `None`).

## Testing Strategy

| Layer | What to Test | Approach |
|-------|-------------|----------|
| Unit | `snap_roll_completes_to_90deg` — full anim sums to ~PI/2 | Loop `advance_snap` for `duration / dt` ticks; assert cumulative ≈ PI/2 |
| Unit | `snap_roll_easing_monotonic` — ease-out delta non-increasing | Collect deltas; assert `deltas[i] >= deltas[i+1]` for all `i` |
| Unit | `snap_roll_same_direction_extends_target` — Q + Q → +PI rad | Build first anim, retarget mid-animation, assert new `target_angle` |
| Unit | `snap_roll_opposite_direction_reverses_to_lattice` — Q then E lands on lattice below `current_applied` | Build CCW anim, retarget at known `current_applied`, assert target is nearest -PI/2 multiple |
| Unit | `continuous_mode_unchanged` — `Player::default().roll_mode == Continuous` and `snap_anim.is_none()` | Direct field assertion |

All tests are pure-function. No Bevy `App`, no display required. Run via `cargo test --lib`.

## Migration / Rollout

No migration. Persistence format unchanged (`player_data.json` only stores spawn position).
No public API removal — three new fields on `PlayerConfig` default to current behaviour
(`Continuous`). `Player::default().roll_mode == RollMode::Continuous` is asserted in test.
Rollback = `git revert` of the archive commit; consumers that opted in only drop the field.

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Network desync if downstream replicates raw `Transform` mid-anim | Med | `multiplayer-readiness.md` paragraph (archive phase): only `target_angle` authoritative; clients animate locally |
| Frame-rate spike → overshoot past target | Low | `t.clamp(0.0, 1.0)`; on `finished`, write exact `target_angle` and clear `snap_anim` |
| Floating-point drift over many cycles | Low | `current_applied` reset to anchor on completion; no persistent residue |
| Default flips to `Snap` accidentally in review | Low | Unit test pins `Player::default().roll_mode == Continuous` |

## Open Questions

- [ ] None. All semantics resolved during proposal. `start_or_retarget_snap` decision tree
      and `advance_snap` clamp behaviour are explicit; tests pin them.
