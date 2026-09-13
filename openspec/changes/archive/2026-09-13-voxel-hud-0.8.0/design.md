# Design: Author-Defined Voxel HUD for 0.8.0

## Technical Approach

Add one non-generic, explicit `VoxelHudPlugin` backed by a narrow presentation model in `ui::hud`. Authors register immutable widget definitions and publish screen-ready values; the plugin reconciles retained Bevy UI entities. It never queries content, stats, effects, players, or voxel types.

## Public Model and Contracts

| API | Contract |
|---|---|
| `HudElementId`, `HudValueId` | Serializable string newtypes matching existing ID ergonomics (`new`, `as_str`, `From`, `Display`). |
| `HudValue` | Closed view-state enum: `Text(String)`, `Scalar(HudScalar)`, `Count(u32)`, `Bool(bool)`, `Icon(AssetPath<'static>)`. It intentionally has no arbitrary numbers/lists/colors/attributes, so it is not a second `ContentValue` language. `HudScalar::new(value,min,max)` rejects non-finite data, normalizes reversed bounds, and exposes clamped ratio resolution. |
| `HudPresentation` | `Text(HudText)`, `Bar(HudBar)`, `Icon(HudIcon)`, `Segments(HudSegments)`. Builders are `HudText::new(value)`, `HudBar::new(value)`, `HudIcon::{asset,bound}`, and `HudSegments::new(value, capacity)`; text adds explicit prefix/suffix/precision, and segments may add filled/empty asset paths. No format-string DSL. |
| `HudElementDefinition` | `new(id, presentation)` plus `.placement`, `.order`, `.visible_when`, `.fallback(HudValue)`, and `.style(HudStyleOverride)`. Registration validates binding/fallback kinds and nonzero bounded segment capacity. |
| `HudPlacement` | `HudAnchor` (nine variants), pixel `[f32; 2]` offset, `HudStackDirection::{Horizontal,Vertical}`, and signed order. Ties sort by `HudElementId` text. |
| `HudTheme` | Resource containing default typography, colors, padding/gaps, bar dimensions, icon/segment sizes, and default `z_index = 50`; `HudStyleOverride` is the same narrow set as `Option` fields. Loading (`100`) and debug (`200`) overlays remain above it. |
| Storage/inspection | `HudRegistry` and `HudValueSet` are app-wide resources with `register`/`insert_or_replace`/`remove`, keyed reads, monotonic revisions, and deterministic `HudElementSnapshot`/`HudValueSnapshot` output. They are inspection APIs, not persistence/config formats. |
| Errors/messages | `HudRegistryError::{DuplicateId,MissingId,InvalidDefinition}` and `HudValueError::{NonFinite,InvalidRange}` are returned synchronously. `set_hud_value` emits `HudValueChanged` only on real changes; `HudDiagnostic` reports deduplicated missing, mismatch, invalid, or asset-unavailable states without panics. |

Definitions and dynamic icon values store owned `AssetPath<'static>`; the renderer privately resolves and caches `Handle<Image>`. Public handles were rejected because they make deterministic snapshots and asset-free tests opaque. Runtime-generated handle-only images are deferred; solid-color/glyph-free defaults keep bar/segment tests asset-free.

## Architecture Decisions

| Concern | Choice and tradeoff |
|---|---|
| Plugin split | One public `VoxelHudPlugin` initializes model, messages, theme, and renderer. A model/default-renderer split is premature ceremony; internal boundaries preserve a future split. The unit plugin needs no `V`, default type parameter, `PhantomData`, `new`, or `default`. |
| Invalid input | Hide by default or apply a compatible author fallback; emit diagnostics only when failure state changes. Coercion was rejected because it hides author mistakes. |
| Reconciliation | Preserve each element root entity. Value/style changes mutate only affected components; placement changes reparent/reorder; presentation-kind changes rebuild only children. Segment capacity changes reconcile child count. |
| Domain integration | Authors publish explicitly in their own systems. Automatic bridges/selectors were rejected because ownership and schedule choices are game-specific. |

## Data Flow and Scheduling

```text
author systems -> HudRegistry/HudValueSet -> pure resolution -> retained ECS UI
                                              |-> HudDiagnostic
```

Public `HudSystemSet::{Publish,Reconcile,Render}` is chained in `Update`, with an explicit deferred-command barrier before rendering. Authors place same-frame publishers in `Publish`. `PostStartup` runs reconcile/barrier/render after every `Startup` registration, preventing first-frame races. Reconcile runs only on registry/theme revision changes; render runs on relevant value changes and compares cached resolved fingerprints before writing components.

## File Changes

| Path | Action |
|---|---|
| `src/ui/hud.rs` | Create model, pure resolution, plugin, retained renderer, inline tests. |
| `src/ui/mod.rs`, `src/prelude.rs`, `src/lib.rs`, `src/plugin_category.rs` | Re-export/document APIs and classify only `VoxelHudPlugin` as `DefaultImplementationPlugin`; leave `UiPlugin` and bundles unchanged. |
| `examples/hud.rs`, `Cargo.toml` | Add neutral four-widget example with no required feature. |
| `README.md`, `docs/hud.md`, `CHANGELOG.md` | Add usage, coexistence/custom-overlay migration, non-goals, and 0.8.0 notes. |

## Testing Strategy

Unit tests cover IDs, registration/replacement/errors, snapshots, scalar normalization/formatting, compatibility/fallbacks, anchors/theme resolution, ordering, and segment bounds. `MinimalPlugins` ECS tests assert startup/update order, unique stable roots, selective mutation, child reconciliation, diagnostics, and no asset I/O. Acceptance runs `cargo test`, `cargo check`, and example checks with default and `--no-default-features`; never `cargo build`.

## Migration / Rollout

Additive only: no save migration or feature flag. Existing `UiPlugin` and custom HUDs continue unchanged; consumers opt into `VoxelHudPlugin` and may migrate overlay-by-overlay by replacing direct queries with explicit publication systems.

## Open Questions

None.
