# Multiplayer Readiness — Audit & Invariants

> **Status**: voxel-framework does not implement networking and has no plans to.
> This document lists invariants the framework currently satisfies and ones to
> preserve as the codebase evolves, so future multiplayer work (in a downstream
> game, or via a layer added on top) is not blocked by avoidable design choices.
>
> Audited on: 2026-04-27. Commit: 1a4254e.

## Why this matters

Voxel multiplayer architectures (lockstep simulation, server-authoritative,
client-prediction with rollback) all share one requirement: given the same
inputs, every machine must produce the same world state. If the framework
introduces non-determinism in the simulation path — random numbers from the
OS, wall-clock reads, unordered map iteration in generator output — that
requirement breaks before the game even gets a packet.

## Invariants currently satisfied

1. **Seeded generation, no OS randomness.** A repo-wide search for
   `thread_rng`, `rand::random`, `rand::rng`, and `fastrand` returned **zero
   hits** in `src/`. All generators are seeded from `WorldConfig::seed: u64`
   (`src/world/config.rs:60`, `:194`).
2. **Deterministic noise.** `src/noise/value.rs:39` defines `hash3(x, y, z, seed) -> f32`
   as a pure MurmurHash3-style finalizer; `value_noise` (`:90`) and `fbm`
   (`src/noise/fbm.rs:68`) compose `hash3` with explicit per-octave seeds
   (`octave_seed = seed.wrapping_add(i * 7919)` at `fbm.rs:80`). No external
   noise crates; no global state.
3. **Deterministic asteroid generator.** `src/generator/asteroid.rs:229–258`
   derives every asteroid's position, radius, and per-asteroid seed via
   `hash3(i, axis, 0, per_seed)` where `per_seed = config.seed.wrapping_add(i * 97 + 1)`.
   Mineral selection (`asteroid.rs:273`) uses three FBM passes with offset
   seeds (`+100`, `+200`, `+300`).
4. **Deterministic terrain generator.** `src/generator/terrain.rs:179–321`
   reads `cfg.seed` for heightmap, biome (`seed + 1000`), and per-column
   feature placement via `hash3`.
5. **Pure `ChunkGenerator` / `RegionGenerator`.** Both traits are bounded
   `Send + Sync + 'static` (`src/generator/mod.rs:133`, `src/generator/region.rs:71`)
   and the `generate` signature takes only `IVec3` + `&mut Chunk<V>` — no
   Bevy `World`, no `Res`, no `Commands`. Documented at `src/generator/mod.rs:26`
   and `:94`.
6. **Message-driven mutations.** Voxel changes flow through `ChunkModified`,
   `VoxelBroken`, and `VoxelPlaced` (Bevy 0.18 Message API), declared in
   `src/world/mod.rs`, `src/world/voxel_ops.rs`, `src/world/mining.rs`,
   `src/world/placement.rs`, and consumed by `mesh/systems.rs`,
   `world/occlusion.rs`, `world/persistence.rs`, `fragments/mod.rs`,
   `audio/mod.rs`, `items/mod.rs`. A future replicator can intercept these
   without rewriting the simulation.
7. **Wall-clock free simulation.** Repo-wide search for `SystemTime::now`
   and `Instant::now` in `src/` returned **zero hits**. All time-driven
   behavior uses Bevy's `Time` resource (per-frame delta, also seedable via
   fixed timestep).
8. **Persistence is event-driven, not time-driven.** Binary chunk writes
   key off `ChunkModified` (`src/world/persistence.rs`); no timestamps in
   the on-disk format.

## Invariants to preserve

For each new PR, the following must hold in code paths that affect persisted
or replicated state (chunks, voxels, mob positions, inventory):

- **Seeded RNG only.** Use the world seed plus a sub-seed for any randomness.
  Never `thread_rng()` or `rand::random()` in generation, simulation, or
  anything written to disk.
- **No wall-clock in simulation.** `SystemTime::now()` and `Instant::now()`
  are fine for FX timing, telemetry, and unloading idle assets, but never
  for advancing simulation state.
- **Deterministic iteration.** `HashMap` / `HashSet` iteration order is
  unspecified; do not use it for generator output, persisted data, or any
  state cross-replicated. Use `BTreeMap` or sort before serializing.
- **Pure generators.** `ChunkGenerator::generate` and `RegionGenerator::generate`
  must remain `Send + Sync` and side-effect free. No Bevy `World`, no global
  mutables, no logging that affects state.
- **Message-driven mutations.** Voxel changes go through `ChunkModified` /
  `VoxelBroken` / `VoxelPlaced`. Direct `Chunk::set` from input handlers is
  acceptable for single-player; replicators wrap the messages.

## Known violations / non-issues

| Hit | Location | Status | Rationale |
|---|---|---|---|
| `thread_rng` / `rand::random` / `fastrand` | (none) | NON-ISSUE | Zero hits in `src/`. |
| `SystemTime::now` / `Instant::now` | (none) | NON-ISSUE | Zero hits in `src/`. |
| `f32::partial_cmp` / `sort_by` | (none) | NON-ISSUE | Zero hits in `src/`. No NaN-ordering footguns. |
| `HashMap<IVec3, Entity>` loaded chunks | `src/world/mod.rs:252` | NON-ISSUE | Local entity cache; iteration order never escapes to disk or wire. |
| `HashSet<IVec3>` pending loads | `src/world/mod.rs:254` | NON-ISSUE | Set membership only; processed by event-driven streaming. |
| `HashMap<u8, …>` drop entries | `src/items/drops.rs:70` | NON-ISSUE | Lookup by id; iteration only inside roll-from-seed paths if the game wires one. Worth revisiting if a drop-roll system is added that relies on iteration order. |
| `HashMap<String, Vec<String>>` audio map | `src/audio/mod.rs:183`, `src/world/manifest.rs:81` | NON-ISSUE | Audio is FX-only, not simulation. |
| `HashMap<V, [f32; 4]>` voxel palette | `src/voxel.rs:720` | NON-ISSUE | Render-side color lookup; not part of authoritative state. |
| `HashSet` in `noise/value.rs:151` | test code | NON-ISSUE | Only collects distinct hash outputs in a determinism test. |

No real blockers found.

## Suggested CI / lint enforcement (future work)

A grep guard in CI would catch regressions:

```sh
rg -t rust 'thread_rng|rand::random' src/ && exit 1
rg -t rust 'SystemTime::now|Instant::now' src/{generator,chunk,world/persistence.rs,noise} && exit 1
```

A custom Clippy lint or a small `xtask` would be cleaner long-term. Adding a
`[lints]` section to `Cargo.toml` is intentionally **not** done in this PR —
that is a separate decision.

## Out of scope

- Connection handling, packet serialization, NAT traversal, lobby/matchmaking
- Authority models (server-authoritative vs P2P)
- Anti-cheat, rate limiting, replay protection
- Latency hiding (client-side prediction, lag compensation)

These are game-specific and belong in a network layer above the framework,
not in voxel-framework itself.
