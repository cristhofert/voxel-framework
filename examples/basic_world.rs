//! Basic world example — minimal voxel-framework demo.
//!
//! Controls: see [`voxel_framework::player`] docs (WASD, Space/Shift, Q/E roll, mouse).

use std::sync::Arc;
use bevy::prelude::*;
use voxel_framework::prelude::*;
use voxel_framework::player::PlayerPlugin;

fn main() {
    let world_config = WorldConfig::from_file("world.toml").unwrap_or_default();
    let generator    = world_config.build_asteroid_generator();
    let spawn_pos    = Vec3::from_array(world_config.spawn_position);

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(VoxelPlugin)
        .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())
        .add_plugins(PersistencePlugin::<DefaultVoxel>::new())
        .add_plugins(PlayerPlugin::<DefaultVoxel>::new())
        .insert_resource(GeneratorResource::<DefaultVoxel> {
            generator: Arc::new(generator),
        })
        .insert_resource(world_config)
        .insert_resource(PlayerConfig {
            spawn_position: spawn_pos,
            speed: 16.0,
            ..default()
        })
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, mut viewer: ResMut<ViewerPosition>, config: Res<PlayerConfig>) {
    // Seed streaming origin so chunks load on frame 1.
    viewer.position = config.spawn_position;

    commands.spawn((
        DirectionalLight {
            illuminance: 15_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(100.0, 200.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
