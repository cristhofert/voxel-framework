//! Spawn-point persistence for the player.
//!
//! Saves and loads the player's spawn position to/from a JSON file
//! (`player_data.json`) inside the world data directory.  The format is
//! intentionally simple and hand-editable:
//!
//! ```json
//! {"spawn_x": 8.0, "spawn_y": 60.0, "spawn_z": 8.0}
//! ```
//!
//! # Contract
//!
//! * [`save_spawn_position`] creates or overwrites `{data_dir}/player_data.json`.
//! * [`load_spawn_position`] returns `None` if the file is missing or
//!   unparseable — callers should fall back to their configured default.
//! * Both functions are infallible from the caller's perspective: errors are
//!   logged with [`eprintln!`] but never propagate as panics.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// ─── On-disk format ──────────────────────────────────────────────────────────

/// JSON representation of the player's persistent spawn point.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct PlayerSaveData {
    pub spawn_x: f32,
    pub spawn_y: f32,
    pub spawn_z: f32,
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Saves the spawn position to `{data_dir}/player_data.json`.
///
/// Creates the directory and file if they do not exist; overwrites the file if
/// it does.  Any I/O or serialisation error is logged with [`eprintln!`] and
/// silently swallowed so that a disk failure never crashes the game.
///
/// # Examples
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::player::persistence::save_spawn_position;
///
/// save_spawn_position(Vec3::new(8.0, 60.0, 8.0), "./worlds/survival");
/// ```
pub fn save_spawn_position(pos: Vec3, data_dir: &str) {
    let data = PlayerSaveData {
        spawn_x: pos.x,
        spawn_y: pos.y,
        spawn_z: pos.z,
    };
    let path = format!("{}/player_data.json", data_dir);
    if let Err(e) = std::fs::create_dir_all(data_dir) {
        eprintln!("[warn] Failed to create world data directory '{}': {}", data_dir, e);
        return;
    }
    match serde_json::to_string(&data) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                eprintln!("[warn] Failed to save spawn position to '{}': {}", path, e);
            }
        }
        Err(e) => {
            eprintln!("[warn] Failed to serialise spawn position: {}", e);
        }
    }
}

/// Loads the spawn position from `{data_dir}/player_data.json`.
///
/// Returns `None` if the file does not exist or cannot be parsed.  The caller
/// should fall back to [`crate::player::PlayerConfig::spawn_position`] in that
/// case.
///
/// # Examples
///
/// ```rust,no_run
/// use voxel_framework::player::persistence::load_spawn_position;
///
/// if let Some(pos) = load_spawn_position("./worlds/survival") {
///     println!("Loaded spawn: {:?}", pos);
/// }
/// ```
pub fn load_spawn_position(data_dir: &str) -> Option<Vec3> {
    let path = format!("{}/player_data.json", data_dir);
    let json = std::fs::read_to_string(&path).ok()?;
    let data: PlayerSaveData = match serde_json::from_str(&json) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[warn] Failed to parse '{}': {}", path, e);
            return None;
        }
    };
    Some(Vec3::new(data.spawn_x, data.spawn_y, data.spawn_z))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_serialise() {
        let pos = Vec3::new(1.5, 64.0, -3.25);
        let data = PlayerSaveData {
            spawn_x: pos.x,
            spawn_y: pos.y,
            spawn_z: pos.z,
        };
        let json = serde_json::to_string(&data).unwrap();
        let loaded: PlayerSaveData = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.spawn_x, pos.x);
        assert_eq!(loaded.spawn_y, pos.y);
        assert_eq!(loaded.spawn_z, pos.z);
    }
}
