# Design
Pass active ToolStats.tool_level into the private break_voxel helper. Check the loaded neighbor's mining_level before mutation or messages. Keep the primary path and geometry unchanged.
Use an actual camera raycast and two mining frames against a mixed four-voxel patch. Compare level-0 hands and a level-2 tool, including VoxelBroken and actual DropsPlugin output, plus an outside-patch sentinel.
