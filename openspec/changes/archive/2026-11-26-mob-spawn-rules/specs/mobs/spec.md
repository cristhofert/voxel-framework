# Mob spawning requirements

## Mob kinds

- `MobRegistry` MUST map a `MobKind` name to a `MobConfig` and iterate kinds in
  name order. `register` MUST replace an existing kind of the same name.
- `register_variant(kind, base, customize)` MUST register a copy of `base`'s
  config after applying `customize`, MUST return `UnknownBase` and register
  nothing when `base` is missing, and later changes to `base` MUST NOT affect
  the variant.
- `spawn_mob_kind` MUST spawn the basic mob with the kind's config plus a
  `MobKind` component, and MUST return `None` without spawning for an
  unregistered kind. `MobPlugin` MUST initialize an empty `MobRegistry`.

## Spawn rules

- `TimeWindow::Any` MUST always apply. `Day` MUST apply for `time_of_day` in
  `[0.25, 0.75)` and `Night` for the rest. `Range { start, end }` MUST apply for
  `[start, end)` with both bounds wrapped into `[0, 1)`, wrapping across
  midnight when the wrapped `start > end`. It MUST be empty when
  `start == end`, MUST always apply when `end - start >= 1`, and MUST NOT
  apply when a bound is not finite.
- Without a `DayCycleConfig` resource only `TimeWindow::Any` rules MUST apply.
- A rule MUST be eligible for a player only when its time window applies, its
  kind is registered, its weight is positive and finite, and fewer than `cap`
  live mobs of that kind are within the rule's outer ring distance of that
  player.
- Among eligible rules one MUST be chosen with probability proportional to its
  weight.
- `SpawnPlacement::Ground` MUST only spawn where the voxel below the mob is
  solid, the two voxels it occupies are not solid, and all three are in loaded
  chunks; the mob MUST rest on that surface at the centre of its voxel column.
  The horizontal distance to the player MUST be at least the ring's inner
  distance and the 3D distance MUST NOT exceed its outer distance, so every
  spawned mob counts toward the cap.
- A `Ground` rule whose effective ring is narrower than 1.5 units MUST NOT
  spawn (it may contain no column centre).
- `SpawnPlacement::Air` MUST only spawn where the voxels the capsule occupies
  are loaded and not solid; the 3D distance to the player MUST lie within the
  ring.
- A rule with a `zone` predicate MUST only spawn where the predicate returns
  `true` for the candidate `SpawnContext`.
- The effective outer distance MUST be capped below the kind's
  `despawn_distance` so a spawned mob is not despawned immediately; a rule
  whose inner distance is not below that cap MUST NOT spawn. An infinite outer
  distance MUST mean "up to that cap".

## Spawner

- `MobSpawnerPlugin<V>` MUST evaluate rules once every
  `MobSpawnRules::interval_secs` and spawn at most one mob per player per
  evaluation, trying at most `attempts_per_player` candidate positions.
- Every `Player` entity MUST be considered, not only the first.
- Given the same seed, evaluation count, player positions, world and rules, the
  spawner MUST choose the same rule and position.
- A mob spawned by a rule with `despawn_outside_window` MUST carry
  `DespawnOutsideWindow` and MUST be despawned once its window no longer
  applies; other mobs MUST be unaffected.
- With no rules, no players or `interval_secs <= 0` the spawner MUST do nothing,
  and it MUST stay idle (not panic) without `Assets<Mesh>` or
  `Assets<StandardMaterial>`.
- Distance despawn in `MobPlugin` MUST measure to the nearest `Player`, so mobs
  left behind are removed with any number of players. With one player the
  behavior MUST be unchanged; with none nothing MUST be despawned.

## Scenarios

- Given a `Night` rule and `time_of_day = 0.5`, when the spawner evaluates,
  then nothing spawns; at `0.9` a mob of the rule's kind spawns.
- Given `Range { start: 0.9, end: 0.1 }`, when `time_of_day` is `0.95` or
  `0.05`, then the window applies; at `0.5` it does not.
- Given `cap = 2` and two live mobs of the kind inside the ring, when the
  spawner evaluates, then that rule is not eligible.
- Given a flat loaded floor and a `Ground` rule with ring `[8, 16]`, when a mob
  spawns, then it stands on the floor between 8 and 16 units from the player.
- Given a `Ground` rule with ring `[2, 6]` and a player 14 units above the
  floor, when the spawner evaluates, then nothing spawns.
- Given two players 100 units apart and a mob 50 units from both with
  `despawn_distance = 10`, when distance despawn runs, then the mob is removed
  and mobs within 10 units of either player stay.
- Given a night mob with `despawn_outside_window`, when `time_of_day` reaches
  `0.25`, then the mob is despawned.
