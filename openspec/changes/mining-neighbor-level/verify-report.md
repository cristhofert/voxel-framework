# Verification
Baseline regression failed with actual Air instead of expected Ore for the protected neighbor.
After the fix: regression 1/1, mining 11/11, drops 12/12, library 336/336 passed.
The test runs mining_input_system with a real Camera3d raycast and fixed frame time;
it checks hands and a level-2 tool, primary and neighboring voxels, an outside-patch
sentinel, exact break messages, and actual DropsPlugin output. It also runs without
items, where drop-specific assertions are feature-gated.
No public signature, reach, offsets, hardness, or timing changes.
Test harness compiled with serial linker and stripped debug info for local memory.
Detailed logs remain in target/verification/mining-neighbor in the primary workspace.
Release verification must pass on this final commit before registry publication.
