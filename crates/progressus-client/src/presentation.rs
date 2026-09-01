#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use progressus_app::{CharacterSnapshot, ChunkCoord, MovementState, WorldCell, WorldPosition};

    use super::{
        CharacterSyncAction, VisibleChunkWindow, character_sync_actions, terrain_refresh_needed,
    };

    #[test]
    fn radius_one_window_is_row_major_three_by_three() {
        let window = VisibleChunkWindow::around(ChunkCoord::new(4, -2)).unwrap();
        assert_eq!(
            window.coordinates(),
            &[
                ChunkCoord::new(3, -3),
                ChunkCoord::new(4, -3),
                ChunkCoord::new(5, -3),
                ChunkCoord::new(3, -2),
                ChunkCoord::new(4, -2),
                ChunkCoord::new(5, -2),
                ChunkCoord::new(3, -1),
                ChunkCoord::new(4, -1),
                ChunkCoord::new(5, -1),
            ]
        );
    }

    #[test]
    fn viewport_window_covers_its_chunk_bounds_and_margin() {
        let window = VisibleChunkWindow::covering(
            ChunkCoord::new(2, 3),
            ChunkCoord::new(1, 2),
            ChunkCoord::new(3, 4),
            1,
        )
        .unwrap();

        assert_eq!(window.coordinates().first(), Some(&ChunkCoord::new(0, 1)));
        assert_eq!(window.coordinates().last(), Some(&ChunkCoord::new(4, 5)));
        assert_eq!(window.coordinates().len(), 25);
    }

    #[test]
    fn terrain_rebuild_happens_only_for_initial_or_changed_center() {
        let center = ChunkCoord::new(0, 0);
        assert!(terrain_refresh_needed(None, center));
        assert!(!terrain_refresh_needed(Some(center), center));
        assert!(terrain_refresh_needed(Some(center), ChunkCoord::new(1, 0)));
    }

    #[test]
    fn reconciliation_uses_stable_ids_and_removes_only_missing_ids() {
        let rendered = BTreeSet::from([
            progressus_app::EntityId::new(3).unwrap(),
            progressus_app::EntityId::new(8).unwrap(),
        ]);
        let snapshots = vec![CharacterSnapshot {
            id: progressus_app::EntityId::new(3).unwrap(),
            name: "Cora".to_owned(),
            position: WorldPosition::from_cell_center(WorldCell::new(32, 0)).unwrap(),
            containing_cell: WorldCell::new(32, 0),
            movement: MovementState::Idle,
        }];

        assert_eq!(
            character_sync_actions(&rendered, &snapshots),
            vec![
                CharacterSyncAction::Update(snapshots[0].clone()),
                CharacterSyncAction::Despawn(progressus_app::EntityId::new(8).unwrap()),
            ],
        );
    }
}
use std::collections::{BTreeMap, BTreeSet};

use progressus_app::{CharacterSnapshot, ChunkCoord, EntityId};

pub const VISIBLE_CHUNK_RADIUS: i64 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresentationError {
    VisibleWindowOutOfRange { center: ChunkCoord },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisibleChunkWindow {
    center: ChunkCoord,
    coordinates: Vec<ChunkCoord>,
}

impl VisibleChunkWindow {
    pub fn around(center: ChunkCoord) -> Result<Self, PresentationError> {
        Self::covering(center, center, center, VISIBLE_CHUNK_RADIUS)
    }

    pub fn covering(
        center: ChunkCoord,
        minimum: ChunkCoord,
        maximum: ChunkCoord,
        margin: i64,
    ) -> Result<Self, PresentationError> {
        let minimum_x = minimum
            .x()
            .checked_sub(margin)
            .ok_or(PresentationError::VisibleWindowOutOfRange { center })?;
        let maximum_x = maximum
            .x()
            .checked_add(margin)
            .ok_or(PresentationError::VisibleWindowOutOfRange { center })?;
        let minimum_y = minimum
            .y()
            .checked_sub(margin)
            .ok_or(PresentationError::VisibleWindowOutOfRange { center })?;
        let maximum_y = maximum
            .y()
            .checked_add(margin)
            .ok_or(PresentationError::VisibleWindowOutOfRange { center })?;
        let mut coordinates = Vec::new();
        for y in minimum_y..=maximum_y {
            for x in minimum_x..=maximum_x {
                coordinates.push(ChunkCoord::new(x, y));
            }
        }
        Ok(Self {
            center,
            coordinates,
        })
    }

    pub const fn center(&self) -> ChunkCoord {
        self.center
    }

    pub fn coordinates(&self) -> &[ChunkCoord] {
        &self.coordinates
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CharacterSyncAction {
    Spawn(CharacterSnapshot),
    Update(CharacterSnapshot),
    Despawn(EntityId),
}

pub fn controlled_character(characters: &[CharacterSnapshot]) -> Option<&CharacterSnapshot> {
    let cora = EntityId::new(3)?;
    characters.iter().find(|character| character.id == cora)
}

pub fn terrain_refresh_needed(rendered: Option<ChunkCoord>, current: ChunkCoord) -> bool {
    rendered != Some(current)
}

pub fn character_sync_actions(
    rendered: &BTreeSet<EntityId>,
    characters: &[CharacterSnapshot],
) -> Vec<CharacterSyncAction> {
    let authoritative = characters
        .iter()
        .cloned()
        .map(|character| (character.id, character))
        .collect::<BTreeMap<_, _>>();
    let mut actions = Vec::new();
    for (id, character) in &authoritative {
        actions.push(if rendered.contains(id) {
            CharacterSyncAction::Update(character.clone())
        } else {
            CharacterSyncAction::Spawn(character.clone())
        });
    }
    for id in rendered {
        if !authoritative.contains_key(id) {
            actions.push(CharacterSyncAction::Despawn(*id));
        }
    }
    actions
}
