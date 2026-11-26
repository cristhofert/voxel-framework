# Design

Everything lives in `src/mobs/`: kinds in `mod.rs`, the spawner in
`spawner.rs`. All of it stays behind the `mobs` feature.

Kinds are data, not behavior: `MobKind(String)` follows the `ContentId`
newtype convention (private field, `From<&str>`/`From<String>`, `as_str`).
`MobRegistry` wraps a `BTreeMap` so iteration order is stable, which the
spawner's determinism depends on.

Rules are plain data with a builder (`SpawnRule::new(kind).ring(8.0, 24.0)
.time(TimeWindow::Night).cap(4)`). `zone` is an
`Arc<dyn Fn(&SpawnContext) -> bool + Send + Sync>` rather than a trait: it
leaves a hook for biomes without designing them now, and keeps `SpawnRule:
Clone`. `MobSpawnRules` is one resource holding the rule list plus
`interval_secs` (default 2.0), `attempts_per_player` (default 8) and `seed`.

Decision logic is pure and unit-tested without a Bevy `App`, matching the repo
convention: `TimeWindow::contains`, `pick_weighted`, `effective_ring`, `rule_weights`,
`candidate_offset`, `find_ground_spawn`, `air_spawn_clear`,
`find_spawn_position`. World lookups are
passed as closures returning `Option<bool>` (`None` = chunk not loaded).
Randomness is `SpawnRng`, a counter over `noise::hash3` keyed by seed,
evaluation number and player index; no external RNG crate.

Ground search walks the candidate column outward from the player's height
(±16 voxels) and takes the nearest valid surface, so cave floors work as well
as open terrain. The spawn height is the floor top plus the capsule rest
height, so the mob starts grounded and clear of the spine-in-voxel case.

Systems, both in `Update`:

1. `mob_spawner_system<V>`: accumulates time; on each interval sorts players by
   `Entity` for a stable order, builds eligibility from live `(MobKind,
   Transform)` counts, picks a rule, tries candidate positions, and spawns with
   `spawn_mob_kind`. Mobs spawned earlier in the same evaluation count toward
   caps so two nearby players do not both fill the last slot.
2. `mob_window_despawn_system`: despawns mobs whose `DespawnOutsideWindow`
   window no longer contains the current time. The component stores the window
   itself, so editing or removing rules later does not strand mobs.

No explicit ordering against `MobPlugin` systems: spawning goes through
`Commands`, so new mobs are first simulated on the next frame either way.
`MobSpawnerPlugin<V>` is generic over the voxel type because it reads
`Chunk<V>`; it uses `PhantomData` and offers `::new()` and `Default` like
`MobPlugin<V>`. It requires `MobPlugin<V>` (for `MobRegistry`) but initializes
the registry itself as well, so plugin order does not matter. It is registered
as a `GameplayPlugin` in `plugin_category.rs`.

The day clock is read through `Option<Res<DayCycleConfig>>`, so the spawner
works without `DayCyclePlugin` (only `TimeWindow::Any` rules apply).

Distances: the cap and `mob_despawn_system` measure in 3D. Ground candidates
are therefore rejected when their 3D distance exceeds the ring's outer
distance (the surface may be far below a flying player); otherwise such mobs
would never count toward the cap and the rule would spawn without bound.

`mob_despawn_system` now measures to the nearest player instead of
`player_q.single()`. With several players the old code never despawned, so a
multi-player spawner would have accumulated mobs forever. Single-player
behavior is identical.

The spawner system is gated with `run_if(resource_exists::<Assets<Mesh>>.and(
resource_exists::<Assets<StandardMaterial>>))`, so a headless app that adds
the plugin idles instead of failing parameter validation.

Known limit kept out of scope: mob AI still uses `player_q.single()`, so with
several players mobs spawn and despawn correctly but do not act.
