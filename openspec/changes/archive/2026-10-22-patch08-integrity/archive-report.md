# Archive Report: patch08-integrity

**Status:** success  
**Phase:** sdd-archive  
**Backend:** tracked OpenSpec artifacts  
**Archive date:** 2026-10-22  
**Verification:** PASS; 23/23 tasks complete; 10/10 GitHub Actions jobs passed on the final candidate commit.

## Summary

The verified 0.8.1 inventory, crafting, and inventory persistence change was integrated into `dev`, its accepted specifications were synchronized into `openspec/specs/`, and the complete change directory was archived.

Tested and integrated commit: `84cde5455a69788d0741f8cba5e67d64f71d4e85`.

## Specification sync

| Capability | Action |
|---|---|
| Items | Added compatible atomic exchange and representational integrity requirements to the existing capability spec |
| Crafting | Created the accepted crafting capability spec |
| Inventory persistence | Created the accepted JSON persistence capability spec |
| Candidate verification | Created the accepted exact-candidate verification spec |

README and CHANGELOG already contained the accepted 0.8.1 public behavior and release notes, so no additional product documentation edit was required during archive.

## Verification evidence

- Linux exact-candidate `./scripts/release-preflight.sh 0.8.1`: PASS
- Native Windows inventory persistence: PASS
- Rust 1.89.0 checks and focused tests: PASS
- Stable default, no-default-features, all-features, examples, docs, and package audit: PASS
- Candidate GitHub Actions run: https://github.com/cristhofert/voxel-framework/actions/runs/39860779323
- Final documentation commit GitHub Actions run: https://github.com/cristhofert/voxel-framework/actions/runs/39860779323

The full evidence and known non-blocking warnings remain in `verify-report.md`.

## Archive contents

The complete active directory moved from:

`openspec/changes/patch08-integrity/`

to:

`openspec/changes/archive/2026-10-22-patch08-integrity/`

It retains the proposal, design, delta specifications, task checklist, historical apply progress, final verification report, state, and this archive report. Historical pending statements in `apply-progress.md` are preserved as provenance and superseded by `verify-report.md`, the archived state, and this report.

## Result

The SDD cycle is complete and there is no remaining implementation or verification work for `patch08-integrity`. Tagging `v0.8.1`, publishing to crates.io, and creating a GitHub Release remain separate human-controlled release operations.
