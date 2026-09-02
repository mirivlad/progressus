use std::collections::{BTreeMap, BTreeSet};

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use progressus_app::{
    CHUNK_SIDE, CharacterSnapshot, ChunkCoord, EntityId, GroundItemSnapshot, LocalCell,
    NaturalResourceKind, NaturalResourceSnapshot, SUBUNITS_PER_CELL, WorldCell, WorldPosition,
};

use crate::navigation::{SelectedCharacter, VisualMotion, interpolate_trace};
use crate::presentation::{
    CharacterSyncAction, GroundItemSyncAction, NaturalResourceSyncAction, VisibleChunkWindow,
    character_sync_actions, ground_item_sync_actions, natural_resource_sync_actions,
};
use crate::procedural_assets::{
    ProceduralAssetParams, ProceduralAssetRegistry, character_asset, item_asset, resource_asset,
    terrain_asset,
};
use crate::runtime::AuthoritativeClient;

const CELL_SIZE: f32 = 12.0;
const TERRAIN_Z: f32 = 0.0;
const NATURAL_RESOURCE_Z: f32 = 3.0;
const GROUND_ITEM_Z: f32 = 5.0;
const CHARACTER_Z: f32 = 10.0;
const CAMERA_PAN_SPEED: f32 = 500.0;
const MIN_CAMERA_SCALE: f32 = 0.25;
const MAX_CAMERA_SCALE: f32 = 8.0;
const PRESENTATION_CHUNK_MARGIN: i64 = 1;

#[derive(Component)]
pub(crate) struct TerrainRoot;

#[derive(Component)]
pub(crate) struct CharacterVisual {
    pub(crate) id: EntityId,
}

#[derive(Component)]
pub(crate) struct NaturalResourceVisual;

#[derive(Component)]
pub(crate) struct GroundItemVisual;

#[derive(Resource, Default)]
pub(crate) struct PresentationCache {
    pub(crate) render_origin: Option<WorldCell>,
    pub(crate) central_chunk: Option<ChunkCoord>,
    pub(crate) visible_window: Option<VisibleChunkWindow>,
    pub(crate) terrain_root: Option<Entity>,
    pub(crate) exploration_revision: Option<u64>,
    pub(crate) item_revision: Option<u64>,
    pub(crate) resource_revision: Option<u64>,
    pub(crate) characters: BTreeMap<EntityId, Entity>,
    pub(crate) ground_items: BTreeMap<EntityId, Entity>,
    pub(crate) natural_resources: BTreeMap<WorldCell, Entity>,
}

#[derive(Resource, Default)]
pub(crate) struct NavigationDebug(pub(crate) bool);

pub(crate) fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

pub(crate) fn sync_presentation(
    mut commands: Commands,
    mut authoritative: ResMut<AuthoritativeClient>,
    mut cache: ResMut<PresentationCache>,
    mut selected: ResMut<SelectedCharacter>,
    mut motion: ResMut<VisualMotion>,
    mut procedural_assets: ProceduralAssetParams,
    cameras: Query<(&Transform, &Projection), With<Camera2d>>,
) {
    let snapshot_dirty = authoritative.take_snapshot_dirty();
    if cache.render_origin.is_none() {
        cache.render_origin = authoritative
            .snapshot()
            .characters
            .iter()
            .find(|character| character.id == crate::runtime::cora_id())
            .map(|character| character.containing_cell)
            .or_else(|| {
                cache
                    .central_chunk
                    .and_then(|center| center.world_cell(LocalCell::new(0, 0)))
            });
    }
    let Some(origin) = cache.render_origin else {
        warn!("cannot render terrain without an authoritative character origin");
        return;
    };

    if snapshot_dirty {
        {
            let (images, registry) = procedural_assets.parts();
            sync_characters(
                &mut commands,
                &authoritative,
                &mut cache,
                Some(origin),
                images,
                registry,
            );
        }

        if let Some(id) = selected.0 {
            if !authoritative
                .snapshot()
                .characters
                .iter()
                .any(|character| character.id == id)
            {
                selected.0 = None;
                motion.clear();
            } else if let Some(navigation) = authoritative.snapshot().navigation.as_ref()
                && navigation.character_id == id
            {
                motion.replace(
                    id,
                    authoritative.snapshot().tick,
                    navigation.last_tick_motion_trace.clone(),
                );
            }
        }
    }

    let (current_center, current_window) = camera_window(cameras.iter().next(), origin)
        .unwrap_or_else(|| {
            let center = origin.split().0;
            (
                center,
                VisibleChunkWindow::around(center)
                    .expect("a render origin has a representable radius-one window"),
            )
        });

    if cache.visible_window.as_ref() != Some(&current_window)
        || cache.exploration_revision != Some(authoritative.snapshot().exploration_revision)
        || cache.item_revision != Some(authoritative.snapshot().item_revision)
        || cache.resource_revision != Some(authoritative.snapshot().resource_revision)
    {
        let terrain = match authoritative.terrain_snapshot(current_window.coordinates().to_vec()) {
            Ok(terrain) => terrain,
            Err(error) => {
                error!("presentation sync failed: {error}");
                return;
            }
        };

        let render_origin = cache.render_origin.expect("origin was established above");
        if let Some(previous_root) = cache.terrain_root {
            commands.entity(previous_root).despawn();
        }
        let new_root = {
            let (images, registry) = procedural_assets.parts();
            spawn_terrain(
                &mut commands,
                &terrain.chunks,
                render_origin,
                images,
                registry,
            )
        };
        cache.terrain_root = Some(new_root);
        {
            let (images, registry) = procedural_assets.parts();
            sync_natural_resources(
                &mut commands,
                &mut cache,
                &terrain.natural_resources,
                render_origin,
                images,
                registry,
            );
        }
        {
            let (images, registry) = procedural_assets.parts();
            sync_ground_items(
                &mut commands,
                &mut cache,
                &terrain.ground_items,
                render_origin,
                images,
                registry,
            );
        }
        cache.central_chunk = Some(current_center);
        cache.visible_window = Some(current_window);
        cache.exploration_revision = Some(terrain.exploration_revision);
        cache.item_revision = Some(terrain.item_revision);
        cache.resource_revision = Some(terrain.resource_revision);
        position_characters(&mut commands, &authoritative, &cache, render_origin);
    }
}

fn camera_window(
    camera: Option<(&Transform, &Projection)>,
    origin: WorldCell,
) -> Option<(ChunkCoord, VisibleChunkWindow)> {
    let (transform, projection) = camera?;
    let center = camera_chunk(transform, origin)?;
    let (minimum, maximum) = match projection {
        Projection::Orthographic(projection) => (
            camera_world_cell(
                transform.translation.x + projection.area.min.x,
                transform.translation.y + projection.area.min.y,
                origin,
            )?
            .split()
            .0,
            camera_world_cell(
                transform.translation.x + projection.area.max.x,
                transform.translation.y + projection.area.max.y,
                origin,
            )?
            .split()
            .0,
        ),
        _ => (center, center),
    };
    VisibleChunkWindow::covering(center, minimum, maximum, PRESENTATION_CHUNK_MARGIN)
        .ok()
        .map(|window| (center, window))
}

fn camera_chunk(camera: &Transform, origin: WorldCell) -> Option<ChunkCoord> {
    camera_world_cell(camera.translation.x, camera.translation.y, origin).map(|cell| cell.split().0)
}

fn camera_world_cell(x: f32, y: f32, origin: WorldCell) -> Option<WorldCell> {
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    let x = i128::from(origin.x()).checked_add((x / CELL_SIZE).floor() as i128)?;
    let y = i128::from(origin.y()).checked_add((y / CELL_SIZE).floor() as i128)?;
    let x = i64::try_from(x).ok()?;
    let y = i64::try_from(y).ok()?;
    Some(WorldCell::new(x, y))
}

fn spawn_terrain(
    commands: &mut Commands,
    chunks: &[progressus_app::ChunkSnapshot],
    origin: WorldCell,
    images: &mut Assets<Image>,
    procedural_assets: &mut ProceduralAssetRegistry,
) -> Entity {
    let mut root = commands.spawn((TerrainRoot, Transform::default(), Visibility::default()));
    let root_id = root.id();
    root.with_children(|parent| {
        for chunk in chunks {
            for local_y in 0..CHUNK_SIDE {
                for local_x in 0..CHUNK_SIDE {
                    let local = LocalCell::new(local_x, local_y);
                    let world_cell = chunk
                        .coordinate
                        .world_cell(local)
                        .expect("a successfully generated chunk contains valid world cells");
                    let Some(terrain) = chunk.known_terrain_at(local) else {
                        continue;
                    };
                    parent.spawn((
                        procedural_assets.sprite(
                            images,
                            terrain_asset(terrain, world_cell),
                            Vec2::splat(CELL_SIZE),
                        ),
                        Transform::from_translation(world_translation(
                            world_cell, origin, TERRAIN_Z,
                        )),
                    ));
                }
            }
        }
    });
    root_id
}

fn sync_natural_resources(
    commands: &mut Commands,
    cache: &mut PresentationCache,
    resources: &[NaturalResourceSnapshot],
    origin: WorldCell,
    images: &mut Assets<Image>,
    procedural_assets: &mut ProceduralAssetRegistry,
) {
    let rendered = cache
        .natural_resources
        .keys()
        .copied()
        .collect::<BTreeSet<_>>();
    for action in natural_resource_sync_actions(&rendered, resources) {
        match action {
            NaturalResourceSyncAction::Spawn(resource) => {
                let cell = resource.cell;
                let entity = commands
                    .spawn((
                        procedural_assets.sprite(
                            images,
                            resource_asset(resource.kind, resource.cell),
                            natural_resource_size(resource.kind),
                        ),
                        Transform::from_translation(world_translation(
                            cell,
                            origin,
                            NATURAL_RESOURCE_Z,
                        )),
                        NaturalResourceVisual,
                    ))
                    .id();
                cache.natural_resources.insert(cell, entity);
            }
            NaturalResourceSyncAction::Update(resource) => {
                if let Some(entity) = cache.natural_resources.get(&resource.cell) {
                    commands.entity(*entity).insert((
                        procedural_assets.sprite(
                            images,
                            resource_asset(resource.kind, resource.cell),
                            natural_resource_size(resource.kind),
                        ),
                        Transform::from_translation(world_translation(
                            resource.cell,
                            origin,
                            NATURAL_RESOURCE_Z,
                        )),
                    ));
                }
            }
            NaturalResourceSyncAction::Despawn(cell) => {
                if let Some(entity) = cache.natural_resources.remove(&cell) {
                    commands.entity(entity).despawn();
                }
            }
        }
    }
}

fn natural_resource_size(kind: NaturalResourceKind) -> Vec2 {
    match kind {
        NaturalResourceKind::Tree => Vec2::splat(CELL_SIZE * 1.25),
        NaturalResourceKind::StoneOutcrop => Vec2::splat(CELL_SIZE * 0.95),
    }
}

fn sync_ground_items(
    commands: &mut Commands,
    cache: &mut PresentationCache,
    items: &[GroundItemSnapshot],
    origin: WorldCell,
    images: &mut Assets<Image>,
    procedural_assets: &mut ProceduralAssetRegistry,
) {
    let rendered = cache.ground_items.keys().copied().collect::<BTreeSet<_>>();
    for action in ground_item_sync_actions(&rendered, items) {
        match action {
            GroundItemSyncAction::Spawn(item) => {
                let id = item.id;
                let entity = commands
                    .spawn((
                        procedural_assets.sprite(
                            images,
                            item_asset(item.kind, item.id),
                            Vec2::splat(CELL_SIZE * 0.55),
                        ),
                        Transform::from_translation(item_translation(&item, origin)),
                        GroundItemVisual,
                    ))
                    .id();
                cache.ground_items.insert(id, entity);
            }
            GroundItemSyncAction::Update(item) => {
                if let Some(entity) = cache.ground_items.get(&item.id) {
                    commands.entity(*entity).insert((
                        procedural_assets.sprite(
                            images,
                            item_asset(item.kind, item.id),
                            Vec2::splat(CELL_SIZE * 0.55),
                        ),
                        Transform::from_translation(item_translation(&item, origin)),
                    ));
                }
            }
            GroundItemSyncAction::Despawn(id) => {
                if let Some(entity) = cache.ground_items.remove(&id) {
                    commands.entity(entity).despawn();
                }
            }
        }
    }
}

fn item_translation(item: &GroundItemSnapshot, origin: WorldCell) -> Vec3 {
    let origin = WorldPosition::from_cell_center(origin)
        .expect("a valid world cell has a representable fixed-point center");
    world_position_translation(item.position, origin, GROUND_ITEM_Z)
}

fn sync_characters(
    commands: &mut Commands,
    authoritative: &AuthoritativeClient,
    cache: &mut PresentationCache,
    rendered_origin: Option<WorldCell>,
    images: &mut Assets<Image>,
    procedural_assets: &mut ProceduralAssetRegistry,
) {
    let origin = rendered_origin;
    let rendered = cache.characters.keys().copied().collect::<BTreeSet<_>>();
    for action in character_sync_actions(&rendered, &authoritative.snapshot().characters) {
        match action {
            CharacterSyncAction::Spawn(character) => {
                let visual = CharacterVisual { id: character.id };
                let character_id = visual.id;
                let entity = commands
                    .spawn((
                        procedural_assets.sprite(
                            images,
                            character_asset(character.id),
                            Vec2::splat(CELL_SIZE * 0.9),
                        ),
                        origin.map_or_else(Transform::default, |origin| {
                            Transform::from_translation(character_translation(&character, origin))
                        }),
                        visual,
                    ))
                    .id();
                cache.characters.insert(character_id, entity);
            }
            CharacterSyncAction::Update(character) => {
                if let Some(origin) = origin {
                    let entity = cache.characters[&character.id];
                    commands.entity(entity).insert(Transform::from_translation(
                        character_translation(&character, origin),
                    ));
                }
            }
            CharacterSyncAction::Despawn(id) => {
                if let Some(entity) = cache.characters.remove(&id) {
                    commands.entity(entity).despawn();
                }
            }
        }
    }
}

fn position_characters(
    commands: &mut Commands,
    authoritative: &AuthoritativeClient,
    cache: &PresentationCache,
    origin: WorldCell,
) {
    for character in &authoritative.snapshot().characters {
        if let Some(entity) = cache.characters.get(&character.id) {
            commands
                .entity(*entity)
                .insert(Transform::from_translation(character_translation(
                    character, origin,
                )));
        }
    }
}

fn character_translation(character: &CharacterSnapshot, origin: WorldCell) -> Vec3 {
    let origin = WorldPosition::from_cell_center(origin)
        .expect("a valid world cell has a representable fixed-point center");
    world_position_translation(character.position, origin, CHARACTER_Z)
}

pub(crate) fn world_position_translation(
    position: WorldPosition,
    origin: WorldPosition,
    z: f32,
) -> Vec3 {
    let relative_x =
        (position.x_subunits() - origin.x_subunits()) as f32 / SUBUNITS_PER_CELL as f32;
    let relative_y =
        (position.y_subunits() - origin.y_subunits()) as f32 / SUBUNITS_PER_CELL as f32;
    Vec3::new(relative_x * CELL_SIZE, relative_y * CELL_SIZE, z)
}

pub(crate) fn interpolate_selected_visual(
    time: Res<Time>,
    mut motion: ResMut<VisualMotion>,
    selected: Res<SelectedCharacter>,
    authoritative: Res<AuthoritativeClient>,
    cache: Res<PresentationCache>,
    mut transforms: Query<&mut Transform, With<CharacterVisual>>,
) {
    let Some(id) = selected.0 else {
        return;
    };
    if motion.character_id != Some(id) || motion.trace.is_empty() {
        return;
    }
    let Some(origin_cell) = cache.render_origin else {
        return;
    };
    let Ok(origin) = WorldPosition::from_cell_center(origin_cell) else {
        return;
    };
    let Some(entity) = cache.characters.get(&id) else {
        return;
    };
    let Ok(mut transform) = transforms.get_mut(*entity) else {
        return;
    };
    motion.elapsed_seconds += time.delta_secs();
    let position = interpolate_trace(
        &motion.trace,
        motion.elapsed_seconds / crate::interaction::TICK_INTERVAL.as_secs_f32(),
    );
    transform.translation = world_position_translation(position, origin, CHARACTER_Z);

    if motion.elapsed_seconds >= crate::interaction::TICK_INTERVAL.as_secs_f32()
        && authoritative.snapshot().navigation.is_none()
    {
        motion.clear();
    }
}

pub(crate) fn draw_navigation_debug(
    keys: Res<ButtonInput<KeyCode>>,
    mut debug: ResMut<NavigationDebug>,
    selected: Res<SelectedCharacter>,
    authoritative: Res<AuthoritativeClient>,
    cache: Res<PresentationCache>,
    mut gizmos: Gizmos,
) {
    if keys.just_pressed(KeyCode::F3) {
        debug.0 = !debug.0;
    }
    if !debug.0 {
        return;
    }
    let (Some(id), Some(origin_cell)) = (selected.0, cache.render_origin) else {
        return;
    };
    let Some(navigation) = authoritative.snapshot().navigation.as_ref() else {
        return;
    };
    if navigation.character_id != id {
        return;
    }
    let Ok(origin) = WorldPosition::from_cell_center(origin_cell) else {
        return;
    };
    let to_visual = |position| world_position_translation(position, origin, CHARACTER_Z + 1.0);
    if let Some(character) = authoritative
        .snapshot()
        .characters
        .iter()
        .find(|character| character.id == id)
    {
        let authority = to_visual(character.position).truncate();
        gizmos.line_2d(
            authority + Vec2::new(-4.0, 0.0),
            authority + Vec2::new(4.0, 0.0),
            Color::WHITE,
        );
        gizmos.line_2d(
            authority + Vec2::new(0.0, -4.0),
            authority + Vec2::new(0.0, 4.0),
            Color::WHITE,
        );
        let mut previous = authority;
        for waypoint in &navigation.remaining_waypoints {
            let next = to_visual(*waypoint).truncate();
            gizmos.line_2d(previous, next, Color::srgb(0.95, 0.2, 0.8));
            previous = next;
        }
    }
    if let Some(destination) = navigation.destination {
        let destination = to_visual(destination).truncate();
        gizmos.line_2d(
            destination + Vec2::new(-3.0, -3.0),
            destination + Vec2::new(3.0, 3.0),
            Color::srgb(0.2, 1.0, 1.0),
        );
        gizmos.line_2d(
            destination + Vec2::new(-3.0, 3.0),
            destination + Vec2::new(3.0, -3.0),
            Color::srgb(0.2, 1.0, 1.0),
        );
    }
}

fn world_translation(world_cell: WorldCell, origin: WorldCell, z: f32) -> Vec3 {
    let relative_x = (i128::from(world_cell.x()) - i128::from(origin.x())) as f32;
    let relative_y = (i128::from(world_cell.y()) - i128::from(origin.y())) as f32;
    Vec3::new(relative_x * CELL_SIZE, relative_y * CELL_SIZE, z)
}

pub(crate) fn camera_controls(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut wheel_events: MessageReader<MouseWheel>,
    mut cameras: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let mut pan = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        pan.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        pan.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        pan.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        pan.x += 1.0;
    }
    let scroll = wheel_events
        .read()
        .map(|event| match event.unit {
            MouseScrollUnit::Line => event.y,
            MouseScrollUnit::Pixel => event.y / MouseScrollUnit::SCROLL_UNIT_CONVERSION_FACTOR,
        })
        .sum::<f32>();

    for (mut transform, mut projection) in &mut cameras {
        transform.translation +=
            pan.normalize_or_zero().extend(0.0) * CAMERA_PAN_SPEED * time.delta_secs();
        if let Projection::Orthographic(orthographic) = &mut *projection {
            orthographic.scale = (orthographic.scale * 0.9_f32.powf(scroll))
                .clamp(MIN_CAMERA_SCALE, MAX_CAMERA_SCALE);
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::world::{CommandQueue, World};
    use bevy::prelude::{Assets, Image, Vec3, Visibility};
    use progressus_app::{
        ChunkSnapshot, EntityId, GroundItemSnapshot, ItemKind, KnownTerrain, Terrain, WorldCell,
        WorldPosition,
    };

    use super::{
        CHARACTER_Z, CHUNK_SIDE, GROUND_ITEM_Z, LocalCell, item_translation, spawn_terrain,
        world_position_translation,
    };
    use crate::procedural_assets::ProceduralAssetRegistry;

    #[test]
    fn terrain_root_has_visibility_for_sprite_children() {
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let chunks = [ChunkSnapshot {
            coordinate: progressus_app::ChunkCoord::new(0, 0),
            side: CHUNK_SIDE,
            cells: vec![
                KnownTerrain::Known(Terrain::Grass);
                usize::from(CHUNK_SIDE) * usize::from(CHUNK_SIDE)
            ],
        }];
        let mut images = Assets::<Image>::default();
        let mut procedural_assets = ProceduralAssetRegistry::default();
        let root = {
            let mut commands = bevy::prelude::Commands::new(&mut queue, &world);
            spawn_terrain(
                &mut commands,
                &chunks,
                progressus_app::ChunkCoord::new(0, 0)
                    .world_cell(LocalCell::new(0, 0))
                    .unwrap(),
                &mut images,
                &mut procedural_assets,
            )
        };
        queue.apply(&mut world);

        assert!(world.entity(root).contains::<Visibility>());
    }

    #[test]
    fn ground_item_translation_preserves_exact_subcell_position() {
        let origin_cell = WorldCell::new(0, 0);
        let origin = WorldPosition::from_cell_center(origin_cell).unwrap();
        let item = GroundItemSnapshot {
            id: EntityId::new(6).unwrap(),
            kind: ItemKind::Wood,
            quantity: 8,
            position: origin.checked_translate(256, -128).unwrap(),
        };

        assert_eq!(
            item_translation(&item, origin_cell),
            Vec3::new(3.0, -1.5, GROUND_ITEM_Z)
        );
    }

    #[test]
    fn fixed_point_character_positions_are_aligned_to_terrain_cell_centers() {
        let origin = WorldPosition::from_cell_center(WorldCell::new(0, 0)).unwrap();
        assert_eq!(
            world_position_translation(origin, origin, CHARACTER_Z),
            Vec3::new(0.0, 0.0, CHARACTER_Z)
        );
        assert_eq!(
            world_position_translation(
                origin.checked_translate(256, 0).unwrap(),
                origin,
                CHARACTER_Z,
            ),
            Vec3::new(3.0, 0.0, CHARACTER_Z)
        );
    }
}
