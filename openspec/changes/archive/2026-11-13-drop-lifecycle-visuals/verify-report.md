# Verification — drop lifecycle and visuals

## 0.10.3 candidate verification (2026-11-19)

Integrated on 86f2aed (published 0.10.2 mining fix plus the partial-pickup
regressions). This supersedes the older 0.10.1 candidate below. Inventory capacity,
stack limits, and persistence APIs remain unchanged.

- Real Bevy App integration tests: 4 visual tests and 13 drop lifecycle tests pass.
- Entire default-feature library: 341 passed, 0 failed, 0 ignored.
- Movement regression uses DropsPlugin velocity integration and checks same-frame
  GlobalTransform propagation; replacement mesh, material, and hidden visibility
  survive subsequent updates.
- Stable checks pass: default/all-targets, no-default/library,
  items-only/library+drops example, all-features/all-targets.
- Rust 1.89.0 checks pass for default, no-default, items-only (+drops example),
  and all-features library configurations.
- Formatting and all-feature rustdoc pass.
- Logs: target/verification/drop-visuals-published in the primary workspace.
  Tests use cargo rustc --lib --profile test with debuginfo=0, a serial linker,
  and stripped debug information to fit local memory. Feature/MSRV checks use
  the local offline lock; CI resolves dependencies independently.

Graphical validation and exact-commit release gates are recorded in
docs/verification/drop-visuals-0.10.3.md. Publication requires successful Rust CI
(including Windows) and release preflight on the immutable candidate commit.

## Historical candidate verification


## Reverification on 2026-11-19

Verified in an isolated checkout based on published commit 7184976. Pending
inventory-capacity, stack-limit, and persistence-migration changes are excluded.
No public inventory API or persistence format changed.

- Plugin visual tests: 3 passed, 0 failed.
- Drop lifecycle tests: 13 passed, 0 failed.
- Entire library: 339 passed, 0 failed.
- Stable: default/all-targets, no-default/library, items-only/library+drops,
  all-features/all-targets: passed.
- Rust 1.89.0: default/library, no-default/library, items-only/library+drops,
  all-features/library: passed.
- cargo fmt --all -- --check and git diff --check: passed.
- cargo doc --locked --offline --no-deps --all-features: passed without warnings.
- Graphical example compiled and ran on the local display. Captures show three
  positioned cubes initially and no cubes after expiration without a collector.
  A second run captured the same three cubes, activated collection with Space,
  and captured their disappearance three seconds later (before ten-second expiry).
  The executable was intentionally terminated after each observation.
- Automated tests cover zero/two collectors, absent Transform, same-frame visuals,
  late transforms, custom components, asset reuse, partial pickup, and cleanup.

## Evidence and limitations

Logs and screenshots: target/verification/drop-lifecycle-final in the main
workspace. Screenshots: initial.png, expired.png, pickup-before.png,
pickup-after.png. Test harness built using cargo rustc --lib --profile test with
debuginfo=0, serial linker, and stripped debug information to fit local memory.
All feature/MSRV checks used the locked offline resolution.

Native startup emitted a warning for unavailable VK_EXT_physical_device_drm;
rendering succeeded. Native screenshots validate the default local renderer,
not every GPU/backend. Multiple collectors and missing transforms were verified
headlessly, not visually. No downstream game's integration was changed.

This is a verified capability candidate, not a claim of crates.io publication.
Full exact-candidate release preflight and Windows CI remain required before
registry publication. Prior historical results are superseded by this fresh run.
