# Hotbar item icons — verification and release

## Published release — 2026-11-23

Version **0.10.4** is published on crates.io and is not yanked.
The annotated v0.10.4 tag points to the verified release commit
`c5913ca9dbba3004be77c66e581d28e20f12d337` (implementation commit
`3dd84b92a4263485fa6b0ebca1f706a251963e2e` plus finalized consumer documentation).

- [Rust CI](https://github.com/cristhofert/voxel-framework/actions/runs/45094195537):
  all 10 jobs passed on the release SHA, including Windows, MSRV, feature tests,
  examples, documentation, and package audit.
- [Release preflight](https://github.com/cristhofert/voxel-framework/actions/runs/45094195430):
  the complete repository release script passed on the same SHA in GitHub Actions
  with independently resolved dependencies.
- cargo publish --dry-run --locked: passed, including package compilation.
- cargo publish --locked: succeeded; the sparse registry index confirms 0.10.4,
  rust-version 1.89 and yanked=false.
- Registry SHA-256:
  `c0250c41275c215a54c62c8949f7beeb28c0a23255538b2680c13361dae96256`.
- [Release and immutable tag](https://github.com/cristhofert/voxel-framework/releases/tag/v0.10.4).

Release evidence is retained under target/verification/hotbar-release. The first
offline dry-run could not perform its required HTTP request; an online invocation
was then blocked before execution by an automatic-approval usage limit. After
resuming, review succeeded and dry-run/publication completed normally.

The sections below retain the original local candidate verification and its
then-pending release boundary. Those pending gates are now satisfied by the
release runs above. This documentation follow-up does not move the release tag.

## Original candidate verification

Date: 2026-11-22. Baseline: published v0.10.3
(`7b5888917059e666a4055f449c5576758c98e23a`).
Candidate: 0.10.4, branch codex/hotbar-item-icons.
Worktree: target/hotbar-item-icons inside the voxel-framework repository.

## Scope and implementation

ItemIconPalette<I> explicitly associates full item values with linear RGBA colors,
including colors sampled from any VoxelType. HotbarContentPlugin<I> extends the
legacy HotbarUiPlugin with procedural inset square icons, '?' fallback symbols,
all quantities including one, and a selected name / "Empty hands". It reads the
unique PrimaryContainer + ContainerComponent<I> directly every frame. No item,
voxel, stack, storage, persistence, crafting, or cursor-capture APIs changed.

The shared row now reserves 16px margins and sizes both slot dimensions from the
primary window / UiScale. Default gaps remain 4px and content slots at most 50px.
Legacy config fields and exports remain compatible; larger legacy-only slot sizes
are preserved. Inputs remain in the unchanged items/hotbar.rs implementation.

## Tests and local checks

- Library: **351 passed, 0 failed, 0 ignored**.
- Hotbar (existing input/state tests plus new UI integration): **15 passed**.
- Inventory regressions: **21 passed**. Drops regressions: **13 passed**.
- Default-feature doctests: **112 passed, 3 ignored** (existing ignored examples).
- Stable compilation: default/all-targets; no-default/library;
  items-only/library+hotbar_icons example; all-features/all-targets.
- Exact MSRV Rust 1.89.0 compilation: default, no-default, items-only
  (+hotbar_icons example), and all-features library.
- Formatting, git diff --check, and all-feature rustdoc with warnings denied.

The real Bevy App tests exercise explicit item/voxel associations with different
byte IDs; generic fallback; count 1, stack increment, replacement and clearing;
runtime palette changes; selected name/highlight with no inventory mutation;
missing/replaced/ambiguous and short containers; every digit key 1-9; complete
14-slot wheel traversal, reverse wraparound, and pixel wheel input; disabled UI;
legacy slot-only operation; reversed plugin order and preconfigured palette;
resize/UiScale; and full u32 quantities with constrained text sizing.

Local test executables are built through cargo rustc --lib --profile test,
with debuginfo=0, serial linking, and stripped debug information. The final
test harness additionally uses opt-level=0 for this crate to reduce compilation
cost. Dependencies retain the repository's configured profiles.

## Native evidence

Native Vulkan / NVIDIA GTX 1050 Ti example, no icon assets:
- 800x600: fourteen 50px slots, bounds x=24..776, bottom margin 16px.
- 1280x720: fourteen 50px slots, bounds x=264..1016, bottom margin 16px.
- 1920x1080: fourteen 50px slots, bounds x=584..1336, bottom margin 16px.
- Extra 600x600: slots round to 36/37px, bounds x=16..584; squares, gaps, and
  bottom margin remain correct.

The example's --verify asserts actual computed layout after ten frames: slot
count, square sizes, centering, margins, and gaps. Captures were visually checked
for all four sizes. 1920x1080 uses --fullscreen on the primary 1920x1080 monitor;
the window manager had reduced the initial decorated request to 1920x1005.

An additional native interaction run showed wheel selection of a late empty
slot with "Empty hands" and rock count increasing from 1 to 2 with A. Synthetic
input delivery was not consistent enough to claim every transition from that
run; deterministic App tests supply full key/wheel and clear-slot coverage.

## Failures found and fixed

- The baseline regression failed because the old row had 0px horizontal padding.
- The first native inspection accidentally included all nodes: Bevy installs
  BorderColor on every Node. The example now identifies the configured 2px slots.
- Flex shrinking produced 37x50 rectangles at 600px. Explicit width+height sizing
  fixed it; a headless resize/UiScale regression and native checks protect it.
- The extended keyboard test initially retained a pressed key across simulated
  frames. reset_all now models release between independent presses.
- Vulkan emitted an unavailable VK_EXT_physical_device_drm warning; rendering
  succeeded. No claim is made about other GPUs/backends.

## Reproducibility and release boundary

Logs, native scripts, and PNGs:
target/verification/hotbar-item-icons in the primary workspace.
All Cargo operations ran offline; no dependency sources were patched in the
Cargo cache, and no consumer repository was changed. Existing uncommitted work
in the primary dev checkout was preserved.

The cached local test/feature matrix uses async-executor 1.14.0. The ignored
packaging lock replaces that yanked version with cached 1.14.1, without changing
manifest dependencies. Its exact-MSRV check and packaged-source compilation are
verified separately. A fresh test build against that packaging lock was cancelled
when it began rebuilding Bevy dependencies; no result is claimed for that attempt.

Final `cargo package --offline --locked --allow-dirty` verification passed:
132 files, approximately 1.5 MiB compressed. One final packaging invocation was
interrupted with exit 143 during rustc; retry succeeded without code changes.
The package includes the public UI module, guide, example, and verification
report. A SHA-256 sidecar is retained next to the review artifact in the evidence
directory; it identifies this local snapshot, not a published release.

The candidate package is local only and is not a published release. At verification time, no commit,
push, tag, registry publication, or CI dispatch had been performed. The maintainer
subsequently authorized the local commit, then push and publication. The original
implementation is commit 3dd84b92a4263485fa6b0ebca1f706a251963e2e.
Release gates are being run on the final commit, including consumer-doc updates.
Clean-commit release preflight, independently resolved dependencies, Windows CI,
and registry/tag availability checks remain required before publication. The local --allow-dirty package is review evidence, not a
substitute for those exact-commit gates.

Slot count is configured before Startup. This implementation targets a full
primary-window UI viewport; custom viewports/multiple UI targets require game
layout. At extremely narrow widths, gaps shrink after slots exhaust available
space. Long counts remain complete but use smaller text. No extra inventory
screen or item manipulation UI is introduced.
