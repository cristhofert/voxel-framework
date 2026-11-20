# Items delta
## ADDED Requirements
### Requirement: Per-voxel patch tool eligibility
Every neighbor in the mining input adapter's 2x2 patch MUST independently require tool_level >= mining_level(). Ineligible neighbors MUST remain intact and MUST NOT emit VoxelBroken or create drops. Existing reach, patch selection, and other rules MUST remain unchanged.
#### Scenario: Bare hands next to level-two ore
- GIVEN a level-zero primary voxel with a level-two ore neighbor in its patch
- WHEN level-zero hands complete mining
- THEN eligible voxels SHALL break and the ore SHALL remain without a break message or drop.
#### Scenario: Sufficient tool level
- GIVEN the same patch and a level-two tool
- WHEN mining completes
- THEN all eligible patch voxels SHALL break and emit their own messages and drops.
