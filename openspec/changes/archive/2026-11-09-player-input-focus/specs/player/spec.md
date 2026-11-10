# Player input focus delta

## ADDED Requirements

### Requirement: Explicit focused capture
Spawning a player MUST NOT capture or hide the cursor. The primary window MUST be focused before a left click can capture it. Unfocused clicks and Escape MUST NOT capture or close a window. Losing focus MUST release and show the cursor. Regaining focus alone MUST NOT recapture it. Native keyboard focus and desktop global shortcuts remain platform-owned.

#### Scenario: Unfocused startup
- GIVEN an unfocused primary window
- WHEN a player spawns and input updates run
- THEN its cursor remains free and visible, and player input is inactive

#### Scenario: Capture and focus loss
- GIVEN a focused primary window with a free cursor
- WHEN the user clicks it, then it loses focus
- THEN capture becomes active only after the click, and is released on focus loss
- AND a later focus gain alone leaves control inactive

### Requirement: Input ownership
Player look and movement MUST ignore input unless the primary window is focused and captured. Ignored mouse deltas MUST be drained rather than replayed. Inactive movement MUST clear residual velocity. Callers without a primary window MAY use synthetic input for headless simulations. Collision and viewer synchronization MUST remain independently schedulable.

#### Scenario: Inactive look and movement
- GIVEN pending mouse motion, held movement/roll keys and residual velocity
- WHEN the primary window is unfocused or released
- THEN look and movement do not change the transform, and velocity becomes zero
- AND recapturing does not replay the ignored motion

#### Scenario: Headless compatibility
- GIVEN no primary window and synthetic input
- WHEN the existing player systems run
- THEN the existing movement and horizon tests continue to pass
