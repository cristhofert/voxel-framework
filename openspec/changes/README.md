# `openspec/changes/` — Active SDD Changes

Each subdirectory here is a **change in flight**: an SDD artifact bundle for one substantial modification to `voxel-framework`. When the change is implemented, verified, and accepted, it gets archived (moved to `archive/YYYY-MM-DD-<change-name>/`) and its delta-spec gets merged into `../specs/`.

## When to create a change

Create a change when the work is **substantial** under `docs/sdd/conventions.md`. Bug fixes, typos, internal refactors, and doc-only updates do **not** need a change — go straight to a PR.

## How to create one

Use the SDD slash commands (they invoke skills that handle the file layout for you):

```
/sdd-new <change-name>     # creates proposal.md
/sdd-ff                    # fast-forwards through propose → spec → design → tasks
/sdd-apply                 # implements tasks in batches
/sdd-verify                # validates against spec scenarios
/sdd-archive               # moves to archive/, syncs main specs and public docs
```

Each step also persists to engram (topic key `sdd/<change-name>/<artifact>`), so context survives session compaction.

## Layout per change

```
openspec/changes/<kebab-name>/
├── state.yaml              # DAG state (orchestrator, recovery)
├── proposal.md             # intent, scope, tradeoffs, rollback
├── specs/<capability>/spec.md   # delta specs (Given/When/Then + RFC 2119)
├── design.md               # architecture, sequence diagrams, decisions
├── tasks.md                # numbered checklist, marked [x] by /sdd-apply
└── verify-report.md        # /sdd-verify output
```

Naming: `kebab-case`, descriptive, no version numbers. Examples: `add-block-state-metadata`, `region-generator-disambiguation`, `multiplayer-ready-snapshots`.

## Reference

- Workflow: [`../../docs/sdd/workflow.md`](../../docs/sdd/workflow.md)
- What counts as substantial: [`../../docs/sdd/conventions.md`](../../docs/sdd/conventions.md)
- Project context: `openspec/config.yaml`
- Archived changes: [`archive/`](archive/)
