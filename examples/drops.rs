//! Optional default drop visuals and collector-independent expiration.
//! Space toggles collection; R spawns drops. No terrain or external assets.

use bevy::prelude::*;
use voxel_framework::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((
            MiningPlugin,
            DropsPlugin::<DefaultItem>::new(),
            DropVisualPlugin::<DefaultItem>::new(),
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, (controls, report_pickups))
        .run();
}

fn spawn_batch(commands: &mut Commands) {
    for (index, item) in [
        DefaultItem::RockFragment,
        DefaultItem::Crystal,
        DefaultItem::RareMineral,
    ]
    .into_iter()
    .enumerate()
    {
        commands.spawn((
            DroppedItem {
                item,
                count: 3,
                velocity: Vec3::ZERO,
                lifetime: 10.0,
            },
            Transform::from_xyz((index as f32 - 1.0) * 2.0, 1.0, 0.0),
        ));
    }
}

fn setup(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 4.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Text::new("Drops: Space toggles collection | R respawns | Expire after 10 seconds"),
        TextFont {
            font_size: 20.0,
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: px(16),
            left: px(16),
            ..default()
        },
    ));
    spawn_batch(&mut commands);
}

fn controls(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    collectors: Query<Entity, With<ItemCollector>>,
) {
    if keys.just_pressed(KeyCode::KeyR) {
        spawn_batch(&mut commands);
    }
    if keys.just_pressed(KeyCode::Space) {
        if collectors.is_empty() {
            commands.spawn((
                ItemCollector,
                PrimaryContainer,
                ContainerComponent::<DefaultItem>::new(9),
                Transform::default(),
            ));
        } else {
            for entity in &collectors {
                commands.entity(entity).despawn();
            }
        }
    }
}

fn report_pickups(mut messages: MessageReader<ItemPickedUp<DefaultItem>>) {
    for pickup in messages.read() {
        info!("Collected {} x {}", pickup.count, pickup.item.name());
    }
}
