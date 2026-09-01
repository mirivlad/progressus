#[cfg(test)]
use std::cell::Cell;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use progressus_worldgen::{WorldGenerator, WorldgenError};

use crate::clock::SimulationClock;
use crate::entity::EntityIdAllocator;
use crate::world_state::ModifiedWorld;
use crate::{
    CHUNK_SIDE, CURRENT_WORLDGEN_VERSION, Character, ChunkCoord, Direction, EffectiveChunk,
    EntityId, GeneratedChunk, LocalCell, MovementState, SimulationTick, Terrain, WorldCell,
    WorldSeed, WorldgenVersion,
};

const INITIAL_CHARACTERS: [(&str, i64); 5] = [
    ("Ada", -2),
    ("Borin", -1),
    ("Cora", 0),
    ("Dain", 1),
    ("Elin", 2),
];

#[derive(Clone, Debug)]
pub struct Simulation {
    generator: WorldGenerator,
    clock: SimulationClock,
    id_allocator: EntityIdAllocator,
    characters: BTreeMap<EntityId, Character>,
    modified_world: ModifiedWorld,
    #[cfg(test)]
    base_terrain_query_count: Cell<u64>,
}

impl Simulation {
    pub fn new(seed: WorldSeed) -> Result<Self, SimulationError> {
        let generator = WorldGenerator::new(seed, CURRENT_WORLDGEN_VERSION)?;
        let mut id_allocator = EntityIdAllocator::new();
        let mut characters = BTreeMap::new();

        for (name, x) in INITIAL_CHARACTERS {
            let position = WorldCell::new(x, 0);
            let (chunk_coordinate, local) = position.split();
            let chunk = generator.generate(chunk_coordinate)?;
            if chunk.terrain_at(local) != Some(Terrain::Grass) {
                return Err(SimulationError::SpawnNotWalkable(position));
            }

            let id = id_allocator.allocate()?;
            let character = Character::new(id, name, position);
            if characters.insert(id, character).is_some() {
                return Err(SimulationError::DuplicateEntityId(id));
            }
        }

        Ok(Self {
            generator,
            clock: SimulationClock::new(0),
            id_allocator,
            characters,
            modified_world: ModifiedWorld::default(),
            #[cfg(test)]
            base_terrain_query_count: Cell::new(0),
        })
    }

    pub const fn tick(&self) -> SimulationTick {
        self.clock.tick()
    }

    pub const fn worldgen_version(&self) -> WorldgenVersion {
        self.generator.version()
    }

    pub fn next_entity_id(&self) -> Option<EntityId> {
        self.id_allocator.peek()
    }

    pub fn advance_ticks(&mut self, count: u64) -> Result<(), SimulationError> {
        self.clock
            .tick()
            .value()
            .checked_add(count)
            .ok_or(SimulationError::TickOverflow)?;

        for _ in 0..count {
            self.clock.advance(1)?;
            self.advance_characters_one_tick()?;
        }

        Ok(())
    }

    pub fn set_movement_direction(
        &mut self,
        id: EntityId,
        direction: Direction,
    ) -> Result<(), SimulationError> {
        let position = self
            .characters
            .get(&id)
            .ok_or(SimulationError::UnknownCharacter(id))?
            .position();
        let target = direction
            .adjacent(position)
            .ok_or(SimulationError::MovementCoordinateOverflow(position))?;
        if !self.is_walkable(target)? {
            return Err(SimulationError::MovementDestinationBlocked(target));
        }

        self.characters
            .get_mut(&id)
            .expect("character was checked above")
            .set_movement(MovementState::Moving { direction });
        Ok(())
    }

    pub fn stop_movement(&mut self, id: EntityId) -> Result<(), SimulationError> {
        self.characters
            .get_mut(&id)
            .ok_or(SimulationError::UnknownCharacter(id))?
            .set_movement(MovementState::Idle);
        Ok(())
    }

    pub fn characters(&self) -> impl ExactSizeIterator<Item = &Character> {
        self.characters.values()
    }

    pub fn generated_chunk(
        &self,
        coordinate: ChunkCoord,
    ) -> Result<GeneratedChunk, SimulationError> {
        self.generator.generate(coordinate).map_err(Into::into)
    }

    pub fn set_terrain_override(
        &mut self,
        position: WorldCell,
        terrain: Terrain,
    ) -> Result<(), SimulationError> {
        let (coordinate, local) = position.split();
        let base = self.base_terrain_at(position)?;
        self.modified_world
            .set_override(coordinate, local, base, terrain);
        Ok(())
    }

    pub fn effective_terrain_at(&self, position: WorldCell) -> Result<Terrain, SimulationError> {
        let (coordinate, local) = position.split();
        if let Some(override_terrain) = self.modified_world.override_at(coordinate, local) {
            return Ok(override_terrain);
        }

        self.base_terrain_at(position)
    }

    pub fn effective_chunk(
        &self,
        coordinate: ChunkCoord,
    ) -> Result<EffectiveChunk, SimulationError> {
        let generated = self.generated_chunk(coordinate)?;
        let mut cells = Vec::with_capacity(usize::from(CHUNK_SIDE).pow(2));

        for y in 0..CHUNK_SIDE {
            for x in 0..CHUNK_SIDE {
                let local = LocalCell::new(x, y);
                let base = generated
                    .terrain_at(local)
                    .expect("generated chunks contain every valid local cell");
                cells.push(self.resolve_terrain(coordinate, local, base));
            }
        }

        Ok(EffectiveChunk::new(coordinate, cells))
    }

    fn base_terrain_at(&self, position: WorldCell) -> Result<Terrain, SimulationError> {
        #[cfg(test)]
        self.base_terrain_query_count
            .set(self.base_terrain_query_count.get() + 1);

        let (coordinate, local) = position.split();
        self.generated_chunk(coordinate)?
            .terrain_at(local)
            .ok_or(SimulationError::Worldgen(
                WorldgenError::CoordinateOutOfRange(coordinate),
            ))
    }

    fn resolve_terrain(&self, coordinate: ChunkCoord, local: LocalCell, base: Terrain) -> Terrain {
        self.modified_world
            .override_at(coordinate, local)
            .unwrap_or(base)
    }

    fn advance_characters_one_tick(&mut self) -> Result<(), SimulationError> {
        let ids = self.characters.keys().copied().collect::<Vec<_>>();
        for id in ids {
            let (position, direction) = {
                let character = self
                    .characters
                    .get(&id)
                    .expect("character ID came from the character map");
                match character.movement() {
                    MovementState::Idle => continue,
                    MovementState::Moving { direction } => (character.position(), direction),
                }
            };

            let Some(target) = direction.adjacent(position) else {
                self.characters
                    .get_mut(&id)
                    .expect("character ID came from the character map")
                    .set_movement(MovementState::Idle);
                continue;
            };

            if self.is_walkable(target)? {
                self.characters
                    .get_mut(&id)
                    .expect("character ID came from the character map")
                    .set_position(target);
            } else {
                self.characters
                    .get_mut(&id)
                    .expect("character ID came from the character map")
                    .set_movement(MovementState::Idle);
            }
        }

        Ok(())
    }

    fn is_walkable(&self, position: WorldCell) -> Result<bool, SimulationError> {
        Ok(self.effective_terrain_at(position)? == Terrain::Grass)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SimulationError {
    TickOverflow,
    EntityIdExhausted,
    DuplicateEntityId(EntityId),
    SpawnNotWalkable(WorldCell),
    UnknownCharacter(EntityId),
    MovementCoordinateOverflow(WorldCell),
    MovementDestinationBlocked(WorldCell),
    Worldgen(WorldgenError),
}

impl Display for SimulationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::TickOverflow => formatter.write_str("simulation tick overflow"),
            Self::EntityIdExhausted => formatter.write_str("stable entity ID space exhausted"),
            Self::DuplicateEntityId(id) => {
                write!(formatter, "duplicate stable entity ID {}", id.value())
            }
            Self::SpawnNotWalkable(position) => write!(
                formatter,
                "initial character position ({}, {}) is not walkable",
                position.x(),
                position.y()
            ),
            Self::UnknownCharacter(id) => write!(formatter, "unknown character ID {}", id.value()),
            Self::MovementCoordinateOverflow(position) => write!(
                formatter,
                "movement from ({}, {}) exceeds the world-cell coordinate range",
                position.x(),
                position.y()
            ),
            Self::MovementDestinationBlocked(position) => write!(
                formatter,
                "movement destination ({}, {}) is not walkable",
                position.x(),
                position.y()
            ),
            Self::Worldgen(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for SimulationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Worldgen(error) => Some(error),
            _ => None,
        }
    }
}

impl From<WorldgenError> for SimulationError {
    fn from(error: WorldgenError) -> Self {
        Self::Worldgen(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Direction, MovementState};

    fn cora() -> EntityId {
        EntityId::new(3).unwrap()
    }

    #[test]
    fn movement_crosses_positive_and_negative_chunk_boundaries_with_stable_identity() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();

        let cora = cora();
        place_on_grass(&mut simulation, cora, WorldCell::new(31, 0));
        simulation
            .set_movement_direction(cora, Direction::East)
            .unwrap();
        simulation.advance_ticks(1).unwrap();
        assert_eq!(character(&simulation, cora).id(), cora);
        assert_eq!(
            character(&simulation, cora).position(),
            WorldCell::new(32, 0)
        );

        place_on_grass(&mut simulation, cora, WorldCell::new(0, 0));
        simulation
            .set_movement_direction(cora, Direction::West)
            .unwrap();
        simulation.advance_ticks(1).unwrap();
        assert_eq!(character(&simulation, cora).id(), cora);
        assert_eq!(
            character(&simulation, cora).position(),
            WorldCell::new(-1, 0)
        );
    }

    #[test]
    fn invalid_replacement_preserves_existing_direction() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let (position, accepted, blocked) = find_replacement_fixture(&simulation);
        place_on_grass(&mut simulation, cora, position);

        simulation.set_movement_direction(cora, accepted).unwrap();
        assert!(matches!(
            simulation.set_movement_direction(cora, blocked),
            Err(SimulationError::MovementDestinationBlocked(_))
        ));
        assert_eq!(
            character(&simulation, cora).movement(),
            MovementState::Moving {
                direction: accepted
            }
        );
    }

    #[test]
    fn replacement_direction_starts_from_current_post_tick_cell() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let position = find_turn_fixture(&simulation);
        place_on_grass(&mut simulation, cora, position);

        simulation
            .set_movement_direction(cora, Direction::East)
            .unwrap();
        simulation.advance_ticks(1).unwrap();
        simulation
            .set_movement_direction(cora, Direction::North)
            .unwrap();
        simulation.advance_ticks(1).unwrap();

        assert_eq!(
            character(&simulation, cora).position(),
            WorldCell::new(position.x() + 1, position.y() + 1)
        );
        assert_eq!(
            character(&simulation, cora).movement(),
            MovementState::Moving {
                direction: Direction::North
            }
        );
    }

    #[test]
    fn persisted_movement_stops_on_water_without_changing_position() {
        persisted_movement_stops_on(Terrain::Water);
    }

    #[test]
    fn persisted_movement_stops_on_rock_without_changing_position() {
        persisted_movement_stops_on(Terrain::Rock);
    }

    #[test]
    fn persisted_movement_stops_on_coordinate_overflow_without_wrapping() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let controlled_character = simulation.characters.get_mut(&cora).unwrap();
        controlled_character.set_position(WorldCell::new(i64::MAX, 0));
        controlled_character.set_movement(MovementState::Moving {
            direction: Direction::East,
        });

        simulation.advance_ticks(1).unwrap();

        assert_eq!(
            character(&simulation, cora).position(),
            WorldCell::new(i64::MAX, 0)
        );
        assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
    }

    #[test]
    fn identical_direction_commands_produce_identical_authoritative_state() {
        let mut first = Simulation::new(WorldSeed::new(2)).unwrap();
        let mut second = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();

        for simulation in [&mut first, &mut second] {
            simulation
                .set_movement_direction(cora, Direction::East)
                .unwrap();
            simulation.advance_ticks(8).unwrap();
            simulation.stop_movement(cora).unwrap();
            simulation
                .set_movement_direction(cora, Direction::West)
                .unwrap();
            simulation.advance_ticks(3).unwrap();
        }

        assert_eq!(first.tick(), second.tick());
        assert_eq!(
            first.characters().cloned().collect::<Vec<_>>(),
            second.characters().cloned().collect::<Vec<_>>()
        );
    }

    #[test]
    fn effective_terrain_point_lookup_uses_override_without_base_query() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let position = WorldCell::new(0, 0);

        simulation
            .set_terrain_override(position, Terrain::Rock)
            .unwrap();
        simulation.base_terrain_query_count.set(0);

        assert_eq!(
            simulation.effective_terrain_at(position).unwrap(),
            Terrain::Rock
        );
        assert_eq!(simulation.base_terrain_query_count.get(), 0);
    }

    #[test]
    fn grass_overridden_to_blocked_terrain_blocks_validation_and_persisted_step() {
        for blocked in [Terrain::Rock, Terrain::Water] {
            let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
            let (start, direction) = find_raw_grass_with_neighbor(&simulation, Terrain::Grass);
            let target = direction.adjacent(start).unwrap();
            let cora = cora();
            place_on_grass(&mut simulation, cora, start);

            simulation.set_terrain_override(target, blocked).unwrap();
            assert_eq!(
                simulation.set_movement_direction(cora, direction),
                Err(SimulationError::MovementDestinationBlocked(target)),
            );

            simulation
                .set_terrain_override(target, Terrain::Grass)
                .unwrap();
            simulation.set_movement_direction(cora, direction).unwrap();
            simulation.set_terrain_override(target, blocked).unwrap();
            simulation.advance_ticks(1).unwrap();

            assert_eq!(character(&simulation, cora).position(), start);
            assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
        }
    }

    #[test]
    fn blocked_terrain_overridden_to_grass_allows_validation_and_step() {
        for blocked in [Terrain::Water, Terrain::Rock] {
            let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
            let (start, direction) = find_raw_grass_with_neighbor(&simulation, blocked);
            let target = direction.adjacent(start).unwrap();
            let cora = cora();
            place_on_grass(&mut simulation, cora, start);

            simulation
                .set_terrain_override(target, Terrain::Grass)
                .unwrap();
            simulation.set_movement_direction(cora, direction).unwrap();
            simulation.advance_ticks(1).unwrap();
            assert_eq!(character(&simulation, cora).position(), target);

            simulation.stop_movement(cora).unwrap();
            simulation.set_terrain_override(target, blocked).unwrap();
            simulation
                .characters
                .get_mut(&cora)
                .unwrap()
                .set_position(start);
            assert_eq!(
                simulation.set_movement_direction(cora, direction),
                Err(SimulationError::MovementDestinationBlocked(target)),
            );
        }
    }

    fn character(simulation: &Simulation, id: EntityId) -> &Character {
        simulation.characters.get(&id).unwrap()
    }

    fn place_on_grass(simulation: &mut Simulation, id: EntityId, position: WorldCell) {
        assert_eq!(
            simulation.effective_terrain_at(position).unwrap(),
            Terrain::Grass
        );
        simulation
            .characters
            .get_mut(&id)
            .unwrap()
            .set_position(position);
    }

    fn persisted_movement_stops_on(terrain: Terrain) {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let (position, direction) = find_adjacent_terrain_fixture(&simulation, terrain);
        place_on_grass(&mut simulation, cora, position);
        simulation
            .characters
            .get_mut(&cora)
            .unwrap()
            .set_movement(MovementState::Moving { direction });

        simulation.advance_ticks(1).unwrap();

        assert_eq!(character(&simulation, cora).position(), position);
        assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
    }

    fn find_replacement_fixture(simulation: &Simulation) -> (WorldCell, Direction, Direction) {
        for y in -64..=64 {
            for x in -64..=64 {
                let position = WorldCell::new(x, y);
                if raw_terrain_at(simulation, position) != Terrain::Grass {
                    continue;
                }
                for accepted in [
                    Direction::East,
                    Direction::West,
                    Direction::North,
                    Direction::South,
                ] {
                    if raw_terrain_at(simulation, accepted.adjacent(position).unwrap())
                        != Terrain::Grass
                    {
                        continue;
                    }
                    for blocked in [
                        Direction::East,
                        Direction::West,
                        Direction::North,
                        Direction::South,
                    ] {
                        if raw_terrain_at(simulation, blocked.adjacent(position).unwrap())
                            != Terrain::Grass
                        {
                            return (position, accepted, blocked);
                        }
                    }
                }
            }
        }
        panic!("expected an adjacent grass and blocked terrain fixture");
    }

    fn find_turn_fixture(simulation: &Simulation) -> WorldCell {
        for y in -64..=64 {
            for x in -64..=64 {
                let position = WorldCell::new(x, y);
                let east = Direction::East.adjacent(position).unwrap();
                let north_from_east = Direction::North.adjacent(east).unwrap();
                if [position, east, north_from_east]
                    .into_iter()
                    .all(|cell| raw_terrain_at(simulation, cell) == Terrain::Grass)
                {
                    return position;
                }
            }
        }
        panic!("expected an east then north grass fixture");
    }

    fn find_adjacent_terrain_fixture(
        simulation: &Simulation,
        target_terrain: Terrain,
    ) -> (WorldCell, Direction) {
        for y in -64..=64 {
            for x in -64..=64 {
                let position = WorldCell::new(x, y);
                if raw_terrain_at(simulation, position) != Terrain::Grass {
                    continue;
                }
                for direction in [
                    Direction::East,
                    Direction::West,
                    Direction::North,
                    Direction::South,
                ] {
                    if raw_terrain_at(simulation, direction.adjacent(position).unwrap())
                        == target_terrain
                    {
                        return (position, direction);
                    }
                }
            }
        }
        panic!("expected adjacent {target_terrain:?} terrain fixture");
    }

    fn raw_terrain_at(simulation: &Simulation, position: WorldCell) -> Terrain {
        let (coordinate, local) = position.split();
        simulation
            .generated_chunk(coordinate)
            .unwrap()
            .terrain_at(local)
            .unwrap()
    }

    fn find_raw_grass_with_neighbor(
        simulation: &Simulation,
        neighbor: Terrain,
    ) -> (WorldCell, Direction) {
        for y in -64..=64 {
            for x in -64..=64 {
                let start = WorldCell::new(x, y);
                if raw_terrain_at(simulation, start) != Terrain::Grass {
                    continue;
                }
                for direction in [
                    Direction::East,
                    Direction::West,
                    Direction::North,
                    Direction::South,
                ] {
                    let target = direction.adjacent(start).unwrap();
                    if raw_terrain_at(simulation, target) == neighbor {
                        return (start, direction);
                    }
                }
            }
        }
        panic!("expected raw grass next to {neighbor:?}");
    }
}
