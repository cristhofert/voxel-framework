//! Hotbar selection state, keyboard/scroll input, and the [`HotbarPlugin`].
//!
//! The hotbar is a strip of `N` item slots — typically displayed at the bottom
//! of the screen — from which the player picks the active tool or block.  This
//! module manages *selection state* only; rendering is handled separately by
//! the UI system in [`crate::ui`].
//!
//! [`HotbarState`] is a [`bevy::prelude::Resource`] that stores which slot
//! index is currently active.  The built-in `hotbar_input_system` updates it
//! from keyboard digit keys (1–9) and the mouse scroll wheel, then emits a
//! [`HotbarSelectionChanged`] message whenever the selection changes.

use bevy::ecs::message::MessageReader;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

// ─── HotbarState ─────────────────────────────────────────────────────────────

/// Selection state for the player's hotbar.
///
/// This resource is the single source of truth for which hotbar slot is active.
/// Game systems that need to know the current tool or block should read
/// `selected` from this resource rather than tracking input themselves.
///
/// The default configuration provides 9 slots (one row), matching standard
/// Minecraft-style layouts.  The initial selection is slot `0`.
///
/// Input is driven by `hotbar_input_system` which is registered automatically
/// when [`HotbarPlugin`] is added.  You can also call [`HotbarState::select`]
/// or [`HotbarState::scroll`] directly from game systems if you need
/// programmatic control.
///
/// # Examples
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::items::HotbarState;
///
/// fn read_hotbar(hotbar: Res<HotbarState>) {
///     println!("active slot: {}", hotbar.selected);
/// }
/// ```
#[derive(Resource, Debug, Clone)]
pub struct HotbarState {
    /// Zero-based index of the currently selected slot.
    ///
    /// Always in `[0, slots - 1]`.  Writes outside this range are clamped by
    /// [`HotbarState::select`] and wrapped by [`HotbarState::scroll`].
    pub selected: usize,
    /// Total number of slots in the hotbar.
    ///
    /// Changing this value at runtime is supported; [`HotbarState::select`]
    /// and [`HotbarState::scroll`] respect the new count immediately.
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

/// Message emitted when the active hotbar slot changes.
///
/// Written by `hotbar_input_system` (and by any system that calls
/// [`HotbarState::select`] or [`HotbarState::scroll`] directly) whenever
/// `selected` differs from the value at the start of the frame.
///
/// Subscribe to this message to update UI highlights, swap active tools, or
/// play a selection sound.
#[derive(Message, Debug, Clone)]
pub struct HotbarSelectionChanged {
    /// The newly active slot index, in `[0, HotbarState::slots - 1]`.
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

/// Plugin that registers the hotbar resource, message type, and input system.
///
/// Registering this plugin:
///
/// * Inserts [`HotbarState`] (default: 9 slots, slot 0 selected).
/// * Registers the [`HotbarSelectionChanged`] message type.
/// * Adds `hotbar_input_system` to the `Update` schedule.
///
/// This plugin is included automatically by [`crate::VoxelPlugin`], so game
/// code using that plugin does **not** need to add it separately.  If you are
/// using [`crate::VoxelCorePlugin`] without [`crate::VoxelPlugin`], add
/// `HotbarPlugin` explicitly.
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
