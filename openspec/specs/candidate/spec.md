# Candidate Verification Specification
## Purpose
Preparation without integration/publication.

## Requirements
### Requirement: Exact-candidate gate (V1)
Candidate 0.8.1 MUST NOT be ready before default/no-default/all-features check/test, exact Rust 1.89.0, targeted Windows persistence, docs/package evidence on its exact snapshot. Scope MUST remain crafting/inventory/inventory-persistence only; metadata/docs/CI MAY support it. No integration, publication, automation dispatch, or cargo build SHALL occur.

#### Scenario: Incomplete gate
- GIVEN prepared metadata but missing exact-candidate evidence
- WHEN reporting status
- THEN candidate_ready MUST remain false.
