# Archive — 2026-11-28

Implemented and locally verified: chunk persistence saves only chunks with
unsaved changes, and saved chunks are restored on the generator's background
task. The requirements were merged into `openspec/specs/persistence/spec.md`,
the first accepted spec for the `persistence` capability. `README.md`,
`CHANGELOG.md` (`[Unreleased]`), `examples/basic_world.md`,
`docs/architecture.md` and item 8 of `docs/multiplayer-readiness.md` describe
the new behavior.

The change was authored by hand because the `/sdd-*` skills are not installed
in Claude Code; the artifact layout follows the earlier archived changes.

Not verified at archive time: frame pacing and load time in a real window, and
the CI-only lanes listed in `verify-report.md`. No release was prepared; the
downstream game that asked for this pins a published version, so it is
unblocked only once a release that includes this change exists.
