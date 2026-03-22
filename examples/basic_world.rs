//! Basic world example for voxel-framework.
//!
//! Demonstrates loading [`WorldConfig`] from `world.toml`, building an
//! [`AsteroidGenerator`] from the config, and wiring everything into a Bevy
//! app with persistence, a flying camera, and a directional light.
//!
//! Controls:
//!   W/S   — move forward/backward
//!   A/D   — strafe left/right
//!   Space — move up
//!   Shift — move down

use std::sync::Arc;

use bevy::prelude::*;

use voxel_framework::prelude::*;

fn main() {
    // ── 1. Load world configuration ──────────────────────────────────────────
    // Try to read world.toml from the working directory; fall back to defaults
    // if the file is absent or malformed.
    let world_config = WorldConfig::from_file("world.toml")
        .inspect_err(|e| {
            eprintln!("basic_world: could not load world.toml ({e}), using defaults");
        })
        .unwrap_or_default();

    // ── 2. Build the generator from config ───────────────────────────────────
    // build_asteroid_generator() maps the TOML asteroid settings to the
    // runtime AsteroidConfig, filling noise/mineral fields with defaults.
    let generator = world_config.build_asteroid_generator();

    // ── 3. Capture spawn position before moving world_config into the app ────
    let spawn_pos = Vec3::from_array(world_config.spawn_position);

    // ── 4. Assemble the Bevy app ─────────────────────────────────────────────
    App::new()
        .add_plugins(DefaultPlugins)
        // Core voxel framework: streaming, render, mesh generation.
        .add_plugins(VoxelPlugin)
        // Async chunk generation pipeline for DefaultVoxel.
        .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())
        // Auto-save modified chunks to disk (./chunks/ by default).
        .add_plugins(PersistencePlugin::<DefaultVoxel>::new())
        // Supply the generator built from world config.
        .insert_resource(GeneratorResource::<DefaultVoxel> {
            generator: Arc::new(generator),
        })
        // Make WorldConfig available as a Bevy resource.
        .insert_resource(world_config)
        .add_systems(
            Startup,
            // Pass the resolved spawn position into the startup system via a
            // closure so we avoid a potential system-ordering issue with the
            // WorldConfigPlugin startup system.
            move |commands: Commands, viewer: ResMut<ViewerPosition>| {
                startup_system(commands, viewer, spawn_pos);
            },
        )
        .add_systems(Update, (movement_system, chunk_log_system))
        .run();
}

/// Spawns the camera and directional light, and positions the viewer.
fn startup_system(
    mut commands: Commands,
    mut viewer: ResMut<ViewerPosition>,
    spawn_pos: Vec3,
) {
    info!("voxel-framework basic_world example started");

    // ── Viewer position ───────────────────────────────────────────────────────
    // Set the streaming origin so that the chunk manager loads chunks around
    // the spawn location defined in world.toml.
    viewer.position = spawn_pos;

    // ── Camera ────────────────────────────────────────────────────────────────
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(spawn_pos).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // ── Directional light (sun-like) ─────────────────────────────────────────
    commands.spawn((
        DirectionalLight {
            illuminance: 15_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(100.0, 200.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn movement_system(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut viewer: ResMut<ViewerPosition>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
) {
    const SPEED: f32 = 16.0;
    let delta = time.delta_secs() * SPEED;

    let mut direction = Vec3::ZERO;

    if keys.pressed(KeyCode::KeyW) {
        direction.z -= 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        direction.z += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        direction.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        direction.x += 1.0;
    }
    if keys.pressed(KeyCode::Space) {
        direction.y += 1.0;
    }
    if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
        direction.y -= 1.0;
    }

    if direction != Vec3::ZERO {
        let movement = direction.normalize() * delta;
        viewer.position += movement;

        if let Ok(mut transform) = camera_query.single_mut() {
            transform.translation += movement;
        }
    }
}

fn chunk_log_system(mut load_reader: MessageReader<ChunkLoadRequest>) {
    for &ChunkLoadRequest(coord) in load_reader.read() {
        info!("Chunk loaded at {coord}");
    }
}
