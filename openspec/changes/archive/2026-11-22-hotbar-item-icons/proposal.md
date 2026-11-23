# Inventory-backed hotbar icons

Add opt-in inventory content to the legacy hotbar, with explicit item-to-color
registration and code-generated squares. Reuse VoxelType::color through an
explicit association, independent of item/voxel ids. Keep inventory and input
rules unchanged. No external textures, consumer edits, persistence, or crafting.
Prepare an unpublished immutable release candidate; no push or publication.

This is a substantial public API / UI scheduling change. SDD files are maintained
directly because the referenced SDD skills are not installed in this environment.
