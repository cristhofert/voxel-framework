# Archive — 2026-11-28

Implemented and locally verified: `PlayerPlugin` and `player_cursor_system`
work together with `PauseMenuPlugin`, and `PlayerSystems` lets a game gate or
order the plugin's systems. The two requirements ("Pause menu coexistence" and
"Player system set") were appended to `openspec/specs/player/spec.md`; the
existing focus and input-ownership requirements are unchanged. `README.md`,
`CHANGELOG.md` (`[Unreleased]`) and `examples/basic_world.md` describe the
combined setup. `docs/multiplayer-readiness.md` needed no change: cursor
ownership is local presentation state.

The change was authored by hand because the `/sdd-*` skills are not installed
in Claude Code; the artifact layout follows the earlier archived changes.

Not verified at archive time: behavior in a real window, and the CI-only lanes
listed in `verify-report.md`. No release was prepared.
