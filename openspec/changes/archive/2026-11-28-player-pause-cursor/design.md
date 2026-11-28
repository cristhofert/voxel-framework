# Design

## One Escape owner

The conflict is two systems reading the same key. `player_cursor_system`
takes `Option<Res<State<PauseMenuState>>>` and, when the state exists, skips
its Escape branch entirely. The pause menu's `toggle_pause` then owns the key:
it sets `Paused`, and `OnEnter(Paused)` releases the cursor one frame later.

The check is the presence of the state, not a plugin flag or a new resource,
because `player` already reads `PauseMenuState` in `gameplay_active`, the `ui`
module is always compiled, and a runtime check makes plugin order irrelevant.

Alternatives rejected:

- **Remove `player_cursor_system` from `PlayerPlugin` when the pause menu is
  present.** The pause menu would then have to grow click-capture and
  focus-loss handling, duplicating logic the accepted player spec already
  assigns to the cursor system (the `player-input-focus` change decided
  against a second cursor controller).
- **Order the two Escape handlers.** Ordering cannot stop both from reacting
  to the same `just_pressed`.
- **A builder option on `PlayerPlugin`.** It would not help games that
  register the system by hand, and it leaves the bundled examples' default
  setup broken.

## Clicks while a menu is open

A game that registers the system by hand without a pause run condition would
capture the cursor when a menu button is clicked. The system therefore returns
early for a focused window when the state exists and is not `Playing`. The
focus-loss release runs before that check, in any state. Because the system
now gates itself, `PlayerPlugin` no longer puts `gameplay_active` on it, so
focus loss also releases a cursor that is still captured while a menu is open.

## Focus loss does not pause

Requested behavior is "release on focus loss". Opening the menu on focus loss
is a product decision (some games keep simulating in the background) and is
left to the game.

## `PlayerSystems`

A unit `SystemSet` on all three `add_systems(Update, …)` calls of the plugin:
the movement chain, health and respawn, and the cursor system. It is a single
set rather than one per group because the two uses that push games to compose
the systems by hand — an extra run condition and ordering against streaming —
apply to the plugin as a whole. `gameplay_active` stays on the individual
groups, so set conditions add to it and health/respawn keep ignoring pause.

## Pause-menu cursor helpers

`release_cursor` and `lock_cursor` now query the primary window only, and
`lock_cursor` skips an unfocused window. This matches the accepted "the
primary window MUST be focused before … capture" rule, which the resume path
used to bypass.

## Tests without the full plugin

`PauseMenuPlugin::build` needs asset infrastructure (`embedded_asset!`), so
the transition tests wire the private systems as the plugin does, with
`StatesPlugin`. The cursor-system tests insert `State::new(..)` directly.

## Known limits

- Mining and placement input are not gated on focus or pause; a capturing
  click or the Resume click can also mine. Unchanged by this change.
- `PlayerPlugin` still always inserts `WorldDataDir` and saves the spawn
  position; there is no opt-out.
- Behavior in a real window (OS focus events, grab success) is not covered by
  the headless tests.
