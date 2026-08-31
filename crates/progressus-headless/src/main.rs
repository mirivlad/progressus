use std::env;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::process::ExitCode;

use progressus_app::{
    Application, ChunkCoord, Command, NewGameOptions, SnapshotQuery, Terrain, WorldSeed,
};

const USAGE: &str = "usage: progressus-headless --seed <u64> --ticks <u64>";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Config {
    seed: u64,
    ticks: u64,
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
    application.execute(Command::AdvanceTicks {
        count: config.ticks,
    })?;
    let snapshot = application.snapshot(SnapshotQuery {
        chunks: vec![
            ChunkCoord::new(-1, 0),
            ChunkCoord::new(0, 0),
            ChunkCoord::new(1, 0),
        ],
    })?;

    println!(
        "seed={} tick={} worldgen_version={} chunks={} characters={}",
        config.seed,
        snapshot.tick.value(),
        snapshot.worldgen_version.value(),
        snapshot.chunks.len(),
        snapshot.characters.len()
    );
    for character in &snapshot.characters {
        println!(
            "character id={} name={} position=({}, {})",
            character.id.value(),
            character.name,
            character.position.x(),
            character.position.y()
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

fn parse_arguments(arguments: impl IntoIterator<Item = String>) -> Result<Config, CliError> {
    let mut arguments = arguments.into_iter();
    let mut seed = None;
    let mut ticks = None;

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
            unknown => return Err(CliError(format!("unknown argument '{unknown}'"))),
        }
    }

    Ok(Config {
        seed: seed.ok_or_else(|| CliError(USAGE.to_owned()))?,
        ticks: ticks.ok_or_else(|| CliError(USAGE.to_owned()))?,
    })
}
