//! Configurable SFX system.
//!
//! The framework emits [`SfxRequest`] messages when audio should play.
//! Add [`DefaultAudioPlugin`] to handle them with Bevy's built-in audio,
//! or implement your own handler for custom audio engines.
//!
//! # Real audio with Bevy
//!
//! By default `bevy` is compiled without the `bevy_audio` feature because
//! `voxel-framework` sets `default-features = false`. To enable real audio
//! playback:
//!
//! 1. Add `"bevy_audio"` (and a backend such as `"vorbis"`) to the `features`
//!    list of the `bevy` dependency in your game's `Cargo.toml`.
//! 2. Replace [`DefaultAudioPlugin`] (which only logs) with your own plugin
//!    that reads [`SfxRequest`] messages and uses `AudioPlayer` + `PlaybackSettings`.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::world::{VoxelBroken, VoxelPlaced};

// ─── SfxEvent ─────────────────────────────────────────────────────────────────

/// Identifies a sound effect category.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SfxEvent {
    Mining,
    MiningComplete,
    BlockPlace,
    ItemPickup,
    HotbarScroll,
    PauseOpen,
    PauseClose,
    /// Custom event for game-specific sounds.
    Custom(String),
}

impl SfxEvent {
    /// Returns a stable string key for this event used as the `HashMap` key in
    /// [`SfxConfig`].
    pub fn key(&self) -> String {
        match self {
            SfxEvent::Mining => "Mining".to_string(),
            SfxEvent::MiningComplete => "MiningComplete".to_string(),
            SfxEvent::BlockPlace => "BlockPlace".to_string(),
            SfxEvent::ItemPickup => "ItemPickup".to_string(),
            SfxEvent::HotbarScroll => "HotbarScroll".to_string(),
            SfxEvent::PauseOpen => "PauseOpen".to_string(),
            SfxEvent::PauseClose => "PauseClose".to_string(),
            SfxEvent::Custom(s) => format!("Custom:{s}"),
        }
    }
}

// ─── SfxRequest ───────────────────────────────────────────────────────────────

/// Emitted by framework systems when a sound should play.
#[derive(Message, Debug, Clone)]
pub struct SfxRequest {
    pub event: SfxEvent,
    /// Optional world position for spatial audio. `None` = 2D/UI sound.
    pub position: Option<Vec3>,
}

// ─── SfxConfig ────────────────────────────────────────────────────────────────

/// Maps [`SfxEvent`] categories to asset paths.
///
/// Each category can have multiple paths — one is chosen deterministically
/// using a seed (see [`Self::pick_path`]).
///
/// # Example
///
/// ```rust
/// use voxel_framework::audio::{SfxConfig, SfxEvent};
///
/// let mut config = SfxConfig::new();
/// config.register(SfxEvent::BlockPlace, vec!["sounds/place1.ogg", "sounds/place2.ogg"]);
/// assert_eq!(config.pick_path(&SfxEvent::BlockPlace, 0), Some("sounds/place1.ogg"));
/// assert_eq!(config.pick_path(&SfxEvent::BlockPlace, 1), Some("sounds/place2.ogg"));
/// ```
#[derive(Resource, Debug, Clone)]
pub struct SfxConfig {
    /// event_key → list of asset paths for that event.
    pub sounds: HashMap<String, Vec<String>>,
    /// Master volume in [0.0, 1.0]. Default: `1.0`.
    pub volume: f32,
}

impl SfxConfig {
    /// Creates an empty [`SfxConfig`] with master volume `1.0`.
    pub fn new() -> Self {
        Self {
            sounds: HashMap::new(),
            volume: 1.0,
        }
    }

    /// Registers one or more asset paths for the given event category.
    ///
    /// Calling this a second time for the same event **replaces** the
    /// existing path list.
    pub fn register(&mut self, event: SfxEvent, paths: Vec<&str>) {
        self.sounds.insert(
            event.key(),
            paths.into_iter().map(|p| p.to_string()).collect(),
        );
    }

    /// Returns all registered paths for an event, or `None` if unregistered.
    pub fn get_paths(&self, event: &SfxEvent) -> Option<&Vec<String>> {
        self.sounds.get(&event.key())
    }

    /// Deterministically picks one path for `event` using `seed`.
    ///
    /// The selection is `paths[seed % paths.len()]`, so the same seed always
    /// produces the same result. Returns `None` if the event has no paths.
    pub fn pick_path(&self, event: &SfxEvent, seed: u64) -> Option<&str> {
        let paths = self.get_paths(event)?;
        if paths.is_empty() {
            return None;
        }
        let index = (seed % paths.len() as u64) as usize;
        Some(paths[index].as_str())
    }
}

impl Default for SfxConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Systems ──────────────────────────────────────────────────────────────────

/// Listens for [`VoxelBroken`] messages and emits an [`SfxRequest`] for
/// [`SfxEvent::MiningComplete`] at the voxel's world position.
pub fn mining_sfx_system(
    mut broken_reader: MessageReader<VoxelBroken>,
    mut sfx_writer: MessageWriter<SfxRequest>,
) {
    for broken in broken_reader.read() {
        sfx_writer.write(SfxRequest {
            event: SfxEvent::MiningComplete,
            position: Some(broken.world_pos),
        });
    }
}

/// Listens for [`VoxelPlaced`] messages and emits an [`SfxRequest`] for
/// [`SfxEvent::BlockPlace`] at the voxel's world position.
pub fn placement_sfx_system(
    mut placed_reader: MessageReader<VoxelPlaced>,
    mut sfx_writer: MessageWriter<SfxRequest>,
) {
    for placed in placed_reader.read() {
        sfx_writer.write(SfxRequest {
            event: SfxEvent::BlockPlace,
            position: Some(placed.world_pos),
        });
    }
}

// ─── SfxPlugin ────────────────────────────────────────────────────────────────

/// Core SFX plugin.
///
/// Registers [`SfxConfig`] and the [`SfxRequest`] message channel, and adds
/// the framework-side systems that translate world events into [`SfxRequest`]
/// messages.
///
/// This plugin does **not** play any audio itself. Add [`DefaultAudioPlugin`]
/// (stub logger) or your own plugin that reads [`SfxRequest`] messages for
/// actual playback.
pub struct SfxPlugin;

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SfxConfig>();
        app.add_message::<SfxRequest>();
        app.add_systems(Update, (mining_sfx_system, placement_sfx_system));
    }
}

// ─── DefaultAudioPlugin ───────────────────────────────────────────────────────

/// Plays sounds using Bevy's built-in audio system.
///
/// **Current implementation**: stub that logs each [`SfxRequest`] to stderr.
/// Real `bevy_audio` playback requires enabling the `"bevy_audio"` feature
/// (and an audio backend such as `"vorbis"`) in your game's `Cargo.toml`.
/// See the module-level documentation for details.
pub struct DefaultAudioPlugin;

impl Plugin for DefaultAudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, play_sfx_system);
    }
}

/// Reads every [`SfxRequest`] and logs it.
///
/// Replace this system (or the whole [`DefaultAudioPlugin`]) with one that
/// calls `asset_server.load::<AudioSource>(path)` and spawns
/// `(AudioPlayer(handle), PlaybackSettings::ONCE)` once `bevy_audio` is
/// enabled.
fn play_sfx_system(mut sfx_reader: MessageReader<SfxRequest>) {
    for request in sfx_reader.read() {
        eprintln!("SFX: {:?}", request.event);
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_paths() -> SfxConfig {
        let mut cfg = SfxConfig::new();
        cfg.register(
            SfxEvent::BlockPlace,
            vec!["sounds/place1.ogg", "sounds/place2.ogg", "sounds/place3.ogg"],
        );
        cfg
    }

    #[test]
    fn test_sfx_config_pick_deterministic() {
        let cfg = config_with_paths();
        let first = cfg.pick_path(&SfxEvent::BlockPlace, 0);
        let second = cfg.pick_path(&SfxEvent::BlockPlace, 0);
        assert_eq!(first, second, "same seed must always produce the same path");
    }

    #[test]
    fn test_sfx_config_pick_wraps() {
        let cfg = config_with_paths();
        // 3 paths; seed 5 → 5 % 3 = 2 → "sounds/place3.ogg"
        let path = cfg.pick_path(&SfxEvent::BlockPlace, 5);
        assert!(path.is_some(), "seed > paths.len() must still return a valid path");
        assert_eq!(path.unwrap(), "sounds/place3.ogg");
    }

    #[test]
    fn test_sfx_config_returns_none_for_unregistered_event() {
        let cfg = SfxConfig::new();
        assert!(cfg.pick_path(&SfxEvent::Mining, 0).is_none());
    }

    #[test]
    fn test_sfx_config_get_paths() {
        let cfg = config_with_paths();
        let paths = cfg.get_paths(&SfxEvent::BlockPlace).unwrap();
        assert_eq!(paths.len(), 3);
    }

    #[test]
    fn test_sfx_event_key_custom() {
        let event = SfxEvent::Custom("footstep".to_string());
        assert_eq!(event.key(), "Custom:footstep");
    }
}
