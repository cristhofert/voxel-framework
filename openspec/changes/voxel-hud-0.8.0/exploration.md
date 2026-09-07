# Exploration: voxel-hud-0.8.0

## Current State

`voxel-framework` 0.7 has three deliberately horizontal author-defined layers:
content (`ContentId` plus registries and arbitrary `ContentValue` attributes),
stats (`StatId`, `StatRegistry`, global or entity-local `StatSet`, normalized
read helpers, and `StatChanged`), and effects (`EffectId`, `EffectRegistry`,
global or entity-local `ActiveEffectSet`, duration/stack inspection, and
lifecycle messages). None registers game semantics or presets.

The current `ui` module predates those layers. It contains independent Bevy UI
plugins with one configuration resource each (`CrosshairPlugin`,
`LoadingScreenPlugin`, item-gated `HotbarUiPlugin`, item-gated
`PickupNotificationPlugin`, `PauseMenuPlugin`, and `DebugOverlayPlugin`). Their
common pattern is: initialize a resource, spawn a fixed entity tree at startup,
then update `Text`, `Node`, `BackgroundColor`, or `Visibility` from a resource or
message. `UiPlugin` is a default umbrella for the fixed crosshair/loading/hotbar/
pickup set. There is no general HUD registry, value binding abstraction, theme,
or reusable bar/icon/segment presentation.

The audio subsystem provides a useful separation precedent: `SfxPlugin`
produces backend-neutral requests and `DefaultAudioPlugin` provides replaceable
Bevy playback. The existing `minetest` example also proves that custom HUDs are
possible, but its effect panel rebuilds every row every frame and binds directly
to the older player-specific `ActiveEffects<MinetestEffect>` model. That is an
example-local solution, not a reusable 0.8 API.

## Constraints and Design Goals

- The framework MUST remain a library, not define a game. It must not introduce
  health, oxygen, hunger, poison, buff/debuff, heart, or survival semantics.
- The author MUST be able to display arbitrary values without adding content,
  stats, or effects plugins. Those domains may be sources, never prerequisites.
- Configuration is explicit Rust API. JSON/YAML declarations, a schema language,
  and an editor are outside 0.8.
- Presentation must cover at least text, continuous bars, icons, and a generic
  repeated-segment form from which an author can create hearts. “Hearts” is a
  visual choice, not a health model.
- Placement must be author-selectable rather than fixed. A small screen-anchor
  model (nine anchors, offset, stack direction/order) is preferable to exposing
  all Bevy `Node` layout details in the first release.
- Theme data must be separable from value bindings. A global/default theme plus
  per-widget overrides is sufficient for 0.8; a full stylesheet/cascade is not.
- Registration, value publication, and rendering order must be explicit. Existing
  author-defined registries are commonly populated in `Startup`, so HUD entity
  creation cannot race those systems.
- Runtime updates should mutate retained entities. Rebuilding complete widget
  trees every frame, as the example effect HUD currently does, is not suitable as
  the framework pattern.
- Rules are deferred to 0.10+ and interactions to 0.9. HUD code may visualize
  values but must not threshold, drain, damage, apply effects, or trigger gameplay.

## Affected Areas

- `src/ui/mod.rs` — current fixed overlays and umbrella; likely host or re-export
  point for the default Bevy HUD renderer.
- `src/audio/mod.rs` — precedent for separating a horizontal request/model layer
  from a replaceable default backend.
- `src/stats/mod.rs` — optional source with bounds, normalization, global/entity
  scopes, and `StatChanged`; must not become a hard HUD dependency.
- `src/effects/mod.rs` — optional source with stacks and remaining ratios;
  collection ownership and entity scope make an automatic bridge non-trivial.
- `src/content/mod.rs` — optional metadata source for display names, colors, and
  author attributes; these remain author data, not reserved HUD keys.
- `src/player/effects.rs` and `src/player/mod.rs` — older player-specific health/
  effects APIs must not define the new generic HUD contract.
- `src/lib.rs` and `src/plugin_category.rs` — public module/plugin exposure and
  Default Implementation classification.
- `src/prelude.rs` — re-exports for the selected public HUD surface.
- `Cargo.toml` — feature/version analysis; no new feature is currently justified.
- `examples/` and `README.md` — eventual explicit Rust registration/publication
  example; should demonstrate neutral identifiers rather than a game preset.

## Approaches

### 1. Domain-bound HUD definitions

Define sources such as `HudSource::Stat(StatId)`,
`HudSource::Effect(EffectId)`, `HudSource::PlayerHealth`, and content attribute
lookups directly in each widget definition. One plugin resolves those sources and
renders Bevy UI.

- **Pros:** very little consumer glue; stat/effect examples are concise; bounds,
  labels, colors, and durations can be inferred.
- **Cons:** couples a presentation plugin to optional gameplay domains; global vs
  entity-local ownership requires selectors or player assumptions; every future
  data source expands the core enum; `PlayerHealth` and effect-specific cases pull
  vertical semantics into 0.8; difficult to show an arbitrary game resource.
- **Effort:** Medium initially, High as source types grow.
- **Assessment:** viable for a game or a separate convenience adapter package,
  but a poor primary framework architecture.

### 2. Generic HUD registry plus author-published values

Add an empty author-defined HUD registry and value store. Widget definitions bind
`HudElementId` to a neutral `HudValueId`; author systems publish typed values such
as number-with-range, text, boolean/visibility, or icon selection. A retained Bevy
UI renderer supports `Text`, `Bar`, `Icon`, and generic `Segments` presentations,
plus placement and theme configuration. It never queries stats, effects, content,
player health, or game entities directly.

A minimum public vocabulary could be:

- identity/data: `HudElementId`, `HudValueId`, `HudRegistry`, `HudValueSet`,
  `HudValue`;
- definition: `HudElementDefinition`, `HudPresentation`, `HudPlacement`,
  `HudTheme`;
- lifecycle: one explicit `VoxelHudPlugin` (or `HudPlugin`; naming should be
  finalized in proposal/design), an empty registry by default, and optional
  change notification only when a published value actually changes.

Bars consume a ranged numeric value. Text uses explicit label/precision/suffix
options rather than interpreting arbitrary format strings. Icons consume a
static or author-selected visual. Segments consume a normalized/count value and
can render hearts, shields, pips, cells, or any other repeated glyph/image.

- **Pros:** satisfies arbitrary values and strict domain independence; simple
  Rust registration; stable extension point for later sources; easy headless
  model tests; retained rendering avoids per-frame respawn; the author controls
  exactly what gameplay data crosses into presentation.
- **Cons:** consumers write small publishing systems; duplicate IDs/type mismatch
  need diagnostics; image/font handle lifecycle and first-render schedule require
  careful design; a single plugin mixes model and default renderer internally.
- **Effort:** Medium.
- **Assessment:** best minimum viable 0.8 architecture.

The model and renderer should be separate internal layers from day one. A second
public renderer plugin, analogous to `DefaultAudioPlugin`, can be introduced only
if design shows a real headless/custom-backend benefit that outweighs the extra
plugin ceremony. The public registry/value types must not prevent that later
split.

### 3. ECS UI primitives only

Expose public components/bundles such as `HudBar`, `HudText`, `HudIcon`,
`HudSegments`, and `HudAnchor`. Authors spawn the entity tree and write ordinary
systems that mutate those components. The plugin performs only visual updates.

- **Pros:** maximally Bevy-native; no registry or source language; consumers own
  dynamic collections and layout; easy to combine with custom UI.
- **Cons:** substantial boilerplate; configuration is scattered through entity
  spawning; hard to validate duplicate bindings or apply coherent themes; public
  API becomes tightly coupled to Bevy UI component changes; provides little more
  than raw Bevy for the common “map value to HUD” use case.
- **Effort:** Low for framework, High for every consumer.
- **Assessment:** viable as lower-level escape hatches, not as the primary 0.8 API.

## Recommendation

Proceed with **Approach 2: a generic registry/value publication boundary with a
retained Bevy UI renderer**.

Minimum viable 0.8 scope:

1. Empty HUD registry and value store; no built-in elements and no reserved ids.
2. Explicit Rust builder API for text, bar, icon, and generic segments.
3. Nine-position screen anchoring, pixel offset, stack direction, and stable
   ordering. Arbitrary nested/grid layout is deferred.
4. One default theme resource with colors, typography, spacing, bar dimensions,
   and segment/icon defaults, plus narrow per-element overrides.
5. Typed publication with validation and predictable missing/type-mismatch
   behavior: keep the widget hidden or show an author-configurable fallback;
   never panic during frame updates.
6. Retained entities keyed by element id; only changed text/style/width/
   visibility is mutated.
7. Registration ordering that is safe with author `Startup` systems (for example,
   reconcile after startup and on registry revision changes). Value rendering
   should occur after normal gameplay publication without forcing gameplay
   system ordering into the plugin.
8. Public re-exports and Default Implementation plugin-category registration.
9. A headless example/test path that publishes neutral values directly.

Do not make automatic stats/effects/content bridge plugins part of the initial
minimum. Global vs entity-local ownership, effect collections, and source-system
ordering make “automatic” adapters opinionated. Instead, document tiny author
systems that publish `StatSet::normalized`, effect remaining ratio/stacks, or
content display metadata into `HudValueSet`. Pure conversion helpers may be added
only when they do not select an entity, reserve attribute names, register domain
plugins, or apply rules. A later convenience adapter can be considered from real
consumer repetition.

Keep the new plugin explicit in 0.8.0: do not add it to `VoxelCorePlugin`, and do
not replace/deprecate existing `UiPlugin` overlays. Adding it to `UiPlugin` would
be behaviorally quiet with an empty registry, but explicit opt-in better protects
the flat catalog while the public API settles.

## Boundaries and Non-Goals

- No default health/oxygen/hunger/stamina/effect ids, icons, thresholds, colors,
  or HUD layouts.
- No rules, alarms, damage, regeneration, effect application, interaction prompts,
  mining/placement interaction changes, or 0.9/0.10 work.
- No JSON/YAML/TOML HUD declarations, persistence format, reflection editor, or
  hot-reload schema.
- No automatic player discovery or privileged “player” entity.
- No general menu framework, inventory redesign, pause-menu rewrite, responsive
  stylesheet, animation timeline, localization system, or accessibility suite.
- No arbitrary dynamic list/repeater templating in the MVP. Known widgets can be
  shown/hidden and segments can repeat a bounded visual; author-created effect
  lists remain custom until a neutral collection contract is proven.
- No forced integration with the legacy `PlayerHealth` or
  `player::effects::StatusEffectsPlugin` APIs.

## Testability and Headless Considerations

- Unit-test IDs, duplicate registration, value replacement/change detection,
  range normalization/clamping, numeric formatting, segment fill counts, anchor
  resolution, theme override resolution, and missing/type-mismatch fallbacks as
  pure logic.
- With `MinimalPlugins`, verify the model plugin initializes empty resources and
  registers messages without a window, renderer, audio, filesystem, or GPU.
- Renderer tests should inspect spawned ECS components (`Text`, `Node`,
  `Visibility`, colors, and child counts) rather than screenshots.
- Keep asset loading outside pure resolution. Image icons should accept Bevy
  handles or another design-time-resolved representation so tests can use default
  handles and avoid filesystem I/O. Glyph/solid-color fallbacks keep the basic
  path asset-free.
- Lock schedule behavior with app tests: definitions registered during `Startup`
  are rendered; same-frame published values are visible in the intended render
  phase; unchanged values do not rebuild entities.

## Migration and Feature-Flag Implications

- This should be additive public API with no chunk/world/item save migration.
- No new Cargo feature is recommended for 0.8. Bevy UI/text/sprite/asset support is
  already unconditional in this crate, so a `hud` flag would add API fragmentation
  without meaningful dependency savings. Revisit only as part of a broader UI
  dependency split.
- Any item-specific convenience remains under `items`; the generic HUD must
  compile and operate under `--no-default-features`.
- Existing `UiPlugin`, custom UIs, and downstream games remain valid. README and
  changelog updates belong to implementation/archive after the API is accepted.

## Risks

- A value enum that is too broad becomes a second generic data language; one that
  is too narrow cannot support icons/segments without churn.
- Public placement/theme types can accidentally freeze Bevy layout details or
  become an unbounded stylesheet API. Keep 0.8 intentionally small.
- Startup/update ordering can cause missing first-frame widgets or one-frame lag
  unless schedule contracts are explicit and tested.
- Automatic adapters can silently choose the wrong global/entity-local source or
  introduce hidden plugin dependencies.
- Image and font handles couple definitions to asset lifecycle; storing asset
  paths instead would introduce loader policy. The design phase must pick one
  explicit contract and preserve a no-asset fallback.
- Dynamic registry edits and effect-list rendering can expand scope sharply.
  Treat runtime reconciliation as bounded support, not a general declarative UI
  engine.
- `UiPlugin` and a new HUD plugin may confuse consumers unless docs clearly
  distinguish existing fixed defaults from author-defined HUD mapping.

## Ready for Proposal

**Yes.** The proposal should lock the neutral value-publication boundary,
explicit opt-in, MVP widget/placement/theme vocabulary, no automatic domain
bridges, and the non-goal line separating 0.8 presentation from 0.9 interactions
and 0.10+ rules. Exact type names, icon asset representation, and whether the
model/renderer split is one or two public plugins should be resolved in design
without changing those boundaries.
