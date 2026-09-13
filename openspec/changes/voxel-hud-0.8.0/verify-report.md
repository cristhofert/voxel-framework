# Formal Verification Report: voxel-hud-0.8.0

**Verdict: INCOMPLETE — clean preflight pending; no release/archive approval.**
**Implementation/scenario review: PASS, 10/10 scenarios compliant.**
**Date:** 2026-09-13. **Backend:** HYBRID.
**Checkout:** `/home/cris/Projects/voxel-framework`, branch `codex/voxel-hud-0.8.0`, base HEAD `48d236b7b91ebdcaf5bc0ad98bb2fe14a563e63d` **plus reviewed uncommitted blocker fixes**. This is not verification of a new clean commit.

## Executive summary

Independently reviewed the combined MSRV, image lifecycle, CI/docs and final YAML/changelog changes. Both previous implementation blockers are resolved on this worktree: the minimum is truthfully 1.89 and actual Rust 1.89.0 library checks pass with/without defaults; genuine AssetServer regressions and a real PNG graphical consumer prove loaded images and all-four updates. Thirty-three HUD tests pass in each feature mode. No new concrete production defect or public-domain boundary regression was found. Task 7.4 remains pending, 18/19; only the complete clean-preflight gate remains an approval blocker.

## Inputs and evidence provenance

Loaded verification/memory skills and shared conventions; read CLAUDE, project config and local proposal/spec/design/tasks/progress/state. Full Engram planning observations recovered in the initial review: #712 proposal, #716 spec, #718 design, #723 tasks, #728 apply-progress, #713 state; full current report #786/state/progress recovered for reconciliation. The actual spec's ten scenarios remain authoritative.

The initial review rejected progress-only assertions of 19/19 because historical `/tmp` preflight logs were unavailable. Nothing in the blocker-fix evidence supplies that missing complete clean gate. The old archived package names clean base HEAD, but it does not prove the full script's command/exit/override environment. It now also predates the image/MSRV changes. No dirty override or preflight invocation was performed here. The orchestrator plans to commit separately, then run the full preflight; evidence must name that eventual candidate commit, not silently reuse base HEAD.

Reviewed actual production diff, raw final logs (including earlier failure logs), test assertions, graphical probe source and both screenshots. `/tmp/voxel-msrv-0.8/verification-summary.txt` correctly includes earlier failed checks followed by successful final rechecks, not only success claims. Its earlier HUD hash differs from the final asset worker hash; therefore fresh inexpensive cached 1.89 checks were run on the exact combined HUD source. Source SHA256 at this review: `eac2e1ac931100249dee26bf360445bf31fa5606c502fa32aa3e136e8b8aad08`, matching `/tmp/hud-loaded-image/results.md`.

## Completeness

| Metric | Value |
|---|---:|
| Planned tasks | 19 |
| Recorded complete | 18 |
| Pending | 7.4 |
| Compliant scenarios | 10/10 |
| Partial/failing scenarios | 0/0 |

Independent review and diff check portions of 7.4 pass. Its complete clean-tree `./scripts/release-preflight.sh 0.8.0` portion has NOT passed or been run by this review. Historical 7.1–7.3 recorded completion is bookkeeping for the earlier base; it does not certify the combined worktree full matrix or current package archive. Do not increment to 19/19 yet.

## Fresh focused execution

Commands run against the combined worktree; exit status 0 appears in each log:

| Command | Result | Absolute log |
|---|---|---|
| `cargo test --locked --offline --lib ui::hud` | 33 passed, 0 failed/ignored, 245 filtered | `/tmp/voxel-hud-combined-review/tests-default.log` |
| Same with `--no-default-features` | 33 passed, 0 failed/ignored, 198 filtered | `/tmp/voxel-hud-combined-review/tests-no-default.log` |
| `CARGO_TARGET_DIR=target/msrv-1.89.0 cargo +1.89.0 check --locked --offline --lib` | Passed, 1.85s | `/tmp/voxel-hud-combined-review/msrv-default.log` |
| Same with `--no-default-features` | Passed, 1.11s | `/tmp/voxel-hud-combined-review/msrv-no-default.log` |
| `cargo fmt --all -- --check` | Passed | `/tmp/voxel-hud-combined-review/format.log` |
| `git diff --check` | Passed | `/tmp/voxel-hud-combined-review/diff-check.log` |

Exact minimum compiler: rustc `1.89.0 (29483883e 2025-08-04)`, cargo `1.89.0 (c24e10642 2025-06-23)`, Linux x86_64-unknown-linux-gnu. Focused tests use current stable 1.94.1. Root lockfile is ignored/untracked, not committed; locked checks prove the current local dependency graph. No broad dependency update occurred.

`openspec/config.yaml` parses with Python YAML: configured test command cargo test, check command cargo check, coverage threshold 0 (disabled). Fresh execution deliberately targets HUD and minimum-compiler risks; no cargo build, full expensive historical matrix, all-features suite, rustdoc suite, package recreation or publish dry-run rerun. Unrelated tests are filtered, not passed.

## Grounded worker evidence reused

- `/tmp/voxel-msrv-0.8/recheck-default.log` and `recheck-no-default.log`: complete crate checks exit0 after resolving the intermediate missing AssetEvent::id method; summaries alone were not accepted. Fresh minimum-compiler results above supersede source-hash ambiguity.
- `/tmp/hud-loaded-image/tests-default.log`, `tests-no-default.log`: 33 passed each, exit0. `/tmp/hud-loaded-image/repeat-asset-tests.log`: four asset tests passed three repetitions, exit0. `/tmp/hud-loaded-image/red-recovery.log` and `red-reload-failure.log`: actual behavioral failed assertions before fixes, not compilation failures.
- Four genuine in-memory AssetServer integration regressions are in `ui::hud::asset_runtime_tests`; three use a controlled async RGBA Image loader and one also uses stock Bevy ImageLoader with embedded real PNG bytes. Do not call all four stock-PNG decode tests.
- `/tmp/hud-loaded-image/graphical-probe-final.log`: standalone public consumer exits0, asserts both PNG handles loaded and used by ImageNodes, registers neutral definitions and publishes through HudSystemSet::Publish. Probe source `/tmp/hud-loaded-image/probe/src/main.rs`, real PNGs `/tmp/hud-loaded-image/probe/assets/orange.png` and `blue.png`. HUD source matches final worker hash.
- Independently inspected `/tmp/hud-loaded-image/first.png` and `/tmp/hud-loaded-image/updated.png` (941×1030): legible default-font “HUD first”→“HUD updated”, center green bar25%→75%, top-right orange→blue icon, bottom-center five textured segments2 orange/3 blue→4 orange/1 blue. All four remain visible. These observations ground scenario10 consumer acceptance; ECS tests, not screenshots, prove identity.
- `/tmp/hud-loaded-image/gpu-escalated.log` grounds NVIDIA GTX1050Ti/Vulkan scene. Missing VK_EXT_physical_device_drm and VK_LAYER_KHRONOS_validation warnings remain; no validation-layer-clean claim.

## Specification compliance matrix

Original model/ECS tests below are in `/home/cris/Projects/voxel-framework/src/ui/hud.rs`, `ui::hud::tests`; new lifecycle tests are in `ui::hud::asset_runtime_tests`. All 33 passed in BOTH fresh combined-review logs. The same four presentation types remain headless-testable; visual consumer evidence is additional.

| # | Requirement / exact scenario | Behavioral evidence | Result |
|---|---|---|---|
| 1 | Publication and registration / Publish and replace a value | `publication_helpers_emit_only_real_change_messages` (ready→active and previous/current); `retained_renderer_preserves_roots_and_reconciles_only_changed_children` (replacement keyed value renders); `value_set_tracks_real_changes_removal_and_ordered_snapshots` | COMPLIANT |
| 2 | Publication and registration / Register a duplicate | `registry_tracks_only_real_definition_changes` (duplicate error, explicit replacement readable); `registry_duplicate_error_precedes_revalidation_and_invalid_replace_is_atomic` | COMPLIANT |
| 3 | MVP presentations and safe inputs / Resolve all presentations | `resolution_handles_all_presentations_and_bounds_numeric_output` (all four, injected available-image predicate); `resolution_formats_supported_text_values_without_coercing_kinds` | COMPLIANT; additionally covered by real loaded-image tests |
| 4 | MVP presentations and safe inputs / Resolve invalid data | `scalar_rejects_non_finite_and_degenerate_ranges`; `missing_and_mismatched_values_use_only_compatible_fallbacks`; `invalid_definition_and_theme_inputs_hide_without_panicking`; `excessive_segments_are_never_clamped_and_may_use_a_bounded_fallback`; `missing_asset_server_hides_images_without_starting_asset_io`; `ecs_diagnostics_deduplicate_and_reemit_after_recovery` | COMPLIANT |
| 5 | Retained reconciliation / Update retained state | `publish_set_updates_startup_definition_in_the_same_app_update`; `retained_renderer_preserves_roots_and_reconciles_only_changed_children`; `bar_render_transition_mutates_fill_without_rebuilding_tree` | COMPLIANT |
| 6 | Retained reconciliation / Process an unchanged frame | `empty_standalone_plugin_starts_headlessly_and_keeps_structure_stable` (canvas/18 anchor identities); `hud_and_existing_ui_plugin_coexist_without_replacing_overlays` (nonempty retained root); `unchanged_and_unrelated_values_do_not_mutate_retained_text` (no changed Text) | COMPLIANT; new all-four asset test directly asserts every root/child identity on unchanged frames |
| 7 | Placement and theme / Resolve layout and style | `every_anchor_and_style_field_resolves_from_author_configuration`; `resolution_merges_style_maps_anchors_and_orders_ties_by_id`; `reconciliation_orders_ties_and_reparents_without_replacing_roots` | COMPLIANT |
| 8 | Independent composition / Compose plugins | `empty_standalone_plugin_starts_headlessly_and_keeps_structure_stable`; `hud_and_existing_ui_plugin_coexist_without_replacing_overlays` (legacy UI prerequisites explicitly seeded, no gameplay plugins) | COMPLIANT |
| 9 | Deterministic inspection and headless tests / Verify headlessly | `registry_snapshot_is_identity_ordered_and_serializable`; `value_set_tracks_real_changes_removal_and_ordered_snapshots`; MinimalPlugins ECS tests including no-AssetServer test | COMPLIANT |
| 10 | Consumer acceptance / Follow the example | `asset_runtime_tests::real_assets_render_and_update_all_four_presentations_and_reuse_cached_handles`; `asset_runtime_tests::real_png_loader_and_gated_icon_show_expected_image_nodes_without_gpu`; public standalone consumer setup/publication; independently inspected first/updated screenshots | COMPLIANT |

**Summary: 10/10 COMPLIANT, 0 partial, 0 failing.** Scenario10 is now grounded by real loaded-image ECS assertions plus a public consumer following the documented registration/publication pattern; the bundled headless example intentionally remains asset-free and hides illustrative icons. It has not been transformed into a graphical example.

## Combined diff / invariants review

- Production HUD changes are limited to private optional typed Image event/failure cursors and early-return invalidation. Event folding consumes all messages, including after the first relevant event; only currently cached handle IDs invalidate resolution. Unrelated assets do not rewrite HUD. Asset-free MinimalPlugins startup is preserved by optional message resources/AssetServer; no implicit Image/Asset/gameplay plugin installation.
- Both same-path defects are covered: failed→successful reload no longer remains hidden; loaded→failed reload no longer remains visible, without publication. Failure messages matter because failed loads may produce no successful Image asset event. Pending polling, fallback resolution, transition diagnostics, strong handle caching/pruning and per-element fingerprints remain intact.
- New tests directly assert loaded ImageNode handles/Assets presence, real async pending and failed loaders, compatible fallback, recovery/reentry diagnostics, shared handle/load reuse, unrelated image-event selectivity, and cache pruning. All-four unchanged frames explicitly preserve every root and child identity/count and produce no ImageNode writes; the earlier suggested coverage gap is closed.
- Existing root/child query-disjointness filters, Publish→Reconcile→ApplyDeferred→Render chains in PostStartup/Update, startup registration and same-frame publication are unchanged. No retained root replacement, per-frame tree rebuild, semantic coercion, VoxelType dependency, automatic player/domain bridge, preset, interaction, persistence/config DSL or new feature was added. New helper visibility is test-only under cfg(test); no public API expansion.
- Cargo minimum1.89, README, AGENTS, CLAUDE, llms minimum statement and OpenSpec current stack agree. CI adds isolated exact1.89.0 Linux default/no-default library checks with fresh lockfile generation, audio development dependencies and normal read-only permissions. Existing jobs/triggers remain unchanged. README/CLAUDE correctly qualify local lockfile vs fresh consumer/other-target resolutions. GitHub lane was parsed/reviewed, NOT executed remotely.
- Final `/home/cris/Projects/voxel-framework/openspec/config.yaml` syntax repair preserves apply instructions under an instructions list while retaining tdd/test fields and verify rules. Parsed config/state/CI YAML successfully (`metadata-review.log`). Config and changelog were read after their worker updated them; this reviewer edited neither.
- Final 0.8 CHANGELOG Fixed section accurately states corrected1.89 minimum and both image reload outcomes. Historical0.7 and older changelog sections byte-identical to HEAD. Date/version/install conditional-publication guidance remains unchanged; no publication/tag availability asserted.

## Approval blocker and remaining work

**Only pending approval gate: task7.4 clean full preflight.** Orchestrator must separately commit the stabilized combined changes, run `./scripts/release-preflight.sh 0.8.0` without RELEASE_PREFLIGHT_ALLOW_DIRTY, and retain complete exit0 output plus exact new commit/clean-tree provenance. The new run must validate the complete matrix/docs/package for that candidate. This report does not approve release/archive or claim19/19.

Previous MSRV mismatch and loaded-image consumer blockers are CLOSED by reviewed corrections and independent execution. YAML syntax/release-note repairs are also reviewed and complete. No concrete additional production defect identified; reviewer made no production edits.

## Residual risks / warnings (not scenario blockers)

- Visual tests cover the observed default-font text and four placements on one NVIDIA/Vulkan single-camera/window scene. Still untested: exhaustive nine-anchor/direction multi-widget stacking, DPI/scaling matrix, overlay/loading/debug ZIndex overlap, custom font/Unicode shaping, multiple cameras/windows, visual failure/fallback transitions, other GPUs/backends/compressed textures. Do NOT describe all font/layout/GPU behavior as untested now.
- Asset lifecycle coverage now includes genuine loading/failure/fallback/recovery, same-path explicit reloads, path publication updates, textured segments, shared caching/pruning and stock PNG decode. OS file-watcher-triggered hot reload, broad formats, multiple asset sources/server replacement and generalized direct Assets removal are not independently tested. Controlled explicit reload tests are not a full filesystem-watcher matrix.
- Current package was not recreated: prior archive121 files proves base-HEAD package inclusion/exclusions only, NOT final HUD/MSRV/docs contents. Final full preflight must package candidate fixes. Prior archive/audit `/home/cris/Projects/voxel-framework/target/package/voxel-framework-0.8.0.crate`, `/tmp/voxel-hud-formal-48d236b/archive-audit.log`.
- Pre-existing docs drift remains nonblocking: llms.txt still introduces old0.4 release status/core “pure engine” language despite corrected current0.8 MSRV; config distribution text still says explicitly not crates.io while public docs describe future released installs; README CI-mirrors-publish-dry-run wording exceeds actual CI/preflight commands. No unauthorized documentation scope expansion performed.
- Minimum checks prove local locked Linux library graph, not all examples/features/platforms on1.89. CI fresh lock resolution and ignored local lockfile mean dependencies are not determined by Git HEAD alone. Full historical matrix not redundantly reproduced here.

## Persistence / audit artifacts

Report `/home/cris/Projects/voxel-framework/openspec/changes/voxel-hud-0.8.0/verify-report.md`, state/apply-progress/tasks reconciled to10 compliant and18/19 with7.4 pending; corresponding Engram canonical topics synchronized. Archive-report remains false, approval false.

Combined diff and source provenance `/tmp/voxel-hud-combined-review/combined-production.diff`, `review-start-hashes.log`, `review-final-hashes.log`, `metadata-review.log`, focused logs above; prior report preserved at `/tmp/voxel-hud-combined-review/prior-verify-report.md`. Original clean-gate audit evidence remains `/tmp/voxel-hud-formal-48d236b/initial-progress.diff` and `initial-status.log`.

**Next recommended:** orchestrator-controlled candidate commit then complete clean preflight; reconcile formal approval only after valid evidence. No commit/Git writes, publish/tag/archive, config/changelog or production edits by this reviewer.
