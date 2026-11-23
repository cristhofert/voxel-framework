//! Optional, asset-free default presentation for collectible drops.

use std::{collections::HashMap, marker::PhantomData};

use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use super::{DroppedItem, ItemType};

/// Draws positioned drops as small, unlit cubes colored by [`ItemType::color`].
///
/// Add after `DefaultPlugins`, alongside [`super::DropsPlugin`]. This plugin
/// requires `Assets<Mesh>` and `Assets<StandardMaterial>`; it does not install
/// gameplay, cameras, or lighting. It is not included in convenience bundles.
///
/// Cubes are 0.25 world units wide, sharing one mesh and one material per item
/// variant. Render components live on the drop itself, so pickup and expiration
/// also remove its visual. Drops with an existing `Mesh3d` or
/// `MeshMaterial3d<StandardMaterial>` are left untouched. To own all presentation,
/// omit this plugin. Drop transforms and quantities are never changed here.
///
/// Colors are sampled on first use; changing a rendered drop's item identity or
/// dynamically recoloring the palette is not supported by this default.
///
/// ```no_run
/// use bevy::prelude::*;
/// use voxel_framework::prelude::*;
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins((MiningPlugin, DropsPlugin::<DefaultItem>::new(), DropVisualPlugin::<DefaultItem>::new()));
/// ```
pub struct DropVisualPlugin<I: ItemType> {
    _marker: PhantomData<I>,
}

impl<I: ItemType> DropVisualPlugin<I> {
    /// Creates the optional default drop renderer.
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<I: ItemType> Default for DropVisualPlugin<I> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: ItemType> Plugin for DropVisualPlugin<I> {
    fn build(&self, app: &mut App) {
        app.insert_resource(DropVisualAssets::<I> {
            mesh: None,
            materials: HashMap::new(),
        })
        .add_systems(
            PostUpdate,
            attach_visuals::<I>
                .before(TransformSystems::Propagate)
                .before(VisibilitySystems::VisibilityPropagate)
                .before(VisibilitySystems::CalculateBounds),
        );
    }
}

#[derive(Resource)]
struct DropVisualAssets<I: ItemType> {
    mesh: Option<Handle<Mesh>>,
    materials: HashMap<I, Handle<StandardMaterial>>,
}

fn attach_visuals<I: ItemType>(
    mut commands: Commands,
    drops: Query<
        (Entity, &DroppedItem<I>),
        (
            With<Transform>,
            Without<Mesh3d>,
            Without<MeshMaterial3d<StandardMaterial>>,
        ),
    >,
    mut cache: ResMut<DropVisualAssets<I>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, drop) in &drops {
        let mesh = cache
            .mesh
            .get_or_insert_with(|| meshes.add(Cuboid::new(0.25, 0.25, 0.25)))
            .clone();
        let material = cache
            .materials
            .entry(drop.item)
            .or_insert_with(|| {
                let [r, g, b, a] = drop.item.color();
                materials.add(StandardMaterial {
                    base_color: Color::srgba(r, g, b, a),
                    unlit: true,
                    alpha_mode: if a < 1.0 {
                        AlphaMode::Blend
                    } else {
                        AlphaMode::Opaque
                    },
                    ..default()
                })
            })
            .clone();
        commands
            .entity(entity)
            .insert((Mesh3d(mesh), MeshMaterial3d(material)))
            .insert_if_new(Visibility::Inherited);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::{
        ContainerComponent, DefaultItem, DropTable, DropsPlugin, ItemCollector, ItemPickedUp,
        PrimaryContainer,
    };
    use crate::world::VoxelBroken;
    use bevy::camera::visibility::VisibilityPlugin;
    use bevy::transform::TransformPlugin;

    fn app(visuals: bool) -> App {
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_message::<VoxelBroken>()
            .add_plugins((
                TransformPlugin,
                VisibilityPlugin,
                DropsPlugin::<DefaultItem>::new(),
            ));
        if visuals {
            app.add_plugins(DropVisualPlugin::<DefaultItem>::new());
        }
        app
    }

    fn spawn(app: &mut App, item: DefaultItem, count: u32) -> Entity {
        app.world_mut()
            .spawn((
                DroppedItem {
                    item,
                    count,
                    velocity: Vec3::ZERO,
                    lifetime: 1.0,
                },
                Transform::from_xyz(2.0, 0.0, 0.0),
            ))
            .id()
    }

    #[test]
    fn mapped_drop_gets_visible_shared_assets_in_spawn_frame() {
        let mut app = app(true);
        app.world_mut()
            .resource_mut::<DropTable<DefaultItem>>()
            .register(1, DefaultItem::RockFragment, 1, 1);
        let position = Vec3::new(2.5, 3.5, 4.5);
        app.world_mut().write_message(VoxelBroken {
            chunk_coord: IVec3::ZERO,
            local: (2, 3, 4),
            world_pos: position,
            voxel_byte: 1,
        });
        app.update();
        let world = app.world_mut();
        let mut query = world.query_filtered::<Entity, With<DroppedItem<DefaultItem>>>();
        let entity = query.single(world).unwrap();
        let mesh = world
            .get::<Mesh3d>(entity)
            .expect("mapped drop must have a mesh")
            .0
            .clone();
        let material = world
            .get::<MeshMaterial3d<StandardMaterial>>(entity)
            .unwrap()
            .0
            .clone();
        assert_eq!(
            world.get::<GlobalTransform>(entity).unwrap().translation(),
            position
        );
        assert_eq!(
            *world.get::<Visibility>(entity).unwrap(),
            Visibility::Inherited
        );
        assert!(world.get::<InheritedVisibility>(entity).unwrap().get());
        let assets = world.resource::<Assets<StandardMaterial>>();
        let mat = assets.get(&material).unwrap();
        assert!(mat.unlit);
        let [r, g, b, a] = DefaultItem::RockFragment.color();
        assert_eq!(mat.base_color, Color::srgba(r, g, b, a));
        assert!(world.resource::<Assets<Mesh>>().get(&mesh).is_some());
        let second = spawn(&mut app, DefaultItem::RockFragment, 4);
        let other = spawn(&mut app, DefaultItem::Crystal, 1);
        app.update();
        assert_eq!(app.world().get::<Mesh3d>(second).unwrap().0, mesh);
        assert_eq!(
            app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(second)
                .unwrap()
                .0,
            material
        );
        assert_eq!(app.world().get::<Mesh3d>(other).unwrap().0, mesh);
        assert_ne!(
            app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(other)
                .unwrap()
                .0,
            material
        );
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 1);
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), 2);
    }

    #[test]
    fn visuals_are_opt_in_and_preserve_custom_components_and_late_transforms() {
        let mut app = app(false);
        let ordinary = spawn(&mut app, DefaultItem::RockFragment, 1);
        app.update();
        assert!(app.world().get::<Mesh3d>(ordinary).is_none());
        app.add_plugins(DropVisualPlugin::<DefaultItem>::new());
        let custom = spawn(&mut app, DefaultItem::Crystal, 1);
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(1.0, 1.0, 1.0));
        app.world_mut()
            .entity_mut(custom)
            .insert(Mesh3d(mesh.clone()));
        let custom_material = spawn(&mut app, DefaultItem::Crystal, 1);
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        app.world_mut()
            .entity_mut(custom_material)
            .insert(MeshMaterial3d(material.clone()));
        let late = app
            .world_mut()
            .spawn(DroppedItem {
                item: DefaultItem::Crystal,
                count: 1,
                velocity: Vec3::ZERO,
                lifetime: 1.0,
            })
            .id();
        app.update();
        assert!(app.world().get::<Mesh3d>(ordinary).is_some());
        assert_eq!(app.world().get::<Mesh3d>(custom).unwrap().0, mesh);
        assert!(
            app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(custom)
                .is_none()
        );
        assert_eq!(
            app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(custom_material)
                .unwrap()
                .0,
            material
        );
        assert!(app.world().get::<Mesh3d>(custom_material).is_none());
        assert!(app.world().get::<Mesh3d>(late).is_none());
        app.world_mut()
            .entity_mut(late)
            .insert(Transform::default());
        app.update();
        assert!(app.world().get::<Mesh3d>(late).is_some());
    }

    #[test]
    fn moving_drop_propagates_position_and_keeps_runtime_visual_replacement() {
        let mut app = app(true);
        let entity = spawn(&mut app, DefaultItem::RockFragment, 1);
        app.world_mut()
            .spawn((ItemCollector, Transform::from_xyz(50.0, 0.0, 0.0)));
        app.world_mut()
            .get_mut::<DroppedItem<DefaultItem>>(entity)
            .unwrap()
            .velocity = Vec3::new(1.0, 2.0, 3.0);
        app.update();
        let initial = app
            .world()
            .get::<GlobalTransform>(entity)
            .unwrap()
            .translation();
        let default_mesh = app.world().get::<Mesh3d>(entity).unwrap().0.clone();
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(100));
        app.update();
        let moved = app.world().get::<Transform>(entity).unwrap().translation;
        assert_ne!(moved, initial, "DropsPlugin must actually move the drop");
        assert_eq!(
            app.world()
                .get::<GlobalTransform>(entity)
                .unwrap()
                .translation(),
            moved
        );
        assert_eq!(app.world().get::<Mesh3d>(entity).unwrap().0, default_mesh);

        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Sphere::new(0.5));
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: Color::srgb(0.1, 0.8, 0.2),
                ..default()
            });
        app.world_mut().entity_mut(entity).insert((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Visibility::Hidden,
            Transform::from_xyz(-4.0, 5.0, 6.0),
        ));
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::ZERO);
        app.update();
        assert_eq!(
            app.world()
                .get::<GlobalTransform>(entity)
                .unwrap()
                .translation(),
            Vec3::new(-4.0, 5.0, 6.0)
        );
        assert_eq!(app.world().get::<Mesh3d>(entity).unwrap().0, mesh);
        assert_eq!(
            app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(entity)
                .unwrap()
                .0,
            material
        );
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
    }

    #[test]
    fn visible_remainder_survives_partial_pickup_and_cleans_up_on_collection_or_expiry() {
        let mut app = app(true);
        let item = DefaultItem::RockFragment;
        let entity = spawn(&mut app, item, 3);
        app.update();
        let mesh = app.world().get::<Mesh3d>(entity).unwrap().0.clone();
        let material = app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(entity)
            .unwrap()
            .0
            .clone();
        let mut inventory = ContainerComponent::<DefaultItem>::new(1);
        inventory.0.add(item, u32::MAX - 1);
        let player = app
            .world_mut()
            .spawn((
                ItemCollector,
                PrimaryContainer,
                inventory,
                Transform::from_xyz(2.0, 0.0, 0.0),
            ))
            .id();
        for expected in [1, 0, 2] {
            if expected == 2 {
                app.world_mut()
                    .get_mut::<ContainerComponent<DefaultItem>>(player)
                    .unwrap()
                    .0
                    .remove(item, 2);
            }
            app.update();
            let pickups: Vec<_> = app
                .world_mut()
                .resource_mut::<Messages<ItemPickedUp<DefaultItem>>>()
                .drain()
                .collect();
            assert_eq!(pickups.iter().map(|p| p.count).sum::<u32>(), expected);
            if expected != 2 {
                assert_eq!(
                    app.world()
                        .get::<DroppedItem<DefaultItem>>(entity)
                        .unwrap()
                        .count,
                    2
                );
                assert_eq!(app.world().get::<Mesh3d>(entity).unwrap().0, mesh);
                assert_eq!(
                    app.world()
                        .get::<MeshMaterial3d<StandardMaterial>>(entity)
                        .unwrap()
                        .0,
                    material
                );
            }
        }
        assert!(app.world().get_entity(entity).is_err());
        app.world_mut().despawn(player);
        let expired = spawn(&mut app, item, 1);
        app.update();
        assert_eq!(app.world().get::<Mesh3d>(expired).unwrap().0, mesh);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(1));
        app.update();
        assert!(app.world().get_entity(expired).is_err());
        assert_eq!(
            app.world_mut().query::<&Mesh3d>().iter(app.world()).count(),
            0
        );
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 1);
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), 1);
    }
}
