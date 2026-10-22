# Inventory Persistence Specification
## Purpose
Inventory JSON integrity only.

## Requirements
### Requirement: Save replacement (P1)
Signatures/schema MUST remain unchanged. Invalid source or serialized-DTO totals that overflow after item-byte decoding MUST yield InvalidInput before filesystem changes; runtime equality and the JSON schema remain unchanged. Save MUST preserve old bytes on pre-replace failure, clean temps, and atomically expose whole JSON; never delete-old-first/direct-write fallback. Existing Permissions/link MUST survive symlink-following replacement. Dangling/nonregular/readonly targets and Unix nlink>1 MUST be rejected; new files MUST be private. On Windows, private creation MUST install a protected DACL granting only the effective TokenUser SID via CREATE_NEW; persistent ACL support and handle-based single-user/noninherited/full-control DACL readback MUST pass before JSON data, otherwise fail closed. Native calls MAY use isolated documented unsafe with RAII; optional target-Windows windows-sys =0.61.2 (six required Win32 features) is activated by items. Power-cut durability and ownership/ACL/xattrs SHALL NOT be guaranteed.

#### Scenario: Failures and concurrent writers
- GIVEN an existing save
- WHEN partial-write/sync/replace fails or writers finish
- THEN failure MUST preserve old bytes/clean temps; success MUST leave one whole snapshot.

### Requirement: Layout-preserving load (P2)
Load MUST reject capacity!=slots.len() and per-item aggregate overflow as InvalidData. Zero/unknown/default/is_empty MUST become None at unchanged indexes; valid duplicates MUST survive. No broad migration/arbitrary capacity limit SHALL be introduced.

#### Scenario: Sparse and malformed saves
- GIVEN holes, duplicates, zero/unknown entries, or shape/aggregate errors
- WHEN loading
- THEN valid layout MUST survive normalization; shape/aggregate errors MUST fail.
