# Cursor ownership with the pause menu

A downstream game cannot use `PlayerPlugin` together with `PauseMenuPlugin`,
so it registers the public player systems by hand and loses the plugin's
spawn-position persistence. It also reported that the pause menu alone starts
with a free cursor and does not release it on focus loss.

What the code shows:

- `player_cursor_system` already captures on a focused left click and releases
  on focus loss. `PauseMenuPlugin` does neither, and its docs say not to
  combine it with that system.
- Both bundled examples combine the two plugins anyway. In that setup, Escape
  with a free cursor (before the first click, or after a focus loss) opens the
  pause menu **and** despawns the window in the same frame, because both
  handlers react to the key.
- The pause menu's cursor helpers act on every window and capture without
  checking focus.
- `PlayerPlugin` offers no way to add a run condition or ordering to its
  systems, which is the other reason games compose them by hand.

Scope:

- `player_cursor_system` defers Escape to the pause menu when
  `PauseMenuState` exists and ignores clicks while a menu is open.
- `PlayerSystems`: a public system set over the plugin's `Update` systems.
- Pause-menu cursor helpers limited to the primary window; resume does not
  capture an unfocused window.
- Docs: the combination becomes the documented setup.
- Out of scope: pausing on focus loss, gating mining/placement input on focus
  or pause, an opt-out for `PlayerPlugin`'s spawn persistence, a separate
  cursor plugin.

Consumer impact: `player_cursor_system` gains one system parameter, which does
not affect code that registers it with `add_systems`; a direct call (not a
supported use) would need the extra argument. `PlayerSystems` is additive.
Behavior changes only when `PauseMenuState` exists: Escape no longer releases
the cursor or closes the window from the cursor system. Games without the
pause menu keep the two-stage Escape. Affected modules: `src/player/mod.rs`,
`src/ui/pause_menu.rs`, `src/prelude.rs`.

Rollback: revert the change; no data format or feature flag is involved.

This is a substantial change (public system behavior, new public type). SDD
files are maintained directly because the referenced SDD skills are not
installed in this environment.
