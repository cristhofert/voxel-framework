# Archive — 2026-11-13

Implemented collector-independent drop expiration and explicit DropVisualPlugin<I>
for consumer games. Merged the additive requirements into openspec/specs/items/spec.md.
Updated public exports/category, README, CHANGELOG (Unreleased), rustdoc, and the
standalone drops example and integration guide. See verify-report.md for evidence
and the native visual-test limitation. The multiplayer audit has no invariant
requiring a change: this remains a single-collector pickup model, with local
presentation and collector-independent expiry. No release, commit, or push.
