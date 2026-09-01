use std::error::Error;
use std::fmt::{self, Display, Formatter};

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use progressus_app::{
    Application, ApplicationError, ChunkCoord, ClientSnapshot, Command, EntityId, NewGameOptions,
    SnapshotQuery, WorldSeed,
};

use crate::interaction::{TickScheduler, movement_command};
use crate::navigation::{SelectedCharacter, VisualMotion, quantize_local_click, select_nearest};
use crate::presentation::PresentationError;
use crate::render::{
    NavigationDebug, PresentationCache, camera_controls, setup_camera, sync_presentation,
};

impl Resource for TickScheduler {}

#[derive(Resource)]
pub(crate) struct AuthoritativeClient {
    application: Application,
    snapshot: ClientSnapshot,
    snapshot_dirty: bool,
}

impl AuthoritativeClient {
    pub(crate) fn new() -> Result<Self, ClientError> {
        let application = Application::new_game(NewGameOptions {
            seed: WorldSeed::new(0),
        })?;
        let snapshot = application.snapshot(SnapshotQuery::default())?;
        Ok(Self {
            application,
            snapshot,
            snapshot_dirty: true,
        })
    }

    pub(crate) fn snapshot(&self) -> &ClientSnapshot {
        &self.snapshot
    }

    pub(crate) fn terrain_snapshot(
        &self,
        chunks: Vec<ChunkCoord>,
    ) -> Result<ClientSnapshot, ClientError> {
        Ok(self.application.snapshot(SnapshotQuery {
            chunks,
            ..SnapshotQuery::default()
        })?)
    }

    pub(crate) fn take_snapshot_dirty(&mut self) -> bool {
        std::mem::take(&mut self.snapshot_dirty)
    }

    fn refresh_lightweight_snapshot(
        &mut self,
        navigation_for: Option<EntityId>,
    ) -> Result<(), ClientError> {
        self.snapshot = self.application.snapshot(SnapshotQuery {
            navigation_for,
            ..SnapshotQuery::default()
        })?;
        self.snapshot_dirty = true;
        Ok(())
    }
}

pub(crate) fn cora_id() -> EntityId {
    EntityId::new(3).expect("3 is a valid nonzero Progressus entity ID")
}

pub(crate) fn advance_authority(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut scheduler: ResMut<TickScheduler>,
    mut authoritative: ResMut<AuthoritativeClient>,
    selected: Res<SelectedCharacter>,
) {
    let mut command_attempted = false;
    if let Some(command) = movement_command(&keys, cora_id()) {
        command_attempted = true;
        if let Err(error) = authoritative.application.execute(command) {
            warn!("movement command rejected: {error}");
        }
    }
    let tick_due = scheduler.advance(time.delta());
    if tick_due
        && let Err(error) = authoritative
            .application
            .execute(Command::AdvanceTicks { count: 1 })
    {
        error!("authoritative tick failed: {error}");
        return;
    }
    if (command_attempted || tick_due)
        && let Err(error) = authoritative.refresh_lightweight_snapshot(selected.0)
    {
        error!("authoritative snapshot failed: {error}");
    }
}

pub(crate) fn pointer_navigation(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut selected: ResMut<SelectedCharacter>,
    mut motion: ResMut<VisualMotion>,
    mut authoritative: ResMut<AuthoritativeClient>,
    cache: Res<PresentationCache>,
) {
    if !buttons.just_pressed(MouseButton::Left) && !buttons.just_pressed(MouseButton::Right) {
        return;
    }
    let (Ok(window), Ok((camera, camera_transform))) = (windows.single(), cameras.single()) else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Some(world) = camera.viewport_to_world_2d(camera_transform, cursor).ok() else {
        return;
    };
    let Some(center) = cache.central_chunk else {
        return;
    };
    let Some(origin_cell) = center.world_cell(progressus_app::LocalCell::new(0, 0)) else {
        return;
    };
    let Ok(origin) = progressus_app::WorldPosition::from_cell_center(origin_cell) else {
        return;
    };
    let Ok(target) = quantize_local_click(origin, world.x, world.y) else {
        warn!("pointer position cannot be represented as an authoritative world position");
        return;
    };

    if buttons.just_pressed(MouseButton::Left) {
        selected.0 = select_nearest(
            authoritative
                .snapshot()
                .characters
                .iter()
                .map(|character| (character.id, character.position)),
            target,
            progressus_app::SUBUNITS_PER_CELL / 2,
        );
        if selected.0.is_none() {
            motion.clear();
        }
        if let Err(error) = authoritative.refresh_lightweight_snapshot(selected.0) {
            error!("authoritative snapshot failed after selection: {error}");
        }
        return;
    }

    let Some(character_id) = selected.0 else {
        return;
    };
    match authoritative.application.execute(Command::MoveTo {
        character_id,
        destination: target,
    }) {
        Ok(()) => {
            if let Err(error) = authoritative.refresh_lightweight_snapshot(Some(character_id)) {
                error!("authoritative snapshot failed after move command: {error}");
            }
        }
        Err(error) => warn!("move command rejected: {error}"),
    }
}

#[derive(Debug)]
pub enum ClientError {
    Application(ApplicationError),
    Presentation(PresentationError),
}

impl Display for ClientError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Application(error) => Display::fmt(error, formatter),
            Self::Presentation(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for ClientError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Application(error) => Some(error),
            Self::Presentation(error) => Some(error),
        }
    }
}

impl From<ApplicationError> for ClientError {
    fn from(error: ApplicationError) -> Self {
        Self::Application(error)
    }
}

impl From<PresentationError> for ClientError {
    fn from(error: PresentationError) -> Self {
        Self::Presentation(error)
    }
}

impl Display for PresentationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::VisibleWindowOutOfRange { center } => write!(
                formatter,
                "a complete visible chunk window cannot be formed around ({}, {})",
                center.x(),
                center.y()
            ),
        }
    }
}

impl Error for PresentationError {}

pub fn run() -> Result<(), ClientError> {
    App::new()
        .insert_resource(AuthoritativeClient::new()?)
        .insert_resource(TickScheduler::default())
        .insert_resource(PresentationCache::default())
        .insert_resource(NavigationDebug::default())
        .insert_resource(SelectedCharacter::default())
        .insert_resource(VisualMotion::default())
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Progressus — Prototype 01".to_owned(),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup_camera)
        .add_systems(
            Update,
            (
                pointer_navigation,
                advance_authority,
                sync_presentation,
                crate::render::interpolate_selected_visual,
                crate::render::draw_navigation_debug,
                camera_controls,
            )
                .chain(),
        )
        .run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, VecDeque, btree_map::Entry};

    use bevy::prelude::{
        App, ButtonInput, Children, Entity, IntoScheduleConfigs, KeyCode, Time, Transform, Update,
        Vec3,
    };
    use progressus_app::{
        Application, CHUNK_SIDE, CharacterSnapshot, ChunkCoord, ClientSnapshot, Command, Direction,
        EntityId, LocalCell, MovementState, SUBUNITS_PER_CELL, SnapshotQuery, Terrain, WorldCell,
        WorldPosition,
    };

    use super::{AuthoritativeClient, advance_authority};
    use crate::interaction::TickScheduler;
    use crate::navigation::{SelectedCharacter, VisualMotion};
    use crate::render::{CharacterVisual, PresentationCache, TerrainRoot, sync_presentation};

    const TERRAIN_CELL_COUNT: usize = 9 * (CHUNK_SIDE as usize) * (CHUNK_SIDE as usize);
    const CROSSING_WALK_STEP_LIMIT: u64 = 1_024;
    const WALKER_DIRECTIONS: [Direction; 4] = [
        Direction::East,
        Direction::North,
        Direction::South,
        Direction::West,
    ];

    fn presentation_app(authoritative: AuthoritativeClient) -> App {
        let mut app = App::new();
        app.insert_resource(authoritative)
            .insert_resource(PresentationCache::default())
            .insert_resource(SelectedCharacter::default())
            .insert_resource(VisualMotion::default())
            .add_systems(Update, sync_presentation);
        app
    }

    fn mark_snapshot_dirty(app: &mut App) {
        app.world_mut()
            .resource_mut::<AuthoritativeClient>()
            .snapshot_dirty = true;
    }

    fn character(app: &App, id: EntityId) -> CharacterSnapshot {
        app.world()
            .resource::<AuthoritativeClient>()
            .snapshot()
            .characters
            .iter()
            .find(|character| character.id == id)
            .unwrap()
            .clone()
    }

    fn character_entity(app: &App, id: EntityId) -> Entity {
        app.world().resource::<PresentationCache>().characters[&id]
    }

    fn character_visual_count(app: &mut App, id: EntityId) -> usize {
        let world = app.world_mut();
        let mut visuals = world.query::<&CharacterVisual>();
        visuals.iter(world).filter(|visual| visual.id == id).count()
    }

    fn terrain_children(app: &App, root: Entity) -> Vec<Entity> {
        app.world()
            .entity(root)
            .get::<Children>()
            .unwrap()
            .iter()
            .copied()
            .collect()
    }

    #[test]
    fn new_game_exposes_cora_in_lightweight_snapshot() {
        let client = AuthoritativeClient::new().unwrap();
        assert!(
            client
                .snapshot()
                .characters
                .iter()
                .any(|character| character.id == EntityId::new(3).unwrap())
        );
        assert!(client.snapshot().chunks.is_empty());
    }

    #[test]
    fn character_reconciliation_precedes_the_missing_cora_gate() {
        let mut authoritative = AuthoritativeClient::new().unwrap();
        authoritative
            .snapshot
            .characters
            .retain(|character| character.id != super::cora_id());

        let ada_id = EntityId::new(1).unwrap();
        let mut app = App::new();
        let ada_visual = app
            .world_mut()
            .spawn((CharacterVisual { id: ada_id }, Transform::default()))
            .id();
        let cora_visual = app
            .world_mut()
            .spawn((
                CharacterVisual {
                    id: super::cora_id(),
                },
                Transform::default(),
            ))
            .id();
        let mut cache = PresentationCache {
            central_chunk: Some(ChunkCoord::new(0, 0)),
            ..Default::default()
        };
        cache.characters.insert(ada_id, ada_visual);
        cache.characters.insert(super::cora_id(), cora_visual);
        app.insert_resource(authoritative)
            .insert_resource(cache)
            .insert_resource(SelectedCharacter(Some(super::cora_id())))
            .insert_resource(VisualMotion::default())
            .add_systems(Update, sync_presentation);

        app.update();

        let cache = app.world().resource::<PresentationCache>();
        assert_eq!(cache.characters.get(&ada_id), Some(&ada_visual));
        assert!(!cache.characters.contains_key(&super::cora_id()));
        assert_eq!(
            app.world()
                .entity(ada_visual)
                .get::<Transform>()
                .unwrap()
                .translation,
            Vec3::new(-24.0, 0.0, 10.0)
        );
        assert!(app.world().get_entity(cora_visual).is_err());
        assert_eq!(app.world().resource::<SelectedCharacter>().0, None);
    }

    #[test]
    fn dirty_sync_rebuilds_terrain_only_after_authoritative_chunk_crossing() {
        let mut app = presentation_app(AuthoritativeClient::new().unwrap());

        app.update();

        let initial_root = {
            let cache = app.world().resource::<PresentationCache>();
            cache.terrain_root.unwrap()
        };
        let initial_children = terrain_children(&app, initial_root);
        assert_eq!(
            app.world().resource::<PresentationCache>().central_chunk,
            Some(ChunkCoord::new(0, 0))
        );
        assert_eq!(initial_children.len(), TERRAIN_CELL_COUNT);

        let crossing_from = {
            let mut authoritative = app.world_mut().resource_mut::<AuthoritativeClient>();
            let crossing_from = walk_to_selected_positive_x_crossing(
                &mut authoritative.application,
                super::cora_id(),
            );
            authoritative.refresh_lightweight_snapshot(None).unwrap();
            crossing_from
        };
        app.update();

        assert_eq!(
            character(&app, super::cora_id()).containing_cell,
            crossing_from
        );
        assert_eq!(crossing_from.x(), 31);
        let crossing_center = crossing_from.split().0;
        assert_eq!(crossing_center.x(), 0);
        let crossing_origin = crossing_center.world_cell(LocalCell::new(0, 0)).unwrap();
        let (root_before, center_before, characters_before) = {
            let cache = app.world().resource::<PresentationCache>();
            (
                cache.terrain_root.unwrap(),
                cache.central_chunk,
                cache.characters.clone(),
            )
        };
        let children_before = terrain_children(&app, root_before);
        assert_eq!(center_before, Some(crossing_center));
        assert_eq!(children_before.len(), TERRAIN_CELL_COUNT);

        mark_snapshot_dirty(&mut app);
        app.update();

        let cache = app.world().resource::<PresentationCache>();
        assert_eq!(cache.terrain_root, Some(root_before));
        assert_eq!(cache.central_chunk, center_before);
        assert_eq!(cache.characters, characters_before);
        assert_eq!(terrain_children(&app, root_before), children_before);
        assert_eq!(
            app.world()
                .entity(character_entity(&app, super::cora_id()))
                .get::<Transform>()
                .unwrap()
                .translation,
            Vec3::new(
                31.0 * 12.0,
                (crossing_from.y() - crossing_origin.y()) as f32 * 12.0,
                10.0,
            )
        );

        {
            let mut authoritative = app.world_mut().resource_mut::<AuthoritativeClient>();
            authoritative
                .application
                .execute(Command::SetMovementDirection {
                    character_id: super::cora_id(),
                    direction: Direction::East,
                })
                .unwrap();
            authoritative
                .application
                .execute(Command::AdvanceTicks { count: 4 })
                .unwrap();
            authoritative.refresh_lightweight_snapshot(None).unwrap();
        }
        app.update();

        let crossing_to = character(&app, super::cora_id()).containing_cell;
        assert_eq!(crossing_to, WorldCell::new(32, crossing_from.y()));
        let new_center = crossing_to.split().0;
        assert_eq!(new_center, ChunkCoord::new(1, crossing_center.y()));
        let (new_root, characters) = {
            let cache = app.world().resource::<PresentationCache>();
            assert_eq!(cache.central_chunk, Some(new_center));
            (cache.terrain_root.unwrap(), cache.characters.clone())
        };
        assert_ne!(new_root, root_before);
        assert!(app.world().get_entity(root_before).is_err());
        let new_children = terrain_children(&app, new_root);
        assert_eq!(new_children.len(), TERRAIN_CELL_COUNT);
        let (min_x, max_x, min_y, max_y) = new_children.iter().fold(
            (
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
            ),
            |(min_x, max_x, min_y, max_y), entity| {
                let translation = app
                    .world()
                    .entity(*entity)
                    .get::<Transform>()
                    .unwrap()
                    .translation;
                (
                    min_x.min(translation.x),
                    max_x.max(translation.x),
                    min_y.min(translation.y),
                    max_y.max(translation.y),
                )
            },
        );
        assert_eq!((min_x, max_x), (-32.0 * 12.0, 63.0 * 12.0));
        assert_eq!((min_y, max_y), (-32.0 * 12.0, 63.0 * 12.0));
        let terrain_root_count = {
            let world = app.world_mut();
            let mut terrain_roots = world.query::<&TerrainRoot>();
            terrain_roots.iter(world).count()
        };
        assert_eq!(terrain_root_count, 1);

        let render_origin =
            WorldPosition::from_cell_center(new_center.world_cell(LocalCell::new(0, 0)).unwrap())
                .unwrap();
        for authoritative in &app
            .world()
            .resource::<AuthoritativeClient>()
            .snapshot()
            .characters
        {
            let expected = Vec3::new(
                (authoritative.position.x_subunits() - render_origin.x_subunits()) as f32
                    / SUBUNITS_PER_CELL as f32
                    * 12.0,
                (authoritative.position.y_subunits() - render_origin.y_subunits()) as f32
                    / SUBUNITS_PER_CELL as f32
                    * 12.0,
                10.0,
            );
            assert_eq!(
                app.world()
                    .entity(characters[&authoritative.id])
                    .get::<Transform>()
                    .unwrap()
                    .translation,
                expected
            );
        }
    }

    fn walk_to_selected_positive_x_crossing(
        application: &mut Application,
        character_id: EntityId,
    ) -> WorldCell {
        let mut snapshot = application.snapshot(SnapshotQuery::default()).unwrap();
        let mut position = snapshot_character_position(&snapshot, character_id);
        let mut visit_counts = BTreeMap::from([(position, 1_u64)]);

        for steps in 0..CROSSING_WALK_STEP_LIMIT {
            let direction = select_least_visited_grass_direction(
                application,
                position,
                &visit_counts,
            )
            .unwrap_or_else(|| {
                panic!(
                    "least-visited walker has no adjacent grass cell before the x=0 -> x=1 crossing; character_id={} position=({}, {}) steps={steps} limit={CROSSING_WALK_STEP_LIMIT}",
                    character_id.value(),
                    position.x(),
                    position.y(),
                )
            });
            let target = direction.adjacent(position).unwrap_or_else(|| {
                panic!(
                    "least-visited walker selected an overflowing step; character_id={} position=({}, {}) direction={direction:?} steps={steps} limit={CROSSING_WALK_STEP_LIMIT}",
                    character_id.value(),
                    position.x(),
                    position.y(),
                )
            });

            if position.x() == 31 && target.x() == 32 && target.y() == position.y() {
                return position;
            }

            application
                .execute(Command::SetMovementDirection {
                    character_id,
                    direction,
                })
                .unwrap();
            application
                .execute(Command::AdvanceTicks { count: 4 })
                .unwrap();
            snapshot = application.snapshot(SnapshotQuery::default()).unwrap();
            let reached = snapshot_character_position(&snapshot, character_id);
            assert_eq!(
                reached,
                target,
                "authoritative walker step did not reach the selected grass cell; character_id={} from=({}, {}) direction={direction:?} steps={} limit={CROSSING_WALK_STEP_LIMIT}",
                character_id.value(),
                position.x(),
                position.y(),
                steps + 1,
            );
            position = reached;
            *visit_counts.entry(reached).or_insert(0) += 1;
        }

        panic!(
            "least-visited walker did not select a reachable x=31 -> x=32 grass crossing; character_id={} position=({}, {}) steps={CROSSING_WALK_STEP_LIMIT} limit={CROSSING_WALK_STEP_LIMIT}",
            character_id.value(),
            position.x(),
            position.y(),
        );
    }

    fn select_least_visited_grass_direction(
        application: &Application,
        position: WorldCell,
        visit_counts: &BTreeMap<WorldCell, u64>,
    ) -> Option<Direction> {
        let candidates = WALKER_DIRECTIONS
            .into_iter()
            .enumerate()
            .filter_map(|(order, direction)| {
                direction
                    .adjacent(position)
                    .map(|cell| (order, direction, cell))
            })
            .collect::<Vec<_>>();
        let mut chunks = candidates
            .iter()
            .map(|(_, _, cell)| cell.split().0)
            .collect::<Vec<_>>();
        chunks.sort_unstable();
        chunks.dedup();
        let terrain = application
            .snapshot(SnapshotQuery {
                chunks,
                ..SnapshotQuery::default()
            })
            .unwrap();

        candidates
            .into_iter()
            .filter(|(_, _, cell)| snapshot_terrain_at(&terrain, *cell) == Some(Terrain::Grass))
            .min_by_key(|(order, _, cell)| (visit_counts.get(cell).copied().unwrap_or(0), *order))
            .map(|(_, direction, _)| direction)
    }

    fn snapshot_character_position(snapshot: &ClientSnapshot, id: EntityId) -> WorldCell {
        snapshot
            .characters
            .iter()
            .find(|character| character.id == id)
            .unwrap()
            .containing_cell
    }

    fn snapshot_terrain_at(snapshot: &ClientSnapshot, position: WorldCell) -> Option<Terrain> {
        let (chunk, local) = position.split();
        snapshot
            .chunks
            .iter()
            .find(|candidate| candidate.coordinate == chunk)
            .and_then(|candidate| candidate.terrain_at(local))
    }

    #[test]
    fn character_reappearance_replaces_bevy_entity_once() {
        let mut app = presentation_app(AuthoritativeClient::new().unwrap());
        app.update();

        let ada_id = EntityId::new(1).unwrap();
        let ada = character(&app, ada_id);
        let old_visual = character_entity(&app, ada_id);
        {
            let mut authoritative = app.world_mut().resource_mut::<AuthoritativeClient>();
            authoritative
                .snapshot
                .characters
                .retain(|character| character.id != ada_id);
            authoritative.snapshot_dirty = true;
        }
        app.update();

        assert!(
            !app.world()
                .resource::<PresentationCache>()
                .characters
                .contains_key(&ada_id)
        );
        assert!(app.world().get_entity(old_visual).is_err());

        {
            let mut authoritative = app.world_mut().resource_mut::<AuthoritativeClient>();
            authoritative.snapshot.characters.push(ada);
            authoritative.snapshot_dirty = true;
        }
        app.update();

        let new_visual = character_entity(&app, ada_id);
        assert_ne!(new_visual, old_visual);
        assert_eq!(
            app.world()
                .entity(new_visual)
                .get::<CharacterVisual>()
                .unwrap()
                .id,
            ada_id
        );
        let new_transform = *app.world().entity(new_visual).get::<Transform>().unwrap();
        assert_eq!(character_visual_count(&mut app, ada_id), 1);

        mark_snapshot_dirty(&mut app);
        app.update();

        assert_eq!(character_entity(&app, ada_id), new_visual);
        assert_eq!(character_visual_count(&mut app, ada_id), 1);
        assert_eq!(
            *app.world().entity(new_visual).get::<Transform>().unwrap(),
            new_transform
        );
    }

    #[test]
    fn blocked_edge_intent_resyncs_from_authority_without_manual_presentation_changes() {
        let mut authoritative = AuthoritativeClient::new().unwrap();
        let (path, blocked_direction, blocked_key) =
            blocked_step_from_public_snapshots(&authoritative);
        for direction in path {
            authoritative
                .application
                .execute(Command::SetMovementDirection {
                    character_id: super::cora_id(),
                    direction,
                })
                .unwrap();
            authoritative
                .application
                .execute(Command::AdvanceTicks { count: 4 })
                .unwrap();
        }
        authoritative
            .application
            .execute(Command::StopMovement {
                character_id: super::cora_id(),
            })
            .unwrap();
        authoritative.refresh_lightweight_snapshot(None).unwrap();
        let blocked_from = authoritative
            .snapshot()
            .characters
            .iter()
            .find(|character| character.id == super::cora_id())
            .unwrap()
            .containing_cell;
        let blocked_target = blocked_direction.adjacent(blocked_from).unwrap();
        assert_ne!(terrain_at(&authoritative, blocked_target), Terrain::Grass);

        let mut app = App::new();
        app.insert_resource(authoritative)
            .insert_resource(TickScheduler::default())
            .insert_resource(PresentationCache::default())
            .insert_resource(SelectedCharacter::default())
            .insert_resource(VisualMotion::default())
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(Time::<()>::default())
            .add_systems(Update, (advance_authority, sync_presentation).chain());
        app.update();

        let authoritative_before = app
            .world()
            .resource::<AuthoritativeClient>()
            .snapshot()
            .clone();
        assert_eq!(
            authoritative_before
                .characters
                .iter()
                .find(|character| character.id == super::cora_id())
                .unwrap()
                .movement,
            MovementState::Idle
        );
        let (root_before, center_before, handles_before, transforms_before) = {
            let cache = app.world().resource::<PresentationCache>();
            let transforms = cache
                .characters
                .iter()
                .map(|(id, entity)| {
                    (
                        *id,
                        *app.world().entity(*entity).get::<Transform>().unwrap(),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            (
                cache.terrain_root,
                cache.central_chunk,
                cache.characters.clone(),
                transforms,
            )
        };

        app.world_mut()
            .resource_mut::<AuthoritativeClient>()
            .snapshot
            .characters
            .iter_mut()
            .find(|character| character.id == super::cora_id())
            .unwrap()
            .position = WorldPosition::from_cell_center(WorldCell::new(
            blocked_from.x() + 7,
            blocked_from.y() + 11,
        ))
        .unwrap();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(blocked_key);

        app.update();

        let authoritative = app.world().resource::<AuthoritativeClient>();
        assert_eq!(
            authoritative
                .snapshot()
                .characters
                .iter()
                .find(|character| character.id == super::cora_id())
                .unwrap()
                .position,
            authoritative_before
                .characters
                .iter()
                .find(|character| character.id == super::cora_id())
                .unwrap()
                .position
        );
        assert_eq!(
            authoritative
                .snapshot()
                .characters
                .iter()
                .find(|character| character.id == super::cora_id())
                .unwrap()
                .movement,
            MovementState::ManualDirectional {
                direction: blocked_direction
            }
        );
        assert!(!authoritative.snapshot_dirty);
        let cache = app.world().resource::<PresentationCache>();
        assert_eq!(cache.terrain_root, root_before);
        assert_eq!(cache.central_chunk, center_before);
        assert_eq!(cache.characters, handles_before);
        for (id, entity) in &cache.characters {
            assert_eq!(
                app.world().entity(*entity).get::<Transform>().unwrap(),
                &transforms_before[id]
            );
        }
    }

    fn blocked_step_from_public_snapshots(
        authoritative: &AuthoritativeClient,
    ) -> (Vec<Direction>, Direction, KeyCode) {
        let start = authoritative
            .snapshot()
            .characters
            .iter()
            .find(|character| character.id == super::cora_id())
            .unwrap()
            .containing_cell;
        let center = start.split().0;
        let chunks = (-1..=1)
            .flat_map(|y| (-1..=1).map(move |x| ChunkCoord::new(center.x() + x, center.y() + y)))
            .collect();
        let terrain = authoritative.terrain_snapshot(chunks).unwrap();
        let terrain_by_cell = terrain
            .chunks
            .iter()
            .flat_map(|chunk| {
                (0..CHUNK_SIDE).flat_map(move |local_y| {
                    (0..CHUNK_SIDE).map(move |local_x| {
                        let local = progressus_app::LocalCell::new(local_x, local_y);
                        (
                            chunk.coordinate.world_cell(local).unwrap(),
                            chunk.terrain_at(local).unwrap(),
                        )
                    })
                })
            })
            .collect::<BTreeMap<_, _>>();
        let directions = [
            (Direction::East, KeyCode::ArrowRight),
            (Direction::North, KeyCode::ArrowUp),
            (Direction::South, KeyCode::ArrowDown),
            (Direction::West, KeyCode::ArrowLeft),
        ];
        let mut frontier = VecDeque::from([start]);
        let mut predecessors = BTreeMap::<WorldCell, (WorldCell, Direction)>::new();
        predecessors.insert(start, (start, Direction::East));

        while let Some(position) = frontier.pop_front() {
            for (direction, key) in directions {
                let neighbor = direction.adjacent(position).unwrap();
                let Some(terrain) = terrain_by_cell.get(&neighbor) else {
                    continue;
                };
                if *terrain != Terrain::Grass {
                    let mut reversed = Vec::new();
                    let mut cursor = position;
                    while cursor != start {
                        let (previous, step) = predecessors[&cursor];
                        reversed.push(step);
                        cursor = previous;
                    }
                    reversed.reverse();
                    return (reversed, direction, key);
                }
                if let Entry::Vacant(entry) = predecessors.entry(neighbor) {
                    entry.insert((position, direction));
                    frontier.push_back(neighbor);
                }
            }
        }

        panic!("seed-0 public terrain snapshots contain no reachable blocked cardinal step")
    }

    fn terrain_at(authoritative: &AuthoritativeClient, position: WorldCell) -> Terrain {
        let (chunk, local) = position.split();
        authoritative.terrain_snapshot(vec![chunk]).unwrap().chunks[0]
            .terrain_at(local)
            .unwrap()
    }
}
