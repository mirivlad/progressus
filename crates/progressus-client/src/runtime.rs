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
    use bevy::prelude::{App, Transform, Update, Vec3};
    use progressus_app::{ChunkCoord, EntityId};

    use super::AuthoritativeClient;
    use crate::render::{CharacterVisual, PresentationCache, sync_presentation};

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
}
