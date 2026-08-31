use progressus_app::{
    Application, CHUNK_SIDE, CharacterSnapshot, ChunkCoord, Command, EntityId, NewGameOptions,
    SimulationTick, SnapshotQuery, WorldCell, WorldSeed, WorldgenVersion,
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
                position: WorldCell::new(-2, 0),
            },
            CharacterSnapshot {
                id: EntityId::new(2).unwrap(),
                name: "Borin".to_owned(),
                position: WorldCell::new(-1, 0),
            },
            CharacterSnapshot {
                id: EntityId::new(3).unwrap(),
                name: "Cora".to_owned(),
                position: WorldCell::new(0, 0),
            },
            CharacterSnapshot {
                id: EntityId::new(4).unwrap(),
                name: "Dain".to_owned(),
                position: WorldCell::new(1, 0),
            },
            CharacterSnapshot {
                id: EntityId::new(5).unwrap(),
                name: "Elin".to_owned(),
                position: WorldCell::new(2, 0),
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
    };

    let expected = application.snapshot(query.clone()).unwrap();
    let mut detached = application.snapshot(query.clone()).unwrap();
    detached.chunks.clear();
    detached.characters[0].name = "renderer-owned name".to_owned();

    assert_eq!(application.snapshot(query).unwrap(), expected);
}
