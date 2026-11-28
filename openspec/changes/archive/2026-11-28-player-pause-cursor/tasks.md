# Tasks

## 1. Cursor system

- [x] 1.1 Tests for Escape with the pause state, menu clicks, and the unchanged no-menu behavior (seen failing first).
- [x] 1.2 `player_cursor_system` defers Escape and ignores menu clicks when `PauseMenuState` exists.

## 2. Pause menu

- [x] 2.1 Transition tests with `StatesPlugin` (seen failing first).
- [x] 2.2 `release_cursor` / `lock_cursor` on the primary window only; no capture without focus.

## 3. Plugin

- [x] 3.1 `PlayerSystems` set over the plugin's `Update` systems; prelude re-export; test.

## 4. Docs

- [x] 4.1 Rustdoc in `player/mod.rs`, `ui/pause_menu.rs`, `ui/mod.rs`.
- [x] 4.2 `README.md`, `CHANGELOG.md`, `examples/basic_world.md`.

## 5. Verify

- [x] 5.1 Local checks and review; `verify-report.md`.
