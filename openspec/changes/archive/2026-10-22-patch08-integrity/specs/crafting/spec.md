# Crafting Specification
## Purpose
Compatible shapeless crafting integrity.

## Requirements
### Requirement: Normalized requests (C1)
Inputs/outputs MUST aggregate by ItemType equality in first-occurrence order with checked u32 sums; ignore zero, allow empty recipes, reject positive default/is_empty sentinel. Missing/invalid/overflow recipes MUST yield InvalidRecipe; can_craft MUST return false for invalid recipes and otherwise check ingredients only, never capacity.

#### Scenario: Duplicates
- GIVEN inputs A2+A2 and output B1
- WHEN checking/crafting with A3 or A4
- THEN A3 MUST fail insufficient; A4 MUST consume exactly four.

### Requirement: Error and fallback contracts (C2)
try_craft MUST map InvalidRequest to InvalidRecipe, InsufficientItems to InsufficientIngredients, CapacityExceeded/InvalidContainer to InventoryFull, keeping exactly three existing variants. External None fallback MUST normalize/deduplicate, conservatively preflight current space, check actual removals/adds, stop first failure, never falsely succeed or invent rollback. Documentation MUST disclose possible partial mutation.

#### Scenario: Legacy partial operation
- GIVEN a legacy container passing preflight but removing less than requested
- WHEN crafting
- THEN it MUST fail immediately, produce no later outputs, and MAY retain partial mutation.
