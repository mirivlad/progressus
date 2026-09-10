//! Bounded deterministic idle life for characters with no job and no urgent need.

use super::*;

impl Simulation {
    pub(super) fn maintain_idle_behavior(&mut self) -> Result<(), SimulationError> {
        let tick = self.clock.tick().value();
        let seed = self.generator.seed().value();
        let character_ids = self.characters.keys().copied().collect::<Vec<_>>();

        for character_id in character_ids {
            let eligible = self.characters.get(&character_id).is_some_and(|character| {
                character.movement() == MovementState::Idle
                    && !character.is_hungry()
                    && self.job_world.job_for_worker(character_id).is_none()
                    && !self
                        .item_world
                        .iter()
                        .any(|item| item.carrier() == Some(character_id))
            });
            if !eligible {
                continue;
            }

            let phase = 16 + character_id.value().wrapping_mul(5) % 24;
            if tick % IDLE_BEHAVIOR_INTERVAL_TICKS != phase {
                continue;
            }

            let cycle = tick / IDLE_BEHAVIOR_INTERVAL_TICKS;
            let entropy = idle_entropy(seed, character_id.value(), cycle);
            let route = if entropy.is_multiple_of(IDLE_SOCIAL_CHANCE_DIVISOR) {
                self.plan_idle_social_route(character_id, entropy)?
                    .or(self.plan_idle_wander_route(character_id, entropy)?)
            } else {
                self.plan_idle_wander_route(character_id, entropy)?
            };

            if let Some(route) = route {
                self.characters
                    .get_mut(&character_id)
                    .expect("idle character is still present")
                    .set_wandering_route(route);
            }
        }

        Ok(())
    }

    pub(super) fn plan_idle_social_route(
        &self,
        character_id: EntityId,
        entropy: u64,
    ) -> Result<Option<NavigationRoute>, SimulationError> {
        let Some(character) = self.characters.get(&character_id) else {
            return Err(SimulationError::UnknownCharacter(character_id));
        };
        let current = character.position().containing_cell();
        let anchor = character.idle_anchor();
        let mut companions = self
            .characters
            .values()
            .filter(|other| other.id() != character_id)
            .filter(|other| other.movement() == MovementState::Idle)
            .filter(|other| !other.is_hungry())
            .filter(|other| self.job_world.job_for_worker(other.id()).is_none())
            .map(|other| {
                (
                    cell_manhattan_distance(current, other.position().containing_cell()),
                    other.id(),
                    other.position().containing_cell(),
                )
            })
            .filter(|(distance, ..)| *distance <= 6)
            .collect::<Vec<_>>();
        companions.sort_unstable();
        if companions.is_empty() {
            return Ok(None);
        }

        let start = (entropy as usize) % companions.len();
        let directions = [
            Direction::East,
            Direction::North,
            Direction::West,
            Direction::South,
        ];
        for offset in 0..companions.len() {
            let (_, _, companion_cell) = companions[(start + offset) % companions.len()];
            let direction_start =
                ((entropy >> (8 + offset.min(6) * 4)) as usize) % directions.len();
            for direction_offset in 0..directions.len() {
                let direction = directions[(direction_start + direction_offset) % directions.len()];
                let Some(destination) = direction.adjacent(companion_cell) else {
                    continue;
                };
                if !idle_cell_within_anchor(anchor, destination) {
                    continue;
                }
                if self.character_occupies_cell(character_id, destination) {
                    continue;
                }
                if let Some(route) = self.plan_idle_route_to_cell(character_id, destination)? {
                    return Ok(Some(route));
                }
            }
        }
        Ok(None)
    }

    pub(super) fn plan_idle_wander_route(
        &self,
        character_id: EntityId,
        entropy: u64,
    ) -> Result<Option<NavigationRoute>, SimulationError> {
        let anchor = self
            .characters
            .get(&character_id)
            .ok_or(SimulationError::UnknownCharacter(character_id))?
            .idle_anchor();

        for attempt in 0..IDLE_DESTINATION_ATTEMPTS {
            let mixed =
                mix_idle_entropy(entropy.wrapping_add(attempt.wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            let dx = (mixed % 7) as i64 - IDLE_WANDER_RADIUS_CELLS;
            let dy = ((mixed >> 11) % 7) as i64 - IDLE_WANDER_RADIUS_CELLS;
            if dx == 0 && dy == 0 || dx.abs() + dy.abs() > IDLE_WANDER_RADIUS_CELLS {
                continue;
            }
            let Some(x) = anchor.x().checked_add(dx) else {
                continue;
            };
            let Some(y) = anchor.y().checked_add(dy) else {
                continue;
            };
            let destination = WorldCell::new(x, y);
            if self.character_occupies_cell(character_id, destination) {
                continue;
            }
            if let Some(route) = self.plan_idle_route_to_cell(character_id, destination)? {
                return Ok(Some(route));
            }
        }
        Ok(None)
    }

    pub(super) fn plan_idle_route_to_cell(
        &self,
        character_id: EntityId,
        destination: WorldCell,
    ) -> Result<Option<NavigationRoute>, SimulationError> {
        let character = self
            .characters
            .get(&character_id)
            .ok_or(SimulationError::UnknownCharacter(character_id))?;
        let current_position = character.position();
        let current = current_position.containing_cell();
        let anchor = character.idle_anchor();
        if current == destination
            || !idle_cell_within_anchor(anchor, destination)
            || !self.is_explored(destination)
            || !self.is_walkable(destination)?
        {
            return Ok(None);
        }

        let cells = match find_explored_path(self, current, destination)? {
            Ok(cells) => cells,
            Err(PathfindingError::PathNotFound | PathfindingError::SearchBudgetExceeded) => {
                return Ok(None);
            }
        };
        if !cells
            .iter()
            .copied()
            .all(|cell| idle_cell_within_anchor(anchor, cell))
        {
            return Ok(None);
        }

        let destination_position = WorldPosition::from_cell_center(destination)?;
        Ok(Some(NavigationRoute {
            destination: destination_position,
            waypoints: build_waypoints(current_position, destination_position, &cells)?,
        }))
    }

    pub(super) fn character_occupies_cell(&self, character_id: EntityId, cell: WorldCell) -> bool {
        self.characters.values().any(|character| {
            character.id() != character_id && character.position().containing_cell() == cell
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::test_support::*;

    #[test]
    fn idle_characters_wander_deterministically_inside_their_local_anchor() {
        let mut first = Simulation::new(WorldSeed::new(0)).unwrap();
        let mut second = Simulation::new(WorldSeed::new(0)).unwrap();
        let initial_anchors = first
            .characters()
            .map(|character| (character.id(), character.idle_anchor()))
            .collect::<BTreeMap<_, _>>();
        let mut saw_wandering = false;

        for _ in 0..96 {
            first.advance_ticks(1).unwrap();
            second.advance_ticks(1).unwrap();
            for character in first.characters() {
                let anchor = initial_anchors[&character.id()];
                assert_eq!(character.idle_anchor(), anchor);
                assert!(idle_cell_within_anchor(
                    anchor,
                    character.position().containing_cell()
                ));
                if let MovementState::Wandering { destination } = character.movement() {
                    saw_wandering = true;
                    assert!(first.is_explored(destination.containing_cell()));
                    assert!(idle_cell_within_anchor(
                        anchor,
                        destination.containing_cell()
                    ));
                }
            }
        }

        assert!(
            saw_wandering,
            "idle settlement should visibly move without player orders"
        );
        let first_state = first
            .characters()
            .map(|character| {
                (
                    character.id(),
                    character.position(),
                    character.movement(),
                    character.idle_anchor(),
                )
            })
            .collect::<Vec<_>>();
        let second_state = second
            .characters()
            .map(|character| {
                (
                    character.id(),
                    character.position(),
                    character.movement(),
                    character.idle_anchor(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(first_state, second_state);
    }

    #[test]
    fn real_work_preempts_idle_wandering() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cora = cora();
        let route = (0..128)
            .find_map(|entropy| simulation.plan_idle_wander_route(cora, entropy).unwrap())
            .expect("seed 0 should provide a local idle route for Cora");
        simulation
            .characters
            .get_mut(&cora)
            .unwrap()
            .set_wandering_route(route);
        for id in [1_u64, 2, 4, 5].map(|id| EntityId::new(id).unwrap()) {
            simulation.characters.get_mut(&id).unwrap().set_movement(
                MovementState::ManualDirectional {
                    direction: Direction::East,
                },
            );
        }

        let (source, _) = harvest_fixture(&simulation);
        let job_id = simulation.designate_harvest(source).unwrap();
        simulation.try_assign_harvest(job_id, source).unwrap();

        assert_eq!(
            simulation.job_world.get(job_id).unwrap().state().worker(),
            Some(cora)
        );
        assert!(matches!(
            character(&simulation, cora).movement(),
            MovementState::Navigating { .. }
        ));
    }
}
