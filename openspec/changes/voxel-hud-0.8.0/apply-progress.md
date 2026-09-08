# Apply Progress: Author-Defined Voxel HUD for 0.8.0

## Completed Batch

- [x] 1.1 Public IDs, validated scalar/value vocabulary, errors, and owned icon paths.
- [x] 1.2 Presentation builders, definitions, nine anchors, placement/order, theme/overrides, and deterministic snapshots.
- [x] 1.3 Revisioned registry/value stores, keyed inspection, duplicate-safe registration, explicit replacement/removal, and real-change messages.
- [x] 2.1 Pure text/bar/segment/visibility/style/layout resolution with deterministic anchor/order/id sorting.
- [x] 2.2 Safe missing, mismatched, invalid, excessive, and unavailable-asset handling with compatible fallbacks and transition-deduplicated diagnostics.

## Implementation

- Added the private pure resolution model in `src/ui/hud.rs` for all four presentations, nine anchors, merged theme overrides, and deterministic ordering.
- Added public domain-neutral `HudDiagnostic`, `HudDiagnosticKind`, and `HudBindingKind` message vocabulary for renderer wiring in Batch 3.
- Invalid inputs hide safely or use only a compatible author fallback; excessive segment counts are never coerced, and asset availability is injected for asset-free tests.
- Did not add renderer/plugin wiring, prelude exports, examples, docs, release metadata, or bundle integration.

## Validation

- `cargo fmt --all -- --check` — passed.
- `cargo check --locked --lib` — passed.
- `cargo check --locked --no-default-features --lib` — passed.
- `cargo test --locked ui::hud::tests --lib` — 19 passed, 0 failed, 245 filtered out (8 focused Batch 2 tests added).
- `cargo test --locked --no-default-features ui::hud::tests --lib` — 19 passed, 0 failed, 198 filtered out.
- `git diff --check` — passed.

## Remaining

Batch 3 onward remains untouched. The next sequential work is Batch 3 retained renderer and wiring.
