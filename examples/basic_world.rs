//! Basic world example — minimal voxel-framework demo.
//!
//! Controls: WASD / Space / Shift / Q+E roll / mouse look.
//! Escape — pause menu  |  Left click — lock cursor.

use std::sync::Arc;
use bevy::prelude::*;
use voxel_framework::prelude::*;
use voxel_framework::audio::{SfxConfig, SfxEvent, SfxRequest};
use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings};

fn main() {
    let world_config = WorldConfig::from_file("world.toml").unwrap_or_default();
    let generator    = world_config.build_asteroid_generator();
    let spawn_pos    = Vec3::from_array(world_config.spawn_position);

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(VoxelPlugin::<DefaultVoxel>::new())
        .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())
        .add_plugins(PersistencePlugin::<DefaultVoxel>::new())
        .add_plugins(PauseMenuPlugin)
        .insert_resource({
            let mut sfx = SfxConfig::new();
            sfx.register(SfxEvent::Mining, vec![
                "audio/impactMining_000.ogg",
                "audio/impactMining_001.ogg",
                "audio/impactMining_002.ogg",
                "audio/impactMining_003.ogg",
                "audio/impactMining_004.ogg",
            ]);
            sfx.register(SfxEvent::MiningComplete, vec![
                "audio/impactGeneric_light_000.ogg",
                "audio/impactGeneric_light_001.ogg",
                "audio/impactGeneric_light_002.ogg",
            ]);
            sfx.register(SfxEvent::BlockPlace, vec![
                "audio/impactTin_medium_002.ogg",
            ]);
            sfx.register(SfxEvent::ItemPickup,    vec!["audio/rollover1.ogg"]);
            sfx.register(SfxEvent::HotbarScroll,  vec!["audio/rollover1.ogg"]);
            sfx.register(SfxEvent::PauseOpen,     vec!["audio/click1.ogg"]);
            sfx.register(SfxEvent::PauseClose,    vec!["audio/click1.ogg"]);
            sfx
        })
        .insert_resource(GeneratorResource::<DefaultVoxel> {
            generator: Arc::new(generator),
        })
        .insert_resource(world_config)
        .insert_resource(PlayerConfig {
            spawn_position: spawn_pos,
            speed: 16.0,
            ..default()
        })
        .add_systems(Startup, (spawn_player, setup))
        .add_systems(Update, play_sfx)
        .add_systems(
            Update,
            (player_look, player_move, player_resolve_collision::<DefaultVoxel>)
                .chain()
                .run_if(in_state(PauseMenuState::Playing)),
        )
        .run();
}

fn play_sfx(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    sfx_config: Res<SfxConfig>,
    mut sfx_reader: bevy::ecs::message::MessageReader<SfxRequest>,
    mut seed: Local<u64>,
) {
    for request in sfx_reader.read() {
        *seed = seed.wrapping_add(1);
        if let Some(path) = sfx_config.pick_path(&request.event, *seed) {
            let handle = asset_server.load::<AudioSource>(path.to_string());
            commands.spawn((AudioPlayer(handle), PlaybackSettings::ONCE));
        }
    }
}

fn setup(mut commands: Commands, mut viewer: ResMut<ViewerPosition>, config: Res<PlayerConfig>, asset_server: Res<AssetServer>) {
    viewer.position = config.spawn_position;

    commands.spawn((
        DirectionalLight {
            illuminance: 15_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(100.0, 200.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.insert_resource(PauseMenuConfig {
        font: asset_server.load("fonts/NotoSans-Regular.ttf"),
    });
}
