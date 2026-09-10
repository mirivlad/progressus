use super::*;
use progressus_app::{
    ChunkCoord, ChunkSnapshot, DoorState, ItemKind, LocalCell, NaturalResourceKind, StructureKind,
    WorldPosition,
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ObjectKey {
    Resource(WorldCell),
    Item(EntityId),
    Workbench(EntityId),
    Structure(EntityId),
    Site(EntityId),
    Carried(EntityId),
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Object {
    kind: ModelKind,
    position: WorldPosition,
    variant: u8,
}
struct TerrainEntry {
    source: ChunkSnapshot,
    entity: Entity,
    mesh: Handle<Mesh>,
}
#[derive(Resource, Default)]
pub(super) struct SceneCache {
    chunks: Vec<ChunkCoord>,
    revisions: Option<[u64; 3]>,
    terrain: BTreeMap<ChunkCoord, TerrainEntry>,
    objects: BTreeMap<ObjectKey, (Object, Entity)>,
    pawns: BTreeMap<EntityId, Entity>,
    resources: Vec<progressus_app::NaturalResourceSnapshot>,
    items: Vec<progressus_app::GroundItemSnapshot>,
}

fn item_model(kind: ItemKind) -> ModelKind {
    match kind {
        ItemKind::Wood => ModelKind::Wood,
        ItemKind::Stone => ModelKind::Stone,
        ItemKind::PrimitiveTool => ModelKind::PrimitiveTool,
        ItemKind::Berries => ModelKind::Berries,
    }
}
fn object_transform(object: &Object, origin: WorldCell) -> Transform {
    Transform::from_translation(space::local(object.position, origin))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn sync(
    mut commands: Commands,
    mut game: ResMut<Game>,
    mut view: ResMut<View>,
    mut cache: ResMut<SceneCache>,
    mut palette: ResMut<Palette>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut motion: ResMut<VisualMotion>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    windows: Query<&Window>,
) {
    let (Ok((camera, _)), Ok(window)) = (cameras.single(), windows.single()) else {
        return;
    };
    let transform = &GlobalTransform::from(view.camera_transform());
    let mut points = Vec::new();
    for screen in [
        Vec2::ZERO,
        Vec2::new(window.width(), 0.),
        Vec2::new(0., window.height()),
        Vec2::new(window.width(), window.height()),
    ] {
        if let Ok(ray) = camera.viewport_to_world(transform, screen)
            && let Some(p) = space::ground(ray)
        {
            points.push(p);
        }
    }
    if points.len() != 4 {
        return;
    }
    let chunks = space::visible_chunks(&points, view.origin);
    let revisions = [
        game.snapshot.exploration_revision,
        game.snapshot.item_revision,
        game.snapshot.resource_revision,
    ];
    let viewport_changed = chunks != cache.chunks;
    let terrain_changed =
        viewport_changed || cache.revisions.is_none_or(|old| old[0] != revisions[0]);
    let items_changed = viewport_changed
        || cache
            .revisions
            .is_none_or(|old| old[1] != revisions[1] || old[0] != revisions[0]);
    let resources_changed = viewport_changed
        || cache
            .revisions
            .is_none_or(|old| old[2] != revisions[2] || old[0] != revisions[0]);
    if terrain_changed || items_changed || resources_changed {
        let spatial = match game.app.snapshot(SnapshotQuery {
            chunks: chunks.clone(),
            include_terrain: terrain_changed,
            include_ground_items: items_changed,
            include_natural_resources: resources_changed,
            ..default()
        }) {
            Ok(s) => s,
            Err(e) => {
                game.message = e.to_string();
                return;
            }
        };
        if terrain_changed {
            let retained: BTreeSet<_> = spatial.chunks.iter().map(|c| c.coordinate).collect();
            cache.terrain.retain(|coord, entry| {
                if retained.contains(coord) {
                    true
                } else {
                    commands.entity(entry.entity).despawn();
                    meshes.remove(entry.mesh.id());
                    false
                }
            });
            for chunk in spatial.chunks {
                if cache
                    .terrain
                    .get(&chunk.coordinate)
                    .is_some_and(|e| e.source == chunk)
                {
                    continue;
                }
                let mesh = meshes.add(terrain::terrain_mesh(&chunk));
                let start = chunk
                    .coordinate
                    .world_cell(LocalCell::new(0, 0))
                    .expect("materialized chunk origin");
                let transform = Transform::from_translation(space::cell_local(start, view.origin));
                let entity = if let Some(old) = cache.terrain.remove(&chunk.coordinate) {
                    meshes.remove(old.mesh.id());
                    commands
                        .entity(old.entity)
                        .insert((Mesh3d(mesh.clone()), transform));
                    old.entity
                } else {
                    commands
                        .spawn((
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(palette.material.clone()),
                            transform,
                        ))
                        .id()
                };
                cache.terrain.insert(
                    chunk.coordinate,
                    TerrainEntry {
                        source: chunk,
                        entity,
                        mesh,
                    },
                );
            }
        }
        if items_changed {
            cache.items = spatial.ground_items;
        }
        if resources_changed {
            cache.resources = spatial.natural_resources;
        }
        cache.chunks = chunks;
        cache.revisions = Some(revisions);
    }
    if !game.dirty && !viewport_changed && !view.rebased && !items_changed && !resources_changed {
        return;
    }
    if view.rebased {
        for (coord, entry) in &cache.terrain {
            let start = coord
                .world_cell(LocalCell::new(0, 0))
                .expect("materialized origin");
            commands
                .entity(entry.entity)
                .insert(Transform::from_translation(space::cell_local(
                    start,
                    view.origin,
                )));
        }
    }
    let mut objects = BTreeMap::new();
    let visible: BTreeSet<_> = cache.chunks.iter().copied().collect();
    let mut insert = |key, kind, cell: WorldCell, variant| {
        if visible.contains(&cell.split().0) {
            objects.insert(
                key,
                Object {
                    kind,
                    position: WorldPosition::from_cell_center(cell).expect("valid object cell"),
                    variant,
                },
            );
        }
    };
    for r in &cache.resources {
        let kind = match r.kind {
            NaturalResourceKind::Tree => ModelKind::Tree,
            NaturalResourceKind::StoneOutcrop => ModelKind::StoneOutcrop,
            NaturalResourceKind::BerryBush => ModelKind::BerryBush,
        };
        let variant = (r.cell.x() as u64)
            .wrapping_mul(37)
            .wrapping_add(r.cell.y() as u64) as u8
            % 4;
        insert(ObjectKey::Resource(r.cell), kind, r.cell, variant);
    }
    for w in &game.snapshot.workstations {
        insert(ObjectKey::Workbench(w.id), ModelKind::Workbench, w.cell, 0);
    }
    for s in &game.snapshot.structures {
        let kind = match s.kind {
            StructureKind::StoneWall => ModelKind::Wall,
            StructureKind::Door => {
                if s.door_state == Some(DoorState::Open) {
                    ModelKind::OpenDoor
                } else {
                    ModelKind::Door
                }
            }
        };
        insert(ObjectKey::Structure(s.id), kind, s.cell, 0);
    }
    for s in &game.snapshot.construction_sites {
        insert(
            ObjectKey::Site(s.id),
            if s.kind == StructureKind::Door {
                ModelKind::ConstructionDoor
            } else {
                ModelKind::ConstructionWall
            },
            s.cell,
            0,
        );
    }
    for item in &cache.items {
        objects.insert(
            ObjectKey::Item(item.id),
            Object {
                kind: item_model(item.kind),
                position: item.position,
                variant: 0,
            },
        );
    }
    for item in &game.snapshot.carried_items {
        if let Some(c) =
            game.snapshot.characters.iter().find(|c| {
                c.id == item.character_id && visible.contains(&c.containing_cell.split().0)
            })
        {
            objects.insert(
                ObjectKey::Carried(item.id),
                Object {
                    kind: item_model(item.kind),
                    position: c.position,
                    variant: 0,
                },
            );
        }
    }
    cache.objects.retain(|key, (_, entity)| {
        if objects.contains_key(key) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });
    for (key, object) in objects {
        if let ObjectKey::Carried(id) = key
            && let (Some((_, entity)), Some(item)) = (
                cache.objects.get(&key),
                game.snapshot.carried_items.iter().find(|i| i.id == id),
            )
        {
            commands
                .entity(*entity)
                .insert(MotionTarget(item.character_id, Vec3::new(0.25, 0.45, 0.)));
        }
        if let Some((old, entity)) = cache.objects.get_mut(&key) {
            if *old != object {
                commands.entity(*entity).insert((
                    Mesh3d(palette.get(object.kind, object.variant, &mut meshes)),
                    object_transform(&object, view.origin),
                ));
                *old = object;
            } else if view.rebased {
                commands
                    .entity(*entity)
                    .insert(object_transform(&object, view.origin));
            }
        } else {
            let entity = commands
                .spawn((
                    Mesh3d(palette.get(object.kind, object.variant, &mut meshes)),
                    MeshMaterial3d(palette.material.clone()),
                    object_transform(&object, view.origin),
                ))
                .id();
            if let ObjectKey::Carried(id) = key
                && let Some(item) = game.snapshot.carried_items.iter().find(|i| i.id == id)
            {
                commands
                    .entity(entity)
                    .insert(MotionTarget(item.character_id, Vec3::new(0.25, 0.45, 0.)));
            }
            cache.objects.insert(key, (object, entity));
        }
    }
    let characters: BTreeSet<_> = game
        .snapshot
        .characters
        .iter()
        .filter(|c| visible.contains(&c.containing_cell.split().0))
        .map(|c| c.id)
        .collect();
    cache.pawns.retain(|id, entity| {
        if characters.contains(id) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });
    motion.retain(characters.iter().copied());
    for c in &game.snapshot.characters {
        if !characters.contains(&c.id) {
            continue;
        }
        motion.replace(
            c.id,
            game.snapshot.tick,
            if c.last_tick_motion_trace.is_empty() {
                vec![c.position]
            } else {
                c.last_tick_motion_trace.clone()
            },
        );
        cache.pawns.entry(c.id).or_insert_with(|| {
            commands
                .spawn((
                    Pawn(c.id),
                    MotionTarget(c.id, Vec3::ZERO),
                    Mesh3d(palette.get(ModelKind::Character, c.id.value() as u8 % 4, &mut meshes)),
                    MeshMaterial3d(palette.material.clone()),
                    Transform::from_translation(space::local(c.position, view.origin)),
                ))
                .id()
        });
    }
    game.dirty = false;
    view.rebased = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::camera::{CameraProjection, ComputedCameraValues, RenderTargetInfo};
    fn app() -> App {
        let mut app = App::new();
        let application = Application::new_game(NewGameOptions {
            seed: WorldSeed::new(0),
        })
        .unwrap();
        let snapshot = application.snapshot(SnapshotQuery::default()).unwrap();
        let mut projection = OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: 18.,
            },
            ..OrthographicProjection::default_3d()
        };
        projection.update(1440., 900.);
        let camera = Camera {
            computed: ComputedCameraValues {
                clip_from_view: projection.get_clip_from_view(),
                target_info: Some(RenderTargetInfo {
                    physical_size: UVec2::new(1440, 900),
                    scale_factor: 1.,
                }),
                ..default()
            },
            ..default()
        };
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<VisualMotion>()
            .init_resource::<SceneCache>()
            .insert_resource(Palette {
                models: BTreeMap::new(),
                material: Handle::default(),
            })
            .insert_resource(Game {
                app: application,
                snapshot,
                selected: None,
                dirty: true,
                message: String::new(),
            })
            .insert_resource(View {
                origin: WorldCell::new(0, 0),
                focus: Vec3::ZERO,
                height: 18.,
                yaw: std::f32::consts::FRAC_PI_4,
                rebased: true,
            })
            .add_systems(Update, sync);
        app.world_mut()
            .spawn((Camera3d::default(), camera, GlobalTransform::default()));
        app.world_mut().spawn(Window {
            resolution: (1440, 900).into(),
            ..default()
        });
        app
    }
    #[test]
    fn idle_updates_and_item_revisions_preserve_terrain_entities_and_meshes() {
        let mut app = app();
        app.update();
        let before: Vec<_> = app
            .world()
            .resource::<SceneCache>()
            .terrain
            .values()
            .map(|e| (e.entity, e.mesh.id()))
            .collect();
        assert!(!before.is_empty());
        let count = app.world().resource::<Assets<Mesh>>().len();
        for _ in 0..10 {
            app.update();
        }
        {
            let mut game = app.world_mut().resource_mut::<Game>();
            game.snapshot.item_revision += 1;
            game.dirty = true;
        }
        app.update();
        let after: Vec<_> = app
            .world()
            .resource::<SceneCache>()
            .terrain
            .values()
            .map(|e| (e.entity, e.mesh.id()))
            .collect();
        assert_eq!(before, after);
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), count);
    }
    #[test]
    fn panning_to_unknown_space_evicts_meshes_without_changing_save_or_discovery() {
        let mut app = app();
        app.update();
        let before = app.world().resource::<Game>().app.save_json().unwrap();
        app.world_mut().resource_mut::<View>().focus = Vec3::new(10000., 0., 10000.);
        app.update();
        assert!(app.world().resource::<SceneCache>().terrain.is_empty());
        assert!(app.world().resource::<SceneCache>().pawns.is_empty());
        assert_eq!(
            app.world().resource::<Game>().app.save_json().unwrap(),
            before
        );
        let cache = app.world().resource::<Palette>();
        assert_eq!(
            app.world().resource::<Assets<Mesh>>().len(),
            cache.models.len()
        );
    }
    #[test]
    fn move_commands_use_real_authority_and_traces_without_camera_dependency() {
        let mut app = app();
        app.update();
        let id = app.world().resource::<Game>().snapshot.characters[0].id;
        let destination = WorldPosition::from_cell_center(WorldCell::new(2, 0)).unwrap();
        {
            let mut game = app.world_mut().resource_mut::<Game>();
            game.selected = Some(id);
            game.execute(Command::MoveTo {
                character_id: id,
                destination,
            });
            assert_eq!(
                game.snapshot.navigation.as_ref().unwrap().destination,
                Some(destination)
            );
            game.execute(Command::AdvanceTicks { count: 1 });
        }
        app.update();
        let motion = app.world().resource::<VisualMotion>();
        let trace = &motion.characters[&id].trace;
        assert!(!trace.is_empty());
        assert_eq!(
            interpolate_trace(trace, 1.),
            app.world()
                .resource::<Game>()
                .snapshot
                .characters
                .iter()
                .find(|c| c.id == id)
                .unwrap()
                .position
        );
    }
}
