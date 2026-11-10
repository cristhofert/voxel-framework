//! Native focus/capture diagnostic. Run with `cargo run --example player_focus`.
//! Click to control, Escape to release, Alt-Tab away and back, then click again.
//! Compare ordinary W/A/S/D keys with desktop global shortcuts separately.
use bevy::{prelude::*, window::WindowFocused};
use voxel_framework::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Player focus: click to control; Escape to release".into(),
                ..default()
            }),
            ..default()
        }))
        .init_resource::<PlayerConfig>()
        .init_resource::<PlayerHealth>()
        .init_resource::<PlayerCrouching>()
        .init_resource::<ChunkManager>()
        .init_resource::<ViewerPosition>()
        .add_systems(Startup, (spawn_player, setup_scene))
        .add_systems(
            Update,
            (
                player_cursor_system,
                player_look,
                player_move::<DefaultVoxel>,
                report_focus,
            )
                .chain(),
        )
        .run();
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(4.0, 4.0, 4.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.7, 0.9),
            unlit: true,
            ..default()
        })),
    ));
}

fn report_focus(
    mut events: MessageReader<WindowFocused>,
    mut keyboard: MessageReader<bevy::input::keyboard::KeyboardInput>,
    windows: Query<(&Window, &bevy::window::CursorOptions)>,
    players: Query<&Transform, With<Player>>,
) {
    for event in events.read() {
        info!(window = ?event.window, focused = event.focused, "Native focus event");
    }
    for event in keyboard.read() {
        if event.state.is_pressed() && !event.repeat {
            if let Ok((window, cursor)) = windows.get(event.window) {
                info!(key = ?event.key_code, focused = window.focused,
                    grab = ?cursor.grab_mode, position = ?players.single().ok().map(|p| p.translation),
                    "Keyboard event received by game");
            }
        }
    }
}
