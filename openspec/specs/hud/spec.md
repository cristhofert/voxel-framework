# HUD Specification

## Purpose

Define game-neutral retained HUD behavior for author-published values.

## Requirements

### Requirement: Publication and registration

The system MUST publish generic typed values under `HudValueId` without domain or voxel-type dependencies. `HudRegistry` MUST reject duplicate `HudElementId` registration, offer explicit replacement, and provide keyed reads and membership consistent with content, stats, and effects registries.

#### Scenario: Publish and replace a value
- GIVEN a value exists under an id
- WHEN another compatible value is published under that id
- THEN keyed reads MUST return the replacement without game semantics

#### Scenario: Register a duplicate
- GIVEN an element id is registered
- WHEN registration and explicit replacement are attempted
- THEN registration MUST return a duplicate-id error and replacement MUST become readable

### Requirement: MVP presentations and safe inputs

Rust definitions MUST support text, continuous bar, icon, and bounded repeated segments. Bars MUST resolve a bounded proportion and segments a bounded count. Missing, mismatched, non-finite, invalid-range, or excessive values MUST NOT panic, MUST use a deterministic hidden or configured fallback, and SHOULD produce diagnostics.

#### Scenario: Resolve all presentations
- GIVEN valid definitions and compatible values for all four presentations
- WHEN presentation state is resolved
- THEN text, bar, icon, and segment state MUST match author data

#### Scenario: Resolve invalid data
- GIVEN a presentation receives missing or invalid data
- WHEN resolved or rendered
- THEN its safe state MUST be used and the application MUST continue

### Requirement: Retained reconciliation

The renderer MUST key entities by element identity, preserve identity across updates, mutate affected state, and MUST NOT rebuild unchanged trees each frame. Definition changes MUST reconcile without duplicate identities.

#### Scenario: Update retained state
- GIVEN an element registered during `Startup` is rendered with identity
- WHEN its value changes before the HUD update
- THEN identity MUST reflect the value in that update

#### Scenario: Process an unchanged frame
- GIVEN definitions and values are unchanged
- WHEN another update runs
- THEN element identities and child counts MUST remain unchanged

### Requirement: Placement and theme

Definitions MUST support nine screen anchors, pixel offsets, stack direction, stable ordering, a default theme, and narrow overrides. Equal-order elements MUST resolve deterministically.

#### Scenario: Resolve layout and style
- GIVEN elements share an anchor and some override theme values
- WHEN layout is resolved
- THEN offset, stack, deterministic order, defaults, and overrides MUST be applied

### Requirement: Independent composition

`VoxelHudPlugin` MUST initialize an empty HUD without gameplay plugins or default Cargo features. It MUST remain explicit, MUST NOT join `UiPlugin` or convenience bundles, and MUST coexist with other UI.

#### Scenario: Compose plugins
- GIVEN an app uses `VoxelHudPlugin` alone or alongside `UiPlugin`
- WHEN it starts with zero or more HUD definitions
- THEN it MUST initialize without domain prerequisites or replacing existing overlays

### Requirement: Deterministic inspection and headless tests

Registries and value sets MUST provide keyed reads and deterministic identity-ordered inspection; inspection MUST NOT imply persistence or configuration. Model behavior MUST be headless-testable, and renderer behavior testable through ECS state without screenshots or asset I/O.

#### Scenario: Verify headlessly
- GIVEN equivalent data inserted in different orders under `MinimalPlugins`
- WHEN inspection and updates are tested
- THEN inspection MUST match and state MUST be assertable without GPU, window, filesystem, or gameplay plugins

### Requirement: Consumer acceptance

Public exports, a neutral example, migration guidance, and 0.8.0 release notes MUST document registration, publication, updates, fallbacks, and composition. The example MUST check default and no-default-feature configurations.

#### Scenario: Follow the example
- GIVEN a consumer has no gameplay-domain plugins
- WHEN they follow the documentation
- THEN all four presentations MUST render and update without reserved ids or semantics

## Non-goals

The HUD MUST NOT define game semantics, automatic stats/effects/content bridges, rules, interactions, a JSON/config DSL, a full stylesheet, or accessibility beyond supported Bevy basics. Authors MUST explicitly publish external-domain data.
