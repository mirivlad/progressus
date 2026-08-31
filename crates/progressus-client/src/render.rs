use std::collections::{BTreeMap, BTreeSet};

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use progressus_app::{
    CHUNK_SIDE, CharacterSnapshot, ChunkCoord, EntityId, LocalCell, Terrain, WorldCell,
};

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

pub(crate) fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

pub(crate) fn sync_presentation(
    mut commands: Commands,
    mut authoritative: ResMut<AuthoritativeClient>,
    mut cache: ResMut<PresentationCache>,
) {
    if !authoritative.take_snapshot_dirty() {
        return;
    }

    let Some(controlled) = controlled_character(&authoritative.snapshot().characters) else {
        warn!("controlled character is missing from authoritative snapshot");
        return;
    };
    let current_center = controlled.position.split().0;

    if terrain_refresh_needed(cache.central_chunk, current_center) {
        let window = match VisibleChunkWindow::around(current_center) {
            Ok(window) => window,
            Err(error) => {
                error!("presentation sync failed: {error}");
                let rendered_center = cache.central_chunk;
                sync_characters(&mut commands, &authoritative, &mut cache, rendered_center);
                return;
            }
        };
        let terrain = match authoritative.terrain_snapshot(window.coordinates().to_vec()) {
            Ok(terrain) => terrain,
            Err(error) => {
                error!("presentation sync failed: {error}");
                let rendered_center = cache.central_chunk;
                sync_characters(&mut commands, &authoritative, &mut cache, rendered_center);
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
    }

    let rendered_center = cache.central_chunk;
    sync_characters(&mut commands, &authoritative, &mut cache, rendered_center);
}

fn spawn_terrain(
    commands: &mut Commands,
    chunks: &[progressus_app::ChunkSnapshot],
    origin: WorldCell,
) -> Entity {
    let mut root = commands.spawn((TerrainRoot, Transform::default()));
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
    let Some(center) = rendered_center else {
        return;
    };
    let origin = center
        .world_cell(LocalCell::new(0, 0))
        .expect("a chunk derived from a world cell has a valid lower-left cell");
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
                        Transform::from_translation(character_translation(&character, origin)),
                        visual,
                    ))
                    .id();
                cache.characters.insert(character_id, entity);
            }
            CharacterSyncAction::Update(character) => {
                let entity = cache.characters[&character.id];
                commands
                    .entity(entity)
                    .insert(Transform::from_translation(character_translation(
                        &character, origin,
                    )));
            }
            CharacterSyncAction::Despawn(id) => {
                if let Some(entity) = cache.characters.remove(&id) {
                    commands.entity(entity).despawn();
                }
            }
        }
    }
}

fn character_translation(character: &CharacterSnapshot, origin: WorldCell) -> Vec3 {
    world_translation(character.position, origin, CHARACTER_Z)
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
