# Per-voxel mining level in face patches
Correct the input adapter's 2x2 mining expansion: every neighbor must satisfy its own mining_level with the active tool.
Scope: private break helper, regression, consumer contract. Preserve six-unit raycast, quadrant/face offsets, timing, hardness behavior, and notifications for successfully broken voxels. No public signatures or save changes.
