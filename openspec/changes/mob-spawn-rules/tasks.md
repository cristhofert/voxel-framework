# Tasks

## 1. Kinds

- [x] 1.1 `MobKind`, `MobRegistry`, `MobRegistryError`, `spawn_mob_kind` with tests.
- [x] 1.2 `MobPlugin` initializes `MobRegistry`; prelude re-exports.

## 2. Spawn rules (pure)

- [x] 2.1 `TimeWindow` with midnight wrap.
- [x] 2.2 `SpawnRule` builder, `SpawnRing`, `SpawnPlacement`, `SpawnContext`, `MobSpawnRules`.
- [x] 2.3 `SpawnRng`, `pick_weighted`, `effective_ring`, `rule_weights`.
- [x] 2.4 `candidate_offset`, `find_ground_spawn`, `air_spawn_clear`.

## 3. Plugin wiring

- [x] 3.1 `mob_spawner_system<V>` and `mob_window_despawn_system`.
- [x] 3.2 `MobSpawnerPlugin<V>`, `GameplayPlugin` marker, prelude re-exports.

- [x] 3.3 Nearest-player distance despawn in `MobPlugin`.

## 4. Docs and verification

- [x] 4.1 `CHANGELOG.md` and `README.md`.
- [x] 4.2 fmt, tests, example and no-default-features checks; verify report.
