# SDD Workflow — `voxel-framework`

> Spec-Driven Development with AI, applied to this repo. Read this once before contributing a substantial change. For trivial work, see [`conventions.md`](./conventions.md) — most fixes don't need this flow.

## TL;DR

```
substantial change?
  ├── no  → write the code, open a PR, done.
  └── yes → /sdd-new <name>
            ├── proposal.md     (intent, scope, tradeoffs)
            ├── spec.md         (Given/When/Then + RFC 2119)
            ├── design.md       (architecture, sequence diagrams, decisions)
            ├── tasks.md        (numbered checklist)
            ├── /sdd-apply      (implement, mark [x])
            ├── /sdd-verify     (run cargo test, compare against spec)
            └── /sdd-archive    (merge delta into openspec/specs/, bump README/CHANGELOG if needed)
```

## Backend mode: hybrid

This project uses **hybrid** persistence:

- **Files** (`openspec/`) — versioned in git, visible in PR diffs. The reviewer can read the spec the same way they read the code.
- **Engram** — observations under topic keys `sdd-init/voxel-framework` and `sdd/<change>/<artifact>`. Survives context compaction; recoverable across sessions with `mem_search` + `mem_get_observation`.

Both are kept in sync by the SDD skills. You shouldn't need to touch engram directly.

## Phase reference

| Phase | Skill | Reads | Writes (file) | Writes (engram) |
|---|---|---|---|---|
| explore | `sdd-explore` | nothing | `openspec/changes/<c>/exploration.md` (optional) | `sdd/<c>/explore` |
| propose | `sdd-propose` | exploration (if exists) | `openspec/changes/<c>/proposal.md` | `sdd/<c>/proposal` |
| spec | `sdd-spec` | proposal | `openspec/changes/<c>/specs/<cap>/spec.md` | `sdd/<c>/spec` |
| design | `sdd-design` | proposal | `openspec/changes/<c>/design.md` | `sdd/<c>/design` |
| tasks | `sdd-tasks` | spec + design | `openspec/changes/<c>/tasks.md` | `sdd/<c>/tasks` |
| apply | `sdd-apply` | tasks + spec + design | updates `tasks.md` (marks `[x]`) | `sdd/<c>/apply-progress` |
| verify | `sdd-verify` | spec + tasks | `openspec/changes/<c>/verify-report.md` | `sdd/<c>/verify-report` |
| archive | `sdd-archive` | all artifacts | moves change folder; merges delta into `openspec/specs/<cap>/` | `sdd/<c>/archive-report` |

Slash commands map to these skills directly. `/sdd-ff` runs propose → spec → design → tasks in sequence; `/sdd-continue` picks up wherever the dependency chain has the next gap.

## Closing the loop on docs (drift prevention)

When a change with **public-API impact** is archived:

1. The accepted delta-spec is merged into `openspec/specs/<capability>/spec.md`.
2. `README.md` Quick Start section is updated if any usage example changed (plugin construction, feature flags, exposed types).
3. `CHANGELOG.md` gets a new entry describing the user-visible change.
4. If the change introduces or alters an invariant, `docs/multiplayer-readiness.md` (or any other audited doc) is checked and updated.

`CLAUDE.md` is **agent-facing** — it's updated only when the conventions or build commands change, not on every feature.

## Recovering after compaction or new session

If your session was compacted or you're picking up a partial change:

```
mem_search "sdd/<change-name>/state"     # find DAG state observation
mem_get_observation <id>                 # full state.yaml content
```

Or just read the local files: `openspec/changes/<change-name>/` is the same view, persisted to disk.

## Why this exists

`voxel-framework` is a **library** — every public surface change ripples downstream (Starframe, future games). We've already had drift between `README.md`, `CLAUDE.md`, and the code (e.g., 3-LOD vs 2-LOD wording, stale plugin usage examples). The SDD flow exists to make that drift impossible: the change can't be archived without the spec, and the README is regenerated from the accepted spec.

## Reference

- Backend file convention: `~/.claude/skills/_shared/openspec-convention.md`
- Engram naming: `~/.claude/skills/_shared/engram-convention.md`
- What counts as "substantial" for this repo: [`conventions.md`](./conventions.md)
- Project context (auto-detected): `openspec/config.yaml`
- Engram topic for project context: `sdd-init/voxel-framework`
