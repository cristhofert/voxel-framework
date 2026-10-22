# Tasks: patch08-integrity
Implementation tasks 1.1–3.3 and verification tasks 4.1–4.5 are complete. Linux exact-candidate preflight, the complete GitHub Actions matrix, native Windows inventory persistence, MSRV Rust 1.89.0, docs, and package audit all passed. Candidate 0.8.1 is ready for separately authorized integration. Requirement IDs refer to specs. Evidence: verify-report.md. Writesets/dependencies: state.yaml.

## Local preflight — 2026-09-17
- [x] Focused `unix_socket_and_hardlinks_are_rejected_unchanged` passed locally after the test-only restricted-Unix-socket portability guard. This historical environment-portability check was followed by the completed exact-candidate gates below.

## 1. Inventory API / crafting
- [x] 1.1 src/items/inventory.rs: implement neutral error/default hook, internal ordered normalization and validate_integrity; no new bounds (I1/I2).
- [x] 1.2 src/items/inventory.rs: stage consume/produce, exact-return checks, atomic commit; delegate Component (I1).
- [x] 1.3 src/items/inventory.rs: bound add/headroom/leftovers; saturating count_of; malformed source rejection (I2).
- [x] 1.4 src/prelude.rs: reexport error under items; verify existing items wildcard export (I1).
- [x] 1.5 src/crafting/mod.rs: normalize entire recipe, ingredients-only can_craft, hook/error mappings (C1/C2).
- [x] 1.6 src/crafting/mod.rs: conservative deduplicated current-space fallback, checked totals/actual returns; stop failure; document partial mutation (C2).
- [x] 1.7 Inventory/crafting inline tests: duplicate inputs sufficient/insufficient, output duplicates/existing stacks/same-input, freed capacity and true-full unchanged layouts (I1/C1).
- [x] 1.8 Inventory/crafting inline tests: invalid shape/live stacks, zero/default/is_empty/empty recipes, recipe/projected overflow, add leftovers/count saturation; failures unchanged (I2/C1).
- [x] 1.9 Crafting tests: legacy implementor without hook, exhaustive three-error match, opted-in mock, Component parity, partial remove/add stopping and no false success (I1/C2).

## 2. Inventory persistence
- [x] 2.1 Candidate worker: Cargo.toml optional tempfile =3.27.0 and target-Windows windows-sys =0.61.2 (six features), items dep activation; resolve ignored lockfile, no MSRV assumption (P1/V1).
- [x] 2.2 src/items/persistence.rs: source/decoded-DTO alias totals InvalidInput before filesystem effects; unchanged DTO/signatures; shape/aggregate InvalidData; index-preserving normalization (P1/P2).
- [x] 2.3 src/items/persistence.rs: validate/resolve target, writable probe, preserve Permissions/link; reject dangling/nonregular/readonly/Unix hardlinks; private sibling creation (Windows protected effective-user DACL/persistent ACL readback before JSON), write_all/sync_all/persist and cleanup (P1).
- [x] 2.4 Persistence inline tests/private adapter: prefix-write/sync/replace failure and order; old bytes or absent target unchanged, no temps (P1).
- [x] 2.5 Persistence tests: roundtrip holes/layout/duplicates/capacity0, malformed JSON/missing file, short/long mismatch, zero/unknown/default/overflow (P2).
- [x] 2.6 Persistence tests: basename/nested paths, overwrite/permissions/private new file, symlink/rejected targets, concurrent whole JSON, platform-gated cases (P1).

## 3. Candidate CI / docs
- [x] 3.1 Cargo.toml/CHANGELOG.md/README.md: prepare 0.8.1, dated candidate notes and integrity/atomicity/metadata limitations; no published claim (V1).
- [x] 3.2 .github/workflows/rust.yml: retain Linux feature matrix, exact1.89 checks; add targeted Windows items-persistence tests (V1).
- [x] 3.3 scripts/release-preflight.sh/docs/releasing.md: only scoped adjustments needed for exact candidate gate; never dispatch automation (V1).

## 4. Verification
- [x] 4.1 Audit writesets/scope; scoped rustfmt only; record exact candidate snapshot and lock/toolchain evidence in verify-report.md (V1).
- [x] 4.2 Run locked default/no-default/all-features check/test, examples; Rust1.89.0 library checks all three modes plus scoped items/crafting tests (I1–V1).
- [x] 4.3 Obtain targeted Windows persistence results; inspect platform exclusions; no Linux-only readiness (P1/V1).
- [x] 4.4 Exact clean-candidate preflight0.8.1, docs --no-deps --all-features, package --list/--no-verify; no cargo build/publish (V1).
- [x] 4.5 Save verification/state in the tracked change artifacts; ready only after every exact-candidate gate; stop without integration/dev push/tag/publication.
