//! Decoupled sound-effect system for the voxel framework.
//!
//! # Design
//!
//! The audio module is deliberately split into two layers:
//!
//! * **Event layer** ([`SfxPlugin`]) — framework systems translate world events
//!   (voxel broken, voxel placed, …) into [`SfxRequest`] messages.  This layer
//!   has no knowledge of any audio engine.
//! * **Playback layer** — the game is responsible for consuming [`SfxRequest`]
//!   messages and driving its chosen audio backend.  [`DefaultAudioPlugin`] is
//!   provided as a stub that only logs requests; replace it for real playback.
//!
//! This split keeps `voxel-framework` free of a mandatory `bevy_audio`
//! dependency.  Because `voxel-framework` compiles Bevy with
//! `default-features = false`, the `bevy_audio` crate (and any codec backend
//! such as `vorbis`) is **not** included unless the consuming game opts in.
//!
//! # Integration
//!
//! 1. Add `"bevy_audio"` and a codec feature (e.g. `"vorbis"`) to the `bevy`
//!    dependency in your game's `Cargo.toml`.
//! 2. Populate [`SfxConfig`] with asset paths for each [`SfxEvent`] category.
//! 3. Add [`SfxPlugin`] to register the event channel and framework systems.
//! 4. Add your own playback plugin (or use [`DefaultAudioPlugin`] for
//!    development) to consume [`SfxRequest`] messages.
//!
//! ## Example
//!
//! ```no_run
//! use bevy::prelude::*;
//! use voxel_framework::audio::{SfxConfig, SfxEvent, SfxPlugin, SfxRequest};
//! use bevy::ecs::message::MessageReader;
//!
//! // Custom playback plugin that uses bevy_audio (requires "bevy_audio" feature).
//! struct MyAudioPlugin;
//!
//! impl Plugin for MyAudioPlugin {
//!     fn build(&self, app: &mut App) {
//!         app.add_systems(Update, play_sfx);
//!     }
//! }
//!
//! fn play_sfx(
//!     mut reader: MessageReader<SfxRequest>,
//!     config: Res<SfxConfig>,
//!     asset_server: Res<AssetServer>,
//!     mut commands: Commands,
//! ) {
//!     for (i, request) in reader.read().enumerate() {
//!         if let Some(path) = config.pick_path(&request.event, i as u64).map(|p| p.to_owned()) {
//!             let source: Handle<AudioSource> = asset_server.load(path);
//!             commands.spawn((AudioPlayer(source), PlaybackSettings::ONCE));
//!         }
//!     }
//! }
//!
//! fn main() {
//!     let mut sfx_config = SfxConfig::new();
//!     sfx_config.register(SfxEvent::BlockPlace, vec!["sounds/place1.ogg"]);
//!     sfx_config.register(SfxEvent::MiningComplete, vec!["sounds/break1.ogg"]);
//!
//!     App::new()
//!         .insert_resource(sfx_config)
//!         .add_plugins(SfxPlugin)
//!         .add_plugins(MyAudioPlugin)
//!         .run();
//! }
//! ```

use std::collections::HashMap;

use bevy::prelude::*;

use crate::world::{VoxelBroken, VoxelPlaced};

// ─── SfxEvent ─────────────────────────────────────────────────────────────────

/// A category of sound effect that the framework can request.
///
/// Each variant corresponds to a distinct gameplay moment.  Framework systems
/// emit these through [`SfxRequest`]; games map them to asset paths via
/// [`SfxConfig`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SfxEvent {
    /// Emitted repeatedly while the player is actively mining a voxel (e.g.
    /// for a continuous digging sound loop or per-hit chip).
    Mining,
    /// Emitted once when a voxel is fully broken and removed from the world.
    /// Triggered by the `VoxelBroken` message.
    MiningComplete,
    /// Emitted when a voxel is successfully placed into the world.
    /// Triggered by the `VoxelPlaced` message.
    BlockPlace,
    /// Emitted when the player picks up an item from the world.
    ItemPickup,
    /// Emitted when the player scrolls the hotbar slot selection.
    HotbarScroll,
    /// Emitted when the pause menu is opened.
    PauseOpen,
    /// Emitted when the pause menu is closed.
    PauseClose,
    /// Emitted on each footstep when the player moves while grounded.
    ///
    /// The inner value is the byte representation of the voxel type underfoot,
    /// allowing games to map different surface types to different sounds.
    Footstep(u8),
    /// Escape hatch for game-specific sounds not covered by the framework
    /// variants.  The inner string is an arbitrary identifier chosen by the
    /// game and must match the key used in [`SfxConfig`].
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
            SfxEvent::Footstep(surface) => format!("Footstep:{surface}"),
            SfxEvent::Custom(s) => format!("Custom:{s}"),
        }
    }
}

// ─── SfxRequest ───────────────────────────────────────────────────────────────

/// A request for a sound effect to be played, emitted by framework systems.
///
/// [`SfxPlugin`] adds systems that produce these messages in response to world
/// events.  A playback plugin (e.g. [`DefaultAudioPlugin`] or a custom one)
/// reads them and drives the audio backend.
#[derive(Message, Debug, Clone)]
pub struct SfxRequest {
    /// The sound effect category to play.
    pub event: SfxEvent,
    /// World-space position for spatial (3-D) audio, or `None` for a 2-D / UI
    /// sound that has no position.
    ///
    /// When `Some`, the playback system may attenuate volume and apply panning
    /// based on the listener's position.  When `None`, the sound should be
    /// played at a fixed volume regardless of the camera position.
    pub position: Option<Vec3>,
}

// ─── SfxConfig ────────────────────────────────────────────────────────────────

/// Maps [`SfxEvent`] categories to lists of asset paths.
///
/// Each category can hold multiple paths so the game can register variant
/// sounds (e.g. several digging sounds) and play them in round-robin order.
/// A specific path is selected deterministically by `pick_path` using a
/// caller-supplied seed, making the selection reproducible across frames.
///
/// `SfxConfig` is registered as a `Resource` by [`SfxPlugin`]; insert your
/// populated instance before adding the plugin, or mutate the resource after
/// `App::run` via a startup system.
///
/// # Examples
///
/// ```no_run
/// use voxel_framework::audio::{SfxConfig, SfxEvent};
///
/// let mut config = SfxConfig::new();
/// config.register(SfxEvent::BlockPlace, vec!["sounds/place1.ogg", "sounds/place2.ogg"]);
/// assert_eq!(config.pick_path(&SfxEvent::BlockPlace, 0), Some("sounds/place1.ogg"));
/// assert_eq!(config.pick_path(&SfxEvent::BlockPlace, 1), Some("sounds/place2.ogg"));
/// // Seeds wrap around: seed 2 → index 0 again for a two-entry list.
/// assert_eq!(config.pick_path(&SfxEvent::BlockPlace, 2), Some("sounds/place1.ogg"));
/// ```
#[derive(Resource, Debug, Clone)]
pub struct SfxConfig {
    /// Maps the string key of each [`SfxEvent`] to its registered asset paths.
    ///
    /// Keys are produced by [`SfxEvent::key`].  Prefer using
    /// [`SfxConfig::register`] rather than inserting into this map directly.
    pub sounds: HashMap<String, Vec<String>>,
    /// Master playback volume applied to all sounds, in the range `[0.0, 1.0]`.
    ///
    /// `0.0` is silent; `1.0` is full volume.  Default: `1.0`.  Values outside
    /// `[0.0, 1.0]` are not clamped by the framework — the audio backend
    /// determines how they are handled.
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

/// Translates `VoxelBroken` messages into [`SfxRequest`] messages with event
/// [`SfxEvent::MiningComplete`].
///
/// The request carries the broken voxel's world position so that a spatial
/// audio handler can place the sound in 3-D space.  This system is added by
/// [`SfxPlugin`] and runs in the [`Update`] schedule.
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

/// Translates `VoxelPlaced` messages into [`SfxRequest`] messages with event
/// [`SfxEvent::BlockPlace`].
///
/// The request carries the placed voxel's world position so that a spatial
/// audio handler can place the sound in 3-D space.  This system is added by
/// [`SfxPlugin`] and runs in the [`Update`] schedule.
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

/// Core plugin for the framework sound-effect system.
///
/// # What it registers
///
/// * [`SfxConfig`] — initialized as a default resource (all categories empty,
///   volume `1.0`).  Insert a pre-populated instance before adding this plugin
///   to override the defaults.
/// * The [`SfxRequest`] message channel so framework systems can emit requests
///   and game systems can read them.
/// * [`mining_sfx_system`] and [`placement_sfx_system`] in the [`Update`]
///   schedule.
///
/// # What it does NOT do
///
/// `SfxPlugin` never loads audio assets, never creates audio entities, and
/// never calls into any audio backend.  Actual playback is the responsibility
/// of a separate plugin — either [`DefaultAudioPlugin`] (development stub) or
/// a custom plugin that reads [`SfxRequest`] events.
pub struct SfxPlugin;

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SfxConfig>();
        app.add_message::<SfxRequest>();
        app.add_systems(Update, (mining_sfx_system, placement_sfx_system));
    }
}

// ─── DefaultAudioPlugin ───────────────────────────────────────────────────────

/// A round-robin counter per SFX key so successive identical events cycle
/// through registered variant paths rather than always picking the same one.
#[derive(Resource, Default)]
struct SfxCounter(HashMap<String, u64>);

/// Ready-to-use audio plugin that plays [`SfxRequest`] messages via
/// Bevy's built-in `bevy_audio` backend.
///
/// For each incoming [`SfxRequest`], this plugin:
/// 1. Resolves the event key through [`SfxConfig::pick_path`] (round-robin
///    across registered variants).
/// 2. Loads the asset with `AssetServer::load`.
/// 3. Spawns an `(AudioPlayer, PlaybackSettings::ONCE)` entity that Bevy's
///    audio system will play and automatically despawn when finished.
///
/// # Setup
///
/// Register asset paths with [`SfxConfig`] before adding the plugin, or
/// populate them in a startup system:
///
/// ```no_run
/// use bevy::prelude::*;
/// use voxel_framework::audio::{DefaultAudioPlugin, SfxConfig, SfxEvent, SfxPlugin};
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(SfxPlugin)
///     .add_plugins(DefaultAudioPlugin)
///     .add_systems(Startup, |mut cfg: ResMut<SfxConfig>| {
///         cfg.register(SfxEvent::MiningComplete, vec!["sounds/break.ogg"]);
///         cfg.register(SfxEvent::BlockPlace,     vec!["sounds/place.ogg"]);
///     })
///     .run();
/// ```
///
/// # Missing files
///
/// If an asset path is not found on disk, Bevy logs a warning and skips
/// playback — no panic, no crash.
///
/// # Spatial audio
///
/// When the [`SfxRequest::position`] field is `Some`, the plugin attaches a
/// [`SpatialListener`]-compatible [`Transform`] to the audio entity so Bevy
/// can attenuate volume by distance.  Requires a [`SpatialListener`] on the
/// camera/player entity.
pub struct DefaultAudioPlugin;

impl Plugin for DefaultAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SfxCounter>();
        app.add_systems(Update, play_sfx_system);
    }
}

fn play_sfx_system(
    mut sfx_reader: MessageReader<SfxRequest>,
    config: Res<SfxConfig>,
    asset_server: Res<AssetServer>,
    mut counter: ResMut<SfxCounter>,
    mut commands: Commands,
) {
    for request in sfx_reader.read() {
        let key = request.event.key();
        let seed = {
            let c = counter.0.entry(key.clone()).or_insert(0);
            let s = *c;
            *c = c.wrapping_add(1);
            s
        };
        let Some(path) = config.pick_path(&request.event, seed).map(|p| p.to_owned()) else {
            continue;
        };
        let source: Handle<AudioSource> = asset_server.load(path);
        let mut settings = PlaybackSettings::ONCE;
        settings.volume = bevy::audio::Volume::Linear(config.volume);
        let mut entity = commands.spawn((AudioPlayer(source), settings));
        if let Some(pos) = request.position {
            entity.insert(Transform::from_translation(pos));
        }
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
