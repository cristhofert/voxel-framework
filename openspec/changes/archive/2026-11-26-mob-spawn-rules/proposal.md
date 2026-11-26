# Rule-driven mob spawning

Games currently spawn mobs by hand: `examples/minetest.rs` drops five identical
mobs at fixed coordinates at startup. Add named mob types and a rule-driven
spawner so mobs appear around players according to time of day, placement and
an optional zone predicate (the hook for a future biome system, which does not
exist yet and is not part of this change).

Scope:

- `MobKind`, `MobRegistry`, `MobRegistryError`, `spawn_mob_kind`: named mob
  types, each a `MobConfig` of the existing basic mob. No new mob behavior.
- `MobSpawnRules`, `SpawnRule`, `SpawnRing`, `TimeWindow`, `SpawnPlacement`,
  `SpawnContext`, `DespawnOutsideWindow`, `MobSpawnerPlugin<V>` (Gameplay).
- `mob_despawn_system` measures to the nearest `Player` (unchanged with one
  player) so spawned mobs stay bounded with several players.
- Out of scope: biomes, mob damage, multi-player mob AI (`mob_ai_system` still
  assumes one `Player`), migrating the `minetest` example (follow-up change).

Consumer impact: purely additive behind the existing `mobs` feature. `MobPlugin`
now also initializes an empty `MobRegistry`; `spawn_mob` is unchanged. Games
that do not add `MobSpawnerPlugin` see no behavior change. Affected modules:
`src/mobs/`, `src/plugin_category.rs`, `src/prelude.rs`.

Rollback: remove the plugin and the new types; no persistence format, Cargo
feature or existing signature changes.

This is a substantial public API change. SDD files are maintained directly
because the referenced SDD skills are not installed in this environment.
