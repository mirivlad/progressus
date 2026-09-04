use progressus_app::{
    Application, CHUNK_SIDE, CURRENT_WORLDGEN_VERSION, ChunkCoord, Command, Direction, EntityId,
    ItemKind, JobKind, JobState, KnownTerrain, LocalCell, MovementState, NaturalResourceKind,
    NewGameOptions, ProductionZoneKind, RESIDENT_CHUNKS_PER_CENTER, RecipeId, SimulationTick,
    SnapshotQuery, StructureKind, Terrain, WorkstationKind, WorldCell, WorldPosition, WorldSeed,
};

fn snapshot_after_long_run(seed: u64) -> progressus_app::ClientSnapshot {
    let mut application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(seed),
    })
    .unwrap();
    application
        .execute(Command::AdvanceTicks { count: 100_000 })
        .unwrap();
    let lightweight = application.snapshot(SnapshotQuery::default()).unwrap();
    let mut chunks = lightweight.resident_chunks.clone();
    if let Some(first) = chunks.first().copied() {
        chunks.push(first);
    }
    application
        .snapshot(SnapshotQuery {
            chunks,
            ..SnapshotQuery::default()
        })
        .unwrap()
}

#[test]
fn snapshot_is_bounded_ordered_and_renderable() {
    let snapshot = snapshot_after_long_run(42);

    assert_eq!(snapshot.tick, SimulationTick::new(100_000));
    assert_eq!(snapshot.worldgen_version, CURRENT_WORLDGEN_VERSION);
    assert!(
        snapshot
            .chunks
            .windows(2)
            .all(|pair| pair[0].coordinate < pair[1].coordinate)
    );
    assert!(snapshot.chunks.iter().all(|chunk| {
        chunk.side == CHUNK_SIDE
            && chunk.cells.len() == usize::from(CHUNK_SIDE) * usize::from(CHUNK_SIDE)
    }));

    let item_keys = snapshot
        .ground_items
        .iter()
        .map(|item| (item.position.containing_cell().split().0, item.id))
        .collect::<Vec<_>>();
    assert!(item_keys.windows(2).all(|pair| pair[0] < pair[1]));
    let item_ids = snapshot
        .ground_items
        .iter()
        .map(|item| item.id)
        .collect::<Vec<_>>();
    for expected in 6..=9 {
        assert!(item_ids.contains(&EntityId::new(expected).unwrap()));
    }
    let berries = snapshot
        .ground_items
        .iter()
        .filter(|item| item.kind == ItemKind::Berries)
        .map(|item| item.quantity)
        .sum::<u32>();
    assert!(
        berries > 0 && berries < 700,
        "long run must physically consume food"
    );

    let identities = snapshot
        .characters
        .iter()
        .map(|character| (character.id.value(), character.name.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        identities,
        vec![
            (1, "Ada"),
            (2, "Borin"),
            (3, "Cora"),
            (4, "Dain"),
            (5, "Elin")
        ]
    );
    assert!(
        snapshot
            .characters
            .iter()
            .all(|character| character.satiety <= 100)
    );
    assert!(
        snapshot
            .characters
            .iter()
            .all(|character| character.satiety > 0)
    );
}

#[test]
fn resident_chunk_diagnostics_are_bounded_and_cover_every_character() {
    let application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(42),
    })
    .unwrap();
    let snapshot = application.snapshot(SnapshotQuery::default()).unwrap();

    assert!(
        snapshot.resident_chunks.len() <= snapshot.characters.len() * RESIDENT_CHUNKS_PER_CENTER
    );
    for character in &snapshot.characters {
        let chunk = character.containing_cell.split().0;
        assert!(snapshot.resident_chunks.binary_search(&chunk).is_ok());
    }
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
    detached.ground_items.clear();
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
        chunk.known_terrain_at(LocalCell::new(30, 0)),
        Some(Terrain::Grass)
    );
    assert_eq!(chunk.terrain_at(LocalCell::new(CHUNK_SIDE, 0)), None);
}

#[test]
fn point_terrain_query_respects_exploration_without_materializing_a_chunk() {
    let application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(42),
    })
    .unwrap();

    assert_eq!(
        application.known_terrain_at(WorldCell::new(0, 0)).unwrap(),
        Some(Terrain::Grass)
    );
    assert_eq!(
        application
            .known_terrain_at(WorldCell::new(10_000, -10_000))
            .unwrap(),
        None
    );
}

#[test]
fn terrain_query_does_not_publish_undiscovered_terrain() {
    let application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(42),
    })
    .unwrap();

    let snapshot = application
        .snapshot(SnapshotQuery {
            chunks: vec![ChunkCoord::new(100, -100)],
            ..SnapshotQuery::default()
        })
        .unwrap();

    assert!(snapshot.chunks.is_empty());
}

#[test]
fn natural_resource_snapshots_are_explored_deterministic_and_not_a_query_side_channel() {
    let application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(0),
    })
    .unwrap();
    let snapshot = application
        .snapshot(SnapshotQuery {
            chunks: vec![ChunkCoord::new(-1, 0), ChunkCoord::new(0, 0)],
            ..SnapshotQuery::default()
        })
        .unwrap();

    assert!(!snapshot.natural_resources.is_empty());
    assert!(
        snapshot
            .natural_resources
            .iter()
            .any(|resource| resource.kind == NaturalResourceKind::Tree)
    );
    assert!(
        snapshot
            .natural_resources
            .iter()
            .any(|resource| resource.kind == NaturalResourceKind::StoneOutcrop)
    );
    for resource in &snapshot.natural_resources {
        let (coordinate, local) = resource.cell.split();
        let chunk = snapshot
            .chunks
            .iter()
            .find(|chunk| chunk.coordinate == coordinate)
            .unwrap();
        assert_eq!(chunk.known_terrain_at(local), Some(Terrain::Grass));
        assert!((4..=8).contains(&resource.yield_quantity));
    }

    let unknown = application
        .snapshot(SnapshotQuery {
            chunks: vec![ChunkCoord::new(100, -100)],
            ..SnapshotQuery::default()
        })
        .unwrap();
    assert!(unknown.natural_resources.is_empty());
}

#[test]
fn harvest_designation_and_cancellation_cross_the_public_application_boundary() {
    let mut application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(0),
    })
    .unwrap();
    let terrain = application
        .snapshot(SnapshotQuery {
            chunks: vec![ChunkCoord::new(-1, 0), ChunkCoord::new(0, 0)],
            ..SnapshotQuery::default()
        })
        .unwrap();
    let source = terrain.natural_resources[0].cell;
    let before_revision = terrain.job_revision;

    application
        .execute(Command::DesignateHarvest { source })
        .unwrap();
    let designated = application.snapshot(SnapshotQuery::default()).unwrap();
    assert_eq!(designated.job_revision, before_revision + 1);
    assert_eq!(designated.jobs.len(), 1);
    assert_eq!(designated.jobs[0].kind, JobKind::Harvest { source });
    let job_id = designated.jobs[0].id;

    application.execute(Command::CancelJob { job_id }).unwrap();
    let cancelled = application.snapshot(SnapshotQuery::default()).unwrap();
    assert!(cancelled.jobs.is_empty());
    assert!(cancelled.job_revision > designated.job_revision);
}

#[test]
fn stockpile_and_haul_cycle_cross_the_public_application_boundary() {
    let mut application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(0),
    })
    .unwrap();
    let destination = WorldCell::new(0, 0);
    application
        .execute(Command::CreateStockpile { cell: destination })
        .unwrap();
    let created = application.snapshot(SnapshotQuery::default()).unwrap();
    assert_eq!(created.stockpiles.len(), 1);
    assert_eq!(created.stockpiles[0].cells, vec![destination]);
    let stockpile_id = created.stockpiles[0].id;
    let item_id = EntityId::new(6).unwrap();
    let mut saw_transporting = false;
    let mut saw_carried = false;

    for _ in 0..256 {
        application
            .execute(Command::AdvanceTicks { count: 1 })
            .unwrap();
        let snapshot = application.snapshot(SnapshotQuery::default()).unwrap();
        saw_transporting |= snapshot.jobs.iter().any(|job| {
            matches!(
                (job.kind, job.state),
                (
                    JobKind::Haul { item_id: id, .. },
                    JobState::Transporting { .. }
                ) if id == item_id
            )
        });
        saw_carried |= snapshot.carried_items.iter().any(|item| item.id == item_id);
        let terrain = application
            .snapshot(SnapshotQuery {
                chunks: vec![ChunkCoord::new(0, 0)],
                ..SnapshotQuery::default()
            })
            .unwrap();
        if terrain
            .ground_items
            .iter()
            .any(|item| item.id == item_id && item.position.containing_cell() == destination)
        {
            break;
        }
    }

    assert!(saw_transporting);
    assert!(saw_carried);
    let after = application
        .snapshot(SnapshotQuery {
            chunks: vec![ChunkCoord::new(0, 0)],
            ..SnapshotQuery::default()
        })
        .unwrap();
    assert!(after.carried_items.is_empty());
    assert!(
        after
            .ground_items
            .iter()
            .any(|item| { item.id == item_id && item.position.containing_cell() == destination })
    );
    assert_eq!(after.stockpiles[0].id, stockpile_id);
}

#[test]
fn workbench_and_craft_cycle_cross_the_public_application_boundary() {
    let mut application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(0),
    })
    .unwrap();
    let workbench_cell = WorldCell::new(0, 1);
    application
        .execute(Command::PlaceWorkstation {
            kind: WorkstationKind::Workbench,
            cell: workbench_cell,
        })
        .unwrap();
    let placed = application.snapshot(SnapshotQuery::default()).unwrap();
    assert_eq!(placed.workstations.len(), 1);
    assert_eq!(placed.workstations[0].cell, workbench_cell);
    assert_eq!(placed.workstations[0].kind, WorkstationKind::Workbench);
    let workstation_id = placed.workstations[0].id;
    assert_eq!(placed.production_logistics.len(), 1);
    assert_eq!(
        placed.production_logistics[0].workstation_id,
        workstation_id
    );
    assert_eq!(placed.production_logistics[0].input_cells.len(), 2);
    assert_eq!(placed.production_logistics[0].output_cells.len(), 2);

    let original_inputs = placed.production_logistics[0].input_cells.clone();
    let edited_input = original_inputs[0];
    assert!(
        application
            .execute(Command::SetProductionZoneCell {
                workstation_id,
                kind: ProductionZoneKind::Input,
                cell: edited_input,
                enabled: false,
            })
            .is_err(),
        "workbench input ports are rotated as a pair rather than edited cell-by-cell"
    );
    application
        .execute(Command::CycleWorkstationInputs { workstation_id })
        .unwrap();
    let rotated = application.snapshot(SnapshotQuery::default()).unwrap();
    assert_eq!(rotated.production_logistics[0].input_cells.len(), 2);
    assert_ne!(rotated.production_logistics[0].input_cells, original_inputs);
    let mut layouts = vec![
        original_inputs.clone(),
        rotated.production_logistics[0].input_cells.clone(),
    ];
    for _ in 0..4 {
        application
            .execute(Command::CycleWorkstationInputs { workstation_id })
            .unwrap();
        layouts.push(
            application
                .snapshot(SnapshotQuery::default())
                .unwrap()
                .production_logistics[0]
                .input_cells
                .clone(),
        );
    }
    layouts.sort();
    layouts.dedup();
    assert_eq!(
        layouts.len(),
        6,
        "the workbench exposes all six two-port layouts"
    );
    application
        .execute(Command::CycleWorkstationInputs { workstation_id })
        .unwrap();
    let wrapped = application.snapshot(SnapshotQuery::default()).unwrap();
    assert_eq!(wrapped.production_logistics[0].input_cells, original_inputs);
    let active_input = wrapped.production_logistics[0].input_cells[0];
    assert!(
        application
            .execute(Command::CreateStockpile { cell: active_input })
            .is_err()
    );

    let original_outputs = wrapped.production_logistics[0].output_cells.clone();
    let edited_output = original_outputs[0];
    assert!(
        application
            .execute(Command::SetProductionZoneCell {
                workstation_id,
                kind: ProductionZoneKind::Output,
                cell: edited_output,
                enabled: false,
            })
            .is_err(),
        "workbench output ports are rotated as a pair rather than edited cell-by-cell"
    );

    let first_stock = WorldCell::new(-2, 0);
    let second_stock = WorldCell::new(2, 0);
    application
        .execute(Command::CreateStockpile { cell: first_stock })
        .unwrap();
    let stockpile_id = application
        .snapshot(SnapshotQuery::default())
        .unwrap()
        .stockpiles[0]
        .id;
    application
        .execute(Command::SetStockpileCell {
            stockpile_id,
            cell: second_stock,
            enabled: true,
        })
        .unwrap();
    application
        .execute(Command::AddProductionOrder {
            workstation_id,
            recipe_id: RecipeId::PrimitiveTool,
            target: progressus_app::ProductionTarget::finite(3),
        })
        .unwrap();

    let designated = application.snapshot(SnapshotQuery::default()).unwrap();
    assert_eq!(designated.production_orders.len(), 1);
    let order_id = designated.production_orders[0].id;
    assert_eq!(
        designated.production_orders[0].target.remaining_runs(),
        Some(3)
    );
    assert!(designated.jobs.iter().any(|job| {
        job.kind
            == JobKind::Craft {
                workstation_id,
                order_id,
                recipe_id: RecipeId::PrimitiveTool,
            }
    }));

    let mut produced = false;
    for _ in 0..1024 {
        application
            .execute(Command::AdvanceTicks { count: 1 })
            .unwrap();
        let snapshot = application
            .snapshot(SnapshotQuery {
                chunks: vec![ChunkCoord::new(-1, 0), ChunkCoord::new(0, 0)],
                ..SnapshotQuery::default()
            })
            .unwrap();
        if snapshot
            .production_orders
            .iter()
            .any(|order| order.id == order_id && order.target.remaining_runs() == Some(0))
        {
            let quantity = snapshot
                .ground_items
                .iter()
                .filter(|item| item.kind == ItemKind::PrimitiveTool)
                .map(|item| item.quantity)
                .sum::<u32>();
            produced = quantity == 3;
            break;
        }
    }
    assert!(produced);

    application
        .execute(Command::SetProductionOrderTarget {
            order_id,
            target: progressus_app::ProductionTarget::Infinite,
        })
        .unwrap();
    let infinite = application.snapshot(SnapshotQuery::default()).unwrap();
    assert_eq!(
        infinite.production_orders[0].target,
        progressus_app::ProductionTarget::Infinite
    );
    assert!(infinite.jobs.iter().any(|job| matches!(
        job.kind,
        JobKind::Craft { order_id: job_order_id, .. } if job_order_id == order_id
    )));
    application
        .execute(Command::SetProductionOrderTarget {
            order_id,
            target: progressus_app::ProductionTarget::finite(0),
        })
        .unwrap();

    application
        .execute(Command::RemoveWorkstation { workstation_id })
        .unwrap();
    let removed = application.snapshot(SnapshotQuery::default()).unwrap();
    assert!(removed.workstations.is_empty());
    assert!(removed.production_logistics.is_empty());
}

#[test]
fn workbench_output_rotation_crosses_the_public_application_boundary() {
    let mut application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(0),
    })
    .unwrap();

    for y in -4..=4 {
        for x in -6..=6 {
            let cell = WorldCell::new(x, y);
            if application
                .execute(Command::PlaceWorkstation {
                    kind: WorkstationKind::Workbench,
                    cell,
                })
                .is_err()
            {
                continue;
            }
            let placed = application.snapshot(SnapshotQuery::default()).unwrap();
            let Some(workstation) = placed
                .workstations
                .iter()
                .find(|workstation| workstation.cell == cell)
            else {
                continue;
            };
            let workstation_id = workstation.id;
            let logistics = placed
                .production_logistics
                .iter()
                .find(|logistics| logistics.workstation_id == workstation_id)
                .unwrap();
            if logistics.input_cells.len() != 2 || logistics.output_cells.len() != 2 {
                application
                    .execute(Command::RemoveWorkstation { workstation_id })
                    .unwrap();
                continue;
            }
            let original_inputs = logistics.input_cells.clone();
            let original_outputs = logistics.output_cells.clone();
            let mut layouts = vec![original_outputs.clone()];
            let mut complete_cycle = true;
            for _ in 0..5 {
                if application
                    .execute(Command::CycleWorkstationOutputs { workstation_id })
                    .is_err()
                {
                    complete_cycle = false;
                    break;
                }
                layouts.push(
                    application
                        .snapshot(SnapshotQuery::default())
                        .unwrap()
                        .production_logistics
                        .iter()
                        .find(|logistics| logistics.workstation_id == workstation_id)
                        .unwrap()
                        .output_cells
                        .clone(),
                );
            }
            if complete_cycle {
                layouts.sort();
                layouts.dedup();
                assert_eq!(layouts.len(), 6);
                application
                    .execute(Command::CycleWorkstationOutputs { workstation_id })
                    .unwrap();
                let wrapped = application.snapshot(SnapshotQuery::default()).unwrap();
                let wrapped = wrapped
                    .production_logistics
                    .iter()
                    .find(|logistics| logistics.workstation_id == workstation_id)
                    .unwrap();
                assert_eq!(wrapped.output_cells, original_outputs);
                assert_eq!(wrapped.input_cells, original_inputs);
                return;
            }
            application
                .execute(Command::RemoveWorkstation { workstation_id })
                .unwrap();
        }
    }
    panic!(
        "expected one explored seed-0 workbench fixture with all four diagonal output cells free"
    );
}

#[test]
fn physical_wall_construction_crosses_the_public_application_boundary() {
    let mut application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(0),
    })
    .unwrap();
    let cell = WorldCell::new(0, 1);
    application
        .execute(Command::DesignateConstruction {
            kind: StructureKind::StoneWall,
            cell,
        })
        .unwrap();
    let designated = application.snapshot(SnapshotQuery::default()).unwrap();
    assert_eq!(designated.construction_sites.len(), 1);
    let site_id = designated.construction_sites[0].id;
    assert_eq!(designated.construction_sites[0].cell, cell);
    assert!(designated.construction_revision > 0);

    let mut saw_carried = false;
    for _ in 0..768 {
        application
            .execute(Command::AdvanceTicks { count: 1 })
            .unwrap();
        let snapshot = application.snapshot(SnapshotQuery::default()).unwrap();
        saw_carried |= !snapshot.carried_items.is_empty();
        if snapshot
            .structures
            .iter()
            .any(|structure| structure.id == site_id)
        {
            assert!(snapshot.construction_sites.is_empty());
            assert_eq!(snapshot.structures[0].kind, StructureKind::StoneWall);
            assert_eq!(snapshot.structures[0].cell, cell);
            break;
        }
    }
    let completed = application.snapshot(SnapshotQuery::default()).unwrap();
    assert!(saw_carried);
    assert!(
        completed
            .structures
            .iter()
            .any(|structure| structure.id == site_id)
    );
    assert!(
        application
            .execute(Command::MoveTo {
                character_id: EntityId::new(3).unwrap(),
                destination: WorldPosition::from_cell_center(cell).unwrap(),
            })
            .is_err()
    );
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
    assert_eq!(
        cora.last_tick_motion_trace,
        vec![
            WorldPosition::from_cell_center(WorldCell::new(0, 0)).unwrap(),
            cora.position,
        ]
    );
    assert!(snapshot.navigation.is_none());
    assert!(snapshot.characters.iter().all(|character| {
        !character.last_tick_motion_trace.is_empty()
            && character.last_tick_motion_trace.last() == Some(&character.position)
    }));

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
            matches!(terrain, KnownTerrain::Known(Terrain::Water | Terrain::Rock)).then(|| {
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

#[test]
fn save_load_crosses_the_public_application_boundary_without_internal_access() {
    let mut application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(77),
    })
    .unwrap();
    application
        .execute(Command::AdvanceTicks { count: 37 })
        .unwrap();
    let before = application.save_json().unwrap();
    let metadata = Application::save_metadata(&before).unwrap();
    assert_eq!(metadata.world_seed, WorldSeed::new(77));
    assert_eq!(metadata.tick, SimulationTick::new(37));

    let restored = Application::from_save_json(&before).unwrap();
    assert_eq!(restored.save_json().unwrap(), before);

    let mut corrupted = before.clone();
    corrupted.truncate(corrupted.len() / 2);
    assert!(Application::from_save_json(&corrupted).is_err());
    assert_eq!(application.save_json().unwrap(), before);
}

#[test]
fn nutrition_and_physical_eating_cross_the_public_application_boundary() {
    let mut application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(42),
    })
    .unwrap();
    let initial = application.snapshot(SnapshotQuery::default()).unwrap();
    assert!(
        initial
            .characters
            .iter()
            .all(|character| character.satiety == 100)
    );

    application
        .execute(Command::AdvanceTicks { count: 16 })
        .unwrap();
    let decayed = application.snapshot(SnapshotQuery::default()).unwrap();
    assert!(
        decayed
            .characters
            .iter()
            .all(|character| character.satiety == 99)
    );

    application
        .execute(Command::AdvanceTicks { count: 784 })
        .unwrap();
    let hungry = application.snapshot(SnapshotQuery::default()).unwrap();
    assert!(
        hungry
            .jobs
            .iter()
            .any(|job| matches!(job.kind, JobKind::Eat { .. }))
    );

    application
        .execute(Command::AdvanceTicks { count: 128 })
        .unwrap();
    let recovered = application.snapshot(SnapshotQuery::default()).unwrap();
    assert!(
        recovered
            .characters
            .iter()
            .all(|character| character.satiety > 50)
    );
    let physical = application
        .snapshot(SnapshotQuery {
            chunks: recovered.resident_chunks.clone(),
            ..SnapshotQuery::default()
        })
        .unwrap();
    let berries = physical
        .ground_items
        .iter()
        .filter(|item| item.kind == ItemKind::Berries)
        .map(|item| item.quantity)
        .sum::<u32>();
    assert!(
        berries < 700,
        "autonomous eating must consume physical berries"
    );
}
