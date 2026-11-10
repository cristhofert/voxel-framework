# Verification: player-input-focus

## Result

Automated verification passed. The maintainer accepted the framework example playtest with `anda bien` after the requested click/WASD/Escape/Alt-Tab checks. Only this framework repository was edited. No dependency or package-version changes were made; no commit or publication was performed.

## Reproduction

Before implementation, `CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 cargo test --locked --lib --no-default-features player::tests::inactive_primary_window_discards_look_without_replaying_it` exited 101. An unfocused primary window still allowed the camera to rotate and start a horizon switch: expected identity rotation, observed `Quat(-0.0839413, -0.12706362, 0.36958835, 0.9166314)`. Log: `/tmp/voxel-focus-red.log` (temporary evidence, not a durable attachment).

## Passed

- `CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 cargo test --locked --lib --no-default-features player::tests`: 39 passed, 0 failed. Includes four new focus/capture tests and existing headless movement/look/horizon logic.
- `CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 cargo test --locked --lib player::tests`: 39 passed, 0 failed.
- `CARGO_BUILD_JOBS=1 cargo check --locked --example player_focus --no-default-features`: passed.
- `CARGO_BUILD_JOBS=1 cargo check --locked --examples`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.

## Manual check and acceptance

Run `cargo run --example player_focus` on the native desktop. Confirm the cursor starts free; click the focused window to control the cube view with mouse/WASD. Escape releases input. Alt-Tab away must release capture and stop camera input; return alone must not recapture; click again must restore control without replaying inactive mouse motion. Read the logged WindowFocused and KeyboardInput events to distinguish actual key delivery from pointer capture. Test ordinary keys separately from desktop global shortcuts. Do not expect this API to suppress desktop shortcuts or force OS focus.

`cargo run --example player_focus` compiled successfully. The sandboxed launch failed with `XOpenDisplayFailed`; the authorized desktop launch ran and exited 0. A second desktop launch was supplied for the maintainer's test, followed by the response `anda bien` on 2026-11-09 (America/Montevideo). This records maintainer-reported success for the requested playtest; no independently observed per-key/focus event trace is claimed.

Synthetic Window/CursorOptions tests establish framework policy, not universal grab success. Full unrelated feature matrices, MSRV and publication gates were not run for this scoped uncommitted change. Other desktops and global shortcut interception are outside the verified scope.
