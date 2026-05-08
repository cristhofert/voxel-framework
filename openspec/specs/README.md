# `openspec/specs/` — Accepted Specifications

This directory is the **source of truth** for the spec'd behavior of `voxel-framework`. Each subdirectory is a **capability** (a coherent slice of public surface area), and each `spec.md` inside it states the requirements that capability must satisfy, in Given/When/Then form with RFC 2119 keywords (MUST, SHALL, SHOULD, MAY).

Specs in this directory are **accepted** — they were merged here by `/sdd-archive` after the originating change was implemented and verified.

## Capability layout (initial)

We do not pre-create empty specs. As changes get archived, capability folders fill in. Expected capabilities, mapped to `src/`:

| Capability | Module | What it covers |
|---|---|---|
| `chunk` | `src/chunk/` | Chunk layout, coords, persistence binary format |
| `mesh` | `src/mesh/` | Greedy meshing invariants, LOD thresholds, neighbor awareness |
| `generator` | `src/generator/` | `ChunkGenerator<V>` / `RegionGenerator<V>` traits, MultiGenerator dispatch |
| `world` | `src/world/` | Streaming, raycast, collision, mining, placement, occlusion |
| `render` | `src/render/` | Shared `StandardMaterial`, `ChunkMaterial`, vertex colors |
| `items` | `src/items/` | Inventory, hotbar, drops, pickup (gated behind `items` feature) |
| `crafting` | `src/crafting/` | Recipe registry, item consumption (gated behind `crafting` feature) |
| `mobs` | `src/mobs/` | Mob AI, damage, pathing (gated behind `mobs` feature) |
| `player` | `src/player/` | 6DOF jetpack, capsule collision, health/effects/respawn |
| `persistence` | `src/world/persistence.rs`, `src/items/persistence.rs` | World/item save format compatibility |
| `audio` | `src/audio/` | `SfxPlugin`, `SfxEvent`, `SfxRequest` |
| `ui` | `src/ui/` | Crosshair, loading screen, hotbar UI, pickup notifications, pause menu |

## How specs are created and updated

Don't edit files here by hand. The flow is:

1. A change creates **delta specs** in `openspec/changes/<change>/specs/<capability>/spec.md`.
2. `/sdd-archive` merges the deltas into `openspec/specs/<capability>/spec.md`.
3. If the change touches public docs, `/sdd-archive` also bumps `README.md` and `CHANGELOG.md`.

## Existing audited docs

`docs/multiplayer-readiness.md` is an audit against the live code — it documents invariants that the framework currently satisfies. It can serve as a reference for spec wording until a `multiplayer` capability is formally drafted.
