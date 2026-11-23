//! Native inventory-backed hotbar without external images.
//! cargo run --example hotbar_icons -- 800 600
//! Keys 1-9 / wheel: select; A: add rock; C: clear selected slot.
//! The optional --verify flag asserts actual Bevy layout after startup.
use bevy::prelude::*;
use voxel_framework::prelude::*;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let width = args
        .first()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1280);
    let height = args
        .get(1)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(720);
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Hotbar icons".into(),
            resolution: (width, height).into(),
            mode: if args.iter().any(|arg| arg == "--fullscreen") {
                bevy::window::WindowMode::BorderlessFullscreen(
                    bevy::window::MonitorSelection::Primary,
                )
            } else {
                bevy::window::WindowMode::Windowed
            },
            ..default()
        }),
        ..default()
    }))
    .insert_resource(ClearColor(Color::srgb(0.035, 0.045, 0.065)))
    .insert_resource(HotbarState::new(14))
    .add_plugins((
        HotbarPlugin,
        HotbarUiPlugin,
        HotbarContentPlugin::<DefaultItem>::new(),
    ))
    .add_systems(Startup, setup)
    .add_systems(Update, edit_inventory);
    if args.iter().any(|arg| arg == "--verify") {
        app.add_systems(PostUpdate, verify_layout.after(bevy::ui::UiSystems::Layout));
    }
    app.run();
}

fn setup(mut commands: Commands, mut icons: ResMut<ItemIconPalette<DefaultItem>>) {
    commands.spawn(Camera2d);
    // Explicit associations: fragment byte 20 is not Rock voxel byte 1.
    icons.register_voxel(DefaultItem::RockFragment, DefaultVoxel::Rock);
    icons.register_voxel(DefaultItem::Crystal, DefaultVoxel::SiliconCrystal);
    icons.register_voxel(DefaultItem::RareMineral, DefaultVoxel::RareMineral);
    icons.register_voxel(DefaultItem::IronOre, DefaultVoxel::IronOre);
    // WoodenPickaxe is intentionally unregistered to demonstrate '?'.
    let mut container = ContainerComponent::<DefaultItem>::new(14);
    for (index, item, count) in [
        (0, DefaultItem::RockFragment, 1),
        (1, DefaultItem::Crystal, 12),
        (2, DefaultItem::WoodenPickaxe, 1),
        (4, DefaultItem::RareMineral, 7),
        (13, DefaultItem::IronOre, 3),
    ] {
        container.0.slots[index] = Some(InventorySlot { item, count });
    }
    commands.spawn((PrimaryContainer, container));
    commands.spawn((
        Text::new("Material icons\n1-9 / wheel: select   A: add rock   C: clear slot"),
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
}

fn edit_inventory(
    keys: Res<ButtonInput<KeyCode>>,
    hotbar: Res<HotbarState>,
    mut containers: Query<&mut ContainerComponent<DefaultItem>, With<PrimaryContainer>>,
) {
    let Ok(mut container) = containers.single_mut() else {
        return;
    };
    if keys.just_pressed(KeyCode::KeyA) {
        container.0.add(DefaultItem::RockFragment, 1);
    }
    if keys.just_pressed(KeyCode::KeyC)
        && let Some(slot) = container.0.slots.get_mut(hotbar.selected)
    {
        *slot = None;
    }
}

// Example-only inspection: the only nodes with 2px borders are the fourteen slots.
fn verify_layout(
    windows: Query<&Window>,
    slots: Query<(&Node, &ComputedNode, &UiGlobalTransform)>,
    mut frames: Local<u32>,
) {
    *frames += 1;
    if *frames != 10 {
        return;
    }
    let window = windows.single().unwrap();
    let mut bounds: Vec<_> = slots
        .iter()
        .filter(|(style, _, _)| style.border == UiRect::all(px(2)))
        .map(|(_, node, transform)| {
            let scale = node.inverse_scale_factor();
            let center = transform.to_scale_angle_translation().2 * scale;
            let size = node.size() * scale;
            assert!(
                (size.x - size.y).abs() <= 1.0,
                "slot must stay square: {size:?}"
            );
            assert!(size.x > 0.0 && size.x <= 51.0, "slot size: {size:?}");
            (center - size / 2.0, center + size / 2.0)
        })
        .collect();
    bounds.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
    assert_eq!(bounds.len(), 14);
    assert!(bounds[0].0.x >= 15.0);
    assert!(bounds[13].1.x <= window.width() - 15.0);
    assert!(((bounds[0].0.x + bounds[13].1.x) / 2.0 - window.width() / 2.0).abs() <= 1.0);
    for adjacent in bounds.windows(2) {
        assert!((adjacent[1].0.x - adjacent[0].1.x - 4.0).abs() <= 1.0);
        assert!((adjacent[1].0.y - adjacent[0].0.y).abs() <= 1.0);
    }
    assert!((window.height() - bounds[0].1.y - 16.0).abs() <= 1.0);
    println!(
        "HOTBAR_LAYOUT_OK {}x{} slots=14 first={:?} last={:?}",
        window.width(),
        window.height(),
        bounds[0],
        bounds[13]
    );
}
