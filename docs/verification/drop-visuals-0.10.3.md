# Drop visuals 0.10.3 verification

## Scope

Release baseline: 0.10.2 plus the lossless partial-pickup regressions (86f2aed).
Adds the optional items-gated DropVisualPlugin<I>, collector-independent expiry,
public exports, integration guide, and the standalone drops example. No inventory
capacity, stack type, persistence, mining reach, or patch-shape changes.

## Automated local evidence

- Real Bevy App integration: 4 visual tests, 13 drops tests, all passed.
- Entire default-feature library: 341 passed, 0 failed, 0 ignored.
- Visual cases: VoxelBroken-spawned drops rendered in the same frame; existing
  drops when enabling the plugin; late Transform insertion; shared assets;
  custom mesh/material preservation; actual velocity-driven movement and
  GlobalTransform propagation; runtime replacement and hidden visibility;
  partial/full pickup and expiry cleanup.
- Lifetime cases: zero/two collectors, both with/without Transform; no false pickup.
- Stable: default/all-targets, no-default/library, items-only/library+drops,
  all-features/all-targets passed.
- Exact Rust 1.89.0: default, no-default, items-only/library+drops, all-features
  library passed.
- cargo fmt --all -- --check, git diff --check, and all-feature rustdoc passed.

Local logs and screenshots live in target/verification/drop-visuals-published
in the primary workspace. The library test harness was compiled with cargo rustc
--lib --profile test, debuginfo=0, serial linking and stripped debug information
to fit available memory. Local feature/MSRV checks used a locked offline
resolution. CI resolves independently because the library does not track a lock.

## Graphical evidence

Built with cargo rustc --example drops and ran on the local native display.
Inspected expiry-before.png (three cubes) and expiry-after.png (none, no collector).
Inspected pickup-before.png, pickup-moving.png (cubes attracted toward the center),
and pickup-after.png (none before expiry). The log reports exactly three each of
Crystal, Rock Fragment, and Rare Mineral. Each example process was intentionally
terminated after observation. The first synthetic Space attempt was not received;
repeating with explicit focus and a held key succeeded. No production change was
needed. Vulkan warned that VK_EXT_physical_device_drm was unavailable; rendering
and collection succeeded.

The ignored publication lock upgrades only the yanked async-executor 1.14.0 to
1.14.1. The exact-MSRV check is repeated with that resolution before publication.

## Publication gates

Publication is pending Rust CI (including Windows), the complete release
preflight workflow on the exact release commit, and cargo publish --dry-run.
The final release record will identify their run URLs and the published revision.

## Limits

The default uses 0.25-unit unlit cubes colored by ItemType::color. Colors are
cached per item variant; changing an already-rendered item's identity does not
recolor it. Custom Mesh3d or MeshMaterial3d<StandardMaterial> components suppress
default attachment; games can replace components afterward or omit the plugin.
The visual plugin supplies no camera, lighting, physics, or gameplay. Spatial
pickup still requires one collector and one primary container. Headless tests
validate lifecycle edge cases; the native example only validates the local GPU
backend, not every backend or a downstream game's integration.
