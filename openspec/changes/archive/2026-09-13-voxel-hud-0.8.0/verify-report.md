# Formal Verification Report: voxel-hud-0.8.0

**Verdict: PASS — formal SDD verification complete, with documented residual limitations.**
**Implementation/scenario review: PASS, 10/10 scenarios compliant.**
**Date:** 2026-09-13. **Backend:** HYBRID.
**Checkout:** `/home/cris/Projects/voxel-framework`, branch `codex/voxel-hud-0.8.0`, tested committed HEAD `799ab9130ce8e19d30d84da02a1da2682b31cd5d`. The gate ran on a clean tree; final report edits change only package-excluded OpenSpec artifacts, not the tested product.

## Executive summary

Independently reviewed the combined MSRV, image lifecycle, CI/docs and final YAML/changelog changes. Both previous implementation blockers are resolved on this worktree: the minimum is truthfully 1.89 and actual Rust 1.89.0 library checks pass with/without defaults; genuine AssetServer regressions and a real PNG graphical consumer prove loaded images and all-four updates. Thirty-three HUD tests pass in each feature mode. No new concrete production defect or public-domain boundary regression was found. The complete clean preflight and supplemental publish dry-run/no-default rustdoc evidence now close task7.4: **19/19 tasks,10/10 scenarios, no pending approval blockers**. PASS is verification only, not actual publication or an archive operation.

## Inputs and evidence provenance

Loaded verification/memory skills and shared conventions; read CLAUDE, project config and local proposal/spec/design/tasks/progress/state. Full Engram planning observations recovered in the initial review: #712 proposal, #716 spec, #718 design, #723 tasks, #728 apply-progress, #713 state; full current report #786/state/progress recovered for reconciliation. The actual spec's ten scenarios remain authoritative.

The initial review correctly rejected ungrounded19/19 summaries. That historical gap is now closed by the new complete committed-candidate logs, not by retroactively accepting the original claims. Independent finalization observed clean HEAD799ab91 before artifact writes; worker #808 records initial/final clean status. The current archive VCS names799ab91 without a dirty flag and byte-matches the tested source/docs/metadata/local lock. All previously independently reviewed product hashes match this commit. The script unconditionally adds --allow-dirty to package commands if RELEASE_PREFLIGHT_ALLOW_DIRTY=1; both logged package commands omit that flag, corroborating no override. Script's clean-tree protection and complete exit0 output therefore support the clean gate. See final evidence below. No expensive command rerun or publication was performed by the finalizer.

Reviewed actual production diff, raw final logs (including earlier failure logs), test assertions, graphical probe source and both screenshots. `/tmp/voxel-msrv-0.8/verification-summary.txt` correctly includes earlier failed checks followed by successful final rechecks, not only success claims. Its earlier HUD hash differs from the final asset worker hash; therefore fresh inexpensive cached 1.89 checks were run on the exact combined HUD source. Source SHA256 at this review: `eac2e1ac931100249dee26bf360445bf31fa5606c502fa32aa3e136e8b8aad08`, matching `/tmp/hud-loaded-image/results.md`.

## Completeness

| Metric | Value |
|---|---:|
| Planned tasks | 19 |
| Recorded complete | 19 |
| Pending | None |
| Compliant scenarios | 10/10 |
| Partial/failing scenarios | 0/0 |

Task7.4 is complete: independent scenario/non-goal/invariants review, diff check, and complete clean-tree preflight pass on committed799ab91. Recorded historical matrix claims are no longer needed as substitutes for raw final evidence. No task remains pending.

## Prior independent focused execution (product hashes unchanged)

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

`openspec/config.yaml` parses with Python YAML: configured test command cargo test, check command cargo check, coverage threshold 0 (disabled). Those earlier focused runs target HUD and minimum-compiler risks; their unrelated tests were filtered. The final complete matrix is separately evidenced below, including unfiltered suites, package recreation and publish dry-run. The finalizer did not rerun any expensive command or run cargo build. Coverage remains unmeasured/disabled.

## Grounded worker evidence reused

- `/tmp/voxel-msrv-0.8/recheck-default.log` and `recheck-no-default.log`: complete crate checks exit0 after resolving the intermediate missing AssetEvent::id method; summaries alone were not accepted. Fresh minimum-compiler results above supersede source-hash ambiguity.
- `/tmp/hud-loaded-image/tests-default.log`, `tests-no-default.log`: 33 passed each, exit0. `/tmp/hud-loaded-image/repeat-asset-tests.log`: four asset tests passed three repetitions, exit0. `/tmp/hud-loaded-image/red-recovery.log` and `red-reload-failure.log`: actual behavioral failed assertions before fixes, not compilation failures.
- Four genuine in-memory AssetServer integration regressions are in `ui::hud::asset_runtime_tests`; three use a controlled async RGBA Image loader and one also uses stock Bevy ImageLoader with embedded real PNG bytes. Do not call all four stock-PNG decode tests.
- `/tmp/hud-loaded-image/graphical-probe-final.log`: standalone public consumer exits0, asserts both PNG handles loaded and used by ImageNodes, registers neutral definitions and publishes through HudSystemSet::Publish. Probe source `/tmp/hud-loaded-image/probe/src/main.rs`, real PNGs `/tmp/hud-loaded-image/probe/assets/orange.png` and `blue.png`. HUD source matches final worker hash.
- Independently inspected `/tmp/hud-loaded-image/first.png` and `/tmp/hud-loaded-image/updated.png` (941×1030): legible default-font “HUD first”→“HUD updated”, center green bar25%→75%, top-right orange→blue icon, bottom-center five textured segments2 orange/3 blue→4 orange/1 blue. All four remain visible. These observations ground scenario10 consumer acceptance; ECS tests, not screenshots, prove identity.
- `/tmp/hud-loaded-image/gpu-escalated.log` grounds NVIDIA GTX1050Ti/Vulkan scene. Missing VK_EXT_physical_device_drm and VK_LAYER_KHRONOS_validation warnings remain; no validation-layer-clean claim.

## Specification compliance matrix

Original model/ECS tests below are in `/home/cris/Projects/voxel-framework/src/ui/hud.rs`, `ui::hud::tests`; new lifecycle tests are in `ui::hud::asset_runtime_tests`. All33 passed in the prior independent combined-review logs AND in each of default/no-default/all-feature suites of the final clean-preflight log. The same four presentation types remain headless-testable; visual consumer evidence is additional.

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

## Final committed clean-gate evidence

The finalizer read all1427 preflight lines and both supplemental logs, audited all commands/summaries, all33 HUD successes in each mode, and the package listing. Only individual successful/ignored unrelated test lines were suppressed in the displayed audit; none were skipped in reading. No failing or unexpected test status was found.

| Evidence / command | Result |
|---|---|
| `./scripts/release-preflight.sh 0.8.0` | **PREFLIGHT_EXIT_STATUS=0**, complete final success line, no dirty override |
| Formatting; default/all-feature all-target checks; no-default check | Passed |
| Default tests |278 unit +19 integration +110 doctests passed;3 doctests ignored |
| No-default tests |231 unit +19 integration +87 doctests passed;3 doctests ignored |
| All-feature tests |278 unit +19 integration +110 doctests passed;3 doctests ignored |
| `cargo test --locked --examples` |Six eligible harnesses passed,0tests each; not a graphical playtest |
| `cargo doc --locked --no-deps --all-features` |Passed in preflight |
| `cargo package --locked --offline --list` and `--no-verify` |Passed without --allow-dirty;121files,3.6MiB(1.5MiB compressed) |
| `cargo publish --dry-run` supplemental |Initial restricted DNS failure101; authorized escalated retry **PUBLISH_DRY_RUN_ESCALATED_EXIT_STATUS=0**, archive compilation/verification succeeded, upload explicitly aborted due to dry run |
| `cargo doc --locked --no-default-features --no-deps` supplemental |**DOC_NO_DEFAULT_FEATURES_EXIT_STATUS=0** |

Complete raw logs:
- `/tmp/voxel-0.8.0-799ab91-clean-preflight.log` (1427lines; SHA256 `6df2a696887e1535a3552014de410daca834fa452d5d4999bdddd07b1b9da36b`).
- `/tmp/voxel-0.8.0-799ab91-publish-dry-run.log` (includes both failed DNS attempt and successful retry).
- `/tmp/voxel-0.8.0-799ab91-doc-no-default-features.log`.

Main gate uses rustc/cargo1.94.1; the prior actual1.89.0 default/no-default library checks remain grounded because product/source/local-lock hashes match the tested799ab91 commit. Three ignored doctests in every mode are `/home/cris/Projects/voxel-framework/src/profile.rs` examples at lines17,28,49, outside HUD; no HUD test ignored and no library/integration test filtered in the clean gate. No preflight/rustdoc/yanked-dependency warnings found. Dry-run warnings are the initial DNS retries and the expected aborted-upload notice, not a successful actual upload.

**Current package audit:** `/home/cris/Projects/voxel-framework/target/package/voxel-framework-0.8.0.crate`,121files, SHA256 `5385944d6626b6be85fcdf703e5620015e5b8bcf735e278c5dde3821b9784d6c`. Archive .cargo_vcs_info.json names799ab9130ce8e19d30d84da02a1da2682b31cd5d with no dirty flag. HUD source/docs/example, Cargo metadata/local lock, README/AGENTS/CHANGELOG/llms byte-match tested product. OpenSpec/.github/docs/sdd excluded. Generated Cargo.toml.orig is standard Cargo package metadata. Audit `/tmp/voxel-hud-final-799ab91/package-audit.log`.

## Approval blockers / verdict

**None pending. PASS; task7.4 complete;19/19 tasks;10/10 scenarios compliant.** All earlier MSRV/loaded-image/YAML/clean-gate blockers resolved with evidence. Retained-renderer identity/reconciliation, same-frame barrier/query-disjointness, asset-free horizontal boundaries and explicit plugin installation remain as independently reviewed above. PASS permits the orchestrator to proceed with its workflow; it is not an actual publish, tag, commit or archive performed by this verifier.


## Residual risks / warnings (not scenario blockers)

- Visual tests cover the observed default-font text and four placements on one NVIDIA/Vulkan single-camera/window scene. Still untested: exhaustive nine-anchor/direction multi-widget stacking, DPI/scaling matrix, overlay/loading/debug ZIndex overlap, custom font/Unicode shaping, multiple cameras/windows, visual failure/fallback transitions, other GPUs/backends/compressed textures. Do NOT describe all font/layout/GPU behavior as untested now.
- Asset lifecycle coverage now includes genuine loading/failure/fallback/recovery, same-path explicit reloads, path publication updates, textured segments, shared caching/pruning and stock PNG decode. OS file-watcher-triggered hot reload, broad formats, multiple asset sources/server replacement and generalized direct Assets removal are not independently tested. Controlled explicit reload tests are not a full filesystem-watcher matrix.
- Package contents and compilation are now verified for799ab91; this does not add broad platform/visual coverage or guarantee future dependency resolutions.
- Pre-existing docs drift remains nonblocking: llms.txt still introduces old0.4 release status/core “pure engine” language despite corrected current0.8 MSRV; config distribution text still says explicitly not crates.io while public docs describe future released installs; README CI-mirrors-publish-dry-run wording exceeds actual CI/preflight commands. No unauthorized documentation scope expansion performed.
- Minimum checks prove local locked Linux library graph, not all examples/features/platforms on1.89. CI fresh lock resolution and ignored local lockfile mean dependencies are not determined by Git HEAD alone. Final matrix is grounded in complete logs rather than redundantly rerun; exact1.89 coverage remains library/default/no-default only, not all targets/features/platforms.

## Persistence / tested-product boundary

Report `/home/cris/Projects/voxel-framework/openspec/changes/voxel-hud-0.8.0/verify-report.md`, tasks.md, apply-progress.md and state.yaml now record PASS,19/19 and10/10; synchronized canonical Engram topics. Phase remains verify; archive-report false. No production/config/Cargo/lock edit, Git write, archive, tag or actual publication by the finalizer.

The worker's gate began/ended clean on799ab91; this finalizer independently observed the clean tree before artifact edits. After these four excluded OpenSpec files are updated, the checkout is naturally dirty **only in those artifacts**. The report refers to unchanged tested product at799ab91, not a fictional clean run after report writes or a future commit.

Final provenance/audits `/tmp/voxel-hud-final-799ab91/{initial-clean-provenance,product-provenance,package-audit,evidence-sha256}.log`; pre-final report preserved at `/tmp/voxel-hud-final-799ab91/pre-final-verify-report.md`. Earlier focused/screenshot evidence remains under `/tmp/voxel-hud-combined-review/` and `/tmp/hud-loaded-image/` as cited above.

**Next recommended:** orchestrator may proceed to its requested next phase; no remaining verification blocker. No commit/archive/publication executed here.
