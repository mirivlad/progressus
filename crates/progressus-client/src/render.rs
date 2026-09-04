use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use bevy::diagnostic::Diagnostics;

use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use progressus_app::{
    CHUNK_SIDE, CharacterSnapshot, ChunkCoord, ConstructionSiteSnapshot, EntityId,
    GroundItemSnapshot, JobKind, JobState, LocalCell, NaturalResourceKind, NaturalResourceSnapshot,
    SUBUNITS_PER_CELL, StockpileSnapshot, StructureSnapshot, WorkstationSnapshot, WorldCell,
    WorldPosition,
};

use crate::client_diagnostics::{
    ITEM_REFRESH_MS, PRESENTATION_MS, RESOURCE_REFRESH_MS, SPATIAL_SNAPSHOT_MS, TERRAIN_REFRESH_MS,
    record_elapsed,
};
use crate::navigation::{SelectedCharacter, VisualMotion, interpolate_trace};
use crate::presentation::{
    CharacterSyncAction, GroundItemSyncAction, NaturalResourceSyncAction, VisibleChunkWindow,
    character_sync_actions, ground_item_sync_actions, natural_resource_sync_actions,
};
use crate::procedural_assets::{
    ProceduralAssetParams, ProceduralAssetRegistry, character_asset, construction_site_asset,
    item_asset, render_terrain_chunk_image, resource_asset, structure_asset, workstation_asset,
};

use crate::runtime::AuthoritativeClient;
use crate::tile_connectivity::CardinalConnections;
use crate::ui::{SelectedStockpile, ToolMode, ToolState, ZoneVisibility};

const CELL_SIZE: f32 = 12.0;
const TERRAIN_Z: f32 = 0.0;
const NATURAL_RESOURCE_Z: f32 = 3.0;
const GROUND_ITEM_Z: f32 = 5.0;
const WORKSTATION_Z: f32 = 7.0;
const CONSTRUCTION_Z: f32 = 8.0;
const CHARACTER_Z: f32 = 10.0;
const CARRIED_ITEM_LOCAL_Z: f32 = 1.0;
const CAMERA_PAN_SPEED: f32 = 500.0;
const MIN_CAMERA_SCALE: f32 = 0.25;
const MAX_CAMERA_SCALE: f32 = 8.0;
const PRESENTATION_CHUNK_MARGIN: i64 = 1;

#[derive(Component)]
pub(crate) struct TerrainRoot;

#[derive(Component)]
pub(crate) struct TerrainChunkVisual;

#[derive(Clone)]
pub(crate) struct TerrainChunkPresentation {
    pub(crate) entity: Entity,
    pub(crate) image: Handle<Image>,
    pub(crate) fingerprint: u64,
}

#[derive(Component)]
pub(crate) struct CharacterVisual {
    pub(crate) id: EntityId,
}

#[derive(Component)]
pub(crate) struct NaturalResourceVisual;

#[derive(Component)]
pub(crate) struct GroundItemVisual;

#[derive(Component)]
pub(crate) struct CarriedItemVisual;

#[derive(Component)]
pub(crate) struct WorkstationVisual;

#[derive(Component)]
pub(crate) struct ConstructionSiteVisual;

#[derive(Component)]
pub(crate) struct StructureVisual;

#[derive(Component)]
pub(crate) struct StockpileZoneVisual;

#[derive(Resource, Default)]
pub(crate) struct PresentationCache {
    pub(crate) render_origin: Option<WorldCell>,
    pub(crate) central_chunk: Option<ChunkCoord>,
    pub(crate) visible_window: Option<VisibleChunkWindow>,
    pub(crate) terrain_root: Option<Entity>,
    pub(crate) terrain_chunks: BTreeMap<ChunkCoord, TerrainChunkPresentation>,
    pub(crate) exploration_revision: Option<u64>,
    pub(crate) item_revision: Option<u64>,
    pub(crate) resource_revision: Option<u64>,
    pub(crate) stockpile_revision: Option<u64>,
    pub(crate) zone_visible: Option<bool>,
    pub(crate) stockpile_zones: Vec<Entity>,
    pub(crate) characters: BTreeMap<EntityId, Entity>,
    pub(crate) ground_items: BTreeMap<EntityId, Entity>,
    pub(crate) ground_item_labels: BTreeMap<EntityId, Entity>,
    pub(crate) carried_items: BTreeMap<EntityId, Entity>,
    pub(crate) natural_resources: BTreeMap<WorldCell, Entity>,
    pub(crate) workstations: BTreeMap<EntityId, Entity>,
    pub(crate) construction_sites: BTreeMap<EntityId, Entity>,
    pub(crate) structures: BTreeMap<EntityId, Entity>,
}

impl PresentationCache {
    pub(crate) fn invalidate_loaded_world(&mut self) {
        self.render_origin = None;
        self.central_chunk = None;
        self.visible_window = None;
        self.exploration_revision = None;
        self.item_revision = None;
        self.resource_revision = None;
        self.stockpile_revision = None;
        self.zone_visible = None;
    }
}

#[derive(Resource, Default)]
pub(crate) struct NavigationDebug(pub(crate) bool);

pub(crate) fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_presentation(
    mut commands: Commands,
    mut authoritative: ResMut<AuthoritativeClient>,
    mut cache: ResMut<PresentationCache>,
    mut selected: ResMut<SelectedCharacter>,
    zones: Res<ZoneVisibility>,
    mut motion: ResMut<VisualMotion>,
    mut procedural_assets: ProceduralAssetParams,
    cameras: Query<(&Transform, &Projection), With<Camera2d>>,
    mut diagnostics: Diagnostics,
) {
    let started = Instant::now();
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
            sync_carried_items(&mut commands, &authoritative, &mut cache, images, registry);
            sync_workstations(
                &mut commands,
                &mut cache,
                &authoritative.snapshot().workstations,
                origin,
                images,
                registry,
            );
            sync_construction(
                &mut commands,
                &mut cache,
                &authoritative.snapshot().construction_sites,
                &authoritative.snapshot().structures,
                origin,
                images,
                registry,
            );
        }

        let tick = authoritative.snapshot().tick;
        motion.retain(
            authoritative
                .snapshot()
                .characters
                .iter()
                .map(|character| character.id),
        );
        for character in &authoritative.snapshot().characters {
            motion.replace(character.id, tick, character.last_tick_motion_trace.clone());
        }

        if let Some(id) = selected.0
            && !authoritative
                .snapshot()
                .characters
                .iter()
                .any(|character| character.id == id)
        {
            selected.0 = None;
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
        || cache.stockpile_revision != Some(authoritative.snapshot().stockpile_revision)
        || cache.zone_visible != Some(zones.visible)
    {
        sync_stockpile_zones(
            &mut commands,
            &mut cache,
            &authoritative.snapshot().stockpiles,
            origin,
            &current_window,
            zones.visible,
        );
        cache.stockpile_revision = Some(authoritative.snapshot().stockpile_revision);
        cache.zone_visible = Some(zones.visible);
    }

    let window_changed = cache.visible_window.as_ref() != Some(&current_window);
    let exploration_changed =
        cache.exploration_revision != Some(authoritative.snapshot().exploration_revision);
    let item_changed = cache.item_revision != Some(authoritative.snapshot().item_revision);
    let resource_changed =
        cache.resource_revision != Some(authoritative.snapshot().resource_revision);
    let refresh_terrain = window_changed || exploration_changed;
    let refresh_items = window_changed || exploration_changed || item_changed;
    let refresh_resources = window_changed || exploration_changed || resource_changed;

    if refresh_terrain || refresh_items || refresh_resources {
        let spatial_started = Instant::now();
        let spatial = match authoritative.spatial_snapshot(
            current_window.coordinates().to_vec(),
            refresh_terrain,
            refresh_items,
            refresh_resources,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                error!("presentation sync failed: {error}");
                return;
            }
        };
        record_elapsed(&mut diagnostics, &SPATIAL_SNAPSHOT_MS, spatial_started);

        let render_origin = cache.render_origin.expect("origin was established above");
        if refresh_terrain {
            let refresh_started = Instant::now();
            let (images, _) = procedural_assets.parts();
            sync_terrain_chunks(
                &mut commands,
                &mut cache,
                &spatial.chunks,
                render_origin,
                images,
            );
            position_characters(&mut commands, &authoritative, &cache, render_origin);
            record_elapsed(&mut diagnostics, &TERRAIN_REFRESH_MS, refresh_started);
        }
        if refresh_resources {
            let refresh_started = Instant::now();
            let (images, registry) = procedural_assets.parts();
            sync_natural_resources(
                &mut commands,
                &mut cache,
                &spatial.natural_resources,
                render_origin,
                images,
                registry,
            );
            record_elapsed(&mut diagnostics, &RESOURCE_REFRESH_MS, refresh_started);
        }
        if refresh_items {
            let refresh_started = Instant::now();
            let (images, registry) = procedural_assets.parts();
            sync_ground_items(
                &mut commands,
                &mut cache,
                &spatial.ground_items,
                render_origin,
                images,
                registry,
            );
            record_elapsed(&mut diagnostics, &ITEM_REFRESH_MS, refresh_started);
        }
        cache.central_chunk = Some(current_center);
        cache.visible_window = Some(current_window);
        cache.exploration_revision = Some(spatial.exploration_revision);
        cache.item_revision = Some(spatial.item_revision);
        cache.resource_revision = Some(spatial.resource_revision);
    }
    record_elapsed(&mut diagnostics, &PRESENTATION_MS, started);
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

fn sync_terrain_chunks(
    commands: &mut Commands,
    cache: &mut PresentationCache,
    chunks: &[progressus_app::ChunkSnapshot],
    origin: WorldCell,
    images: &mut Assets<Image>,
) {
    let root = if let Some(root) = cache.terrain_root {
        root
    } else {
        let root = commands
            .spawn((TerrainRoot, Transform::default(), Visibility::default()))
            .id();
        cache.terrain_root = Some(root);
        root
    };

    let mut known = BTreeMap::<WorldCell, progressus_app::Terrain>::new();
    for chunk in chunks {
        for local_y in 0..CHUNK_SIDE {
            for local_x in 0..CHUNK_SIDE {
                let local = LocalCell::new(local_x, local_y);
                let Some(terrain) = chunk.known_terrain_at(local) else {
                    continue;
                };
                let world_cell = chunk
                    .coordinate
                    .world_cell(local)
                    .expect("a successfully generated chunk contains valid world cells");
                known.insert(world_cell, terrain);
            }
        }
    }

    let visible = chunks
        .iter()
        .map(|chunk| chunk.coordinate)
        .collect::<BTreeSet<_>>();
    let stale = cache
        .terrain_chunks
        .keys()
        .copied()
        .filter(|coordinate| !visible.contains(coordinate))
        .collect::<Vec<_>>();
    for coordinate in stale {
        if let Some(previous) = cache.terrain_chunks.remove(&coordinate) {
            commands.entity(previous.entity).despawn();
            images.remove(previous.image.id());
        }
    }

    for chunk in chunks {
        let coordinate = chunk.coordinate;
        let fingerprint = terrain_chunk_fingerprint(coordinate, &known);
        if let Some(previous) = cache.terrain_chunks.get_mut(&coordinate) {
            commands
                .entity(previous.entity)
                .insert(terrain_chunk_transform(coordinate, origin));
            if previous.fingerprint != fingerprint {
                images
                    .insert(
                        previous.image.id(),
                        render_terrain_chunk_image(coordinate, &known),
                    )
                    .expect("live terrain chunk image handle remains valid");
                previous.fingerprint = fingerprint;
            }
            continue;
        }

        let image = images.add(render_terrain_chunk_image(coordinate, &known));
        let mut sprite = Sprite::from_image(image.clone());
        sprite.custom_size = Some(Vec2::splat(f32::from(CHUNK_SIDE) * CELL_SIZE));
        let entity = commands
            .spawn((
                sprite,
                terrain_chunk_transform(coordinate, origin),
                TerrainChunkVisual,
                ChildOf(root),
            ))
            .id();
        cache.terrain_chunks.insert(
            coordinate,
            TerrainChunkPresentation {
                entity,
                image,
                fingerprint,
            },
        );
    }
}

fn terrain_chunk_transform(coordinate: ChunkCoord, origin: WorldCell) -> Transform {
    let first = coordinate
        .world_cell(LocalCell::new(0, 0))
        .expect("renderable chunk origin has valid world coordinates");
    let first_translation = world_translation(first, origin, TERRAIN_Z);
    let center_offset = (f32::from(CHUNK_SIDE) - 1.0) * CELL_SIZE * 0.5;
    Transform::from_xyz(
        first_translation.x + center_offset,
        first_translation.y + center_offset,
        TERRAIN_Z,
    )
}

fn terrain_chunk_fingerprint(
    coordinate: ChunkCoord,
    known: &BTreeMap<WorldCell, progressus_app::Terrain>,
) -> u64 {
    let first = coordinate
        .world_cell(LocalCell::new(0, 0))
        .expect("snapshotted chunk origin has valid world coordinates");
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let side = i64::from(CHUNK_SIDE);
    for dy in -1..=side {
        for dx in -1..=side {
            let value = first
                .x()
                .checked_add(dx)
                .zip(first.y().checked_add(dy))
                .and_then(|(x, y)| known.get(&WorldCell::new(x, y)).copied())
                .map_or(0_u8, |terrain| match terrain {
                    progressus_app::Terrain::Grass => 1,
                    progressus_app::Terrain::Water => 2,
                    progressus_app::Terrain::Rock => 3,
                });
            hash ^= u64::from(value);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

fn sync_stockpile_zones(
    commands: &mut Commands,
    cache: &mut PresentationCache,
    stockpiles: &[StockpileSnapshot],
    origin: WorldCell,
    window: &VisibleChunkWindow,
    visible: bool,
) {
    for entity in cache.stockpile_zones.drain(..) {
        commands.entity(entity).despawn();
    }
    if !visible {
        return;
    }
    let visible_chunks = window
        .coordinates()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();

    for stockpile in stockpiles {
        for &cell in &stockpile.cells {
            if !visible_chunks.contains(&cell.split().0) {
                continue;
            }
            let entity = commands
                .spawn((
                    Sprite::from_color(
                        Color::srgba(0.20, 0.78, 0.55, 0.30),
                        Vec2::splat(CELL_SIZE),
                    ),
                    Transform::from_translation(world_translation(cell, origin, TERRAIN_Z + 1.0)),
                    StockpileZoneVisual,
                ))
                .id();
            cache.stockpile_zones.push(entity);
        }
    }
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

fn sync_workstations(
    commands: &mut Commands,
    cache: &mut PresentationCache,
    workstations: &[WorkstationSnapshot],
    origin: WorldCell,
    images: &mut Assets<Image>,
    procedural_assets: &mut ProceduralAssetRegistry,
) {
    let authoritative = workstations
        .iter()
        .map(|workstation| (workstation.id, *workstation))
        .collect::<BTreeMap<_, _>>();
    for (id, workstation) in &authoritative {
        let sprite = procedural_assets.sprite(
            images,
            workstation_asset(workstation.kind, workstation.id),
            Vec2::splat(CELL_SIZE * 1.05),
        );
        let transform =
            Transform::from_translation(world_translation(workstation.cell, origin, WORKSTATION_Z));
        if let Some(entity) = cache.workstations.get(id) {
            commands.entity(*entity).insert((sprite, transform));
        } else {
            let entity = commands.spawn((sprite, transform, WorkstationVisual)).id();
            cache.workstations.insert(*id, entity);
        }
    }
    let stale = cache
        .workstations
        .keys()
        .copied()
        .filter(|id| !authoritative.contains_key(id))
        .collect::<Vec<_>>();
    for id in stale {
        if let Some(entity) = cache.workstations.remove(&id) {
            commands.entity(entity).despawn();
        }
    }
}

fn sync_construction(
    commands: &mut Commands,
    cache: &mut PresentationCache,
    sites: &[ConstructionSiteSnapshot],
    structures: &[StructureSnapshot],
    origin: WorldCell,
    images: &mut Assets<Image>,
    procedural_assets: &mut ProceduralAssetRegistry,
) {
    let wall_network_cells = sites
        .iter()
        .filter(|site| site.kind.connects_to_wall_network())
        .map(|site| site.cell)
        .chain(
            structures
                .iter()
                .filter(|structure| structure.kind.connects_to_wall_network())
                .map(|structure| structure.cell),
        )
        .collect::<BTreeSet<_>>();
    let authoritative_sites = sites
        .iter()
        .map(|site| (site.id, *site))
        .collect::<BTreeMap<_, _>>();
    for (id, site) in &authoritative_sites {
        let connections = if site.kind.connects_to_wall_network() {
            CardinalConnections::from_cells(site.cell, &wall_network_cells)
        } else {
            CardinalConnections::default()
        };
        let sprite = procedural_assets.sprite(
            images,
            construction_site_asset(site.kind, connections),
            Vec2::splat(CELL_SIZE),
        );
        let transform =
            Transform::from_translation(world_translation(site.cell, origin, CONSTRUCTION_Z));
        if let Some(entity) = cache.construction_sites.get(id) {
            commands.entity(*entity).insert((sprite, transform));
        } else {
            let entity = commands
                .spawn((sprite, transform, ConstructionSiteVisual))
                .id();
            cache.construction_sites.insert(*id, entity);
        }
    }
    let stale_sites = cache
        .construction_sites
        .keys()
        .copied()
        .filter(|id| !authoritative_sites.contains_key(id))
        .collect::<Vec<_>>();
    for id in stale_sites {
        if let Some(entity) = cache.construction_sites.remove(&id) {
            commands.entity(entity).despawn();
        }
    }

    let authoritative_structures = structures
        .iter()
        .map(|structure| (structure.id, *structure))
        .collect::<BTreeMap<_, _>>();
    for (id, structure) in &authoritative_structures {
        let connections = if structure.kind.connects_to_wall_network() {
            CardinalConnections::from_cells(structure.cell, &wall_network_cells)
        } else {
            CardinalConnections::default()
        };
        let sprite = procedural_assets.sprite(
            images,
            structure_asset(structure.kind, connections, structure.door_state),
            Vec2::splat(CELL_SIZE),
        );
        let transform =
            Transform::from_translation(world_translation(structure.cell, origin, CONSTRUCTION_Z));
        if let Some(entity) = cache.structures.get(id) {
            commands.entity(*entity).insert((sprite, transform));
        } else {
            let entity = commands.spawn((sprite, transform, StructureVisual)).id();
            cache.structures.insert(*id, entity);
        }
    }
    let stale_structures = cache
        .structures
        .keys()
        .copied()
        .filter(|id| !authoritative_structures.contains_key(id))
        .collect::<Vec<_>>();
    for id in stale_structures {
        if let Some(entity) = cache.structures.remove(&id) {
            commands.entity(entity).despawn();
        }
    }
}

fn natural_resource_size(kind: NaturalResourceKind) -> Vec2 {
    match kind {
        NaturalResourceKind::Tree => Vec2::splat(CELL_SIZE * 1.25),
        NaturalResourceKind::StoneOutcrop => Vec2::splat(CELL_SIZE * 0.95),
        NaturalResourceKind::BerryBush => Vec2::splat(CELL_SIZE * 0.88),
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
                            Vec2::splat(CELL_SIZE * 0.78),
                        ),
                        Transform::from_translation(item_translation(&item, origin)),
                        GroundItemVisual,
                    ))
                    .id();
                let label = commands
                    .spawn((
                        procedural_assets.quantity_sprite(images, item.quantity),
                        Transform::from_xyz(CELL_SIZE * 0.27, -CELL_SIZE * 0.34, 1.0),
                        ChildOf(entity),
                    ))
                    .id();
                cache.ground_items.insert(id, entity);
                cache.ground_item_labels.insert(id, label);
            }
            GroundItemSyncAction::Update(item) => {
                if let Some(entity) = cache.ground_items.get(&item.id) {
                    commands.entity(*entity).insert((
                        procedural_assets.sprite(
                            images,
                            item_asset(item.kind, item.id),
                            Vec2::splat(CELL_SIZE * 0.78),
                        ),
                        Transform::from_translation(item_translation(&item, origin)),
                    ));
                    if let Some(label) = cache.ground_item_labels.get(&item.id) {
                        commands
                            .entity(*label)
                            .insert(procedural_assets.quantity_sprite(images, item.quantity));
                    }
                }
            }
            GroundItemSyncAction::Despawn(id) => {
                cache.ground_item_labels.remove(&id);
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

fn sync_carried_items(
    commands: &mut Commands,
    authoritative: &AuthoritativeClient,
    cache: &mut PresentationCache,
    images: &mut Assets<Image>,
    procedural_assets: &mut ProceduralAssetRegistry,
) {
    let authoritative_items = authoritative
        .snapshot()
        .carried_items
        .iter()
        .map(|item| (item.id, *item))
        .collect::<BTreeMap<_, _>>();

    for (id, item) in &authoritative_items {
        let Some(parent) = cache.characters.get(&item.character_id).copied() else {
            continue;
        };
        let sprite = procedural_assets.sprite(
            images,
            item_asset(item.kind, item.id),
            Vec2::splat(CELL_SIZE * 0.56),
        );
        let transform =
            Transform::from_xyz(CELL_SIZE * 0.24, CELL_SIZE * 0.36, CARRIED_ITEM_LOCAL_Z);
        if let Some(entity) = cache.carried_items.get(id).copied() {
            commands
                .entity(entity)
                .insert((sprite, transform, ChildOf(parent)));
        } else {
            let entity = commands
                .spawn((sprite, transform, ChildOf(parent), CarriedItemVisual))
                .id();
            cache.carried_items.insert(*id, entity);
        }
    }

    let stale = cache
        .carried_items
        .keys()
        .copied()
        .filter(|id| !authoritative_items.contains_key(id))
        .collect::<Vec<_>>();
    for id in stale {
        if let Some(entity) = cache.carried_items.remove(&id) {
            commands.entity(entity).despawn();
        }
    }
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

pub(crate) fn interpolate_character_visuals(
    time: Res<Time>,
    mut motion: ResMut<VisualMotion>,
    cache: Res<PresentationCache>,
    mut transforms: Query<&mut Transform, With<CharacterVisual>>,
) {
    let Some(origin_cell) = cache.render_origin else {
        return;
    };
    let Ok(origin) = WorldPosition::from_cell_center(origin_cell) else {
        return;
    };
    let tick_seconds = crate::interaction::TICK_INTERVAL.as_secs_f32();

    for (id, visual_motion) in &mut motion.characters {
        if visual_motion.trace.is_empty() {
            continue;
        }
        let Some(entity) = cache.characters.get(id) else {
            continue;
        };
        let Ok(mut transform) = transforms.get_mut(*entity) else {
            continue;
        };
        visual_motion.elapsed_seconds += time.delta_secs();
        let position = interpolate_trace(
            &visual_motion.trace,
            visual_motion.elapsed_seconds / tick_seconds,
        );
        transform.translation = world_position_translation(position, origin, CHARACTER_Z);
    }
}

pub(crate) fn draw_tool_drag(
    tool: Res<ToolState>,
    cache: Res<PresentationCache>,
    mut gizmos: Gizmos,
) {
    if tool.mode == ToolMode::Select {
        return;
    }
    let (Some(first), Some(last), Some(origin)) =
        (tool.drag_start, tool.drag_current, cache.render_origin)
    else {
        return;
    };
    let min_x = first.x().min(last.x());
    let max_x = first.x().max(last.x());
    let min_y = first.y().min(last.y());
    let max_y = first.y().max(last.y());
    let width = i128::from(max_x) - i128::from(min_x) + 1;
    let height = i128::from(max_y) - i128::from(min_y) + 1;
    if width.saturating_mul(height) > 4096 {
        return;
    }
    let color = match tool.mode {
        ToolMode::Select | ToolMode::Door | ToolMode::Workbench => return,
        ToolMode::StockpileAdd => Color::srgba(0.25, 1.0, 0.72, 0.9),
        ToolMode::StockpileRemove => Color::srgba(1.0, 0.35, 0.32, 0.9),
        ToolMode::Harvest => Color::srgba(1.0, 0.72, 0.18, 0.9),
        ToolMode::Wall => Color::srgba(0.35, 0.88, 1.0, 0.9),
        ToolMode::CancelJobs => Color::srgba(1.0, 0.25, 0.25, 0.9),
    };
    let half = CELL_SIZE * 0.48;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let cell = WorldCell::new(x, y);
            let center = world_translation(cell, origin, CHARACTER_Z + 3.0).truncate();
            let min = center - Vec2::splat(half);
            let max = center + Vec2::splat(half);
            gizmos.line_2d(Vec2::new(min.x, min.y), Vec2::new(max.x, min.y), color);
            gizmos.line_2d(Vec2::new(max.x, min.y), Vec2::new(max.x, max.y), color);
            gizmos.line_2d(Vec2::new(max.x, max.y), Vec2::new(min.x, max.y), color);
            gizmos.line_2d(Vec2::new(min.x, max.y), Vec2::new(min.x, min.y), color);
        }
    }
}

pub(crate) fn draw_stockpiles(
    authoritative: Res<AuthoritativeClient>,
    cache: Res<PresentationCache>,
    selected: Res<SelectedStockpile>,
    zones: Res<ZoneVisibility>,
    mut gizmos: Gizmos,
) {
    if !zones.visible {
        return;
    }
    let (Some(origin), Some(stockpile_id)) = (cache.render_origin, selected.0) else {
        return;
    };
    let Some(stockpile) = authoritative
        .snapshot()
        .stockpiles
        .iter()
        .find(|stockpile| stockpile.id == stockpile_id)
    else {
        return;
    };
    let cells = stockpile.cells.iter().copied().collect::<BTreeSet<_>>();
    let visible_chunks = cache.visible_window.as_ref().map(|window| {
        window
            .coordinates()
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
    });
    let half = CELL_SIZE * 0.50;
    let color = Color::srgba(0.45, 1.0, 0.74, 0.95);

    for &cell in &stockpile.cells {
        if visible_chunks
            .as_ref()
            .is_some_and(|chunks| !chunks.contains(&cell.split().0))
        {
            continue;
        }
        let center = world_translation(cell, origin, GROUND_ITEM_Z - 0.5).truncate();
        let min = center - Vec2::splat(half);
        let max = center + Vec2::splat(half);
        let west = cell.x().checked_sub(1).map(|x| WorldCell::new(x, cell.y()));
        let east = cell.x().checked_add(1).map(|x| WorldCell::new(x, cell.y()));
        let south = cell.y().checked_sub(1).map(|y| WorldCell::new(cell.x(), y));
        let north = cell.y().checked_add(1).map(|y| WorldCell::new(cell.x(), y));

        if west.is_none_or(|neighbor| !cells.contains(&neighbor)) {
            gizmos.line_2d(Vec2::new(min.x, min.y), Vec2::new(min.x, max.y), color);
        }
        if east.is_none_or(|neighbor| !cells.contains(&neighbor)) {
            gizmos.line_2d(Vec2::new(max.x, min.y), Vec2::new(max.x, max.y), color);
        }
        if south.is_none_or(|neighbor| !cells.contains(&neighbor)) {
            gizmos.line_2d(Vec2::new(min.x, min.y), Vec2::new(max.x, min.y), color);
        }
        if north.is_none_or(|neighbor| !cells.contains(&neighbor)) {
            gizmos.line_2d(Vec2::new(min.x, max.y), Vec2::new(max.x, max.y), color);
        }
    }
}

pub(crate) fn draw_job_designations(
    authoritative: Res<AuthoritativeClient>,
    cache: Res<PresentationCache>,
    mut gizmos: Gizmos,
) {
    let Some(origin) = cache.render_origin else {
        return;
    };
    for job in &authoritative.snapshot().jobs {
        let (cell, z) = match job.kind {
            JobKind::Harvest { source } => (source, NATURAL_RESOURCE_Z + 1.0),
            JobKind::Eat { item_id, .. } => {
                let Some(item) = authoritative
                    .snapshot()
                    .ground_items
                    .iter()
                    .find(|item| item.id == item_id)
                else {
                    continue;
                };
                (item.position.containing_cell(), GROUND_ITEM_Z + 1.0)
            }
            JobKind::Craft { workstation_id, .. } => {
                let Some(workstation) = authoritative
                    .snapshot()
                    .workstations
                    .iter()
                    .find(|workstation| workstation.id == workstation_id)
                else {
                    continue;
                };
                (workstation.cell, WORKSTATION_Z + 1.0)
            }
            JobKind::DeliverConstruction { site_id, .. } | JobKind::Construct { site_id } => {
                let Some(site) = authoritative
                    .snapshot()
                    .construction_sites
                    .iter()
                    .find(|site| site.id == site_id)
                else {
                    continue;
                };
                (site.cell, CONSTRUCTION_Z + 1.0)
            }
            JobKind::Haul { .. } | JobKind::SupplyProduction { .. } => continue,
        };
        let center = world_translation(cell, origin, z).truncate();
        let half = CELL_SIZE * 0.46;
        let color = match job.state {
            JobState::Available => Color::srgb(1.0, 0.72, 0.18),
            JobState::Reserved { .. } => Color::srgb(0.35, 0.85, 1.0),
            JobState::Transporting { .. } => Color::srgb(0.35, 0.85, 1.0),
            JobState::Working { .. } => Color::srgb(1.0, 0.42, 0.12),
        };
        let min = center - Vec2::splat(half);
        let max = center + Vec2::splat(half);
        let arm = CELL_SIZE * 0.22;
        for (from, to) in [
            (min, min + Vec2::new(arm, 0.0)),
            (min, min + Vec2::new(0.0, arm)),
            (Vec2::new(max.x, min.y), Vec2::new(max.x - arm, min.y)),
            (Vec2::new(max.x, min.y), Vec2::new(max.x, min.y + arm)),
            (Vec2::new(min.x, max.y), Vec2::new(min.x + arm, max.y)),
            (Vec2::new(min.x, max.y), Vec2::new(min.x, max.y - arm)),
            (max, max - Vec2::new(arm, 0.0)),
            (max, max - Vec2::new(0.0, arm)),
        ] {
            gizmos.line_2d(from, to, color);
        }
    }
}

pub(crate) fn draw_selected_character(
    selected: Res<SelectedCharacter>,
    visuals: Query<(&CharacterVisual, &Transform)>,
    mut gizmos: Gizmos,
) {
    let Some(selected_id) = selected.0 else {
        return;
    };
    let Some((_, transform)) = visuals.iter().find(|(visual, _)| visual.id == selected_id) else {
        return;
    };
    let center = transform.translation.truncate();
    let half = CELL_SIZE * 0.58;
    let arm = CELL_SIZE * 0.22;
    let min = center - Vec2::splat(half);
    let max = center + Vec2::splat(half);
    let color = Color::srgb(0.25, 0.95, 1.0);
    for (from, to) in [
        (min, min + Vec2::new(arm, 0.0)),
        (min, min + Vec2::new(0.0, arm)),
        (Vec2::new(max.x, min.y), Vec2::new(max.x - arm, min.y)),
        (Vec2::new(max.x, min.y), Vec2::new(max.x, min.y + arm)),
        (Vec2::new(min.x, max.y), Vec2::new(min.x + arm, max.y)),
        (Vec2::new(min.x, max.y), Vec2::new(min.x, max.y - arm)),
        (max, max - Vec2::new(arm, 0.0)),
        (max, max - Vec2::new(0.0, arm)),
    ] {
        gizmos.line_2d(from, to, color);
    }
}

pub(crate) fn draw_selected_navigation(
    selected: Res<SelectedCharacter>,
    authoritative: Res<AuthoritativeClient>,
    cache: Res<PresentationCache>,
    visuals: Query<(&CharacterVisual, &Transform)>,
    mut gizmos: Gizmos,
) {
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
    let Some((_, transform)) = visuals.iter().find(|(visual, _)| visual.id == id) else {
        return;
    };
    let to_visual = |position| world_position_translation(position, origin, CHARACTER_Z + 1.0);
    let mut previous = transform.translation.truncate();
    let route_color = Color::srgba(0.25, 0.9, 1.0, 0.72);
    for waypoint in &navigation.remaining_waypoints {
        let next = to_visual(*waypoint).truncate();
        gizmos.line_2d(previous, next, route_color);
        previous = next;
    }
    if let Some(destination) = navigation.destination {
        let route_reaches_destination =
            navigation.remaining_waypoints.last().copied() == Some(destination);
        let destination = to_visual(destination).truncate();
        if route_reaches_destination && previous != destination {
            gizmos.line_2d(previous, destination, route_color);
        }
        let marker = 3.0;
        gizmos.line_2d(
            destination + Vec2::new(-marker, -marker),
            destination + Vec2::new(marker, marker),
            Color::srgb(0.2, 1.0, 1.0),
        );
        gizmos.line_2d(
            destination + Vec2::new(-marker, marker),
            destination + Vec2::new(marker, -marker),
            Color::srgb(0.2, 1.0, 1.0),
        );
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
    let Some(origin_cell) = cache.render_origin else {
        return;
    };

    let resident_color = Color::srgba(0.35, 0.80, 1.0, 0.55);
    let chunk_size = f32::from(CHUNK_SIDE) * CELL_SIZE;
    for coordinate in &authoritative.snapshot().resident_chunks {
        let Some(first_cell) = coordinate.world_cell(LocalCell::new(0, 0)) else {
            continue;
        };
        let first_center = world_translation(first_cell, origin_cell, CHARACTER_Z + 1.0).truncate();
        let minimum = first_center - Vec2::splat(CELL_SIZE * 0.5);
        let maximum = minimum + Vec2::splat(chunk_size);
        gizmos.line_2d(minimum, Vec2::new(maximum.x, minimum.y), resident_color);
        gizmos.line_2d(Vec2::new(maximum.x, minimum.y), maximum, resident_color);
        gizmos.line_2d(maximum, Vec2::new(minimum.x, maximum.y), resident_color);
        gizmos.line_2d(Vec2::new(minimum.x, maximum.y), minimum, resident_color);
    }

    let Some(id) = selected.0 else {
        return;
    };
    let Some(character) = authoritative
        .snapshot()
        .characters
        .iter()
        .find(|character| character.id == id)
    else {
        return;
    };
    let Ok(origin) = WorldPosition::from_cell_center(origin_cell) else {
        return;
    };
    let authority =
        world_position_translation(character.position, origin, CHARACTER_Z + 2.0).truncate();
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
}

fn world_translation(world_cell: WorldCell, origin: WorldCell, z: f32) -> Vec3 {
    let relative_x = (i128::from(world_cell.x()) - i128::from(origin.x())) as f32;
    let relative_y = (i128::from(world_cell.y()) - i128::from(origin.y())) as f32;
    Vec3::new(relative_x * CELL_SIZE, relative_y * CELL_SIZE, z)
}

pub(crate) fn camera_controls(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
    tool: Res<ToolState>,
    mut motion_events: MessageReader<MouseMotion>,
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
    let mouse_drag = motion_events
        .read()
        .fold(Vec2::ZERO, |delta, event| delta + event.delta);
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
            if buttons.pressed(MouseButton::Middle) && !tool.pointer_over_ui {
                transform.translation +=
                    Vec3::new(-mouse_drag.x, mouse_drag.y, 0.0) * orthographic.scale;
            }
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
        CHARACTER_Z, CHUNK_SIDE, GROUND_ITEM_Z, LocalCell, PresentationCache, item_translation,
        sync_terrain_chunks, world_position_translation,
    };

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
        let mut cache = PresentationCache::default();
        {
            let mut commands = bevy::prelude::Commands::new(&mut queue, &world);
            sync_terrain_chunks(
                &mut commands,
                &mut cache,
                &chunks,
                progressus_app::ChunkCoord::new(0, 0)
                    .world_cell(LocalCell::new(0, 0))
                    .unwrap(),
                &mut images,
            );
        }
        queue.apply(&mut world);

        let root = cache.terrain_root.unwrap();
        assert!(world.entity(root).contains::<Visibility>());
        assert_eq!(cache.terrain_chunks.len(), 1);
        let chunk = cache.terrain_chunks.values().next().unwrap();
        assert!(world.get_entity(chunk.entity).is_ok());
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
