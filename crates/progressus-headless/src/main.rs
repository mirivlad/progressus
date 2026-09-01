use std::env;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::process::ExitCode;
use std::{cmp::Ordering, collections::BTreeMap};

use progressus_app::{
    Application, ChunkCoord, ClientSnapshot, Command, DEFAULT_CHARACTER_SPEED, Direction, EntityId,
    NewGameOptions, SUBUNITS_PER_CELL, SnapshotQuery, Terrain, WorldCell, WorldPosition, WorldSeed,
};

const USAGE: &str =
    "usage: progressus-headless --seed <u64> (--ticks <u64> | --travel-chunks <positive u64>)";
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

    println!(
        "seed={} tick={} worldgen_version={} chunks={} characters={}",
        seed,
        snapshot.tick.value(),
        snapshot.worldgen_version.value(),
        snapshot.chunks.len(),
        snapshot.characters.len()
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
    for terrain in snapshot.chunks.iter().flat_map(|chunk| chunk.cells.iter()) {
        match terrain {
            Terrain::Grass => grass += 1,
            Terrain::Water => water += 1,
            Terrain::Rock => rock += 1,
        }
    }
    println!("terrain grass={grass} water={water} rock={rock}");

    Ok(())
}

fn run_travel(
    application: &mut Application,
    seed: u64,
    requested_boundaries: u64,
) -> Result<(), Box<dyn Error>> {
    let mut snapshot = application.snapshot(SnapshotQuery::default())?;
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
                "travel character_id={} seed={} start_chunk_x={} max_chunk_x={} crossed_boundaries={} steps={} position=({}, {})",
                WALKER_CHARACTER_ID.value(),
                seed,
                start_chunk.x(),
                max_chunk_x,
                crossed_boundaries,
                steps + 1,
                position.x(),
                position.y()
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

    let mut chunks = candidates
        .iter()
        .map(|(_, cell)| cell.split().0)
        .collect::<Vec<_>>();
    chunks.sort_unstable();
    chunks.dedup();
    let snapshot = application
        .snapshot(SnapshotQuery {
            chunks,
            ..SnapshotQuery::default()
        })
        .map_err(|_| "terrain snapshot query failed")?;

    candidates
        .into_iter()
        .filter(|(_, cell)| terrain_at(&snapshot, *cell) == Some(Terrain::Grass))
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

fn terrain_at(snapshot: &ClientSnapshot, cell: WorldCell) -> Option<Terrain> {
    let (coordinate, local) = cell.split();
    snapshot
        .chunks
        .iter()
        .find(|chunk| chunk.coordinate == coordinate)
        .and_then(|chunk| chunk.terrain_at(local))
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
            unknown => return Err(CliError(format!("unknown argument '{unknown}'"))),
        }
    }

    let scenario = match (ticks, travel_chunks) {
        (Some(count), None) => Scenario::AdvanceTicks(count),
        (None, Some(count)) => Scenario::TravelChunks(count),
        (None, None) => return Err(CliError(USAGE.to_owned())),
        (Some(_), Some(_)) => {
            return Err(CliError(
                "choose either --ticks or --travel-chunks, not both".to_owned(),
            ));
        }
    };

    Ok(Config {
        seed: seed.ok_or_else(|| CliError(USAGE.to_owned()))?,
        scenario,
    })
}
