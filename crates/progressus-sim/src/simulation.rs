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
    WorldPosition, WorldPositionError, WorldSeed, WorldgenVersion,
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
            let character = Character::new(id, name, WorldPosition::from_cell_center(position)?);
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
        self.characters
            .get_mut(&id)
            .ok_or(SimulationError::UnknownCharacter(id))?
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
            let (mut position, direction, mut remaining) = {
                let character = self
                    .characters
                    .get(&id)
                    .expect("character ID came from the character map");
                match character.movement() {
                    MovementState::Idle => continue,
                    MovementState::Moving { direction } => (
                        character.position(),
                        direction,
                        i128::from(character.speed().subunits_per_tick()),
                    ),
                }
            };

            while remaining > 0 {
                let source = position.containing_cell();
                let entry_distance = entry_distance(position, source, direction)?;
                let target_is_walkable = match direction.adjacent(source) {
                    Some(target) => self.is_walkable(target)?,
                    None => false,
                };

                if !target_is_walkable {
                    if remaining < entry_distance {
                        position = translate(position, direction, remaining)?;
                        self.characters
                            .get_mut(&id)
                            .expect("character ID came from the character map")
                            .set_position(position);
                    } else {
                        position = translate(position, direction, entry_distance - 1)?;
                        let character = self
                            .characters
                            .get_mut(&id)
                            .expect("character ID came from the character map");
                        character.set_position(position);
                        character.set_movement(MovementState::Idle);
                    }
                    break;
                }

                let distance = remaining.min(entry_distance);
                position = translate(position, direction, distance)?;
                self.characters
                    .get_mut(&id)
                    .expect("character ID came from the character map")
                    .set_position(position);
                remaining -= distance;
            }
        }

        Ok(())
    }

    fn is_walkable(&self, position: WorldCell) -> Result<bool, SimulationError> {
        Ok(self.effective_terrain_at(position)? == Terrain::Grass)
    }
}

fn entry_distance(
    position: WorldPosition,
    source: WorldCell,
    direction: Direction,
) -> Result<i128, SimulationError> {
    let lower_x = i128::from(source.x())
        .checked_mul(crate::SUBUNITS_PER_CELL)
        .ok_or(SimulationError::Position(
            WorldPositionError::OutsideWorldCellRange,
        ))?;
    let lower_y = i128::from(source.y())
        .checked_mul(crate::SUBUNITS_PER_CELL)
        .ok_or(SimulationError::Position(
            WorldPositionError::OutsideWorldCellRange,
        ))?;
    let upper_x =
        lower_x
            .checked_add(crate::SUBUNITS_PER_CELL)
            .ok_or(SimulationError::Position(
                WorldPositionError::OutsideWorldCellRange,
            ))?;
    let upper_y =
        lower_y
            .checked_add(crate::SUBUNITS_PER_CELL)
            .ok_or(SimulationError::Position(
                WorldPositionError::OutsideWorldCellRange,
            ))?;

    match direction {
        Direction::East => {
            upper_x
                .checked_sub(position.x_subunits())
                .ok_or(SimulationError::Position(
                    WorldPositionError::OutsideWorldCellRange,
                ))
        }
        Direction::West => position
            .x_subunits()
            .checked_sub(lower_x)
            .and_then(|distance| distance.checked_add(1))
            .ok_or(SimulationError::Position(
                WorldPositionError::OutsideWorldCellRange,
            )),
        Direction::North => {
            upper_y
                .checked_sub(position.y_subunits())
                .ok_or(SimulationError::Position(
                    WorldPositionError::OutsideWorldCellRange,
                ))
        }
        Direction::South => position
            .y_subunits()
            .checked_sub(lower_y)
            .and_then(|distance| distance.checked_add(1))
            .ok_or(SimulationError::Position(
                WorldPositionError::OutsideWorldCellRange,
            )),
    }
}

fn translate(
    position: WorldPosition,
    direction: Direction,
    distance: i128,
) -> Result<WorldPosition, SimulationError> {
    let (delta_x, delta_y) = match direction {
        Direction::East => (distance, 0),
        Direction::West => (-distance, 0),
        Direction::North => (0, distance),
        Direction::South => (0, -distance),
    };
    position
        .checked_translate(delta_x, delta_y)
        .map_err(Into::into)
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
    Position(WorldPositionError),
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
            Self::Position(_) => {
                formatter.write_str("world position is outside the representable world-cell range")
            }
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

impl From<WorldPositionError> for SimulationError {
    fn from(error: WorldPositionError) -> Self {
        Self::Position(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Direction, MovementSpeed, MovementState};

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
        simulation.advance_ticks(4).unwrap();
        assert_eq!(character(&simulation, cora).id(), cora);
        assert_eq!(
            character(&simulation, cora).position(),
            position_at_cell(WorldCell::new(32, 0))
        );

        place_on_grass(&mut simulation, cora, WorldCell::new(0, 0));
        simulation
            .set_movement_direction(cora, Direction::West)
            .unwrap();
        simulation.advance_ticks(4).unwrap();
        assert_eq!(character(&simulation, cora).id(), cora);
        assert_eq!(
            character(&simulation, cora).position(),
            position_at_cell(WorldCell::new(-1, 0))
        );
    }

    #[test]
    fn replacement_direction_is_accepted_without_terrain_prevalidation() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let (position, accepted, blocked) = find_replacement_fixture(&simulation);
        place_on_grass(&mut simulation, cora, position);

        simulation.set_movement_direction(cora, accepted).unwrap();
        simulation.set_movement_direction(cora, blocked).unwrap();
        assert_eq!(
            character(&simulation, cora).movement(),
            MovementState::Moving { direction: blocked }
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
            position_at_cell(position)
                .checked_translate(256, 256)
                .unwrap()
        );
        assert_eq!(
            character(&simulation, cora).movement(),
            MovementState::Moving {
                direction: Direction::North
            }
        );
    }

    #[test]
    fn persisted_movement_stops_on_water_at_last_valid_subunit() {
        persisted_movement_stops_on(Terrain::Water);
    }

    #[test]
    fn persisted_movement_stops_on_rock_at_last_valid_subunit() {
        persisted_movement_stops_on(Terrain::Rock);
    }

    #[test]
    fn persisted_movement_stops_on_coordinate_overflow_without_wrapping() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let controlled_character = simulation.characters.get_mut(&cora).unwrap();
        controlled_character.set_position(position_at_cell(WorldCell::new(i64::MAX, 0)));
        controlled_character.set_movement(MovementState::Moving {
            direction: Direction::East,
        });

        simulation.advance_ticks(3).unwrap();

        assert_eq!(
            character(&simulation, cora).position(),
            WorldPosition::from_subunits((i128::from(i64::MAX) + 1) * 1024 - 1, 512).unwrap()
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
    fn grass_overridden_to_blocked_terrain_stops_only_at_its_boundary() {
        for blocked in [Terrain::Rock, Terrain::Water] {
            let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
            let (start, direction) = find_raw_grass_with_neighbor(&simulation, Terrain::Grass);
            let target = direction.adjacent(start).unwrap();
            let cora = cora();
            place_on_grass(&mut simulation, cora, start);

            simulation.set_terrain_override(target, blocked).unwrap();
            simulation.set_movement_direction(cora, direction).unwrap();
            simulation.advance_ticks(2).unwrap();

            assert_eq!(
                character(&simulation, cora).position(),
                blocked_stop_position(start, direction)
            );
            assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
        }
    }

    #[test]
    fn blocked_terrain_overridden_to_grass_allows_continuous_step() {
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
            simulation.advance_ticks(4).unwrap();
            assert_eq!(
                character(&simulation, cora).position(),
                position_at_cell(target)
            );

            simulation.stop_movement(cora).unwrap();
            simulation.set_terrain_override(target, blocked).unwrap();
            simulation
                .characters
                .get_mut(&cora)
                .unwrap()
                .set_position(position_at_cell(start));
            simulation.set_movement_direction(cora, direction).unwrap();
            simulation.advance_ticks(2).unwrap();
            assert_eq!(
                character(&simulation, cora).position(),
                blocked_stop_position(start, direction)
            );
        }
    }

    #[test]
    fn cardinal_movement_advances_in_subcell_increments() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        place_on_grass(&mut simulation, cora, WorldCell::new(0, 0));
        simulation
            .set_movement_direction(cora, Direction::East)
            .unwrap();

        simulation.advance_ticks(1).unwrap();
        assert_eq!(character(&simulation, cora).position().x_subunits(), 768);
        assert_eq!(character(&simulation, cora).position().y_subunits(), 512);
        assert_eq!(
            character(&simulation, cora).movement(),
            MovementState::Moving {
                direction: Direction::East
            }
        );

        simulation.advance_ticks(3).unwrap();
        assert_eq!(
            character(&simulation, cora).position(),
            position_at_cell(WorldCell::new(1, 0))
        );
    }

    #[test]
    fn blocked_neighbour_allows_approach_then_stops_only_at_boundary() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(0, 0);
        let target = WorldCell::new(1, 0);
        place_on_grass(&mut simulation, cora, start);
        simulation
            .set_terrain_override(target, Terrain::Grass)
            .unwrap();

        simulation
            .set_movement_direction(cora, Direction::East)
            .unwrap();
        simulation.advance_ticks(1).unwrap();
        assert_eq!(character(&simulation, cora).position().x_subunits(), 768);
        assert!(matches!(
            character(&simulation, cora).movement(),
            MovementState::Moving {
                direction: Direction::East
            }
        ));

        simulation
            .set_terrain_override(target, Terrain::Rock)
            .unwrap();
        simulation.advance_ticks(1).unwrap();
        assert_eq!(character(&simulation, cora).position().x_subunits(), 1023);
        assert_eq!(
            character(&simulation, cora).position().containing_cell(),
            start
        );
        assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);

        simulation
            .set_movement_direction(cora, Direction::West)
            .unwrap();
        simulation.advance_ticks(1).unwrap();
        assert_eq!(character(&simulation, cora).position().x_subunits(), 767);
    }

    #[test]
    fn large_speed_consumes_multiple_passable_cell_transitions() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(0, 0);
        place_on_grass(&mut simulation, cora, start);
        for x in 1..=3 {
            simulation
                .set_terrain_override(WorldCell::new(x, 0), Terrain::Grass)
                .unwrap();
        }
        character_mut(&mut simulation, cora).set_speed(MovementSpeed::new(2_500).unwrap());
        simulation
            .set_movement_direction(cora, Direction::East)
            .unwrap();

        simulation.advance_ticks(1).unwrap();

        assert_eq!(character(&simulation, cora).position().x_subunits(), 3_012);
        assert_eq!(
            character(&simulation, cora).position().containing_cell(),
            WorldCell::new(2, 0)
        );
    }

    #[test]
    fn blocked_transitions_stop_at_canonical_boundaries_in_every_direction() {
        for (direction, start, target, expected) in [
            (
                Direction::East,
                WorldCell::new(0, 0),
                WorldCell::new(1, 0),
                (1023, 512),
            ),
            (
                Direction::West,
                WorldCell::new(0, 0),
                WorldCell::new(-1, 0),
                (0, 512),
            ),
            (
                Direction::North,
                WorldCell::new(0, 0),
                WorldCell::new(0, 1),
                (512, 1023),
            ),
            (
                Direction::South,
                WorldCell::new(0, 0),
                WorldCell::new(0, -1),
                (512, 0),
            ),
            (
                Direction::East,
                WorldCell::new(-2, -2),
                WorldCell::new(-1, -2),
                (-1025, -1536),
            ),
            (
                Direction::West,
                WorldCell::new(-2, -2),
                WorldCell::new(-3, -2),
                (-2048, -1536),
            ),
            (
                Direction::North,
                WorldCell::new(-2, -2),
                WorldCell::new(-2, -1),
                (-1536, -1025),
            ),
            (
                Direction::South,
                WorldCell::new(-2, -2),
                WorldCell::new(-2, -3),
                (-1536, -2048),
            ),
        ] {
            let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
            let cora = cora();
            simulation
                .set_terrain_override(start, Terrain::Grass)
                .unwrap();
            place_on_grass(&mut simulation, cora, start);
            simulation
                .set_terrain_override(target, Terrain::Water)
                .unwrap();
            character_mut(&mut simulation, cora).set_speed(MovementSpeed::new(2_000).unwrap());
            simulation.set_movement_direction(cora, direction).unwrap();

            simulation.advance_ticks(1).unwrap();

            let position = character(&simulation, cora).position();
            assert_eq!((position.x_subunits(), position.y_subunits()), expected);
            assert_eq!(position.containing_cell(), start);
            assert_eq!(
                simulation.effective_terrain_at(start).unwrap(),
                Terrain::Grass
            );
            assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
        }
    }

    #[test]
    fn world_edges_allow_interior_motion_then_stop_without_wrapping() {
        for (start, direction, expected) in [
            (
                WorldCell::new(i64::MAX, 0),
                Direction::East,
                ((i128::from(i64::MAX) + 1) * 1024 - 1, 512),
            ),
            (
                WorldCell::new(i64::MIN, 0),
                Direction::West,
                (i128::from(i64::MIN) * 1024, 512),
            ),
            (
                WorldCell::new(0, i64::MAX),
                Direction::North,
                (512, (i128::from(i64::MAX) + 1) * 1024 - 1),
            ),
            (
                WorldCell::new(0, i64::MIN),
                Direction::South,
                (512, i128::from(i64::MIN) * 1024),
            ),
        ] {
            let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
            let cora = cora();
            character_mut(&mut simulation, cora).set_position(position_at_cell(start));
            character_mut(&mut simulation, cora).set_speed(MovementSpeed::new(2_000).unwrap());
            simulation.set_movement_direction(cora, direction).unwrap();

            simulation.advance_ticks(1).unwrap();

            let position = character(&simulation, cora).position();
            assert_eq!((position.x_subunits(), position.y_subunits()), expected);
            assert_eq!(position.containing_cell(), start);
            assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
        }
    }

    fn character(simulation: &Simulation, id: EntityId) -> &Character {
        simulation.characters.get(&id).unwrap()
    }

    fn character_mut(simulation: &mut Simulation, id: EntityId) -> &mut Character {
        simulation.characters.get_mut(&id).unwrap()
    }

    fn position_at_cell(cell: WorldCell) -> WorldPosition {
        WorldPosition::from_cell_center(cell).unwrap()
    }

    fn blocked_stop_position(source: WorldCell, direction: Direction) -> WorldPosition {
        let origin = WorldPosition::from_cell_origin(source).unwrap();
        match direction {
            Direction::East => origin.checked_translate(1023, 512).unwrap(),
            Direction::West => origin.checked_translate(0, 512).unwrap(),
            Direction::North => origin.checked_translate(512, 1023).unwrap(),
            Direction::South => origin.checked_translate(512, 0).unwrap(),
        }
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
            .set_position(position_at_cell(position));
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

        simulation.advance_ticks(3).unwrap();

        assert_eq!(
            character(&simulation, cora).position(),
            blocked_stop_position(position, direction)
        );
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
