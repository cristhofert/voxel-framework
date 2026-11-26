# Verify report

Run locally on 2026-11-26, step by step with `CARGO_BUILD_JOBS=1` (the dev
machine cannot survive `scripts/ci-local.sh` in one go).

| Check | Result |
|---|---|
| `cargo fmt --check` | pass |
| `cargo test --lib` | 490 passed, 0 failed |
| `cargo test --test integration_tests` | 20 passed, 0 failed |
| `cargo test --doc mobs` | 5 passed |
| `cargo check --examples` | pass |
| `cargo check --lib --no-default-features` | pass |
| `cargo check --lib --no-default-features --features mobs` | pass |

Not run locally, left to CI: `--all-features`, the full doctest suite,
`cargo test --examples`, `cargo doc`, and the MSRV 1.89 lane.

Spec coverage: every requirement in `specs/mobs/spec.md` has a unit or
system-level test in `src/mobs/mod.rs` or `src/mobs/spawner.rs`, except
"given the same world" determinism across different chunk contents, which is
covered only for a flat floor.

A read-only adversarial review (three lenses, each finding re-verified) ran
against the first implementation. Confirmed findings fixed before this report:
ground spawns far below the player dodged the cap; mobs accumulated without
bound with two or more players (distance despawn never ran); `Range` bounds
outside `[0, 1)` gave the wrong window; zero-width ground rings never spawned;
the spawner panicked without asset stores.

Known limits: mob AI still assumes one `Player`; the cap counts mobs inside
the ring only, so mobs that wander out are replaced until distance despawn
removes them; no in-game (display) run was done.
