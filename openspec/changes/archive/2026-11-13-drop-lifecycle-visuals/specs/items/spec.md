# Drop lifecycle and default presentation delta

## ADDED Requirements

### Requirement: Collector-independent expiration
The framework MUST decrement drop lifetime and despawn expired drops on every Update independently of collector count and Transform presence. Missing or ambiguous collectors MUST NOT generate pickup events.

#### Scenario: No collector or multiple collectors
- GIVEN drops with and without Transform, and zero or two collectors
- WHEN elapsed time reaches their remaining lifetime
- THEN all expired drops MUST despawn without pickup messages.

### Requirement: Explicit reusable drop visuals
An items-gated DropVisualPlugin<I> MUST be available via items and prelude as Default Implementation. It MUST attach a small cube with the item's color to positioned drops, using shared assets and no external assets or lights. It MUST NOT move drops, modify quantities, or install gameplay plugins. Existing custom Mesh3d or material components MUST be preserved.

#### Scenario: Spawn and lifetime
- GIVEN DropsPlugin<I>, DropVisualPlugin<I>, asset stores, and a mapped VoxelBroken
- WHEN one frame runs
- THEN the resulting positioned drop MUST have mesh, material, and visibility components.
- WHEN the drop is fully collected or expires
- THEN its render entity MUST disappear with it.

#### Scenario: Partial collection and asset reuse
- GIVEN two drops of the same item and limited inventory space
- WHEN one is partially collected
- THEN its visible remainder MUST persist with the same render handles.
- AND repeated spawns MUST reuse the mesh and item material.

#### Scenario: Opt-in and custom presentation
- GIVEN only DropsPlugin<I>
- WHEN a drop is spawned
- THEN no mesh or material MUST be added.
- GIVEN a custom mesh or material on a drop
- WHEN DropVisualPlugin<I> runs
- THEN the custom presentation MUST be left intact.
