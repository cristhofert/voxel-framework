# Verification
## RED
Compiled the pre-fix test harness with:
`cargo rustc --locked --offline --jobs 1 --lib --profile test -- -C debuginfo=0 -C link-arg=-Wl,--threads=1 -C link-arg=-Wl,--strip-debug`
Executed `target/debug/deps/voxel_framework-53c00f48c2ee94d2 render::material::tests --nocapture`.
Result: 1 passed (palette/air), 1 failed (expected Opaque, actual Blend).
Log: /tmp/voxel-opacity-red-result.log.
Normal cargo test linking and two intermediate low-memory attempts were interrupted under memory/swap pressure; they are not test failures or passes.
## GREEN and regressions
- Recompiled the post-fix harness with the same serial, stripped linker command: passed.
- Executed the four render::material::tests: 4 passed, 0 failed.
- Executed the entire library harness: 339 passed, 0 failed, 0 ignored. Log: /tmp/voxel-opacity-library-tests.log.
- `cargo check --locked --offline --jobs 1 --lib --example basic_world`: passed.
- `cargo +1.89.0 check --locked --offline --jobs 1 --lib`: passed.
- `cargo +1.89.0 check --locked --offline --jobs 1 --lib --no-default-features`: passed.
- `cargo fmt --check` and `git diff --check`: passed.
- Reviewed requirements against startup-material, explicit Blend/Mask, palette/air both-LOD, and texture-array tests.
- Checked docs/multiplayer-readiness.md; no affected rendering invariant found.
- No integration-test or graphical executable run; the library harness is the reported regression scope.
## Limitations
No native screenshot comparison or downstream Astrohabita dependency change.
The source-level defect and headless material regression do not prove the sole cause of every reported missing face.
OpenSpec CLI and SDD skills are not installed in this environment; the documented repository artifact workflow is followed directly.
Engram lookup for voxel-framework failed with unknown_project; no memory write or cross-project fallback was attempted.
