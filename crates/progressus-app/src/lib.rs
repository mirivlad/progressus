use std::error::Error;
use std::fmt::{self, Display, Formatter};

mod read_model;

use progressus_sim::{Simulation, SimulationError};

pub use progressus_sim::{
    CHUNK_SIDE, ChunkCoord, DEFAULT_CHARACTER_SPEED, Direction, EntityId, InteractionRadius,
    LocalCell, MovementSpeed, MovementState, SUBUNITS_PER_CELL, SimulationTick, Terrain, WorldCell,
    WorldPosition, WorldSeed, WorldgenVersion,
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
                    .effective_chunk(coordinate)
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

#[cfg(test)]
impl Application {
    fn from_simulation_for_test(simulation: Simulation) -> Self {
        Self { simulation }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_returns_effective_terrain_without_mutating_raw_worldgen() {
        let mut simulation = Simulation::new(WorldSeed::new(42)).unwrap();
        let position = WorldCell::new(0, 0);
        let (coordinate, local) = position.split();

        assert_eq!(
            simulation
                .generated_chunk(coordinate)
                .unwrap()
                .terrain_at(local),
            Some(Terrain::Grass),
        );
        simulation
            .set_terrain_override(position, Terrain::Rock)
            .unwrap();
        assert_eq!(
            simulation
                .generated_chunk(coordinate)
                .unwrap()
                .terrain_at(local),
            Some(Terrain::Grass),
        );

        let application = Application::from_simulation_for_test(simulation);
        let snapshot = application
            .snapshot(SnapshotQuery {
                chunks: vec![coordinate],
            })
            .unwrap();

        assert_eq!(snapshot.chunks[0].terrain_at(local), Some(Terrain::Rock));
    }
}
