# Items Capability — Tool/Material Contract (v1)

This spec defines the tool/material contract spanning `VoxelType` (`src/voxel.rs`),
`ItemType` (`src/items/inventory.rs`), `DropTable` (`src/items/drops.rs`), and the
mining engine (`src/world/mining.rs`). It locks the per-variant break-time formula,
mining-level gating, tool-speed scaling, drop-table resolution, and the
`DefaultVoxel` material overrides. Behavior described here MUST hold for any
voxel type implementing `VoxelType` and any item type implementing `ItemType`.

Source of truth: this file. Last updated by archive of `tool-material-system` on 2026-05-08.

## Requirements

### Requirement: Hardness Multiplier

The system MUST treat `VoxelType::hardness()` as a non-negative multiplier on `MiningConfig::break_time`. The default trait implementation MUST return `1.0`.

#### Scenario: Standard hardness scales break time

- GIVEN `MiningConfig::break_time = 1.5` and a voxel whose `hardness() == 3.0`
- WHEN `tick_mining` first targets that voxel
- THEN `MiningState::effective_break_time` MUST equal `4.5`
- AND `progress` reaches `1.0` only after at least `4.5` seconds of accumulated `delta_secs`

#### Scenario: Hardness 0.0 is unbreakable

- GIVEN a voxel whose `hardness() == 0.0`
- WHEN `tick_mining` targets it for any finite duration
- THEN `effective_break_time` MUST be set to `f32::MAX`
- AND no `VoxelBroken` message SHALL be emitted

### Requirement: Mining-Level Gating

The system MUST refuse to make mining progress when the active tool's level is strictly less than the voxel's `mining_level()`. The default trait implementation of `mining_level()` MUST return `0` (breakable by bare hands).

#### Scenario: Tool level below voxel mining level blocks progress

- GIVEN a voxel with `mining_level() == 2` and `ToolStats { tool_level: 1, .. }`
- WHEN `mining_input_system` raycasts onto the voxel with the mine button held
- THEN it MUST call `stop_mining` instead of `tick_mining`
- AND no `MiningProgress` or `VoxelBroken` message SHALL be emitted for that voxel

#### Scenario: Tool level meets or exceeds requirement

- GIVEN a voxel with `mining_level() == 1` and `ToolStats { tool_level: 1, tool_speed: 1.0 }`
- WHEN the mine button is held and the ray hits the voxel
- THEN `tick_mining` MUST be invoked and progress accumulates normally

### Requirement: Tool Speed Multiplier

The system MUST scale the per-frame mining delta by `ToolStats::tool_speed`. `tool_speed` MUST default to `1.0` when no tool is held or the active slot is empty.

#### Scenario: Faster tool reduces effective break time

- GIVEN `break_time = 1.5`, `voxel.hardness() == 1.0`, and `ToolStats { tool_speed: 2.0, tool_level: 0 }`
- WHEN `mining_input_system` runs each frame with `time.delta_secs() = 0.1`
- THEN the per-call delta passed to `tick_mining` MUST equal `0.2`
- AND `progress` reaches `1.0` after roughly `0.75` real seconds (half the nominal time)

### Requirement: Drop Table Resolution

The system MUST resolve drops from `DropTable<I>` using the broken voxel's `to_byte()` value. Explicit registrations MUST take precedence; if absent, the system MUST attempt the identity fallback `I::from_byte(voxel_byte)`. A resolved item whose `to_byte() == 0` MUST yield no drop.

#### Scenario: Explicit registration wins

- GIVEN `table.register(1, DefaultItem::RockFragment, 1, 3)` and a broken voxel with `to_byte() == 1`
- WHEN `get_drop(1, seed)` is called
- THEN it MUST return `Some((DefaultItem::RockFragment, n))` with `1 <= n <= 3`

#### Scenario: Identity fallback for unregistered byte

- GIVEN no entry registered for `voxel_byte = 2` and `DefaultItem::from_byte(2)` returning a non-empty variant
- WHEN `get_drop(2, seed)` is called
- THEN it MUST return `Some((that_variant, 1))`

#### Scenario: Empty fallback yields no drop

- GIVEN no registration for `voxel_byte = 99` and `I::from_byte(99).to_byte() == 0`
- WHEN `get_drop(99, seed)` is called
- THEN it MUST return `None`

### Requirement: DefaultVoxel Material Overrides

`DefaultVoxel` MUST override `hardness()` and `mining_level()` so that variants do not all share trait defaults. `Air` MUST return `hardness() == 0.0` (unbreakable). The non-air variants MUST return distinct, finite, positive hardness values and `mining_level()` values in the range `0..=2`.

#### Scenario: Air is unbreakable

- GIVEN `voxel = DefaultVoxel::Air`
- WHEN `voxel.hardness()` is called
- THEN it MUST return `0.0`

#### Scenario: Iron ore requires level-2 tool

- GIVEN `voxel = DefaultVoxel::IronOre`
- WHEN `voxel.mining_level()` is called
- THEN it MUST return `2`
- AND a `ToolStats { tool_level: 1, .. }` MUST NOT be able to break it (per "Mining-Level Gating")
