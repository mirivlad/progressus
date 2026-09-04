use std::env;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::process::ExitCode;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

use progressus_app::{
    Application, ChunkCoord, ClientSnapshot, Command, DEFAULT_CHARACTER_SPEED, Direction, EntityId,
    ItemKind, KnownTerrain, LocalCell, NewGameOptions, ProductionTarget,
    RESIDENT_CHUNKS_PER_CENTER, RecipeId, SUBUNITS_PER_CELL, SnapshotQuery, StructureKind, Terrain,
    WorkstationKind, WorldCell, WorldPosition, WorldSeed,
};

const USAGE: &str = "usage: progressus-headless --seed <u64> (--ticks <u64> | --travel-chunks <positive u64> | --activity-smoke)";
const ACTIVITY_SMOKE_TICKS: u64 = 100_000;
const ACTIVITY_BOOTSTRAP_LIMIT: u64 = 1_024;
const WALKER_CHARACTER_ID: EntityId = match EntityId::new(3) {
    Some(id) => id,
    None => panic!("walker character ID must be non-zero"),
};
const DIRECTIONS: [Direction; 4] = [
    Direction::East,
    Direction::North,
    Direction::South,
    Direction::West,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Config {
    seed: u64,
    scenario: Scenario,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Scenario {
    AdvanceTicks(u64),
    TravelChunks(u64),
    ActivitySmoke,
}

#[derive(Debug)]
struct CliError(String);

impl Display for CliError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for CliError {}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let config = parse_arguments(env::args().skip(1))?;
    let mut application = Application::new_game(NewGameOptions {
        seed: WorldSeed::new(config.seed),
    })?;

    match config.scenario {
        Scenario::AdvanceTicks(count) => run_ticks(&mut application, config.seed, count)?,
        Scenario::TravelChunks(requested_boundaries) => {
            run_travel(&mut application, config.seed, requested_boundaries)?;
        }
        Scenario::ActivitySmoke => run_activity_smoke(&mut application, config.seed)?,
    }

    Ok(())
}

fn run_ticks(application: &mut Application, seed: u64, ticks: u64) -> Result<(), Box<dyn Error>> {
    application.execute(Command::AdvanceTicks { count: ticks })?;
    let snapshot = application.snapshot(SnapshotQuery {
        chunks: vec![
            ChunkCoord::new(-1, 0),
            ChunkCoord::new(0, 0),
            ChunkCoord::new(1, 0),
        ],
        ..SnapshotQuery::default()
    })?;
    validate_residency(&snapshot)?;

    println!(
        "seed={} tick={} worldgen_version={} chunks={} characters={} resident_chunks={}",
        seed,
        snapshot.tick.value(),
        snapshot.worldgen_version.value(),
        snapshot.chunks.len(),
        snapshot.characters.len(),
        snapshot.resident_chunks.len()
    );
    for character in &snapshot.characters {
        println!(
            "character id={} name={} position_subunits=({}, {}) containing_cell=({}, {})",
            character.id.value(),
            character.name,
            character.position.x_subunits(),
            character.position.y_subunits(),
            character.containing_cell.x(),
            character.containing_cell.y()
        );
    }

    let mut grass = 0_usize;
    let mut water = 0_usize;
    let mut rock = 0_usize;
    let mut unknown = 0_usize;
    for terrain in snapshot.chunks.iter().flat_map(|chunk| chunk.cells.iter()) {
        match terrain {
            progressus_app::KnownTerrain::Known(Terrain::Grass) => grass += 1,
            progressus_app::KnownTerrain::Known(Terrain::Water) => water += 1,
            progressus_app::KnownTerrain::Known(Terrain::Rock) => rock += 1,
            progressus_app::KnownTerrain::Unknown => unknown += 1,
        }
    }
    println!("terrain grass={grass} water={water} rock={rock} unknown={unknown}");

    Ok(())
}

fn run_travel(
    application: &mut Application,
    seed: u64,
    requested_boundaries: u64,
) -> Result<(), Box<dyn Error>> {
    let mut snapshot = application.snapshot(SnapshotQuery::default())?;
    validate_residency(&snapshot)?;
    let mut max_resident_chunks = snapshot.resident_chunks.len();
    let mut position = character_exact_position(&snapshot, WALKER_CHARACTER_ID)?.containing_cell();
    let (start_chunk, _) = position.split();
    let mut max_chunk_x = start_chunk.x();
    let mut crossed_boundaries = 0_u64;
    let step_limit = requested_boundaries
        .checked_mul(512)
        .ok_or_else(|| CliError("travel chunk count is too large".to_owned()))?
        .max(1_024);
    let mut visit_counts = BTreeMap::new();
    visit_counts.insert(position, 1_u64);

    for steps in 0..step_limit {
        let direction =
            select_direction(application, position, &visit_counts).map_err(|reason| {
                travel_failure(
                    seed,
                    position,
                    max_chunk_x,
                    crossed_boundaries,
                    steps,
                    reason,
                )
            })?;
        let target = direction.adjacent(position).ok_or_else(|| {
            travel_failure(
                seed,
                position,
                max_chunk_x,
                crossed_boundaries,
                steps,
                "selected direction overflows world-cell coordinates",
            )
        })?;

        application
            .execute(Command::SetMovementDirection {
                character_id: WALKER_CHARACTER_ID,
                direction,
            })
            .map_err(|error| {
                travel_failure(
                    seed,
                    position,
                    max_chunk_x,
                    crossed_boundaries,
                    steps,
                    &format!("movement command was rejected: {error}"),
                )
            })?;
        let target_position = WorldPosition::from_cell_center(target).map_err(|error| {
            travel_failure(
                seed,
                position,
                max_chunk_x,
                crossed_boundaries,
                steps,
                &format!("target center is outside the fixed-point world: {error:?}"),
            )
        })?;
        let mut actual = character_exact_position(&snapshot, WALKER_CHARACTER_ID)?;
        let ticks_per_step = SUBUNITS_PER_CELL
            .checked_div(i128::from(DEFAULT_CHARACTER_SPEED.subunits_per_tick()))
            .expect("default movement speed is nonzero");
        for _ in 0..ticks_per_step {
            application
                .execute(Command::AdvanceTicks { count: 1 })
                .map_err(|error| {
                    travel_failure(
                        seed,
                        position,
                        max_chunk_x,
                        crossed_boundaries,
                        steps,
                        &format!("simulation tick failed: {error}"),
                    )
                })?;
            snapshot = application.snapshot(SnapshotQuery::default())?;
            validate_residency(&snapshot)?;
            max_resident_chunks = max_resident_chunks.max(snapshot.resident_chunks.len());
            actual = character_exact_position(&snapshot, WALKER_CHARACTER_ID)?;
            if actual == target_position {
                break;
            }
        }
        if actual != target_position {
            return Err(Box::new(travel_failure(
                seed,
                actual.containing_cell(),
                max_chunk_x,
                crossed_boundaries,
                steps + 1,
                "authoritative position did not reach the chosen cell center",
            )));
        }

        position = actual.containing_cell();
        let count = visit_counts.entry(position).or_insert(0);
        *count = count.checked_add(1).ok_or_else(|| {
            travel_failure(
                seed,
                position,
                max_chunk_x,
                crossed_boundaries,
                steps + 1,
                "walker visit count overflowed",
            )
        })?;

        let (chunk, _) = position.split();
        if chunk.x() > max_chunk_x {
            crossed_boundaries += u64::try_from(chunk.x() - max_chunk_x).expect("positive delta");
            max_chunk_x = chunk.x();
        }
        if crossed_boundaries >= requested_boundaries {
            println!(
                "travel character_id={} seed={} start_chunk_x={} max_chunk_x={} crossed_boundaries={} steps={} position=({}, {}) resident_chunks={} max_resident_chunks={}",
                WALKER_CHARACTER_ID.value(),
                seed,
                start_chunk.x(),
                max_chunk_x,
                crossed_boundaries,
                steps + 1,
                position.x(),
                position.y(),
                snapshot.resident_chunks.len(),
                max_resident_chunks
            );
            return Ok(());
        }
    }

    Err(Box::new(travel_failure(
        seed,
        position,
        max_chunk_x,
        crossed_boundaries,
        step_limit,
        "step limit exhausted",
    )))
}

fn run_activity_smoke(application: &mut Application, seed: u64) -> Result<(), Box<dyn Error>> {
    let designated_resources = setup_activity_world(application)?;
    let chunks = activity_chunks();
    let mut snapshot = application.snapshot(SnapshotQuery {
        chunks: chunks.clone(),
        ..SnapshotQuery::default()
    })?;
    validate_activity_snapshot(&snapshot)?;

    let mut max_jobs = snapshot.jobs.len();
    let mut max_carried = snapshot.carried_items.len();
    let mut max_resident_chunks = snapshot.resident_chunks.len();
    let mut min_satiety = snapshot
        .characters
        .iter()
        .map(|character| character.satiety)
        .min()
        .unwrap_or(0);
    let mut reloaded_while_carrying = false;
    let mut elapsed = 0_u64;

    while elapsed < ACTIVITY_SMOKE_TICKS {
        let remaining = ACTIVITY_SMOKE_TICKS - elapsed;
        let step = if reloaded_while_carrying {
            remaining.min(128)
        } else {
            1
        };
        application.execute(Command::AdvanceTicks { count: step })?;
        elapsed += step;
        snapshot = application.snapshot(SnapshotQuery {
            chunks: chunks.clone(),
            ..SnapshotQuery::default()
        })?;
        validate_activity_snapshot(&snapshot)?;
        max_jobs = max_jobs.max(snapshot.jobs.len());
        max_carried = max_carried.max(snapshot.carried_items.len());
        max_resident_chunks = max_resident_chunks.max(snapshot.resident_chunks.len());
        min_satiety = min_satiety.min(
            snapshot
                .characters
                .iter()
                .map(|character| character.satiety)
                .min()
                .unwrap_or(0),
        );
        if snapshot
            .characters
            .iter()
            .any(|character| character.satiety == 0)
        {
            return Err(Box::new(CliError(format!(
                "activity smoke starved a character at tick {}",
                snapshot.tick.value()
            ))));
        }

        if !reloaded_while_carrying && !snapshot.carried_items.is_empty() {
            let encoded = application.save_json()?;
            *application = Application::from_save_json(&encoded)?;
            let restored = application.snapshot(SnapshotQuery {
                chunks: chunks.clone(),
                ..SnapshotQuery::default()
            })?;
            validate_activity_snapshot(&restored)?;
            if restored.carried_items.is_empty() {
                return Err(Box::new(CliError(
                    "activity save/load lost the in-flight carried item".to_owned(),
                )));
            }
            reloaded_while_carrying = true;
        }

        if !reloaded_while_carrying && elapsed >= ACTIVITY_BOOTSTRAP_LIMIT {
            return Err(Box::new(CliError(format!(
                "activity smoke did not reach physical carrying within {ACTIVITY_BOOTSTRAP_LIMIT} ticks"
            ))));
        }
    }

    if !reloaded_while_carrying {
        return Err(Box::new(CliError(
            "activity smoke never exercised save/load during physical transport".to_owned(),
        )));
    }

    let tools = snapshot
        .ground_items
        .iter()
        .filter(|item| item.kind == ItemKind::PrimitiveTool)
        .map(|item| u64::from(item.quantity))
        .sum::<u64>()
        + snapshot
            .carried_items
            .iter()
            .filter(|item| item.kind == ItemKind::PrimitiveTool)
            .map(|item| u64::from(item.quantity))
            .sum::<u64>();
    if tools == 0 {
        return Err(Box::new(CliError(
            "activity smoke produced no PrimitiveTool output".to_owned(),
        )));
    }
    if snapshot.structures.len() < 3 {
        return Err(Box::new(CliError(format!(
            "activity smoke completed only {} of 3 StoneWall structures",
            snapshot.structures.len()
        ))));
    }

    println!(
        "activity seed={} tick={} designated_resources={} tools={} structures={} min_satiety={} max_jobs={} max_carried={} resident_chunks={} max_resident_chunks={} save_reload_while_carrying=true",
        seed,
        snapshot.tick.value(),
        designated_resources,
        tools,
        snapshot.structures.len(),
        min_satiety,
        max_jobs,
        max_carried,
        snapshot.resident_chunks.len(),
        max_resident_chunks
    );
    Ok(())
}

fn setup_activity_world(application: &mut Application) -> Result<usize, Box<dyn Error>> {
    application.execute(Command::PlaceWorkstation {
        kind: WorkstationKind::Workbench,
        cell: WorldCell::new(0, 1),
    })?;
    let workstation_id = application
        .snapshot(SnapshotQuery::default())?
        .workstations
        .first()
        .ok_or_else(|| CliError("activity workbench was not published".to_owned()))?
        .id;

    application.execute(Command::CreateStockpile {
        cell: WorldCell::new(-2, 0),
    })?;
    let stockpile_id = application
        .snapshot(SnapshotQuery::default())?
        .stockpiles
        .first()
        .ok_or_else(|| CliError("activity stockpile was not published".to_owned()))?
        .id;
    for cell in [WorldCell::new(-3, 0), WorldCell::new(2, 0)] {
        application.execute(Command::SetStockpileCell {
            stockpile_id,
            cell,
            enabled: true,
        })?;
    }

    application.execute(Command::AddProductionOrder {
        workstation_id,
        recipe_id: RecipeId::PrimitiveTool,
        target: ProductionTarget::Infinite,
    })?;

    let discovery = application.snapshot(SnapshotQuery {
        chunks: activity_chunks(),
        ..SnapshotQuery::default()
    })?;
    for cell in activity_construction_cells(&discovery, 3)? {
        application.execute(Command::DesignateConstruction {
            kind: StructureKind::StoneWall,
            cell,
        })?;
    }

    let resource_cells = discovery
        .natural_resources
        .iter()
        .map(|resource| resource.cell)
        .collect::<Vec<_>>();
    for source in &resource_cells {
        application.execute(Command::DesignateHarvest { source: *source })?;
    }
    Ok(resource_cells.len())
}

fn activity_construction_cells(
    snapshot: &ClientSnapshot,
    count: usize,
) -> Result<Vec<WorldCell>, CliError> {
    let mut occupied = BTreeSet::new();
    occupied.extend(snapshot.natural_resources.iter().map(|entry| entry.cell));
    occupied.extend(
        snapshot
            .characters
            .iter()
            .map(|entry| entry.containing_cell),
    );
    occupied.extend(
        snapshot
            .ground_items
            .iter()
            .map(|entry| entry.position.containing_cell()),
    );
    for stockpile in &snapshot.stockpiles {
        occupied.extend(stockpile.cells.iter().copied());
    }
    occupied.extend(snapshot.workstations.iter().map(|entry| entry.cell));
    for logistics in &snapshot.production_logistics {
        occupied.extend(logistics.input_cells.iter().copied());
        occupied.extend(logistics.output_cells.iter().copied());
    }
    occupied.extend(snapshot.construction_sites.iter().map(|entry| entry.cell));
    occupied.extend(snapshot.structures.iter().map(|entry| entry.cell));

    let mut candidates = Vec::new();
    for chunk in &snapshot.chunks {
        for y in 0..chunk.side {
            for x in 0..chunk.side {
                let local = LocalCell::new(x, y);
                if chunk.terrain_at(local) != Some(KnownTerrain::Known(Terrain::Grass)) {
                    continue;
                }
                let Some(cell) = chunk.coordinate.world_cell(local) else {
                    continue;
                };
                if !occupied.contains(&cell) {
                    candidates.push(cell);
                }
            }
        }
    }
    candidates.sort_by_key(|cell| {
        (
            i128::from(cell.x()).abs() + i128::from(cell.y()).abs(),
            cell.x(),
            cell.y(),
        )
    });
    candidates.truncate(count);
    if candidates.len() != count {
        return Err(CliError(format!(
            "activity smoke found only {} free explored construction cells; need {count}",
            candidates.len()
        )));
    }
    Ok(candidates)
}

fn activity_chunks() -> Vec<ChunkCoord> {
    vec![
        ChunkCoord::new(-1, -1),
        ChunkCoord::new(-1, 0),
        ChunkCoord::new(0, -1),
        ChunkCoord::new(0, 0),
    ]
}

fn validate_activity_snapshot(snapshot: &ClientSnapshot) -> Result<(), CliError> {
    validate_residency(snapshot)?;
    let mut owned_ids = BTreeSet::new();
    for (id, label) in snapshot
        .characters
        .iter()
        .map(|entry| (entry.id, "character"))
        .chain(
            snapshot
                .ground_items
                .iter()
                .map(|entry| (entry.id, "ground item")),
        )
        .chain(
            snapshot
                .carried_items
                .iter()
                .map(|entry| (entry.id, "carried item")),
        )
        .chain(snapshot.jobs.iter().map(|entry| (entry.id, "job")))
        .chain(
            snapshot
                .stockpiles
                .iter()
                .map(|entry| (entry.id, "stockpile")),
        )
        .chain(
            snapshot
                .workstations
                .iter()
                .map(|entry| (entry.id, "workstation")),
        )
        .chain(
            snapshot
                .production_orders
                .iter()
                .map(|entry| (entry.id, "production order")),
        )
        .chain(
            snapshot
                .construction_sites
                .iter()
                .map(|entry| (entry.id, "construction site")),
        )
        .chain(
            snapshot
                .structures
                .iter()
                .map(|entry| (entry.id, "structure")),
        )
    {
        if !owned_ids.insert(id) {
            return Err(CliError(format!(
                "activity snapshot contains duplicate globally-owned entity ID {} at {label}",
                id.value()
            )));
        }
    }
    if snapshot.ground_items.iter().any(|item| item.quantity == 0)
        || snapshot.carried_items.iter().any(|item| item.quantity == 0)
    {
        return Err(CliError(
            "activity snapshot contains a zero-quantity physical item stack".to_owned(),
        ));
    }
    Ok(())
}

fn select_direction(
    application: &Application,
    position: WorldCell,
    visit_counts: &BTreeMap<WorldCell, u64>,
) -> Result<Direction, &'static str> {
    let candidates = DIRECTIONS
        .into_iter()
        .filter_map(|direction| direction.adjacent(position).map(|cell| (direction, cell)))
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err("no adjacent world cell is representable");
    }

    candidates
        .into_iter()
        .filter(|(_, cell)| {
            application
                .known_terrain_at(*cell)
                .is_ok_and(|terrain| terrain == Some(Terrain::Grass))
        })
        .min_by(|(_, first), (_, second)| {
            visit_counts
                .get(first)
                .copied()
                .unwrap_or(0)
                .cmp(&visit_counts.get(second).copied().unwrap_or(0))
                .then(Ordering::Equal)
        })
        .map(|(direction, _)| direction)
        .ok_or("no adjacent grass cell is available")
}

fn validate_residency(snapshot: &ClientSnapshot) -> Result<(), CliError> {
    let bound = snapshot
        .characters
        .len()
        .checked_mul(RESIDENT_CHUNKS_PER_CENTER)
        .ok_or_else(|| CliError("resident chunk bound overflowed".to_owned()))?;
    if snapshot.resident_chunks.len() > bound {
        return Err(CliError(format!(
            "resident chunk count {} exceeds bound {bound}",
            snapshot.resident_chunks.len()
        )));
    }
    for character in &snapshot.characters {
        let chunk = character.containing_cell.split().0;
        if snapshot.resident_chunks.binary_search(&chunk).is_err() {
            return Err(CliError(format!(
                "character {} occupies non-resident chunk ({}, {})",
                character.id.value(),
                chunk.x(),
                chunk.y()
            )));
        }
    }
    Ok(())
}

fn character_exact_position(
    snapshot: &ClientSnapshot,
    id: EntityId,
) -> Result<WorldPosition, CliError> {
    snapshot
        .characters
        .iter()
        .find(|character| character.id == id)
        .map(|character| character.position)
        .ok_or_else(|| {
            CliError(format!(
                "character ID {} is missing from snapshot",
                id.value()
            ))
        })
}

fn travel_failure(
    seed: u64,
    position: WorldCell,
    max_chunk_x: i64,
    crossed_boundaries: u64,
    steps: u64,
    reason: &str,
) -> CliError {
    CliError(format!(
        "travel failed: {reason}; seed={seed} character_id={} position=({}, {}) max_chunk_x={max_chunk_x} crossed_boundaries={crossed_boundaries} steps={steps}",
        WALKER_CHARACTER_ID.value(),
        position.x(),
        position.y()
    ))
}

fn parse_arguments(arguments: impl IntoIterator<Item = String>) -> Result<Config, CliError> {
    let mut arguments = arguments.into_iter();
    let mut seed = None;
    let mut ticks = None;
    let mut travel_chunks = None;
    let mut activity_smoke = false;

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--seed" => {
                if seed.is_some() {
                    return Err(CliError("duplicate --seed argument".to_owned()));
                }
                let value = arguments.next().ok_or_else(|| CliError(USAGE.to_owned()))?;
                seed = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| CliError(format!("invalid seed '{value}'")))?,
                );
            }
            "--ticks" => {
                if ticks.is_some() {
                    return Err(CliError("duplicate --ticks argument".to_owned()));
                }
                let value = arguments.next().ok_or_else(|| CliError(USAGE.to_owned()))?;
                ticks = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| CliError(format!("invalid tick count '{value}'")))?,
                );
            }
            "--travel-chunks" => {
                if travel_chunks.is_some() {
                    return Err(CliError("duplicate --travel-chunks argument".to_owned()));
                }
                let value = arguments.next().ok_or_else(|| CliError(USAGE.to_owned()))?;
                let count = value
                    .parse::<u64>()
                    .map_err(|_| CliError(format!("invalid travel chunk count '{value}'")))?;
                if count == 0 {
                    return Err(CliError("travel chunk count must be positive".to_owned()));
                }
                travel_chunks = Some(count);
            }
            "--activity-smoke" => {
                if activity_smoke {
                    return Err(CliError("duplicate --activity-smoke argument".to_owned()));
                }
                activity_smoke = true;
            }
            unknown => return Err(CliError(format!("unknown argument '{unknown}'"))),
        }
    }

    let scenario = match (ticks, travel_chunks, activity_smoke) {
        (Some(count), None, false) => Scenario::AdvanceTicks(count),
        (None, Some(count), false) => Scenario::TravelChunks(count),
        (None, None, true) => Scenario::ActivitySmoke,
        (None, None, false) => return Err(CliError(USAGE.to_owned())),
        _ => {
            return Err(CliError(
                "choose exactly one of --ticks, --travel-chunks, or --activity-smoke".to_owned(),
            ));
        }
    };

    Ok(Config {
        seed: seed.ok_or_else(|| CliError(USAGE.to_owned()))?,
        scenario,
    })
}
