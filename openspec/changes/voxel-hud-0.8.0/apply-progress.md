# Apply Progress: Author-Defined Voxel HUD for 0.8.0

## Completed Batch

- [x] 1.1 Public IDs, validated scalar/value vocabulary, errors, and owned icon paths.
- [x] 1.2 Presentation builders, definitions, nine anchors, placement/order, theme/overrides, and deterministic snapshots.
- [x] 1.3 Revisioned registry/value stores, keyed inspection, duplicate-safe registration, explicit replacement/removal, and real-change messages.

## Implementation

- Added `src/ui/hud.rs` as a game-neutral public model module.
- Added only the `ui::hud` module declaration required to compile and test Batch 1.
- Did not add renderer, plugin wiring, prelude exports, examples, docs, release metadata, or bundle integration.

## Validation

- `cargo fmt --all -- --check` — passed.
- `cargo check --locked --lib` — passed.
- `cargo check --locked --no-default-features --lib` — passed.
- `cargo test --locked ui::hud::tests --lib` — 11 passed, 0 failed, 245 filtered out.
- `cargo test --locked --no-default-features ui::hud::tests --lib` — 11 passed, 0 failed, 198 filtered out.
- `git diff --check` — passed.

## Remaining

Batch 2 onward remains untouched. The next sequential work is Batch 2 pure resolution.
