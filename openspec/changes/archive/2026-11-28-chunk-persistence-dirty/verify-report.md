# Verify report

Run locally on 2026-11-28, step by step with `CARGO_BUILD_JOBS=1` (the dev
machine cannot survive `scripts/ci-local.sh` in one go).

| Check | Result |
|---|---|
| `cargo fmt --check` | pass |
| `cargo test --lib` | 505 passed, 0 failed |
| `cargo test --test integration_tests` | 21 passed, 0 failed |
| `cargo test --doc -- chunk world::persistence generator mesh` | 20 passed |
| `cargo check --examples` | pass |
| `cargo check --lib --no-default-features` | pass |

Not run locally, left to CI: `--all-features`, the full doctest suite,
`cargo test --examples`, `cargo doc`, and the MSRV 1.89 lane. Clippy is not
installed on the dev machine and is not part of CI.

The new tests were written first and seen to fail against the previous
systems (7 failures: clean chunks written by autosave and unload, restored and
generated chunks dirty, restore done in the request system).

Spec coverage: every requirement in `specs/persistence/spec.md` has a unit
test in `src/chunk/storage.rs`, `src/world/persistence.rs`,
`src/mesh/systems.rs` or `src/generator/systems.rs`, and the restore, damaged
file and "generated terrain is not written" cases also run end to end in
`tests/integration_tests.rs`.

A read-only adversarial review (data loss, concurrency/API, tests/docs; each
finding re-verified) ran against the implementation. It found no data-loss or
concurrency defect. Confirmed and fixed: the integration test for "restore
does not rewrite" could pass with a rewrite (it now restores a legacy
half-length file and checks the length is unchanged); a short frame budget in
the lifecycle test; stale text in `docs/architecture.md` and the `world`
module table; a duplicated `### Changed` heading in the changelog.

Not verified: the effect on load time and frame pacing in a real window. No
measurement was taken; the downstream report that motivated the change was not
profiled either.

Known limits: direct writes to the public `Chunk::states` array are not
tracked; state-only edits are saved on unload but not at app exit; untouched
terrain follows generator or seed changes; without a `GeneratorResource` the
restore is still synchronous.
