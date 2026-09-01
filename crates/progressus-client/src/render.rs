use std::collections::{BTreeMap, BTreeSet};

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use progressus_app::{
    CHUNK_SIDE, CharacterSnapshot, ChunkCoord, EntityId, LocalCell, SUBUNITS_PER_CELL, Terrain,
    WorldCell, WorldPosition,
};

use crate::navigation::{SelectedCharacter, VisualMotion, interpolate_trace};
use crate::presentation::{
    CharacterSyncAction, VisibleChunkWindow, character_sync_actions, controlled_character,
    terrain_refresh_needed,
};
use crate::runtime::AuthoritativeClient;

const CELL_SIZE: f32 = 12.0;
const TERRAIN_Z: f32 = 0.0;
const CHARACTER_Z: f32 = 10.0;
const CAMERA_PAN_SPEED: f32 = 500.0;
const MIN_CAMERA_SCALE: f32 = 0.25;
const MAX_CAMERA_SCALE: f32 = 8.0;

#[derive(Component)]
pub(crate) struct TerrainRoot;

#[derive(Component)]
pub(crate) struct CharacterVisual {
    pub(crate) id: EntityId,
}

#[derive(Resource, Default)]
pub(crate) struct PresentationCache {
    pub(crate) central_chunk: Option<ChunkCoord>,
    pub(crate) terrain_root: Option<Entity>,
    pub(crate) characters: BTreeMap<EntityId, Entity>,
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
) {
    if !authoritative.take_snapshot_dirty() {
        return;
    }

    let rendered_center = cache.central_chunk;
    sync_characters(&mut commands, &authoritative, &mut cache, rendered_center);

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
            motion.replace(id, navigation.last_tick_motion_trace.clone());
        }
    }

    let Some(controlled) = controlled_character(&authoritative.snapshot().characters) else {
        warn!("controlled character is missing from authoritative snapshot");
        return;
    };
    let current_center = controlled.containing_cell.split().0;

    if terrain_refresh_needed(cache.central_chunk, current_center) {
        let window = match VisibleChunkWindow::around(current_center) {
            Ok(window) => window,
            Err(error) => {
                error!("presentation sync failed: {error}");
                return;
            }
        };
        let terrain = match authoritative.terrain_snapshot(window.coordinates().to_vec()) {
            Ok(terrain) => terrain,
            Err(error) => {
                error!("presentation sync failed: {error}");
                return;
            }
        };

        let origin = current_center
            .world_cell(LocalCell::new(0, 0))
            .expect("a chunk derived from a world cell has a valid lower-left cell");
        if let Some(previous_root) = cache.terrain_root {
            commands.entity(previous_root).despawn();
        }
        let new_root = spawn_terrain(&mut commands, &terrain.chunks, origin);
        cache.terrain_root = Some(new_root);
        cache.central_chunk = Some(current_center);
        position_characters(&mut commands, &authoritative, &cache, origin);
    }
}

fn spawn_terrain(
    commands: &mut Commands,
    chunks: &[progressus_app::ChunkSnapshot],
    origin: WorldCell,
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
                    let terrain = chunk
                        .terrain_at(local)
                        .expect("a chunk snapshot contains every advertised cell");
                    parent.spawn((
                        Sprite::from_color(terrain_color(terrain), Vec2::splat(CELL_SIZE)),
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

fn sync_characters(
    commands: &mut Commands,
    authoritative: &AuthoritativeClient,
    cache: &mut PresentationCache,
    rendered_center: Option<ChunkCoord>,
) {
    let origin = rendered_center.map(|center| {
        center
            .world_cell(LocalCell::new(0, 0))
            .expect("a chunk derived from a world cell has a valid lower-left cell")
    });
    let rendered = cache.characters.keys().copied().collect::<BTreeSet<_>>();
    for action in character_sync_actions(&rendered, &authoritative.snapshot().characters) {
        match action {
            CharacterSyncAction::Spawn(character) => {
                let visual = CharacterVisual { id: character.id };
                let character_id = visual.id;
                let entity = commands
                    .spawn((
                        Sprite::from_color(
                            Color::srgb(1.0, 0.85, 0.2),
                            Vec2::splat(CELL_SIZE * 0.75),
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
    let Some(center) = cache.central_chunk else {
        return;
    };
    let Some(origin_cell) = center.world_cell(LocalCell::new(0, 0)) else {
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
    let (Some(id), Some(center)) = (selected.0, cache.central_chunk) else {
        return;
    };
    let Some(navigation) = authoritative.snapshot().navigation.as_ref() else {
        return;
    };
    if navigation.character_id != id {
        return;
    }
    let Some(origin_cell) = center.world_cell(LocalCell::new(0, 0)) else {
        return;
    };
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

fn terrain_color(terrain: Terrain) -> Color {
    match terrain {
        Terrain::Grass => Color::srgb(0.25, 0.55, 0.22),
        Terrain::Water => Color::srgb(0.12, 0.35, 0.72),
        Terrain::Rock => Color::srgb(0.42, 0.42, 0.44),
    }
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
    use bevy::prelude::{Vec3, Visibility};
    use progressus_app::{ChunkSnapshot, Terrain, WorldCell, WorldPosition};

    use super::{CHARACTER_Z, CHUNK_SIDE, LocalCell, spawn_terrain, world_position_translation};

    #[test]
    fn terrain_root_has_visibility_for_sprite_children() {
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let chunks = [ChunkSnapshot {
            coordinate: progressus_app::ChunkCoord::new(0, 0),
            side: CHUNK_SIDE,
            cells: vec![Terrain::Grass; usize::from(CHUNK_SIDE) * usize::from(CHUNK_SIDE)],
        }];
        let root = {
            let mut commands = bevy::prelude::Commands::new(&mut queue, &world);
            spawn_terrain(
                &mut commands,
                &chunks,
                progressus_app::ChunkCoord::new(0, 0)
                    .world_cell(LocalCell::new(0, 0))
                    .unwrap(),
            )
        };
        queue.apply(&mut world);

        assert!(world.entity(root).contains::<Visibility>());
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
