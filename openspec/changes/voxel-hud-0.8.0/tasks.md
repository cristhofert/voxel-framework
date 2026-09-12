# Tasks: Author-Defined Voxel HUD for 0.8.0

**Guardrails:** Keep `VoxelHudPlugin` explicit, non-generic, without a Cargo feature, and domain-neutral. Exclude presets, automatic bridges/player discovery, rules/interactions, config DSL/persistence, arbitrary layouts/lists, animation, localization, accessibility suites, and bundle/`UiPlugin` membership.

**Order:** Batches 1→2→3 are sequential. After Batch 3, disjoint Batches 4 and 5 are parallel-safe; Batch 6 follows both, then Batch 7.

## Batch 1: Public model and stores

- [x] 1.1 Create `src/ui/hud.rs` IDs, `HudScalar`, closed `HudValue`, errors, and owned icon paths; enforce finite/range and kind invariants.
- [x] 1.2 Add presentation builders, definitions, nine anchors, placement/order, theme/overrides, and deterministic snapshot types in `src/ui/hud.rs`.
- [x] 1.3 Implement `HudRegistry`/`HudValueSet` keyed reads, duplicate-safe registration, explicit replacement/removal, monotonic revisions, ordered inspection, and real-change messages.

## Batch 2: Pure resolution

- [x] 2.1 Implement text formatting, clamped bar ratios, bounded segment counts, visibility, style merging, anchor/offset/stack layout, and ID tie-breaking in `src/ui/hud.rs`.
- [x] 2.2 Resolve missing, mismatched, invalid, excessive, and unavailable-asset inputs to hidden/compatible fallback state with transition-deduplicated `HudDiagnostic`; never coerce or panic.

## Batch 3: Retained renderer and wiring

- [x] 3.1 Add private root/child identity and cached fingerprint/asset-handle components; create retained trees for text, bar, icon, and segments in `src/ui/hud.rs`.
- [x] 3.2 Reconcile only registry/theme revisions: preserve roots, prevent duplicates, reparent/reorder placement changes, rebuild only changed presentation children, and resize segment children.
- [x] 3.3 Render only relevant value changes and mutate only changed `Text`, `Node`, color, image, and visibility components.
- [x] 3.4 Implement `VoxelHudPlugin` resources/messages plus chained `HudSystemSet::{Publish,Reconcile,Render}` in `Update` and `PostStartup`, with a deferred-command barrier.
- [x] 3.5 Export/document the API through `src/ui/mod.rs`, `src/prelude.rs`, and crate rustdoc in `src/lib.rs`; classify only `VoxelHudPlugin` in `src/plugin_category.rs` and leave bundles unchanged.

## Batch 4: Tests

- [x] 4.1 Add inline model/resolution tests in `src/ui/hud.rs` for IDs, stores, revisions, snapshots, scalar/formatting/bounds, all presentations, fallback, theme, anchors, and ordering.
- [x] 4.2 Add `MinimalPlugins` ECS tests for empty/standalone/coexisting plugin startup, same-frame publication, stable roots/child counts, selective mutation, definition reconciliation, diagnostic deduplication, and zero asset I/O.

## Batch 5: Consumer example and guidance

- [x] 5.1 Add feature-independent headless `examples/hud.rs` and its `Cargo.toml` target, registering/updating neutral text, bar, icon, and segment values.
- [x] 5.2 Add `docs/hud.md`; update `README.md` and `AGENTS.md` with publication, scheduling, fallbacks, inspection, coexistence/custom-overlay migration, explicit opt-in, and non-goals.

## Batch 6: 0.8.0 release metadata

- [x] 6.1 Set `Cargo.toml`/root `Cargo.lock` to `0.8.0` and add dated `CHANGELOG.md` notes covering the additive API and unchanged compatibility boundaries.

## Batch 7: Validation and independent verification

- [x] 7.1 Run `cargo fmt --all -- --check`, `cargo test --locked`, `cargo check --locked --all-targets`, and `cargo doc --locked --no-deps --all-features`.
- [x] 7.2 Run `cargo test --locked --no-default-features`, `cargo check --locked --no-default-features --all-targets`, and `cargo check --locked --no-default-features --example hud`.
- [x] 7.3 Run `cargo package --locked --offline --allow-dirty --list` and `cargo package --locked --offline --allow-dirty --no-verify`; confirm HUD example/docs ship and workflow/OpenSpec state does not.
- [ ] 7.4 Have an independent verifier trace every spec scenario/non-goal to code/tests, inspect `git diff --check` for scope creep, and run `./scripts/release-preflight.sh 0.8.0` once the tree is clean.

Task 7.4 is partially evidenced: the independent trace and `git diff --check`
passed, and the complete script passed with `RELEASE_PREFLIGHT_ALLOW_DIRTY=1`.
The repository release convention explicitly forbids using that override as
release approval, so the clean-tree invocation remains pending while commits
are prohibited.
