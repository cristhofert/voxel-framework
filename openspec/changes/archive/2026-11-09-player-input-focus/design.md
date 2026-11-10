# Design

Use the existing player module rather than a second cursor controller plugin. Expose `player_input_active` as an ordinary run condition for manual composition. Add primary-window focus/capture queries to player look and movement; look drains its own reader on inactive frames, so consumers should keep it scheduled. Movement clears velocity without translating while inactive. No-window headless calls remain enabled.

Remove startup capture from `spawn_player`. Restrict `player_cursor_system` to the primary window and ignore unfocused input after releasing its cursor. Pause-menu capture remains compatible: player systems enforce the same ownership check regardless of caller. No continuous focus requests or OS keyboard grabs.

Tests use real Bevy schedules and Window/CursorOptions components, without a GPU or OS window. A graphical example supports focus transitions for a human check; these tests cannot establish window-manager activation behavior. No new dependency or version bump. SDD artifacts are authored directly because the referenced sdd-* command skills are not installed in this session.
