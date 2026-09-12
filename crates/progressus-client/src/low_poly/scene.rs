use super::*;
use progressus_app::{
    ChunkCoord, ChunkSnapshot, DoorState, ItemId, LocalCell, SnapshotQuery, WorldPosition,
    structure,
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
    /// Deterministic per-cell value that turns and resizes this instance.
    /// `None` for built things, whose orientation carries meaning.
    variety: Option<u32>,
}
pub(crate) struct TerrainEntry {
    pub(crate) source: ChunkSnapshot,
    pub(crate) entity: Entity,
    pub(crate) mesh: Handle<Mesh>,
    neighborhood: Vec<ChunkSnapshot>,
}
#[derive(Resource, Default)]
pub(crate) struct SceneCache {
    chunks: Vec<ChunkCoord>,
    revisions: Option<[u64; 3]>,
    pub(crate) terrain: BTreeMap<ChunkCoord, TerrainEntry>,
    invalidated: bool,
    objects: BTreeMap<ObjectKey, (Object, Entity)>,
    pub(crate) pawns: BTreeMap<EntityId, Entity>,
    pub(crate) resources: Vec<progressus_app::NaturalResourceSnapshot>,
    pub(crate) items: Vec<progressus_app::GroundItemSnapshot>,
    pub(crate) inventory_items: Vec<progressus_app::InventoryItemSnapshot>,
}

impl SceneCache {
    pub(crate) fn invalidate_loaded_world(&mut self) {
        self.invalidated = true;
        self.revisions = None;
    }
}

/// A deterministic, presentation-only value for one cell. The previous scheme
/// multiplied x by 37 and took the result modulo four, but 37 is congruent to
/// one modulo four, so it collapsed to `(x + y) % 4` and laid the world out in
/// diagonal stripes of identical models. Mixing the coordinates removes the
/// lattice without changing anything authoritative.
fn cell_variety(cell: WorldCell) -> u32 {
    let mixed = crate::procedural_assets::mix64(
        (cell.x() as u64)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            .wrapping_add((cell.y() as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f)),
    );
    (mixed >> 24) as u32
}

/// The same deterministic variety, keyed by a stable entity id rather than a
/// cell, for things that do not belong to one.
fn id_variety(id: EntityId) -> u32 {
    (crate::procedural_assets::mix64(id.value()) >> 24) as u32
}

fn item_model(kind: ItemId) -> ModelKind {
    match kind.name() {
        "wood" => ModelKind::Wood,
        "stone" => ModelKind::Stone,
        "primitive_tool" => ModelKind::PrimitiveTool,
        "berries" => ModelKind::Berries,
        "copper_ore" => ModelKind::CopperOre,
        "cart" => ModelKind::Cart,
        _ => ModelKind::Placeholder,
    }
}
/// How far a placed instance may be turned and resized. Bounded so a tree
/// stays a tree and never leaves its own cell.
const MAX_LEAN_RADIANS: f32 = 0.14;
const SCALE_SPREAD: f32 = 0.18;

fn object_transform(object: &Object, origin: WorldCell) -> Transform {
    let translation = space::local(object.position, origin);
    let Some(variety) = object.variety else {
        return Transform::from_translation(translation);
    };
    // One shared mesh at many poses: variety costs no extra mesh and no extra
    // draw batch, which is why it comes before adding more shapes.
    let unit = |shift: u32| ((variety >> shift) & 0xff) as f32 / 255.;
    let yaw = unit(0) * std::f32::consts::TAU;
    let lean_axis = unit(8) * std::f32::consts::TAU;
    let lean = unit(16) * MAX_LEAN_RADIANS;
    let scale = 1. - SCALE_SPREAD * 0.5 + unit(4) * SCALE_SPREAD;
    let stretch = 1. - SCALE_SPREAD * 0.35 + unit(12) * SCALE_SPREAD * 0.7;
    Transform {
        translation,
        rotation: Quat::from_rotation_y(yaw)
            * Quat::from_axis_angle(Vec3::new(lean_axis.cos(), 0., lean_axis.sin()), lean),
        scale: Vec3::new(scale, scale * stretch, scale),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sync(
    mut commands: Commands,
    mut game: ResMut<crate::runtime::AuthoritativeClient>,
    mut view: ResMut<View>,
    mut cache: ResMut<SceneCache>,
    mut palette: ResMut<Palette>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut motion: ResMut<VisualMotion>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    windows: Query<&Window>,
) {
    if cache.invalidated {
        for (_, entry) in std::mem::take(&mut cache.terrain) {
            commands.entity(entry.entity).try_despawn();
            meshes.remove(entry.mesh.id());
        }
        for (_, (_, entity)) in std::mem::take(&mut cache.objects) {
            commands.entity(entity).try_despawn();
        }
        for (_, entity) in std::mem::take(&mut cache.pawns) {
            commands.entity(entity).try_despawn();
        }
        cache.chunks.clear();
        cache.invalidated = false;
        motion.clear();
    }
    let dirty = game.take_snapshot_dirty();
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
        game.snapshot().exploration_revision,
        game.snapshot().item_revision,
        game.snapshot().resource_revision,
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
        let spatial = match game.application_mut().snapshot(SnapshotQuery {
            chunks: chunks.clone(),
            include_terrain: terrain_changed,
            include_ground_items: items_changed,
            include_natural_resources: resources_changed,
            ..default()
        }) {
            Ok(s) => s,
            Err(e) => {
                warn!("spatial snapshot failed: {e}");
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
            let neighborhood = spatial.chunks.clone();
            let known = spatial
                .chunks
                .iter()
                .flat_map(|chunk| {
                    chunk.cells.iter().enumerate().filter_map(move |(i, cell)| {
                        let progressus_app::KnownTerrain::Known(kind) = cell else {
                            return None;
                        };
                        let local = LocalCell::new(
                            (i % usize::from(chunk.side)) as u16,
                            (i / usize::from(chunk.side)) as u16,
                        );
                        chunk.coordinate.world_cell(local).map(|cell| (cell, *kind))
                    })
                })
                .collect();
            for chunk in spatial.chunks {
                let neighborhood = neighborhood
                    .iter()
                    .filter(|other| {
                        (i128::from(other.coordinate.x()) - i128::from(chunk.coordinate.x())).abs()
                            <= 1
                            && (i128::from(other.coordinate.y()) - i128::from(chunk.coordinate.y()))
                                .abs()
                                <= 1
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                if cache
                    .terrain
                    .get(&chunk.coordinate)
                    .is_some_and(|e| e.source == chunk && e.neighborhood == neighborhood)
                {
                    continue;
                }
                let mesh = meshes.add(terrain::terrain_mesh(&chunk, &known));
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
                        neighborhood: neighborhood.clone(),
                        entity,
                        mesh,
                    },
                );
            }
        }
        if items_changed {
            cache.items = spatial.ground_items;
            cache.inventory_items = spatial.inventory_items;
        }
        if resources_changed {
            cache.resources = spatial.natural_resources;
        }
        cache.chunks = chunks;
        cache.revisions = Some(revisions);
    }
    if !dirty && !viewport_changed && !view.rebased && !items_changed && !resources_changed {
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
    let mut insert = |key, kind: ModelKind, cell: WorldCell, mask: u8| {
        if visible.contains(&cell.split().0) {
            // Natural things pick their own shape from the cell; built things
            // are told theirs, because it encodes wall connectivity.
            let variety = kind.accepts_pose_variety().then(|| cell_variety(cell));
            let variant = match variety {
                Some(value) => (value % u32::from(kind.variant_count())) as u8,
                None => mask,
            };
            objects.insert(
                key,
                Object {
                    kind,
                    position: WorldPosition::from_cell_center(cell).expect("valid object cell"),
                    variant,
                    variety,
                },
            );
        }
    };
    for r in &cache.resources {
        let kind = match r.kind.name() {
            "tree" => ModelKind::Tree,
            "stone_outcrop" => ModelKind::StoneOutcrop,
            "berry_bush" => ModelKind::BerryBush,
            "copper_vein" => ModelKind::CopperVein,
            _ => ModelKind::Placeholder,
        };
        insert(ObjectKey::Resource(r.cell), kind, r.cell, 0);
    }
    for w in &game.snapshot().workstations {
        insert(ObjectKey::Workbench(w.id), ModelKind::Workbench, w.cell, 0);
    }
    let building_cells = game
        .snapshot()
        .structures
        .iter()
        .map(|s| s.cell)
        .chain(game.snapshot().construction_sites.iter().map(|s| s.cell))
        .collect();
    for s in &game.snapshot().structures {
        let kind = match s.kind.name() {
            "stone_wall" => ModelKind::Wall,
            "door" => {
                if s.door_state == Some(DoorState::Open) {
                    ModelKind::OpenDoor
                } else {
                    ModelKind::Door
                }
            }
            _ => ModelKind::Placeholder,
        };
        insert(
            ObjectKey::Structure(s.id),
            kind,
            s.cell,
            crate::tile_connectivity::CardinalConnections::from_cells(s.cell, &building_cells)
                .bits(),
        );
    }
    for s in &game.snapshot().construction_sites {
        insert(
            ObjectKey::Site(s.id),
            if s.kind == structure::DOOR {
                ModelKind::ConstructionDoor
            } else {
                ModelKind::ConstructionWall
            },
            s.cell,
            crate::tile_connectivity::CardinalConnections::from_cells(s.cell, &building_cells)
                .bits(),
        );
    }
    for s in &game.snapshot().workstation_construction_sites {
        insert(ObjectKey::Site(s.id), ModelKind::Workbench, s.cell, 0);
    }
    for item in &cache.items {
        objects.insert(
            ObjectKey::Item(item.id),
            Object {
                kind: item_model(item.kind),
                position: item.position,
                // A ground stack varies by its own stable id, so two piles of
                // the same goods do not sit at the same angle.
                variant: (id_variety(item.id) % u32::from(item_model(item.kind).variant_count()))
                    as u8,
                variety: Some(id_variety(item.id)),
            },
        );
    }
    for item in &game.snapshot().carried_items {
        if let Some(c) =
            game.snapshot().characters.iter().find(|c| {
                c.id == item.character_id && visible.contains(&c.containing_cell.split().0)
            })
        {
            objects.insert(
                ObjectKey::Carried(item.id),
                Object {
                    kind: item_model(item.kind),
                    position: c.position,
                    variant: (id_variety(item.id)
                        % u32::from(item_model(item.kind).variant_count()))
                        as u8,
                    // A carried stack follows its bearer, whose transform the
                    // animation owns, so it takes no pose of its own.
                    variety: None,
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
                game.snapshot().carried_items.iter().find(|i| i.id == id),
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
                && let Some(item) = game.snapshot().carried_items.iter().find(|i| i.id == id)
            {
                commands
                    .entity(entity)
                    .insert(MotionTarget(item.character_id, Vec3::new(0.25, 0.45, 0.)));
            }
            cache.objects.insert(key, (object, entity));
        }
    }
    let characters: BTreeSet<_> = game
        .snapshot()
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
    for c in &game.snapshot().characters {
        if !characters.contains(&c.id) {
            continue;
        }
        motion.replace(
            c.id,
            game.snapshot().tick,
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
    view.rebased = false;
}

#[cfg(test)]
mod tests {

    /// The defect this pins: the variant used to be `(37x + y) % 4`, and since
    /// 37 is congruent to 1 modulo 4 that collapsed to `(x + y) % 4`, tiling
    /// the world in diagonal stripes of identical models.
    #[test]
    fn cell_variety_has_no_diagonal_or_axis_pattern() {
        let variant_at = |x: i64, y: i64| cell_variety(WorldCell::new(x, y)) % 6;

        // A lattice would make every cell on a diagonal identical.
        let diagonal: Vec<_> = (0..24).map(|i| variant_at(i, -i)).collect();
        assert!(
            diagonal.iter().any(|v| *v != diagonal[0]),
            "cells along a diagonal all chose the same variant"
        );
        let anti: Vec<_> = (0..24).map(|i| variant_at(i, i)).collect();
        assert!(anti.iter().any(|v| *v != anti[0]));

        // No short period along either axis, which a weak mixer also produces.
        for period in 1..=8 {
            let repeats_x = (0..40).all(|i| variant_at(i, 3) == variant_at(i + period, 3));
            let repeats_y = (0..40).all(|i| variant_at(-5, i) == variant_at(-5, i + period));
            assert!(!repeats_x, "variants repeat every {period} cells along x");
            assert!(!repeats_y, "variants repeat every {period} cells along y");
        }

        // Every shape actually gets used across a modest patch of world.
        let mut seen = [0_usize; 6];
        for y in -30..30 {
            for x in -30..30 {
                seen[variant_at(x, y) as usize] += 1;
            }
        }
        assert!(
            seen.iter().all(|count| *count > 300),
            "variants are unevenly distributed: {seen:?}"
        );
    }

    #[test]
    fn a_cell_always_gets_the_same_shape_and_pose() {
        for cell in [
            WorldCell::new(0, 0),
            WorldCell::new(-4212, 917),
            WorldCell::new(i64::MAX / 2, i64::MIN / 2),
        ] {
            assert_eq!(cell_variety(cell), cell_variety(cell));
            let object = Object {
                kind: ModelKind::Tree,
                position: WorldPosition::from_cell_center(cell).unwrap(),
                variant: 0,
                variety: Some(cell_variety(cell)),
            };
            let origin = WorldCell::new(0, 0);
            assert_eq!(
                object_transform(&object, origin).rotation,
                object_transform(&object, origin).rotation
            );
        }
    }

    /// A pose may turn and resize, but never mirror, invert or shrink an object
    /// out of recognition.
    #[test]
    fn poses_stay_within_their_declared_bounds() {
        for i in 0..4096_u32 {
            let cell = WorldCell::new(i64::from(i) - 2048, i64::from(i % 71) - 35);
            let object = Object {
                kind: ModelKind::Tree,
                position: WorldPosition::from_cell_center(cell).unwrap(),
                variant: 0,
                variety: Some(cell_variety(cell)),
            };
            let transform = object_transform(&object, WorldCell::new(0, 0));
            assert!(transform.scale.x > 0.8 && transform.scale.x < 1.2);
            assert!(transform.scale.y > 0.7 && transform.scale.y < 1.3);
            assert_eq!(
                transform.scale.x, transform.scale.z,
                "footprint is not square"
            );
            // Upright to within the declared lean.
            let up = transform.rotation * Vec3::Y;
            assert!(
                up.y >= MAX_LEAN_RADIANS.cos() - 0.001,
                "leans too far: {up:?}"
            );
        }
    }

    /// Built things must not be turned: wall meshes carry a connectivity mask
    /// and a door's axis follows its neighbours.
    #[test]
    fn built_structures_never_receive_a_pose() {
        for kind in ModelKind::ALL {
            let object = Object {
                kind,
                position: WorldPosition::from_cell_center(WorldCell::new(3, 4)).unwrap(),
                variant: 0,
                variety: kind
                    .accepts_pose_variety()
                    .then(|| cell_variety(WorldCell::new(3, 4))),
            };
            let transform = object_transform(&object, WorldCell::new(0, 0));
            if kind.accepts_pose_variety() {
                continue;
            }
            assert_eq!(transform.rotation, Quat::IDENTITY, "{kind:?} was turned");
            assert_eq!(transform.scale, Vec3::ONE, "{kind:?} was resized");
        }
        for kind in [
            ModelKind::Wall,
            ModelKind::Door,
            ModelKind::OpenDoor,
            ModelKind::ConstructionWall,
            ModelKind::ConstructionDoor,
        ] {
            assert!(!kind.accepts_pose_variety(), "{kind:?} would lose its axis");
        }
    }
    use super::*;
    use crate::runtime::AuthoritativeClient;
    use bevy::camera::{CameraProjection, ComputedCameraValues, RenderTargetInfo};
    use progressus_app::{Command, WorldSeed};
    fn app() -> App {
        let mut app = App::new();
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
            .init_resource::<View>()
            .insert_resource(AuthoritativeClient::new().unwrap())
            .insert_resource(Palette {
                models: BTreeMap::new(),
                material: Handle::default(),
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
    fn terrain_handles(app: &App) -> Vec<(Entity, bevy::asset::AssetId<Mesh>)> {
        app.world()
            .resource::<SceneCache>()
            .terrain
            .values()
            .map(|e| (e.entity, e.mesh.id()))
            .collect()
    }
    #[test]
    fn idle_and_item_updates_retain_terrain_and_pawns() {
        let mut app = app();
        app.update();
        let before = terrain_handles(&app);
        let pawns = app.world().resource::<SceneCache>().pawns.clone();
        assert!(!before.is_empty());
        assert_eq!(pawns.len(), 5);
        for _ in 0..10 {
            app.update();
        }
        {
            let mut game = app.world_mut().resource_mut::<AuthoritativeClient>();
            game.application_mut()
                .execute(Command::AdvanceTicks { count: 1 })
                .unwrap();
            game.refresh_lightweight_snapshot(None).unwrap();
        }
        app.update();
        assert_eq!(before, terrain_handles(&app));
        assert_eq!(pawns, app.world().resource::<SceneCache>().pawns);
    }
    #[test]
    fn distant_view_evicts_meshes_and_return_recreates_pawns_without_discovery() {
        let mut app = app();
        app.update();
        let before = app
            .world()
            .resource::<AuthoritativeClient>()
            .save_json()
            .unwrap();
        app.world_mut().resource_mut::<View>().focus = Vec3::new(10000., 0., 10000.);
        app.update();
        let cache = app.world().resource::<SceneCache>();
        assert!(cache.terrain.is_empty());
        assert!(cache.pawns.is_empty());
        assert_eq!(
            app.world().resource::<Assets<Mesh>>().len(),
            app.world().resource::<Palette>().models.len()
        );
        app.world_mut().resource_mut::<View>().focus = Vec3::ZERO;
        app.update();
        assert_eq!(app.world().resource::<SceneCache>().pawns.len(), 5);
        assert_eq!(
            before,
            app.world()
                .resource::<AuthoritativeClient>()
                .save_json()
                .unwrap()
        );
    }
    #[test]
    fn load_at_same_tick_rebuilds_scene_without_stale_meshes() {
        let mut app = app();
        app.update();
        let old = terrain_handles(&app);
        let old_pawns = app.world().resource::<SceneCache>().pawns.clone();
        let replacement = AuthoritativeClient::new_with_seed(WorldSeed::new(73))
            .unwrap()
            .save_json()
            .unwrap();
        app.world_mut()
            .resource_mut::<AuthoritativeClient>()
            .load_json(&replacement)
            .unwrap();
        app.world_mut()
            .resource_mut::<SceneCache>()
            .invalidate_loaded_world();
        app.update();
        assert_ne!(old, terrain_handles(&app));
        for (entity, mesh) in old {
            assert!(app.world().get_entity(entity).is_err());
            assert!(app.world().resource::<Assets<Mesh>>().get(mesh).is_none());
        }
        for (_, entity) in old_pawns {
            assert!(app.world().get_entity(entity).is_err());
        }
        assert_eq!(app.world().resource::<SceneCache>().pawns.len(), 5);
    }
    #[test]
    fn move_trace_comes_from_authority_and_survives_origin_rebase() {
        let mut app = app();
        app.update();
        let id = app
            .world()
            .resource::<AuthoritativeClient>()
            .snapshot()
            .characters[0]
            .id;
        {
            let mut game = app.world_mut().resource_mut::<AuthoritativeClient>();
            game.application_mut()
                .execute(Command::MoveTo {
                    character_id: id,
                    destination: WorldPosition::from_cell_center(WorldCell::new(2, 0)).unwrap(),
                })
                .unwrap();
            game.application_mut()
                .execute(Command::AdvanceTicks { count: 1 })
                .unwrap();
            game.refresh_lightweight_snapshot(Some(id)).unwrap();
        }
        app.update();
        let endpoint = app
            .world()
            .resource::<AuthoritativeClient>()
            .snapshot()
            .characters
            .iter()
            .find(|c| c.id == id)
            .unwrap()
            .position;
        let trace = &app.world().resource::<VisualMotion>().characters[&id].trace;
        assert_eq!(interpolate_trace(trace, 1.), endpoint);
        let save = app
            .world()
            .resource::<AuthoritativeClient>()
            .save_json()
            .unwrap();
        app.world_mut()
            .resource_mut::<View>()
            .rebase(WorldCell::new(32, 0));
        app.update();
        assert_eq!(
            save,
            app.world()
                .resource::<AuthoritativeClient>()
                .save_json()
                .unwrap()
        );
    }
}
