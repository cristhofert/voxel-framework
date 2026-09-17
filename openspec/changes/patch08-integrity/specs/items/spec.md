# Delta for Items
## ADDED Requirements

### Requirement: Compatible atomic exchange (I1)
Container MUST expose optional try_exchange_atomic(remove,add) returning Option<Result<(),ContainerExchangeError>>, default None, without new bounds/mandatory methods. Public non_exhaustive errors MUST be InvalidRequest, InvalidContainer, InsufficientItems, CapacityExceeded; items/prelude MUST reexport them. None/Err MUST NOT mutate; Ok MUST complete. Built-ins MUST use post-consumption capacity; Component MUST delegate. Direct requests MUST also normalize duplicates by equality/first occurrence, ignore zero, allow empty lists, and reject checked-sum overflow/positive default/is_empty as InvalidRequest.

#### Scenario: Freed capacity
- GIVEN one slot containing A1
- WHEN exchanging A1 for B1+B2
- THEN B3 MUST replace A; any failed exchange MUST preserve slots/capacity.

### Requirement: Representational integrity (I2)
Built-ins MUST reject capacity/length mismatch, zero/sentinel live stacks, or per-item totals above u32MAX. add MUST bound accepted quantities by representational headroom, return every leftover, and reject malformed source/positive sentinel without mutation. count_of MUST defensively saturate; no author stack limit SHALL be added.

#### Scenario: Numeric ceiling
- GIVEN A at u32MAX, or malformed public fields
- WHEN adding A1 or counting A
- THEN add MUST return all unaccepted items without wrap; count MUST not overflow.
