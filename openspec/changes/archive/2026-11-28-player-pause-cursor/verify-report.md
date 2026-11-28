# Verify report

Run locally on 2026-11-28, step by step with `CARGO_BUILD_JOBS=1` (the dev
machine cannot survive `scripts/ci-local.sh` in one go).

| Check | Result |
|---|---|
| `cargo fmt --check` | pass |
| `cargo test --lib` | 500 passed, 0 failed |
| `cargo test --test integration_tests` | 20 passed, 0 failed |
| `cargo test --doc -- player ui` | 25 passed |
| `cargo check --examples` | pass |
| `cargo check --lib --no-default-features` | pass |

Not run locally, left to CI: `--all-features`, the full doctest suite,
`cargo test --examples`, `cargo doc`, and the MSRV 1.89 lane.

The new tests were written first and seen to fail against the previous code
(6 failures). Three of them reproduced the reported conflict: with the pause
transitions and `player_cursor_system` both active, Escape with a free cursor
despawned the window.

Spec coverage: every scenario in `specs/player/spec.md` has a test in
`src/player/mod.rs` or `src/ui/pause_menu.rs`. The `PlayerSystems` test checks
the set through the cursor system only; that the movement chain and
health/respawn are in the set is established by reading `PlayerPlugin::build`,
not by a test.

A read-only adversarial review (behavior and frame ordering, Rust/API
validity, tests/docs; each finding re-verified) found no behavior or
compilation defect. Confirmed and fixed: the "resume without focus" test
passed with the fix reverted (it now asserts before the cursor system can
release the cursor); the module docs still told games with an `AppState` to
skip `PlayerPlugin`; the README manual example lacked `player_cursor_system`;
"a menu button never captures" wording ignored Resume. Following a low
finding, `PlayerPlugin` no longer gates the cursor system on pause, so focus
loss releases the cursor in every state as the spec says.

Not verified: anything in a real window. OS focus events, actual pointer grab
and the look of the menu were not exercised; the tests set `Window::focused`
and `ButtonInput` by hand.

Known limits: pressing Escape and clicking the Settings button in the same
frame can leave the cursor captured in the Settings menu (the two `Paused`
systems disagree; present before this change); mining and placement input are
not gated on focus or pause; `PlayerPlugin` has no opt-out for spawn
persistence.
