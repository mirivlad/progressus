#[cfg(test)]
use std::cell::Cell;
use std::collections::{BTreeMap, VecDeque};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use progressus_worldgen::{WorldGenerator, WorldgenError};

use crate::clock::SimulationClock;
use crate::entity::{EntityIdAllocator, NavigationRoute};
use crate::pathfinding::{PathfindingError, find_path};
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
            .set_movement(MovementState::ManualDirectional { direction });
        Ok(())
    }

    pub fn move_to(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<(), SimulationError> {
        let current = self
            .characters
            .get(&id)
            .ok_or(SimulationError::UnknownCharacter(id))?
            .position();
        if current == destination {
            self.stop_movement(id)?;
            return Ok(());
        }
        if !self.is_walkable(destination.containing_cell())? {
            return Err(SimulationError::MoveToDestinationBlocked(
                destination.containing_cell(),
            ));
        }
        let cells = find_path(
            self,
            current.containing_cell(),
            destination.containing_cell(),
        )?
        .map_err(|error| match error {
            PathfindingError::PathNotFound => SimulationError::MoveToPathNotFound,
            PathfindingError::SearchBudgetExceeded => SimulationError::MoveToSearchBudgetExceeded,
        })?;
        let route = NavigationRoute {
            destination,
            waypoints: build_waypoints(current, destination, &cells)?,
        };
        self.characters
            .get_mut(&id)
            .expect("character was checked above")
            .set_navigation_route(route);
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
                    MovementState::ManualDirectional { direction } => (
                        character.position(),
                        direction,
                        i128::from(character.speed().subunits_per_tick()),
                    ),
                    MovementState::Navigating { .. } => {
                        self.advance_navigation_one_tick(id)?;
                        continue;
                    }
                }
            };
            let trace_start = position;

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
            self.characters
                .get_mut(&id)
                .expect("character ID came from the character map")
                .set_last_tick_motion_trace(vec![trace_start, position]);
        }

        Ok(())
    }

    fn advance_navigation_one_tick(&mut self, id: EntityId) -> Result<(), SimulationError> {
        let (mut position, mut remaining, mut route) = {
            let character = self
                .characters
                .get(&id)
                .expect("character ID came from map");
            (
                character.position(),
                i128::from(character.speed().subunits_per_tick()),
                character
                    .navigation_route()
                    .cloned()
                    .expect("navigating characters have a route"),
            )
        };
        let mut trace = vec![position];
        while remaining > 0 {
            while route.waypoints.front().copied() == Some(position) {
                route.waypoints.pop_front();
                trace.push(position);
            }
            let Some(target) = route.waypoints.front().copied() else {
                let character = self
                    .characters
                    .get_mut(&id)
                    .expect("character ID came from map");
                character.set_position(position);
                character.set_movement(MovementState::Idle);
                character.set_last_tick_motion_trace(trace);
                return Ok(());
            };
            let (direction, distance) = direction_and_distance(position, target)?;
            let budget = remaining.min(distance);
            let (next, consumed, blocked) = self.advance_cardinal(position, direction, budget)?;
            position = next;
            remaining -= consumed;
            if blocked {
                let character = self
                    .characters
                    .get_mut(&id)
                    .expect("character ID came from map");
                character.set_position(position);
                character.set_movement(MovementState::Idle);
                trace.push(position);
                character.set_last_tick_motion_trace(trace);
                return Ok(());
            }
        }
        let character = self
            .characters
            .get_mut(&id)
            .expect("character ID came from map");
        character.set_position(position);
        character.set_navigation_route(route);
        trace.push(position);
        character.set_last_tick_motion_trace(trace);
        Ok(())
    }

    fn advance_cardinal(
        &self,
        position: WorldPosition,
        direction: Direction,
        budget: i128,
    ) -> Result<(WorldPosition, i128, bool), SimulationError> {
        let source = position.containing_cell();
        let entry = entry_distance(position, source, direction)?;
        let walkable = match direction.adjacent(source) {
            Some(cell) => self.is_walkable(cell)?,
            None => false,
        };
        if !walkable {
            return if budget < entry {
                Ok((translate(position, direction, budget)?, budget, false))
            } else {
                Ok((translate(position, direction, entry - 1)?, budget, true))
            };
        }
        let consumed = budget.min(entry);
        Ok((translate(position, direction, consumed)?, consumed, false))
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

fn build_waypoints(
    current: WorldPosition,
    destination: WorldPosition,
    cells: &[WorldCell],
) -> Result<VecDeque<WorldPosition>, SimulationError> {
    let mut points = Vec::new();
    let same_cell = current.containing_cell() == destination.containing_cell();
    if same_cell {
        points.push(WorldPosition::from_subunits(
            destination.x_subunits(),
            current.y_subunits(),
        )?);
    } else {
        let start_center = WorldPosition::from_cell_center(current.containing_cell())?;
        points.push(WorldPosition::from_subunits(
            start_center.x_subunits(),
            current.y_subunits(),
        )?);
        points.push(start_center);
        points.extend(
            cells
                .iter()
                .skip(1)
                .map(|cell| WorldPosition::from_cell_center(*cell))
                .collect::<Result<Vec<_>, _>>()?,
        );
        let destination_center = WorldPosition::from_cell_center(destination.containing_cell())?;
        points.push(WorldPosition::from_subunits(
            destination.x_subunits(),
            destination_center.y_subunits(),
        )?);
    }
    points.push(destination);
    Ok(points.into_iter().filter(|point| *point != current).fold(
        VecDeque::new(),
        |mut waypoints, point| {
            if waypoints.back().copied() != Some(point) {
                waypoints.push_back(point);
            }
            waypoints
        },
    ))
}

fn direction_and_distance(
    position: WorldPosition,
    target: WorldPosition,
) -> Result<(Direction, i128), SimulationError> {
    let delta_x = target.x_subunits() - position.x_subunits();
    let delta_y = target.y_subunits() - position.y_subunits();
    if delta_x > 0 && delta_y == 0 {
        Ok((Direction::East, delta_x))
    } else if delta_x < 0 && delta_y == 0 {
        Ok((Direction::West, -delta_x))
    } else if delta_y > 0 && delta_x == 0 {
        Ok((Direction::North, delta_y))
    } else if delta_y < 0 && delta_x == 0 {
        Ok((Direction::South, -delta_y))
    } else {
        Err(SimulationError::Position(
            WorldPositionError::OutsideWorldCellRange,
        ))
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
    MoveToDestinationBlocked(WorldCell),
    MoveToPathNotFound,
    MoveToSearchBudgetExceeded,
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
            Self::MoveToDestinationBlocked(position) => write!(
                formatter,
                "move-to destination ({}, {}) is not walkable",
                position.x(),
                position.y()
            ),
            Self::MoveToPathNotFound => formatter.write_str("move-to path not found"),
            Self::MoveToSearchBudgetExceeded => {
                formatter.write_str("move-to search budget exceeded")
            }
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
            MovementState::ManualDirectional { direction: blocked }
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
            MovementState::ManualDirectional {
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
        controlled_character.set_movement(MovementState::ManualDirectional {
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
            MovementState::ManualDirectional {
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
    fn move_to_reaches_an_exact_same_cell_target_via_x_then_y() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(0, 0);
        place_on_grass(&mut simulation, cora, start);
        let destination = WorldPosition::from_subunits(800, 900).unwrap();
        simulation.move_to(cora, destination).unwrap();

        simulation.advance_ticks(3).unwrap();

        assert_eq!(character(&simulation, cora).position(), destination);
        assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
        assert_eq!(
            character(&simulation, cora).last_tick_motion_trace(),
            &[WorldPosition::from_subunits(800, 736).unwrap(), destination]
        );
    }

    #[test]
    fn move_to_preserves_a_turn_in_the_same_tick_motion_trace() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(0, 0);
        place_on_grass(&mut simulation, cora, start);
        character_mut(&mut simulation, cora).set_speed(MovementSpeed::new(600).unwrap());
        let destination = WorldPosition::from_subunits(800, 900).unwrap();
        simulation.move_to(cora, destination).unwrap();

        simulation.advance_ticks(1).unwrap();

        assert_eq!(
            character(&simulation, cora).last_tick_motion_trace(),
            &[
                WorldPosition::from_subunits(512, 512).unwrap(),
                WorldPosition::from_subunits(800, 512).unwrap(),
                WorldPosition::from_subunits(800, 824).unwrap(),
            ]
        );
    }

    #[test]
    fn rejected_move_to_preserves_an_existing_navigation_route() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(0, 0);
        place_on_grass(&mut simulation, cora, start);
        let accepted = WorldPosition::from_cell_center(WorldCell::new(1, 0)).unwrap();
        let rejected = WorldPosition::from_cell_center(WorldCell::new(2, 0)).unwrap();
        simulation
            .set_terrain_override(WorldCell::new(1, 0), Terrain::Grass)
            .unwrap();
        simulation
            .set_terrain_override(WorldCell::new(2, 0), Terrain::Rock)
            .unwrap();
        simulation.move_to(cora, accepted).unwrap();

        assert_eq!(
            simulation.move_to(cora, rejected),
            Err(SimulationError::MoveToDestinationBlocked(WorldCell::new(
                2, 0
            )))
        );
        assert_eq!(
            character(&simulation, cora).movement(),
            MovementState::Navigating {
                destination: accepted
            }
        );
        assert_eq!(
            character(&simulation, cora).navigation_destination(),
            Some(accepted)
        );
    }

    #[test]
    fn navigation_stops_at_a_newly_blocked_cell_boundary() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        place_on_grass(&mut simulation, cora, WorldCell::new(0, 0));
        for x in 1..=2 {
            simulation
                .set_terrain_override(WorldCell::new(x, 0), Terrain::Grass)
                .unwrap();
        }
        simulation
            .move_to(
                cora,
                WorldPosition::from_cell_center(WorldCell::new(2, 0)).unwrap(),
            )
            .unwrap();
        simulation.advance_ticks(1).unwrap();
        simulation
            .set_terrain_override(WorldCell::new(1, 0), Terrain::Rock)
            .unwrap();

        simulation.advance_ticks(1).unwrap();

        assert_eq!(character(&simulation, cora).position().x_subunits(), 1023);
        assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
        assert_eq!(character(&simulation, cora).navigation_destination(), None);
    }

    #[test]
    fn navigation_executes_a_cross_chunk_route_with_stable_identity() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(31, 0);
        let destination_cell = WorldCell::new(33, 1);
        for cell in [
            start,
            WorldCell::new(32, 0),
            WorldCell::new(33, 0),
            destination_cell,
        ] {
            simulation
                .set_terrain_override(cell, Terrain::Grass)
                .unwrap();
        }
        for cell in [
            WorldCell::new(30, 0),
            WorldCell::new(31, -1),
            WorldCell::new(31, 1),
            WorldCell::new(32, -1),
            WorldCell::new(32, 1),
            WorldCell::new(33, -1),
        ] {
            simulation
                .set_terrain_override(cell, Terrain::Rock)
                .unwrap();
        }
        place_on_grass(&mut simulation, cora, start);
        character_mut(&mut simulation, cora).set_speed(MovementSpeed::new(4_096).unwrap());
        let destination = WorldPosition::from_cell_center(destination_cell).unwrap();
        simulation.move_to(cora, destination).unwrap();

        simulation.advance_ticks(1).unwrap();

        assert_eq!(character(&simulation, cora).id(), cora);
        assert_eq!(character(&simulation, cora).position(), destination);
        assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
    }

    #[test]
    fn navigation_crosses_the_zero_to_negative_cell_boundary() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(0, 0);
        let destination_cell = WorldCell::new(-1, 0);
        for cell in [start, destination_cell] {
            simulation
                .set_terrain_override(cell, Terrain::Grass)
                .unwrap();
        }
        place_on_grass(&mut simulation, cora, start);
        character_mut(&mut simulation, cora).set_speed(MovementSpeed::new(1_024).unwrap());
        let destination = WorldPosition::from_cell_center(destination_cell).unwrap();
        simulation.move_to(cora, destination).unwrap();

        simulation.advance_ticks(1).unwrap();

        assert_eq!(character(&simulation, cora).position(), destination);
        assert_eq!(character(&simulation, cora).id(), cora);
    }

    #[test]
    fn stop_and_manual_direction_cancel_navigation_without_snapping() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(0, 0);
        place_on_grass(&mut simulation, cora, start);
        for cell in [WorldCell::new(1, 0), WorldCell::new(2, 0)] {
            simulation
                .set_terrain_override(cell, Terrain::Grass)
                .unwrap();
        }
        let destination = WorldPosition::from_cell_center(WorldCell::new(2, 0)).unwrap();
        simulation.move_to(cora, destination).unwrap();
        simulation.advance_ticks(1).unwrap();
        let mid_route_position = character(&simulation, cora).position();

        simulation.stop_movement(cora).unwrap();
        assert_eq!(character(&simulation, cora).position(), mid_route_position);
        assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
        assert_eq!(character(&simulation, cora).navigation_destination(), None);

        simulation.move_to(cora, destination).unwrap();
        simulation
            .set_movement_direction(cora, Direction::West)
            .unwrap();
        assert_eq!(
            character(&simulation, cora).movement(),
            MovementState::ManualDirectional {
                direction: Direction::West
            }
        );
        assert_eq!(character(&simulation, cora).navigation_destination(), None);
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
            MovementState::ManualDirectional {
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
            .set_movement(MovementState::ManualDirectional { direction });

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
