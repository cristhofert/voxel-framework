# Proposal: Author-Defined Voxel HUD for 0.8.0

## Intent

Add a reusable, author-controlled HUD publication boundary and retained renderer without game semantics or per-frame UI rebuilding.

## Scope

### In Scope
- Public vocabulary: `HudElementId`, `HudValueId`, `HudRegistry`, `HudValueSet`, `HudValue`, `HudElementDefinition`, `HudPresentation`, `HudPlacement`, `HudTheme`, and explicit `VoxelHudPlugin`; fields remain design work.
- Rust-configured text, continuous bar, icon, and bounded repeated-segment presentations.
- Nine screen anchors, pixel offsets, stack direction, stable ordering, one default theme, and narrow per-element overrides.
- Retained entities keyed by element identity, safe ordering, diagnostics, and non-panicking missing/type-mismatch behavior.
- Public exports, tests, neutral example, migration guide, and release docs.

### Out of Scope
- Game presets, rules, thresholds, interactions, or automatic player discovery.
- Automatic stats/effects/content ownership or bridges.
- JSON/YAML, persistence, editors, hot reload, arbitrary layouts/lists, animation, localization, or accessibility suites.
- A vertical bundle or new Cargo feature.

## Approach

Authors register definitions in `HudRegistry` and publish neutral typed values through `HudValueSet`. `VoxelHudPlugin` reconciles registrations and mutates retained entities only when inputs change. Values remain independent of voxel type and gameplay domains. Existing `UiPlugin` overlays remain unchanged; HUD joins neither `UiPlugin` nor `VoxelCorePlugin`/`VoxelPlugin`. Migration docs show coexistence and conversion of custom overlays.

## Affected Areas

| Area | Impact | Description |
|---|---|---|
| `src/ui/` | New/Modified | HUD model, renderer, scheduling, compatibility |
| `src/lib.rs`, `src/prelude.rs`, `src/plugin_category.rs` | Modified | Public exposure and Default Implementation classification |
| `examples/`, `tests/`, docs | Modified | Neutral usage, migration, release verification |

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Value/theme APIs overgrow | Med | Keep MVP vocabulary closed and defer low-level shapes to design |
| First-frame lag or rebuilds | Med | Specify schedule contracts and ECS identity tests |
| `UiPlugin` confusion | Med | Explicit opt-in and migration/coexistence documentation |

## Rollback Plan

Remove the additive HUD module, exports, example, and docs; `UiPlugin` and persisted worlds remain untouched.

## Dependencies

- Existing Bevy UI/text/sprite/asset support; no new dependency.

## Success Criteria

- [ ] Neutral text/bar/icon/segments render and update under default and `--no-default-features` configurations.
- [ ] Tests cover model validation, normalization/formatting, placement/theme resolution, startup/update ordering, fallback behavior, and retained-entity updates.
- [ ] Example/docs explain publication, `UiPlugin` migration/coexistence, non-goals, and no domain prerequisites.
- [ ] `cargo test` and `cargo check` pass; 0.8.0 release notes document the additive API.
