# Archive — 2026-11-22

Implemented and locally verified the requested hotbar capability. Merged the
requirements into openspec/specs/items/spec.md and updated public documentation,
changelog, and the presentation-only multiplayer audit entry. Existing inventory
and input rules remain unchanged. See docs/verification/hotbar-item-icons.md.

Prepared 0.10.4 locally, with package compilation and recorded checksum evidence.
At verification time, no commit, push, tag, or publication had been performed.
The maintainer subsequently authorized the local candidate commit only. Publishing remains pending
explicit authorization and clean-commit release preflight / Windows CI. The
archive marks capability implementation complete, not registry publication.

## Publication — 2026-11-23

After explicit maintainer authorization, release commit
c5913ca9dbba3004be77c66e581d28e20f12d337 passed Rust CI (including Windows) and
the complete release preflight. cargo publish --dry-run and cargo publish
succeeded. Registry version 0.10.4 and tag v0.10.4 identify the same release.
See docs/verification/hotbar-item-icons.md for run URLs and registry checksum.
