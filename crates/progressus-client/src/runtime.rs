use std::error::Error;
use std::fmt::{self, Display, Formatter};

use bevy::prelude::*;
use progressus_app::{
    Application, ApplicationError, ChunkCoord, ClientSnapshot, Command, EntityId, NewGameOptions,
    SnapshotQuery, WorldSeed,
};

use crate::interaction::{TickScheduler, movement_command};
use crate::presentation::PresentationError;
use crate::render::{PresentationCache, camera_controls, setup_camera, sync_presentation};

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
        Ok(self.application.snapshot(SnapshotQuery { chunks })?)
    }

    pub(crate) fn take_snapshot_dirty(&mut self) -> bool {
        std::mem::take(&mut self.snapshot_dirty)
    }

    fn refresh_lightweight_snapshot(&mut self) -> Result<(), ClientError> {
        self.snapshot = self.application.snapshot(SnapshotQuery::default())?;
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
        && let Err(error) = authoritative.refresh_lightweight_snapshot()
    {
        error!("authoritative snapshot failed: {error}");
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
            (advance_authority, sync_presentation, camera_controls).chain(),
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
        CHUNK_SIDE, CharacterSnapshot, ChunkCoord, Command, Direction, EntityId, MovementState,
        Terrain, WorldCell,
    };

    use super::{AuthoritativeClient, advance_authority};
    use crate::interaction::TickScheduler;
    use crate::render::{CharacterVisual, PresentationCache, TerrainRoot, sync_presentation};

    const TERRAIN_CELL_COUNT: usize = 9 * (CHUNK_SIDE as usize) * (CHUNK_SIDE as usize);

    fn presentation_app(authoritative: AuthoritativeClient) -> App {
        let mut app = App::new();
        app.insert_resource(authoritative)
            .insert_resource(PresentationCache::default())
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

    fn set_detached_character_position(app: &mut App, id: EntityId, position: WorldCell) {
        let mut authoritative = app.world_mut().resource_mut::<AuthoritativeClient>();
        authoritative
            .snapshot
            .characters
            .iter_mut()
            .find(|character| character.id == id)
            .unwrap()
            .position = position;
        authoritative.snapshot_dirty = true;
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
    }

    #[test]
    fn dirty_sync_rebuilds_terrain_only_after_authoritative_chunk_crossing() {
        let mut app = presentation_app(AuthoritativeClient::new().unwrap());

        app.update();

        let (initial_root, initial_center, initial_characters) = {
            let cache = app.world().resource::<PresentationCache>();
            (
                cache.terrain_root.unwrap(),
                cache.central_chunk,
                cache.characters.clone(),
            )
        };
        let initial_children = terrain_children(&app, initial_root);
        assert_eq!(initial_center, Some(ChunkCoord::new(0, 0)));
        assert_eq!(initial_children.len(), TERRAIN_CELL_COUNT);

        mark_snapshot_dirty(&mut app);
        app.update();

        let cache = app.world().resource::<PresentationCache>();
        assert_eq!(cache.terrain_root, Some(initial_root));
        assert_eq!(cache.central_chunk, initial_center);
        assert_eq!(cache.characters, initial_characters);
        assert_eq!(terrain_children(&app, initial_root), initial_children);

        let cora_y = character(&app, super::cora_id()).position.y();
        set_detached_character_position(&mut app, super::cora_id(), WorldCell::new(31, cora_y));
        app.update();

        assert_eq!(
            app.world().resource::<PresentationCache>().terrain_root,
            Some(initial_root)
        );
        assert_eq!(
            app.world()
                .entity(character_entity(&app, super::cora_id()))
                .get::<Transform>()
                .unwrap()
                .translation,
            Vec3::new(31.0 * 12.0, cora_y as f32 * 12.0, 10.0)
        );

        set_detached_character_position(&mut app, super::cora_id(), WorldCell::new(32, cora_y));
        app.update();

        let (new_root, characters) = {
            let cache = app.world().resource::<PresentationCache>();
            assert_eq!(cache.central_chunk, Some(ChunkCoord::new(1, 0)));
            (cache.terrain_root.unwrap(), cache.characters.clone())
        };
        assert_ne!(new_root, initial_root);
        assert!(app.world().get_entity(initial_root).is_err());
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

        for authoritative in &app
            .world()
            .resource::<AuthoritativeClient>()
            .snapshot()
            .characters
        {
            let expected = Vec3::new(
                (authoritative.position.x() - 32) as f32 * 12.0,
                authoritative.position.y() as f32 * 12.0,
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
    fn rejected_edge_movement_resyncs_from_authority_without_changing_presentation() {
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
                .execute(Command::AdvanceTicks { count: 1 })
                .unwrap();
        }
        authoritative
            .application
            .execute(Command::StopMovement {
                character_id: super::cora_id(),
            })
            .unwrap();
        authoritative.refresh_lightweight_snapshot().unwrap();
        let blocked_from = authoritative
            .snapshot()
            .characters
            .iter()
            .find(|character| character.id == super::cora_id())
            .unwrap()
            .position;
        let blocked_target = blocked_direction.adjacent(blocked_from).unwrap();
        assert_ne!(terrain_at(&authoritative, blocked_target), Terrain::Grass);

        let mut app = App::new();
        app.insert_resource(authoritative)
            .insert_resource(TickScheduler::default())
            .insert_resource(PresentationCache::default())
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
            .position = WorldCell::new(blocked_from.x() + 7, blocked_from.y() + 11);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(blocked_key);

        app.update();

        let authoritative = app.world().resource::<AuthoritativeClient>();
        assert_eq!(authoritative.snapshot(), &authoritative_before);
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
            .position;
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
