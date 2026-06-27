# Proposal: Player Snap Roll Mode

## Intent

The player module rolls continuously while Q/E are held (`transform.rotate_local_z` at `src/player/mod.rs:560-566`, 1.5 rad/s default). Some downstreams (Starframe-style ship controls) want discrete 90 degree snaps with eased animation for a "click-to-orient" feel. Add this as an OPT-IN mode without changing the default behavior or breaking the existing public API.

## Scope

### In Scope
- `pub enum RollMode { Continuous, Snap }` — default `Continuous`.
- `pub enum SnapRollEasing { EaseOutCubic, EaseOutQuart, Smoothstep, Linear }` — default `EaseOutCubic`.
- New `PlayerConfig` fields: `roll_mode`, `snap_roll_duration_secs` (default 0.18), `snap_roll_easing`.
- `pub(crate) snap_anim: Option<SnapRollAnim>` on `Player` (runtime only, not serialized).
- Cancel-and-restart semantics: same direction extends by 90 deg from anchor; opposite direction snaps to nearest 90 deg lattice below current angle in the new direction.
- Roll disabled in `PlayerMode::Grounded` (parity with current).
- 5 unit tests on pure helpers (`apply_easing`, `start_or_retarget_snap`, `advance_snap`).

### Out of Scope
- Snap for yaw/pitch.
- Q/E rebinding (separate input-bindings capability).
- Persisting `roll_mode` per save.
- Other player aspects (jetpack physics, capsule collision, health/respawn) — future changes flesh out the `player` capability.

## Approach

Pure-function helpers in `src/player/mod.rs` (mirrors the `compute_effective_break_time` pattern from the `tool-material-system` SDD). `PlayerMode` enum is UNCHANGED — adding a variant would break downstream pattern matchers. `player_look` (`src/player/mod.rs:534-588`) gains a branch on `player.roll_mode`. Easing is implemented inline (no `bevy_easings` dependency).

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `src/player/mod.rs` | Modified | 2 enums, `SnapRollAnim` struct, 4 `Player` fields, 3 `PlayerConfig` fields, 3 helpers, branch in `player_look`, 5 tests |
| `src/prelude.rs:123-135` | Modified | Re-export `RollMode`, `SnapRollEasing` |
| `src/player/persistence.rs` | Verify | Confirm new fields don't break save format |
| `examples/basic_world.rs:3-4` | Modified (archive) | Update controls comment |
| `README.md`, `CHANGELOG.md`, `docs/multiplayer-readiness.md` | Modified (archive) | Doc deltas |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Mid-animation network desync (Starframe replicates raw `Transform`) | Med | Note in `multiplayer-readiness.md`: replicate post-animation; clients animate locally; only `target_angle` is authoritative |
| Frame-rate spike overshoots target | Low | `advance_snap` clamps `elapsed >= duration` and applies remaining delta exactly |
| Floating-point drift across many snaps | Low | Anchor resets on completion; no persistent accumulator |
| Default flips to `Snap` accidentally in review | Low | Test asserts `Player::default().roll_mode == RollMode::Continuous` |

## Rollback Plan

Single-commit `git revert <sha>` of the archive commit. Safe: no persistence schema change, no Cargo feature, no `PlayerMode` variant, all additions are additive. Consumers that opted into `RollMode::Snap` only drop the field from their `PlayerConfig` literal.

## Dependencies

None. No new crates.

## Success Criteria

- [ ] `cargo test --lib` passes (156 baseline + 5 new = 161).
- [ ] `cargo test --doc` passes (107, no regression).
- [ ] `cargo check --example basic_world` clean.
- [ ] `Player::default().roll_mode == RollMode::Continuous` (existing behavior preserved).
- [ ] In example with `RollMode::Snap`, Q/E animate to 90 deg increments with chosen easing.
- [ ] On archive, spec promoted to `openspec/specs/player/spec.md` (creates the `player` capability).
