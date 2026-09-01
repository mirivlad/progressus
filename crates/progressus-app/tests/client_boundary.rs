use progressus_app::{
    Application, CHUNK_SIDE, CharacterSnapshot, ChunkCoord, Command, Direction, EntityId,
    LocalCell, MovementState, NewGameOptions, SimulationTick, SnapshotQuery, Terrain, WorldCell,
    WorldPosition, WorldSeed, WorldgenVersion,
};

fn snapshot_after_long_run(seed: u64) -> progressus_app::ClientSnapshot {
    let mut application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(seed),
    })
    .unwrap();
    application
        .execute(Command::AdvanceTicks { count: 100_000 })
        .unwrap();
    application
        .snapshot(SnapshotQuery {
            chunks: vec![
                ChunkCoord::new(1, 0),
                ChunkCoord::new(-1, 0),
                ChunkCoord::new(0, 0),
                ChunkCoord::new(1, 0),
            ],
            ..SnapshotQuery::default()
        })
        .unwrap()
}

#[test]
fn snapshot_is_bounded_ordered_and_renderable() {
    let snapshot = snapshot_after_long_run(42);

    assert_eq!(snapshot.tick, SimulationTick::new(100_000));
    assert_eq!(snapshot.worldgen_version, WorldgenVersion::new(1));
    assert_eq!(
        snapshot
            .chunks
            .iter()
            .map(|chunk| chunk.coordinate)
            .collect::<Vec<_>>(),
        vec![
            ChunkCoord::new(-1, 0),
            ChunkCoord::new(0, 0),
            ChunkCoord::new(1, 0)
        ]
    );
    assert!(snapshot.chunks.iter().all(|chunk| {
        chunk.side == CHUNK_SIDE
            && chunk.cells.len() == usize::from(CHUNK_SIDE) * usize::from(CHUNK_SIDE)
    }));

    assert_eq!(
        snapshot.characters,
        vec![
            CharacterSnapshot {
                id: EntityId::new(1).unwrap(),
                name: "Ada".to_owned(),
                position: WorldPosition::from_cell_center(WorldCell::new(-2, 0)).unwrap(),
                containing_cell: WorldCell::new(-2, 0),
                movement: MovementState::Idle,
            },
            CharacterSnapshot {
                id: EntityId::new(2).unwrap(),
                name: "Borin".to_owned(),
                position: WorldPosition::from_cell_center(WorldCell::new(-1, 0)).unwrap(),
                containing_cell: WorldCell::new(-1, 0),
                movement: MovementState::Idle,
            },
            CharacterSnapshot {
                id: EntityId::new(3).unwrap(),
                name: "Cora".to_owned(),
                position: WorldPosition::from_cell_center(WorldCell::new(0, 0)).unwrap(),
                containing_cell: WorldCell::new(0, 0),
                movement: MovementState::Idle,
            },
            CharacterSnapshot {
                id: EntityId::new(4).unwrap(),
                name: "Dain".to_owned(),
                position: WorldPosition::from_cell_center(WorldCell::new(1, 0)).unwrap(),
                containing_cell: WorldCell::new(1, 0),
                movement: MovementState::Idle,
            },
            CharacterSnapshot {
                id: EntityId::new(5).unwrap(),
                name: "Elin".to_owned(),
                position: WorldPosition::from_cell_center(WorldCell::new(2, 0)).unwrap(),
                containing_cell: WorldCell::new(2, 0),
                movement: MovementState::Idle,
            },
        ]
    );
}

#[test]
fn identical_inputs_produce_identical_client_snapshots() {
    assert_eq!(snapshot_after_long_run(73), snapshot_after_long_run(73));
}

#[test]
fn snapshots_do_not_borrow_or_mutate_authoritative_state() {
    let mut application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(42),
    })
    .unwrap();
    application
        .execute(Command::AdvanceTicks { count: 7 })
        .unwrap();
    let query = SnapshotQuery {
        chunks: vec![ChunkCoord::new(0, 0)],
        ..SnapshotQuery::default()
    };

    let expected = application.snapshot(query.clone()).unwrap();
    let mut detached = application.snapshot(query.clone()).unwrap();
    detached.chunks.clear();
    detached.characters[0].name = "renderer-owned name".to_owned();

    assert_eq!(application.snapshot(query).unwrap(), expected);
}

#[test]
fn chunk_snapshots_map_local_coordinates_without_simulation_access() {
    let application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(42),
    })
    .unwrap();
    let snapshot = application
        .snapshot(SnapshotQuery {
            chunks: vec![ChunkCoord::new(-1, 0)],
            ..SnapshotQuery::default()
        })
        .unwrap();
    let chunk = &snapshot.chunks[0];

    assert_eq!(
        chunk.terrain_at(LocalCell::new(30, 0)),
        Some(Terrain::Grass)
    );
    assert_eq!(chunk.terrain_at(LocalCell::new(CHUNK_SIDE, 0)), None);
}

#[test]
fn movement_commands_are_applied_and_published_through_snapshots() {
    let mut application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(42),
    })
    .unwrap();
    let cora = EntityId::new(3).unwrap();

    application
        .execute(Command::SetMovementDirection {
            character_id: cora,
            direction: Direction::East,
        })
        .unwrap();
    application
        .execute(Command::AdvanceTicks { count: 1 })
        .unwrap();

    let snapshot = application.snapshot(SnapshotQuery::default()).unwrap();
    let cora = snapshot
        .characters
        .iter()
        .find(|character| character.id == cora)
        .unwrap();
    assert_eq!(
        cora.position,
        WorldPosition::from_cell_center(WorldCell::new(0, 0))
            .unwrap()
            .checked_translate(256, 0)
            .unwrap()
    );
    assert_eq!(cora.containing_cell, WorldCell::new(0, 0));
    assert_eq!(cora.position.containing_cell(), cora.containing_cell);
    assert_eq!(
        cora.movement,
        MovementState::ManualDirectional {
            direction: Direction::East
        }
    );

    application
        .execute(Command::StopMovement {
            character_id: cora.id,
        })
        .unwrap();
    let stopped = application
        .snapshot(SnapshotQuery::default())
        .unwrap()
        .characters
        .into_iter()
        .find(|character| character.id == cora.id)
        .unwrap();
    assert_eq!(stopped.movement, MovementState::Idle);
}

#[test]
fn selected_navigation_snapshot_is_detached_and_default_query_omits_route() {
    let mut application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(2),
    })
    .unwrap();
    let cora = EntityId::new(3).unwrap();
    let destination = WorldPosition::from_subunits(800, 900).unwrap();
    application
        .execute(Command::MoveTo {
            character_id: cora,
            destination,
        })
        .unwrap();

    assert!(
        application
            .snapshot(SnapshotQuery::default())
            .unwrap()
            .navigation
            .is_none()
    );
    let query = SnapshotQuery {
        navigation_for: Some(cora),
        ..SnapshotQuery::default()
    };
    let expected = application.snapshot(query.clone()).unwrap();
    let navigation = expected.navigation.as_ref().unwrap();
    assert_eq!(navigation.character_id, cora);
    assert_eq!(navigation.destination, Some(destination));
    assert!(!navigation.remaining_waypoints.is_empty());

    let mut detached = application.snapshot(query.clone()).unwrap();
    detached
        .navigation
        .as_mut()
        .unwrap()
        .remaining_waypoints
        .clear();
    assert_eq!(application.snapshot(query).unwrap(), expected);
}

#[test]
fn rejected_move_to_keeps_the_selected_navigation_snapshot() {
    let mut application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(2),
    })
    .unwrap();
    let cora = EntityId::new(3).unwrap();
    let accepted = WorldPosition::from_subunits(800, 900).unwrap();
    application
        .execute(Command::MoveTo {
            character_id: cora,
            destination: accepted,
        })
        .unwrap();
    let query = SnapshotQuery {
        chunks: vec![ChunkCoord::new(0, 0)],
        navigation_for: Some(cora),
    };
    let before = application.snapshot(query.clone()).unwrap();
    let blocked = before
        .chunks
        .first()
        .unwrap()
        .cells
        .iter()
        .enumerate()
        .find_map(|(index, terrain)| {
            (*terrain != Terrain::Grass).then(|| {
                let x = (index % usize::from(CHUNK_SIDE)) as u16;
                let y = (index / usize::from(CHUNK_SIDE)) as u16;
                ChunkCoord::new(0, 0)
                    .world_cell(LocalCell::new(x, y))
                    .unwrap()
            })
        })
        .unwrap();

    assert!(
        application
            .execute(Command::MoveTo {
                character_id: cora,
                destination: WorldPosition::from_cell_center(blocked).unwrap(),
            })
            .is_err()
    );
    assert_eq!(application.snapshot(query).unwrap(), before);
}
