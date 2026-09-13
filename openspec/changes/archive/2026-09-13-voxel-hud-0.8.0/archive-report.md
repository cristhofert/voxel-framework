# Archive Report: voxel-hud-0.8.0

**Status:** success  
**Phase:** sdd-archive  
**Backend:** HYBRID  
**Archive date:** 2026-09-13 (requested environment date)  
**Verification:** PASS; 19/19 tasks complete; 10/10 scenarios compliant; no pending blockers.  
**Tested product:** `799ab9130ce8e19d30d84da02a1da2682b31cd5d` on `codex/voxel-hud-0.8.0`.

## Summary

**What**: Synced the accepted HUD specification and archived the complete change, with archive report and DAG state persisted to OpenSpec and Engram.
**Why**: Requested closure after verified completion, without changing the tested release product.
**Where**: `/home/cris/Projects/voxel-framework/openspec/changes/archive/2026-09-13-voxel-hud-0.8.0` and `/home/cris/Projects/voxel-framework/openspec/specs/hud/spec.md`.
**Learned**: OpenSpec is excluded from the package; archive-only changes do not alter the tested product or existing package. This is not a new clean-tree release gate.

## Prerequisites verified before archive

- Full filesystem artifacts read and compared against full Engram observations: all eight equivalent (ignoring surrounding whitespace).
- Formal verify-report PASS and state approval true; all 19 numbered tasks checked, pending empty, all 10 scenarios compliant, blockers empty.
- Actual checkout branch and full HEAD match the verified candidate. Initial dirty paths are only the four excluded OpenSpec progress/report artifacts.
- Existing clean-preflight log SHA256 matches accepted evidence, includes exit 0 and no package dirty override. Supplemental publish dry-run successful retry and no-default rustdoc logs include exit 0.
- No gate rerun, compilation, actual publication, Git write, or product edit performed in this phase.
- Archive rule for public docs assessed: README and dated 0.8.0 CHANGELOG already contain the accepted HUD API, composition, scheduling, MSRV and reload notes. Explicit OpenSpec-only authorization prohibits further edits; existing nonblocking drift is preserved.

## Specification sync

| Capability | Action | Requirements | Scenarios |
|---|---|---:|---:|
| HUD | Created main spec from complete accepted spec, byte-for-byte | 7 added, 0 modified, 0 removed | 10 |

[Accepted HUD spec](../../../specs/hud/spec.md) matches [archived merge source](specs/hud/spec.md), SHA256 `44fa8ac66ad472d2b98e713e504584e8e044ecb9acfac15747fac83b77b9c886`.
Existing items/player specs and previous archives are untouched. Merge is non-destructive; no removal warning required.

## Archive contents and relocation

The entire directory moved:
`/home/cris/Projects/voxel-framework/openspec/changes/voxel-hud-0.8.0/`
→ `/home/cris/Projects/voxel-framework/openspec/changes/archive/2026-09-13-voxel-hud-0.8.0/`.

- [Exploration](exploration.md)
- [Proposal](proposal.md)
- [HUD specification](specs/hud/spec.md)
- [Design](design.md)
- [Tasks](tasks.md) — 19/19
- [Apply progress](apply-progress.md)
- [Formal verification report](verify-report.md) — PASS, 10/10
- [Archived DAG state](state.yaml)
- This archive report

All seven original Markdown artifacts are preserved byte-for-byte, including historical pending claims explicitly superseded by the final PASS reconciliation. State alone advances from verify to archive, sets archive-report true and records archive metadata, preserving verification/task evidence.
The active change path no longer exists. Historical absolute active paths in the verification report identify where that verifier wrote artifacts; resolve them using the above directory mapping. They are provenance, not current links, and were not rewritten. Artifact-relative links remain valid; this report links to current paths. Evidence paths under /tmp remain original external evidence locations, not durable archive attachments.

## Engram lineage

All full observations retrieved, not search previews:

| Canonical topic suffix under `sdd/voxel-hud-0.8.0/` | Observation ID |
|---|---:|
| explore | 709 |
| proposal | 712 |
| spec | 716 |
| design | 718 |
| tasks | 723 |
| apply-progress | 728 |
| verify-report | 786 |
| state | 713 (updated to archive) |
| archive-report | This report, canonical topic upsert |

Archive report and updated state use their existing canonical topic keys in project `voxel-framework`. Earlier planning/verification topics remain intact.

## Tested-product and package boundary

Manifest `/home/cris/Projects/voxel-framework/Cargo.toml` excludes `openspec/**`, covering accepted specs and dated archive/report/state.
Existing package `/home/cris/Projects/voxel-framework/target/package/voxel-framework-0.8.0.crate`: 121 files; no OpenSpec members; VCS names tested HEAD without dirty flag. All 119 locally present non-generated package members (including original manifest and local lock) byte-match the package.
Package SHA256: `5385944d6626b6be85fcdf703e5620015e5b8bcf735e278c5dde3821b9784d6c`.
HUD SHA256: `eac2e1ac931100249dee26bf360445bf31fa5606c502fa32aa3e136e8b8aad08`.

Evidence retained:
- `/tmp/voxel-0.8.0-799ab91-clean-preflight.log`: 1427 lines; exit 0; SHA256 `6df2a696887e1535a3552014de410daca834fa452d5d4999bdddd07b1b9da36b`.
- `/tmp/voxel-0.8.0-799ab91-publish-dry-run.log`: successful escalated retry exit 0 after initial DNS failure; upload aborted by dry run.
- `/tmp/voxel-0.8.0-799ab91-doc-no-default-features.log`: exit 0.
- `/tmp/voxel-hud-final-799ab91/`: original verification audits.

Archive integrity validation: byte-identical spec sync; preserved original Markdown hashes; complete archive; active path absent; Markdown targets resolve; state parses with retained PASS; `git diff --check` passes; changes only under OpenSpec; HEAD unchanged; package hash unchanged.
Git worktree is naturally dirty in excluded artifacts due to sync/relocation. Clean-gate evidence refers to the unchanged tested product at799ab91, not a clean post-archive checkout or future commit.

## Residual risks

No new archive blockers. Existing verification limits remain: broader layout/DPI/overlay/font/GPU/window/platform coverage, file-watcher/multi-source asset behavior, unexecuted remote CI, local ignored lockfile and exact Rust1.89 Linux default/no-default library scope, and pre-existing documentation drift. Three ignored profile doctests are non-HUD. See the preserved verification report for full qualifications.
External /tmp evidence can be removed independently; this archive preserves references and reported checks, not copies of those logs/screenshots.

## Next release operations pending

No next SDD phase: cycle complete.
Owner-authorized archive review/commit, integration/merge and any push remain pending and were not performed. Tag `v0.8.0`, actual crates.io publication and release announcement remain pending, subject to the owner's release workflow and explicit authorization. Revalidate the exact candidate if product/dependency inputs change; do not transfer clean799ab91 claims to a later commit automatically.
No commit/push/merge/tag/publish or source/docs/config/Cargo/version edits performed.
