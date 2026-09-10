//! Player movement intent, route planning and per-tick authoritative motion.

use progressus_content::terrain;

use super::*;

impl Simulation {
    pub fn set_movement_direction(
        &mut self,
        id: EntityId,
        direction: Direction,
    ) -> Result<(), SimulationError> {
        if !self.characters.contains_key(&id) {
            return Err(SimulationError::UnknownCharacter(id));
        }
        self.interrupt_worker_job(id)?;
        self.characters
            .get_mut(&id)
            .expect("character was checked above")
            .set_movement(MovementState::ManualDirectional { direction });
        Ok(())
    }

    pub fn move_to(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<(), SimulationError> {
        let route = self.plan_player_navigation_route(id, destination)?;
        self.interrupt_worker_job(id)?;
        self.apply_navigation_route(id, destination, route);
        Ok(())
    }

    pub fn stop_movement(&mut self, id: EntityId) -> Result<(), SimulationError> {
        if !self.characters.contains_key(&id) {
            return Err(SimulationError::UnknownCharacter(id));
        }
        self.interrupt_worker_job(id)?;
        self.characters
            .get_mut(&id)
            .expect("character was checked above")
            .set_movement(MovementState::Idle);
        Ok(())
    }

    pub(super) fn plan_player_navigation_route(
        &self,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<Option<NavigationRoute>, SimulationError> {
        let current = self
            .characters
            .get(&id)
            .ok_or(SimulationError::UnknownCharacter(id))?
            .position();
        if current == destination {
            return Ok(None);
        }

        let start = current.containing_cell();
        let goal = destination.containing_cell();
        if self.is_explored(goal) && self.is_walkable(goal)? {
            match find_explored_path(self, start, goal)? {
                Ok(cells) => {
                    return Ok(Some(NavigationRoute {
                        destination,
                        waypoints: build_waypoints(current, destination, &cells)?,
                    }));
                }
                Err(PathfindingError::SearchBudgetExceeded) => {
                    return Err(SimulationError::MoveToSearchBudgetExceeded);
                }
                Err(PathfindingError::PathNotFound) => {}
            }
        }

        let cells =
            find_closest_explored_path(self, start, goal)?.map_err(|error| match error {
                PathfindingError::PathNotFound => SimulationError::MoveToPathNotFound,
                PathfindingError::SearchBudgetExceeded => {
                    SimulationError::MoveToSearchBudgetExceeded
                }
            })?;
        let frontier = *cells.last().expect("closest explored path contains start");
        if frontier == start {
            return Ok(None);
        }
        let segment_destination = WorldPosition::from_cell_center(frontier)?;
        Ok(Some(NavigationRoute {
            destination,
            waypoints: build_waypoints(current, segment_destination, &cells)?,
        }))
    }

    pub(super) fn plan_navigation_route(
        &self,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<Option<NavigationRoute>, SimulationError> {
        let current = self
            .characters
            .get(&id)
            .ok_or(SimulationError::UnknownCharacter(id))?
            .position();
        if current == destination {
            return Ok(None);
        }
        if !self.is_explored(destination.containing_cell()) {
            return Err(SimulationError::MoveToDestinationUndiscovered(
                destination.containing_cell(),
            ));
        }
        if !self.is_walkable(destination.containing_cell())? {
            return Err(SimulationError::MoveToDestinationBlocked(
                destination.containing_cell(),
            ));
        }
        let cells = find_explored_path(
            self,
            current.containing_cell(),
            destination.containing_cell(),
        )?
        .map_err(|error| match error {
            PathfindingError::PathNotFound => SimulationError::MoveToPathNotFound,
            PathfindingError::SearchBudgetExceeded => SimulationError::MoveToSearchBudgetExceeded,
        })?;
        Ok(Some(NavigationRoute {
            destination,
            waypoints: build_waypoints(current, destination, &cells)?,
        }))
    }

    pub(super) fn apply_navigation_route(
        &mut self,
        id: EntityId,
        _destination: WorldPosition,
        route: Option<NavigationRoute>,
    ) {
        let character = self
            .characters
            .get_mut(&id)
            .expect("navigation routes are applied only to known characters");
        match route {
            Some(route) => character.set_navigation_route(route),
            None => {
                character.set_movement(MovementState::Idle);
            }
        }
    }

    pub(super) fn advance_characters_one_tick(&mut self) -> Result<(), SimulationError> {
        let ids = self.characters.keys().copied().collect::<Vec<_>>();
        for id in &ids {
            let character = self
                .characters
                .get_mut(id)
                .expect("character ID came from the character map");
            character.set_last_tick_motion_trace(vec![character.position()]);
        }
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
                    MovementState::Navigating { .. } | MovementState::Wandering { .. } => {
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

        let discovery_cells = self
            .characters
            .iter()
            .map(|(id, character)| (*id, character.position().containing_cell()))
            .collect::<Vec<_>>();
        for (id, cell) in discovery_cells {
            if self.last_discovery_cells.insert(id, cell) != Some(cell) {
                self.explored_world.reveal_around(cell);
            }
        }

        let partial_routes = self
            .characters
            .iter()
            .filter_map(|(id, character)| {
                if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    return None;
                }
                character.navigation_route().and_then(|route| {
                    (route.waypoints.is_empty() && character.position() != route.destination)
                        .then_some((*id, route.destination))
                })
            })
            .collect::<Vec<_>>();
        for (id, destination) in partial_routes {
            let route = self.plan_player_navigation_route(id, destination)?;
            self.apply_navigation_route(id, destination, route);
        }

        Ok(())
    }

    pub(super) fn advance_navigation_one_tick(
        &mut self,
        id: EntityId,
    ) -> Result<(), SimulationError> {
        let (mut position, mut remaining, mut route, wandering) = {
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
                matches!(character.movement(), MovementState::Wandering { .. }),
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
                if position == route.destination {
                    character.set_movement(MovementState::Idle);
                } else if wandering {
                    character.set_wandering_route(route);
                } else {
                    character.set_navigation_route(route);
                }
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
        while route.waypoints.front().copied() == Some(position) {
            route.waypoints.pop_front();
        }
        let character = self
            .characters
            .get_mut(&id)
            .expect("character ID came from map");
        character.set_position(position);
        if route.waypoints.is_empty() && position == route.destination {
            character.set_movement(MovementState::Idle);
        } else if wandering {
            character.set_wandering_route(route);
        } else {
            character.set_navigation_route(route);
        }
        trace.push(position);
        character.set_last_tick_motion_trace(trace);
        Ok(())
    }

    pub(super) fn advance_cardinal(
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

    pub(super) fn is_walkable(&self, position: WorldCell) -> Result<bool, SimulationError> {
        if self.effective_terrain_at(position)? != terrain::GRASS {
            return Ok(false);
        }
        Ok(self
            .construction_world
            .structure_kind_at(position)
            .is_none_or(|kind| kind.definition().navigation_cost.is_some()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::test_support::*;
    use progressus_content::structure;

    #[test]
    fn player_move_to_advances_through_newly_explored_terrain_to_the_intended_destination() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let destination = WorldCell::new(20, 0);
        place_on_grass(&mut simulation, cora, WorldCell::new(0, 0));
        for x in 0..=20 {
            simulation
                .set_terrain_override(WorldCell::new(x, 0), terrain::GRASS)
                .unwrap();
        }
        simulation.advance_ticks(1).unwrap();
        assert!(!simulation.is_explored(destination));

        let destination_position = WorldPosition::from_cell_center(destination).unwrap();
        simulation.move_to(cora, destination_position).unwrap();
        for _ in 0..160 {
            simulation.advance_ticks(1).unwrap();
            if character(&simulation, cora).position() == destination_position {
                break;
            }
        }

        assert_eq!(
            character(&simulation, cora).position(),
            destination_position
        );
        assert!(character(&simulation, cora).is_available_for_work());
        assert!(simulation.is_explored(destination));
    }

    #[test]
    fn player_move_to_stops_at_the_closest_reachable_cell_when_target_is_blocked() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let destination = WorldCell::new(20, 0);
        place_on_grass(&mut simulation, cora, WorldCell::new(0, 0));
        for x in 0..20 {
            simulation
                .set_terrain_override(WorldCell::new(x, 0), terrain::GRASS)
                .unwrap();
        }
        for cell in [
            destination,
            WorldCell::new(21, 0),
            WorldCell::new(20, 1),
            WorldCell::new(20, -1),
        ] {
            simulation
                .set_terrain_override(cell, terrain::ROCK)
                .unwrap();
        }
        simulation.advance_ticks(1).unwrap();

        simulation
            .move_to(cora, WorldPosition::from_cell_center(destination).unwrap())
            .unwrap();
        let closest = WorldPosition::from_cell_center(WorldCell::new(19, 0)).unwrap();
        for _ in 0..160 {
            simulation.advance_ticks(1).unwrap();
            if character(&simulation, cora).position() == closest {
                break;
            }
        }

        assert_eq!(character(&simulation, cora).position(), closest);
        assert!(character(&simulation, cora).is_available_for_work());
    }

    #[test]
    fn exact_destinations_remain_still_after_arrival_and_idle_ticks() {
        let destinations = [
            WorldPosition::from_subunits(512, 512).unwrap(),
            WorldPosition::from_subunits(256, 256).unwrap(),
            WorldPosition::from_subunits(900, 777).unwrap(),
            WorldPosition::from_subunits(1023, 900).unwrap(),
            WorldPosition::from_subunits(1024, 900).unwrap(),
            WorldPosition::from_subunits(-1024, 900).unwrap(),
            WorldPosition::from_subunits(1024, 1024).unwrap(),
        ];

        for destination in destinations {
            let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
            let cora = cora();
            simulation
                .set_terrain_override(destination.containing_cell(), terrain::GRASS)
                .unwrap();
            simulation.move_to(cora, destination).unwrap();
            simulation.advance_ticks(20).unwrap();

            assert_eq!(character(&simulation, cora).position(), destination);
            assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
            assert_eq!(character(&simulation, cora).navigation_destination(), None);
            simulation.advance_ticks(3).unwrap();
            assert_eq!(character(&simulation, cora).position(), destination);
            assert_eq!(
                character(&simulation, cora).last_tick_motion_trace(),
                &[destination]
            );
        }
    }

    #[test]
    fn exact_destination_near_blocked_terrain_remains_still_after_arrival() {
        for blocked in [terrain::WATER, terrain::ROCK] {
            let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
            let cora = cora();
            let (cell, direction) = find_raw_grass_with_neighbor(&simulation, blocked);
            place_on_grass(&mut simulation, cora, cell);
            simulation.advance_ticks(1).unwrap();
            let destination = near_cell_edge(cell, direction);

            simulation.move_to(cora, destination).unwrap();
            simulation.advance_ticks(20).unwrap();

            assert_eq!(character(&simulation, cora).position(), destination);
            assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
            simulation.advance_ticks(3).unwrap();
            assert_eq!(character(&simulation, cora).position(), destination);
            assert_eq!(
                character(&simulation, cora).last_tick_motion_trace(),
                &[destination]
            );
        }
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
        persisted_movement_stops_on(terrain::WATER);
    }

    #[test]
    fn persisted_movement_stops_on_rock_at_last_valid_subunit() {
        persisted_movement_stops_on(terrain::ROCK);
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
    fn grass_overridden_to_blocked_terrain_stops_only_at_its_boundary() {
        for blocked in [terrain::ROCK, terrain::WATER] {
            let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
            let (start, direction) = find_raw_grass_with_neighbor(&simulation, terrain::GRASS);
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
        for blocked in [terrain::WATER, terrain::ROCK] {
            let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
            let (start, direction) = find_raw_grass_with_neighbor(&simulation, blocked);
            let target = direction.adjacent(start).unwrap();
            let cora = cora();
            place_on_grass(&mut simulation, cora, start);

            simulation
                .set_terrain_override(target, terrain::GRASS)
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
    fn cross_cell_waypoints_approach_exact_destination_without_center_backtrack() {
        let cases = [
            (
                WorldCell::new(0, 0),
                WorldCell::new(1, 0),
                WorldPosition::from_subunits(1_100, 900).unwrap(),
                vec![
                    WorldPosition::from_subunits(1_100, 512).unwrap(),
                    WorldPosition::from_subunits(1_100, 900).unwrap(),
                ],
            ),
            (
                WorldCell::new(2, 0),
                WorldCell::new(1, 0),
                WorldPosition::from_subunits(1_900, 100).unwrap(),
                vec![
                    WorldPosition::from_subunits(1_900, 512).unwrap(),
                    WorldPosition::from_subunits(1_900, 100).unwrap(),
                ],
            ),
            (
                WorldCell::new(1, -1),
                WorldCell::new(1, 0),
                WorldPosition::from_subunits(1_800, 100).unwrap(),
                vec![
                    WorldPosition::from_subunits(1_536, 100).unwrap(),
                    WorldPosition::from_subunits(1_800, 100).unwrap(),
                ],
            ),
            (
                WorldCell::new(1, 1),
                WorldCell::new(1, 0),
                WorldPosition::from_subunits(1_200, 900).unwrap(),
                vec![
                    WorldPosition::from_subunits(1_536, 900).unwrap(),
                    WorldPosition::from_subunits(1_200, 900).unwrap(),
                ],
            ),
        ];

        for (start, goal, destination, expected) in cases {
            let current = WorldPosition::from_cell_center(start).unwrap();
            let route = build_waypoints(current, destination, &[start, goal]).unwrap();
            assert_eq!(route.into_iter().collect::<Vec<_>>(), expected);
        }
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
    fn blocked_move_to_replaces_an_existing_route_with_the_closest_approach() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(0, 0);
        place_on_grass(&mut simulation, cora, start);
        let first = WorldPosition::from_cell_center(WorldCell::new(1, 0)).unwrap();
        let blocked = WorldPosition::from_cell_center(WorldCell::new(2, 0)).unwrap();
        simulation
            .set_terrain_override(WorldCell::new(1, 0), terrain::GRASS)
            .unwrap();
        simulation
            .set_terrain_override(WorldCell::new(2, 0), terrain::ROCK)
            .unwrap();
        simulation.move_to(cora, first).unwrap();
        simulation.move_to(cora, blocked).unwrap();

        assert_eq!(
            character(&simulation, cora).movement(),
            MovementState::Navigating {
                destination: blocked
            }
        );
        assert_eq!(
            character(&simulation, cora).navigation_destination(),
            Some(blocked)
        );
        assert_eq!(
            character(&simulation, cora).navigation_waypoints().last(),
            Some(first)
        );
    }

    #[test]
    fn navigation_stops_at_a_newly_blocked_cell_boundary() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        place_on_grass(&mut simulation, cora, WorldCell::new(0, 0));
        for x in 1..=2 {
            simulation
                .set_terrain_override(WorldCell::new(x, 0), terrain::GRASS)
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
            .set_terrain_override(WorldCell::new(1, 0), terrain::ROCK)
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
                .set_terrain_override(cell, terrain::GRASS)
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
                .set_terrain_override(cell, terrain::ROCK)
                .unwrap();
        }
        place_on_grass(&mut simulation, cora, start);
        simulation.advance_ticks(1).unwrap();
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
                .set_terrain_override(cell, terrain::GRASS)
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
                .set_terrain_override(cell, terrain::GRASS)
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
            .set_terrain_override(target, terrain::GRASS)
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
            .set_terrain_override(target, terrain::ROCK)
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
                .set_terrain_override(WorldCell::new(x, 0), terrain::GRASS)
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
                .set_terrain_override(start, terrain::GRASS)
                .unwrap();
            place_on_grass(&mut simulation, cora, start);
            simulation
                .set_terrain_override(target, terrain::WATER)
                .unwrap();
            character_mut(&mut simulation, cora).set_speed(MovementSpeed::new(2_000).unwrap());
            simulation.set_movement_direction(cora, direction).unwrap();

            simulation.advance_ticks(1).unwrap();

            let position = character(&simulation, cora).position();
            assert_eq!((position.x_subunits(), position.y_subunits()), expected);
            assert_eq!(position.containing_cell(), start);
            assert_eq!(
                simulation.effective_terrain_at(start).unwrap(),
                terrain::GRASS
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

    #[test]
    fn pathfinding_treats_doors_as_passable_but_prefers_a_short_door_free_detour() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        for y in 0..=1 {
            for x in 0..=4 {
                simulation
                    .set_terrain_override(WorldCell::new(x, y), terrain::GRASS)
                    .unwrap();
            }
        }
        for x in 1..=3 {
            let cell = WorldCell::new(x, 0);
            let id = simulation.id_allocator.allocate().unwrap();
            simulation
                .construction_world
                .insert_site(ConstructionSite::new(id, structure::DOOR, cell))
                .unwrap();
            simulation.construction_world.complete_site(id).unwrap();
        }

        let path =
            crate::pathfinding::find_path(&simulation, WorldCell::new(0, 0), WorldCell::new(4, 0))
                .unwrap()
                .unwrap();
        assert!(path.iter().any(|cell| cell.y() == 1));
        assert!(
            path.iter()
                .all(|cell| simulation.structure_kind_at(*cell) != Some(structure::DOOR))
        );
    }
}
