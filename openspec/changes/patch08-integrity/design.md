# Design: patch08-integrity

## Baseline and scope
Clean dev 1a82c7e66dfcadde7996de03c4d9ae6218ee1b2e; branch codex/patch08-integrity. Prior exploration #876 and design #879 retrieved fully. Current CLAUDE/Cargo override stale SDD context (MSRV1.89, LGPL, package distribution). Inventory fields are public; Container lacks rollback; existing crafting ignores operation returns; persistence directly truncates. No plugin/schedule/system-order changes. Existing I:ItemType bounds remain; no Container Clone/Serialize/Debug requirement.

## Architecture decisions
| Choice | Rejected alternative / rationale |
|---|---|
| Optional neutral hook | Mandatory method/Clone bound breaks legacy implementors; items must not depend on crafting |
| Detached built-in slots | Generic invented rollback cannot restore arbitrary external policy/state |
| Equality + ordered vectors | Byte identity is serialization, not ItemType equality; deterministic first occurrence |
| Validated representational ceiling | Saturated output loses items; no gameplay stack-limit configuration |
| tempfile =3.27.0, optional via items | Handwritten platform replacement/cleanup increases complexity; transitive MSRV still requires verification |
| Protected effective-user Windows DACL; optional target-Windows windows-sys =0.61.2 via items (six Win32 features) | Inherited ACLs or post-creation repair cannot guarantee private creation; native calls stay isolated with SAFETY/RAII |
| File sync before persist | Directory fsync errors after commit would misrepresent pre-replace preservation; no universal crash-durability promise |

## Interfaces and shared internal contract
Only new public API (reexport error from items and prelude):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContainerExchangeError {
    InvalidRequest, InvalidContainer, InsufficientItems, CapacityExceeded,
}
// Default method on Container<I: ItemType>:
fn try_exchange_atomic(
    &mut self, remove: &[(I, u32)], add: &[(I, u32)],
) -> Option<Result<(), ContainerExchangeError>> { None }
```
None/Err means no mutation; Some(Ok(())) complete. Component delegates to self.0 via Container.

Inventory worker owns pub(crate) Inventory::validate_integrity(&self) -> Result<(),ContainerExchangeError>, returning only InvalidContainer for shape/live-stack/per-item-total violations; persistence maps it to InvalidInput/InvalidData. No public helper needed. An internal normalize_quantities<I:ItemType> accepting tuple slices returns ordered Vec<(I,u32)> or InvalidRequest; crafting may reuse it via crate visibility, never expose it publicly.

## Exchange/crafting flow
normalize both lists -> validate source -> private slots copy -> ordered remove -> ordered add -> commit slots.
Normalize checked u32 equality totals, skip zero before sentinel checks, reject item==I::default() OR is_empty(), allow empty lists. Check all ingredients before staged capacity decisions. Exact removal and zero leftovers required; failure discards copy. Capacity is evaluated after consumption, including same-input outputs and existing duplicate stacks.

add validates source, accepts min(requested,u32MAX-current_total), preserves existing stacking/drain order, returns representational plus physical-space leftovers; malformed/sentinel request returns all. count_of uses defensive saturating fold.

try_craft validates whole recipe first; can_craft remains read-only ingredients-only. Hook mappings: InvalidRequest->InvalidRecipe; InsufficientItems->InsufficientIngredients; InvalidContainer/CapacityExceeded->InventoryFull. Other future non_exhaustive hook errors conservatively map to InventoryFull.
None fallback: aggregate ingredients/distinct outputs, checked current free-space subtraction, u64 projected per-type totals; no anticipated freed-slot credit. Check remove==requested then add==0; stop first failure, mapping partial removal to InsufficientIngredients and leftover to InventoryFull. Explicit possible partial mutation; no compensating operations.

## Persistence flow
validate runtime -> build unchanged DTO -> validate decoded DTO alias totals -> serialize -> prepare/resolve target -> private sibling temp -> verify Windows security -> write_all -> Permissions -> sync_all -> persist replacement.
Keep &str, boxed error, existing serde bounds and JSON. Ensure missing parents; bare basename uses ".". Resolve existing final symlink with canonicalize; preserve link, reject dangling/nonregular/readonly and Unix nlink>1. Probe existing writable access using open write/no truncate, close; preserve std Permissions on temp. Unix new files keep tempfile private permissions. Windows uses Builder::make_in/CreateFileW CREATE_NEW with a protected DACL granting only the current effective TokenUser SID (thread token first; process fallback only ERROR_NO_TOKEN). Before JSON data, handle-based verification requires FILE_PERSISTENT_ACLS and a protected, nondefault DACL with one noninherited full-control allow ACE for that SID. Security/volume-query errors fail closed; token handles, LocalFree allocations and native buffers have isolated SAFETY/RAII ownership. The optional target-Windows windows-sys =0.61.2 dependency uses Foundation, Security, Security_Authorization, Storage_FileSystem, System_Threading and System_SystemServices features; items activates it. No old-file deletion/direct-write fallback. PersistError-owned temp drops for cleanup; no fallible postcommit durability step. Trusted parent namespace assumed: hostile path races, ownership/ACL/xattrs and universal power-cut survival are not promised.

The private decode_inventory helper is shared by save (InvalidInput, before filesystem changes) and load (InvalidData): it rejects alias-collapsed per-item totals above u32MAX. Runtime request equality and the existing byte schema remain unchanged; valid byte aliases canonicalize on load without repacking.

Load: check DTO shape, decode zero/default/is_empty to None, then validate aggregates; preserve indexes, duplicate live stacks and capacity0. No migration/capacity ceiling.

## Files, tests and rollout
Inventory/crafting/prelude worker; persistence-only worker; metadata/docs/CI worker (writesets in state). Persistence uses private generic AtomicWriteOps write/sync/replace adapter for deterministic prefix-write, sync and replace failure injection; production calls real APIs. Unique tempdirs, old/absent-target preservation, cleanup, permissions/link and whole-snapshot concurrency tests. Inventory/crafting tests cover normalization, capacity, overflow, compatibility and partial fallback (tasks I1–V1).

Prepare 0.8.1/changelog without claiming release. Retain existing Linux matrix, extend exact1.89 coverage, add targeted Windows items-persistence lane. Exact-candidate gate includes clean snapshot/preflight, docs and package --no-verify; no build/integration/publication. No migration; no unresolved policy questions. Dependency/platform evidence remains pending verification.
