//! Persistent user settings saved to `worlds/{world_name}/settings.json`.
//!
//! [`PersistedSettings`] captures the four in-game slider values (FOV, SFX
//! volume, render distance, mouse sensitivity) and writes them to disk when the
//! player closes the Settings menu.  On next launch the game reads this file and
//! patches the config resources before inserting them, so the player's choices
//! survive across sessions.
//!
//! # Usage
//!
//! ```rust,no_run
//! use voxel_framework::settings::PersistedSettings;
//!
//! // Save
//! let settings = PersistedSettings {
//!     fov_degrees: 90.0,
//!     sfx_volume: 0.8,
//!     render_distance_chunks: 10,
//!     sensitivity_pct: 100,
//! };
//! settings.save("my_world");
//!
//! // Load
//! if let Some(saved) = PersistedSettings::load("my_world") {
//!     println!("Loaded FOV: {}", saved.fov_degrees);
//! }
//! ```

use serde::{Deserialize, Serialize};

/// User-adjustable settings persisted to `worlds/{world_name}/settings.json`.
///
/// All four fields map directly to the in-game sliders exposed by
/// [`crate::ui::PauseMenuPlugin`]:
///
/// | Field | Slider | Range |
/// |-------|--------|-------|
/// | `fov_degrees` | Field of View | 50–120 |
/// | `sfx_volume` | SFX Volume | 0.0–1.0 |
/// | `render_distance_chunks` | Render Distance | 4–32 (even) |
/// | `sensitivity_pct` | Mouse Sensitivity | 10–100 |
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedSettings {
    /// Vertical field-of-view in degrees. Valid range: `50.0`–`120.0`.
    pub fov_degrees: f32,
    /// SFX volume in the range `[0.0, 1.0]`.
    pub sfx_volume: f32,
    /// Render distance in chunks (even values only). Valid range: `4`–`32`.
    pub render_distance_chunks: u32,
    /// Mouse sensitivity as a percentage of the default.
    /// `100` = default ("Minecraft-like" feel), `10` = slowest.
    /// The underlying [`crate::player::Player::sensitivity`] multiplier is
    /// `sensitivity_pct / 100`, combined internally with the hidden
    /// `MOUSE_SCALE` constant (rad/px) to produce camera rotation per mouse
    /// pixel.
    pub sensitivity_pct: u32,
}

impl PersistedSettings {
    /// Saves these settings to `worlds/{world_name}/settings.json`.
    ///
    /// Creates the directory if it does not exist. Silently ignores I/O errors
    /// so that a read-only filesystem does not crash the game.
    pub fn save(&self, world_name: &str) {
        let dir = format!("worlds/{}", world_name);
        let _ = std::fs::create_dir_all(&dir);
        let path = format!("{}/settings.json", dir);
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Loads settings from `worlds/{world_name}/settings.json`.
    ///
    /// Returns `None` if the file does not exist or cannot be parsed (e.g. on
    /// the first run before any settings have been saved).
    pub fn load(world_name: &str) -> Option<Self> {
        let path = format!("worlds/{}/settings.json", world_name);
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }
}
