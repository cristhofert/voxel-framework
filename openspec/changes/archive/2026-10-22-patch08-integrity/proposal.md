# Proposal: patch08-integrity (candidate 0.8.1)

## Intent
Repair duplicate-recipe accounting, built-in crafting atomicity, inventory arithmetic, and inventory JSON replacement without breaking existing consumers such as Starframe.

## Scope
### In scope
- Equality-based, first-occurrence recipe/request normalization; checked u32 aggregation, zero ignored, empty recipes allowed, positive default/is_empty sentinels rejected.
- Optional neutral Container exchange hook/error; Inventory staging and ContainerComponent delegation; existing crafting errors and ingredients-only can_craft retained.
- Bounded representational inventory totals, defensive counts, source validation.
- Inventory-only atomic JSON save and validated load, unchanged signatures/schema.
- Candidate 0.8.1 metadata, contracts/changelog, Linux feature matrix, exact Rust 1.89.0, targeted Windows persistence CI, docs/package gates.

### Out of scope
Mobs, navigation, game features; chunk/world/spawn/HUD/stats/effects saving; configurable stack limits, broad JSON migration, arbitrary capacity ceilings. No dev merge, push, tag, publication, automation creation/dispatch, or automatic integration.

## Approach
Built-ins validate and consume/produce on detached slots, committing only complete exchanges; opt-in containers obey the same hook contract. Legacy containers retain conservative current-space preflight and checked actual operation returns, with documented possible partial mutation and no fictional rollback.

Serialize validated inventory before sibling NamedTempFile write_all, Permissions preservation, sync_all, and persist replacement. Follow existing symlinks without replacing links; reject dangling/nonregular/readonly targets and Unix multiply-linked files. Keep old bytes through every pre-replace failure; clean temporary files. New files retain private permissions. Load preserves indexes/valid duplicate stacks while clearing zero/unknown/default/empty entries and rejecting shape/aggregate overflow.

## Affected areas
| Area | Impact |
|---|---|
| src/items/inventory.rs; src/prelude.rs | Additive hook/error, checked arithmetic |
| src/crafting/mod.rs | Normalization, mappings, truthful fallback |
| src/items/persistence.rs | Validation, replacement, tests |
| Cargo.toml; README.md; CHANGELOG.md | Optional =3.27.0 tempfile via items; candidate |
| .github/workflows/rust.yml; release gate/docs if needed | Candidate verification only |

## Dependencies and risks
Exact tempfile pin does not prove transitive MSRV; verify Rust 1.89.0. Platform replacement/metadata behavior needs Windows evidence. Atomic visibility is not universal power-cut durability or ownership/ACL/xattr preservation.

## Rollback
Revert scoped patch before integration; no format migration required. Existing valid JSON remains compatible. Never delete an old save as recovery.

## Success criteria
- Approved integrity/compatibility scenarios pass.
- Exact candidate passes feature/MSRV/Windows/docs/package gates before “ready”.
- Preparation only now: no source edits, tests, build, or commits.
