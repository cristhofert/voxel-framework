# Verification report: patch08-integrity

Date: 2026-10-22

## Candidate

- Branch: `codex/patch08-integrity`
- Implementation commit: `9de60dfe9f90676b24dbd801eeecb3beaa4458e5`
- Verification trigger commit: `2ddf2f1dc19dfcc4daab701b1a62d2a91305b028`
- Source tree: `2533dd4f6f2686e8ad6741c2a78b7b92dcf1dff4` (unchanged by the empty verification commit)
- Version: `0.8.1`
- Generated ignored lockfile: 546 packages, SHA-256 `59918a2f30adaf484699282ecf87fbc1af1ccbd0677324289998cad2d74b231c`

## Linux exact-candidate gate

`./scripts/release-preflight.sh 0.8.1` passed on the clean implementation snapshot with the generated lockfile.

Passed checks:

- scoped formatting audit
- stable default, no-default-features, and all-features checks for all targets
- stable default, no-default-features, and all-features tests
- examples tests
- Rust 1.89.0 library checks in all three feature modes
- Rust 1.89.0 focused inventory, persistence, and crafting tests
- focused inventory 21, persistence 17, and crafting 27 tests
- docs with all features and no dependencies
- package list and package creation without verification

Package audit: 121 files, approximately 3.7 MiB unpacked and 1.5 MiB compressed.

Non-blocking warnings:

- unused `bevy::prelude::*` import in `src/ui/hud.rs`
- rustdoc private intra-doc link to `resolved_link_target` in `src/items/persistence.rs`

## GitHub Actions and native Windows

Run: https://github.com/cristhofert/voxel-framework/actions/runs/39856678457

All 10 jobs passed on the unchanged candidate source tree:

- Windows inventory persistence (items only)
- MSRV Rust 1.89.0 library checks and focused tests
- default tests
- no-default-features tests
- all-features tests
- examples tests
- default all-target checks
- no-default-features checks
- all-features all-target checks
- docs and package audit

The native Windows job passed `cargo test --locked --lib --no-default-features --features items items::persistence::tests`, providing the required platform evidence for the Windows-specific path and metadata handling.

## Result

Every patch08-integrity verification gate passed. The 0.8.1 candidate is ready for a separately authorized integration into `dev`. No tag, publication, release, PR, or `dev` update was performed.
