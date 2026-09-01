use std::error::Error;
use std::fmt::{self, Display, Formatter};

mod read_model;

use progressus_sim::{Simulation, SimulationError};

pub use progressus_sim::{
    CHUNK_SIDE, ChunkCoord, DEFAULT_CHARACTER_SPEED, Direction, EntityId, InteractionRadius,
    LocalCell, MovementSpeed, MovementState, SUBUNITS_PER_CELL, SimulationTick, Terrain, WorldCell,
    WorldPosition, WorldSeed, WorldgenVersion,
};
pub use read_model::{
    CharacterSnapshot, ChunkSnapshot, ClientSnapshot, KnownTerrain, NavigationSnapshot,
};

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
    MoveTo {
        character_id: EntityId,
        destination: WorldPosition,
    },
    StopMovement {
        character_id: EntityId,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SnapshotQuery {
    pub chunks: Vec<ChunkCoord>,
    pub navigation_for: Option<EntityId>,
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
            Command::MoveTo {
                character_id,
                destination,
            } => self.simulation.move_to(character_id, destination)?,
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
            .filter_map(|coordinate| {
                let any_known = (0..CHUNK_SIDE).any(|y| {
                    (0..CHUNK_SIDE).any(|x| {
                        let cell = coordinate
                            .world_cell(LocalCell::new(x, y))
                            .expect("valid chunk-local cells produce world cells");
                        self.simulation.is_explored(cell)
                    })
                });
                if !any_known {
                    return None;
                }
                let effective = match self.simulation.effective_chunk(coordinate) {
                    Ok(chunk) => chunk,
                    Err(error) => return Some(Err(error)),
                };
                let cells = (0..CHUNK_SIDE)
                    .flat_map(|y| (0..CHUNK_SIDE).map(move |x| LocalCell::new(x, y)))
                    .map(|local| {
                        let cell = coordinate
                            .world_cell(local)
                            .expect("valid chunk-local cells produce world cells");
                        if self.simulation.is_explored(cell) {
                            KnownTerrain::Known(
                                effective
                                    .terrain_at(local)
                                    .expect("effective chunks contain every local cell"),
                            )
                        } else {
                            KnownTerrain::Unknown
                        }
                    })
                    .collect();
                Some(Ok(ChunkSnapshot {
                    coordinate,
                    side: CHUNK_SIDE,
                    cells,
                }))
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
            exploration_revision: self.simulation.exploration_revision(),
            chunks,
            characters,
            navigation: query.navigation_for.and_then(|id| {
                self.simulation
                    .characters()
                    .find(|character| character.id() == id)
                    .map(NavigationSnapshot::from)
            }),
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
                ..SnapshotQuery::default()
            })
            .unwrap();

        assert_eq!(
            snapshot.chunks[0].known_terrain_at(local),
            Some(Terrain::Rock)
        );
    }

    #[test]
    fn snapshot_publishes_newly_explored_terrain_only_after_authoritative_movement() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        for x in 0..=3 {
            simulation
                .set_terrain_override(WorldCell::new(x, 0), Terrain::Grass)
                .unwrap();
        }
        let mut application = Application::from_simulation_for_test(simulation);
        let coordinate = ChunkCoord::new(0, 0);
        let newly_visible = LocalCell::new(8, 0);

        assert_eq!(
            application
                .snapshot(SnapshotQuery {
                    chunks: vec![coordinate],
                    ..SnapshotQuery::default()
                })
                .unwrap()
                .chunks[0]
                .terrain_at(newly_visible),
            Some(KnownTerrain::Unknown)
        );

        application
            .execute(Command::SetMovementDirection {
                character_id: EntityId::new(3).unwrap(),
                direction: Direction::East,
            })
            .unwrap();
        application
            .execute(Command::AdvanceTicks { count: 12 })
            .unwrap();

        assert!(
            application
                .snapshot(SnapshotQuery {
                    chunks: vec![coordinate],
                    ..SnapshotQuery::default()
                })
                .unwrap()
                .chunks[0]
                .known_terrain_at(newly_visible)
                .is_some()
        );
    }
}
