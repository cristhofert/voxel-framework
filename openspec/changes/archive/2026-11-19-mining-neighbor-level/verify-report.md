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

## Release outcome — 2026-11-19

Published voxel-framework 0.10.2 from bc8386d0540f3f9a46bc52c2a78a5f343bb7b37b.
- Rust CI: all ten jobs passed, including Windows persistence, MSRV, feature
  matrix, tests, examples, docs and package audit:
  https://github.com/cristhofert/voxel-framework/actions/runs/44608110538
- Full release preflight passed on the same commit:
  https://github.com/cristhofert/voxel-framework/actions/runs/44608107764
- cargo publish --dry-run --locked and cargo publish --locked passed.
- Registry index confirmed version 0.10.2, not yanked, checksum
  148793372c523a050ba591abecf32357d324b6a126970fe28866517bbec07741.
- Annotated tag and GitHub release:
  https://github.com/cristhofert/voxel-framework/releases/tag/v0.10.2

The local ignored lockfile replaced yanked async-executor 1.14.0 with 1.14.1.
The original interrupted /tmp run lost its code/logs and is not counted as
verification; the reproduction was recreated and observed in the persistent
checkout. No native game playtest was run: the defect and event/drop outcomes
were reproduced through the real Bevy mining system headlessly.
Pending inventory changes and the separate drop-visual candidate are excluded.
