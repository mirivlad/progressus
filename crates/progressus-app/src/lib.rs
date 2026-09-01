use std::error::Error;
use std::fmt::{self, Display, Formatter};

mod read_model;

use progressus_sim::{Simulation, SimulationError};

pub use progressus_sim::{
    CHUNK_SIDE, ChunkCoord, Direction, EntityId, LocalCell, MovementState, SimulationTick, Terrain,
    WorldCell, WorldSeed, WorldgenVersion,
};
pub use read_model::{CharacterSnapshot, ChunkSnapshot, ClientSnapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NewGameOptions {
    pub seed: WorldSeed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    AdvanceTicks {
        count: u64,
    },
    SetMovementDirection {
        character_id: EntityId,
        direction: Direction,
    },
    StopMovement {
        character_id: EntityId,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SnapshotQuery {
    pub chunks: Vec<ChunkCoord>,
}

#[derive(Clone, Debug)]
pub struct Application {
    simulation: Simulation,
}

impl Application {
    pub fn new_game(options: NewGameOptions) -> Result<Self, ApplicationError> {
        Ok(Self {
            simulation: Simulation::new(options.seed)?,
        })
    }

    pub fn execute(&mut self, command: Command) -> Result<(), ApplicationError> {
        match command {
            Command::AdvanceTicks { count } => self.simulation.advance_ticks(count)?,
            Command::SetMovementDirection {
                character_id,
                direction,
            } => self
                .simulation
                .set_movement_direction(character_id, direction)?,
            Command::StopMovement { character_id } => {
                self.simulation.stop_movement(character_id)?
            }
        }
        Ok(())
    }

    pub fn snapshot(&self, mut query: SnapshotQuery) -> Result<ClientSnapshot, ApplicationError> {
        query.chunks.sort_unstable();
        query.chunks.dedup();

        let chunks = query
            .chunks
            .into_iter()
            .map(|coordinate| {
                self.simulation
                    .generated_chunk(coordinate)
                    .map(ChunkSnapshot::from)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let characters = self
            .simulation
            .characters()
            .map(CharacterSnapshot::from)
            .collect();

        Ok(ClientSnapshot {
            tick: self.simulation.tick(),
            worldgen_version: self.simulation.worldgen_version(),
            chunks,
            characters,
        })
    }
}

#[derive(Debug)]
pub enum ApplicationError {
    Simulation(SimulationError),
}

impl Display for ApplicationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Simulation(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for ApplicationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Simulation(error) => Some(error),
        }
    }
}

impl From<SimulationError> for ApplicationError {
    fn from(error: SimulationError) -> Self {
        Self::Simulation(error)
    }
}
