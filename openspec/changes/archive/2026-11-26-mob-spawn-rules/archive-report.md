# Archive — 2026-11-26

Implemented and locally verified rule-driven mob spawning: `MobKind` /
`MobRegistry` and `MobSpawnerPlugin` with `MobSpawnRules`. The requirements were
merged into `openspec/specs/mobs/spec.md`, the first accepted spec for the
`mobs` capability. `README.md` (new "Mobs" section) and `CHANGELOG.md`
(`[Unreleased]`) already describe the public API. `docs/multiplayer-readiness.md`
needed no change: the spawner adds no replicated state or invariant beyond the
nearest-player distance despawn, which the spec records.

The change was authored by hand because the `/sdd-*` skills are not installed in
Claude Code; the artifact layout follows the earlier archived changes.

Not verified at archive time: no in-game run of `examples/minetest` was done, and
`--all-features`, the full doctest suite, `cargo test --examples`, `cargo doc`
and the MSRV 1.89 lane were left to GitHub CI on the pull request. See
`verify-report.md` for the local results and known limits. No release was
prepared.
