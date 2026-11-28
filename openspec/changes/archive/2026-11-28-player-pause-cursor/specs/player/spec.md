# Player — delta for `player-pause-cursor`

Adds two requirements to the accepted player spec. The existing "Explicit
focused capture" and "Input ownership" requirements are unchanged and still
hold with the pause menu installed.

### Requirement: Pause menu coexistence
`player_cursor_system` and `PlayerPlugin` MUST be usable together with `PauseMenuPlugin`. When `PauseMenuState` exists, `player_cursor_system` MUST NOT react to Escape: it MUST NOT release the cursor and MUST NOT close the window. While the state is `Playing` it MUST keep capturing on a focused left click and releasing on focus loss. While the state is `Paused` or `Settings` a click MUST NOT capture the cursor, even when the system is registered without a run condition. Focus loss MUST release the cursor in any state and MUST NOT change the pause state. Without `PauseMenuState` the two-stage Escape behavior (release, then close the focused window) MUST be unchanged.

The pause menu MUST release and capture only the primary window's cursor, and resuming MUST NOT capture the cursor of an unfocused window.

#### Scenario: Escape before the first click
- GIVEN both plugins' systems, state `Playing`, and a focused primary window with a free cursor
- WHEN Escape is pressed
- THEN the state becomes `Paused`, the window still exists and the cursor stays free

#### Scenario: Resume, focus loss and recapture
- GIVEN the menu is open
- WHEN Escape is pressed again, then the window loses and regains focus, then the user clicks
- THEN resuming captures the cursor, focus loss releases it without pausing, regaining focus alone leaves it free, and the click captures it

#### Scenario: Menu clicks
- GIVEN state `Paused` or `Settings` and `player_cursor_system` registered without a run condition
- WHEN the left button is pressed in the focused window
- THEN the cursor stays free

#### Scenario: Resume without focus
- GIVEN the menu is open and the primary window is unfocused
- WHEN the game resumes
- THEN the cursor stays free

#### Scenario: No pause menu
- GIVEN no `PauseMenuState` and a focused window with a captured cursor
- WHEN Escape is pressed twice
- THEN the first press releases the cursor and the second closes the window

### Requirement: Player system set
Every `Update` system added by `PlayerPlugin` MUST belong to the public `PlayerSystems` set, so a run condition or ordering configured on the set applies to all of them. Conditions on the set MUST combine with the plugin's own pause gating.

#### Scenario: Game-owned condition
- GIVEN `PlayerPlugin` and `PlayerSystems` configured with a condition that is false
- WHEN `Update` runs with a click pending in a focused window
- THEN the cursor is not captured
