# Partial drop pickup verification — 2026-11-19

Reviewed against published v0.10.2 (bc8386d). The production magnet system already
subtracts the unaccepted quantity returned by Inventory::add, emits only positive
accepted quantities, and despawns only an empty drop. No runtime or public API
change was necessary.

## Regression coverage

Extended the existing real DropsPlugin headless app fixtures:
- A positioned three-unit drop reaches an inventory with headroom for one.
  Inventory gains one, exactly one ItemPickedUp reports one, and the same entity
  retains two. The published inventory contract uses u32 aggregate quantities;
  initializing u32::MAX - 1 provides deterministic headroom for exactly one.
- A further full-inventory frame preserves both remaining units and emits nothing.
- Removing two inventory units allows the remainder to be collected exactly once.
  Two subsequent frames preserve inventory totals and emit no further pickups.
- At each step, cumulative pickup quantities plus the remainder equal three.
- A separate inventory filled with another item rejects all three units over
  three frames, emits no pickup, and retains the original drop entity.
- After freeing that slot, the drop is collected as three exactly once; subsequent
  frames preserve all three units and emit no duplicate messages.

## Observed results

- Partial-pickup regression: 1 passed.
- Full-inventory regression: 1 passed.
- Drop tests: 12 passed.
- Inventory tests: 21 passed.
- Mining tests: 11 passed.
- Entire library: 336 passed, 0 failed, 0 ignored.
- cargo fmt --all -- --check: passed.
- git diff --check: passed.
- cargo doc --locked --offline --no-deps --all-features: passed without warnings.

The library test harness was built via cargo rustc --offline --jobs 1 --lib
--profile test --message-format=json, with -C debuginfo=0,
-C link-arg=-Wl,--threads=1 and -C link-arg=-Wl,--strip-debug for local memory.
The emitted executable was run with each filter and then unfiltered.
Evidence is retained in target/verification/partial-pickup in the main workspace.

## Scope and limitations

Tests use real ECS systems/messages with controlled time, without native UI.
No changes to inventory stack policy, capacities, persistence, visuals, or expiry
rules were included. Pending local inventory/drop-visual changes are excluded.
No new registry release is needed for behavior already in v0.10.2; the reviewed
tests and documentation are consumable via an immutable Git commit revision.
An initial compilation with a different ignored lockfile was interrupted to
reuse cached dependencies. Only the completed build/run results above are counted.
