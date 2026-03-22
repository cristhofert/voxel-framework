//! Hotbar selection state and keyboard/scroll input system.

use bevy::ecs::message::MessageReader;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

// ─── HotbarState ─────────────────────────────────────────────────────────────

/// Selection state for the player's hotbar.
///
/// Tracks which slot is currently selected and how many slots exist.
#[derive(Resource, Debug, Clone)]
pub struct HotbarState {
    /// Index of the currently selected slot (0-based).
    pub selected: usize,
    /// Total number of hotbar slots.
    pub slots: usize,
}

impl HotbarState {
    /// Creates a new `HotbarState` with the given number of slots.
    ///
    /// The initially selected slot is 0.
    pub fn new(slots: usize) -> Self {
        Self { selected: 0, slots }
    }

    /// Selects the slot at `index`.
    ///
    /// Clamps to `[0, slots - 1]` so out-of-range indices are safe.
    pub fn select(&mut self, index: usize) {
        if self.slots == 0 {
            self.selected = 0;
            return;
        }
        self.selected = index.min(self.slots - 1);
    }

    /// Scrolls the selection by `delta` slots, wrapping around at the edges.
    ///
    /// Positive `delta` moves toward higher indices, negative toward lower.
    pub fn scroll(&mut self, delta: i32) {
        if self.slots == 0 {
            return;
        }
        let slots = self.slots as i32;
        let new = (self.selected as i32 + delta).rem_euclid(slots);
        self.selected = new as usize;
    }
}

impl Default for HotbarState {
    /// Creates a default hotbar with 9 slots (one row).
    fn default() -> Self {
        Self::new(9)
    }
}

// ─── Messages ────────────────────────────────────────────────────────────────

/// Emitted when the hotbar selection changes.
#[derive(Message, Debug, Clone)]
pub struct HotbarSelectionChanged {
    /// The newly selected slot index.
    pub slot: usize,
}

// ─── Systems ─────────────────────────────────────────────────────────────────

/// Reads keyboard keys 1–9 and mouse scroll wheel to update [`HotbarState`].
///
/// - Keys `1`–`9` select slots 0–8 directly.
/// - Scroll up (`y > 0`) moves selection left (lower index, wraps).
/// - Scroll down (`y < 0`) moves selection right (higher index, wraps).
pub fn hotbar_input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut scroll_events: MessageReader<MouseWheel>,
    mut hotbar: ResMut<HotbarState>,
    mut changed_writer: MessageWriter<HotbarSelectionChanged>,
) {
    let previous = hotbar.selected;

    // Number keys 1–9 map to slots 0–8.
    let digit_keys = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    for (i, &key) in digit_keys.iter().enumerate() {
        if keyboard.just_pressed(key) {
            hotbar.select(i);
            break;
        }
    }

    // Mouse scroll wheel.
    for event in scroll_events.read() {
        let lines = match event.unit {
            MouseScrollUnit::Line => event.y,
            MouseScrollUnit::Pixel => event.y / 53.0, // ~53 px per line
        };
        if lines.abs() > 0.01 {
            // Scroll up → go left (lower index); scroll down → go right.
            hotbar.scroll(-(lines.signum() as i32));
        }
    }

    if hotbar.selected != previous {
        changed_writer.write(HotbarSelectionChanged { slot: hotbar.selected });
    }
}

// ─── HotbarPlugin ────────────────────────────────────────────────────────────

/// Plugin that registers the hotbar resource, message, and input system.
///
/// Included automatically in [`VoxelPlugin`].
pub struct HotbarPlugin;

impl Plugin for HotbarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HotbarState>();
        app.add_message::<HotbarSelectionChanged>();
        app.add_systems(Update, hotbar_input_system);
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── scroll wraps ─────────────────────────────────────────────────────────

    #[test]
    fn test_hotbar_scroll_wraps() {
        let mut hotbar = HotbarState::new(9);
        hotbar.select(0);

        // Scroll back from 0 should wrap to 8.
        hotbar.scroll(-1);
        assert_eq!(hotbar.selected, 8);

        // Scroll forward from 8 should wrap to 0.
        hotbar.scroll(1);
        assert_eq!(hotbar.selected, 0);
    }

    #[test]
    fn test_hotbar_scroll_multiple_steps() {
        let mut hotbar = HotbarState::new(9);
        hotbar.select(7);
        hotbar.scroll(3); // 7 + 3 = 10 % 9 = 1
        assert_eq!(hotbar.selected, 1);
    }

    // ── select clamps ────────────────────────────────────────────────────────

    #[test]
    fn test_hotbar_select_clamped() {
        let mut hotbar = HotbarState::new(9);
        hotbar.select(100); // way out of range
        assert_eq!(hotbar.selected, 8); // clamped to last slot

        hotbar.select(0);
        assert_eq!(hotbar.selected, 0);
    }

    #[test]
    fn test_hotbar_select_exact_last() {
        let mut hotbar = HotbarState::new(5);
        hotbar.select(4);
        assert_eq!(hotbar.selected, 4);
    }

    // ── default ──────────────────────────────────────────────────────────────

    #[test]
    fn test_hotbar_default_is_nine_slots() {
        let hotbar = HotbarState::default();
        assert_eq!(hotbar.slots, 9);
        assert_eq!(hotbar.selected, 0);
    }
}
