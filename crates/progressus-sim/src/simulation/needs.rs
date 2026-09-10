//! Authoritative hunger: deterministic satiety decay and the physical Eat job.

use progressus_content::{item, natural_resource};

use super::*;

impl Simulation {
    pub(super) fn decay_satiety_if_due(&mut self) {
        if !self
            .clock
            .tick()
            .value()
            .is_multiple_of(SATIETY_DECAY_INTERVAL_TICKS)
        {
            return;
        }
        for character in self.characters.values_mut() {
            character.decay_satiety();
        }
    }

    pub(super) fn maintain_nutrition_jobs(&mut self) -> Result<(), SimulationError> {
        let eat_jobs = self
            .job_world
            .iter()
            .filter_map(|job| matches!(job.kind(), JobKind::Eat { .. }).then_some(job.id()))
            .collect::<Vec<_>>();
        for job_id in eat_jobs {
            let Some(job) = self.job_world.get(job_id).cloned() else {
                continue;
            };
            let JobKind::Eat {
                character_id,
                item_id,
            } = job.kind()
            else {
                continue;
            };
            let valid_character = self
                .characters
                .get(&character_id)
                .is_some_and(Character::is_hungry);
            let valid_food = self.item_world.get(item_id).is_some_and(|item| {
                item.kind() == item::BERRIES && item.ground_position().is_some()
            });
            if !valid_character || !valid_food {
                self.cancel_job(job_id)?;
            }
        }

        let character_ids = self.characters.keys().copied().collect::<Vec<_>>();
        for character_id in character_ids {
            let Some(character) = self.characters.get(&character_id) else {
                continue;
            };
            if !character.is_hungry()
                || self.job_world.eat_job_for_character(character_id).is_some()
            {
                continue;
            }

            let current_job = self.job_world.job_for_worker(character_id);
            if current_job.is_none() && !character.is_available_for_work() {
                continue;
            }

            let character_cell = character.position().containing_cell();
            let mut candidates = self
                .item_world
                .iter()
                .filter_map(|item| {
                    let position = item.ground_position()?;
                    (item.kind() == item::BERRIES
                        && self.is_explored(position.containing_cell())
                        && self.job_world.item_job_for_item(item.id()).is_none())
                    .then_some((
                        cell_manhattan_distance(character_cell, position.containing_cell()),
                        item.id(),
                        position,
                        item.quantity().get(),
                    ))
                })
                .collect::<Vec<_>>();
            candidates.sort_unstable_by_key(|(distance, item_id, ..)| (*distance, *item_id));

            let mut chosen = None;
            for (_, item_id, position, quantity) in candidates {
                let route = match self.plan_navigation_route(character_id, position) {
                    Ok(route) => route,
                    Err(
                        SimulationError::MoveToDestinationBlocked(_)
                        | SimulationError::MoveToDestinationUndiscovered(_)
                        | SimulationError::MoveToPathNotFound
                        | SimulationError::MoveToSearchBudgetExceeded,
                    ) => continue,
                    Err(error) => return Err(error),
                };
                chosen = Some((item_id, position, quantity, route));
                break;
            }

            let Some((source_item_id, position, quantity, route)) = chosen else {
                let starving = self
                    .characters
                    .get(&character_id)
                    .is_some_and(Character::is_starving);
                let wandering = self.characters.get(&character_id).is_some_and(|character| {
                    matches!(character.movement(), MovementState::Wandering { .. })
                });
                if starving && (current_job.is_some() || wandering) {
                    if current_job.is_some() {
                        self.interrupt_worker_job(character_id)?;
                    }
                    self.characters
                        .get_mut(&character_id)
                        .expect("starving character is still present")
                        .set_movement(MovementState::Idle);
                }
                self.ensure_renewable_food_harvest(character_id)?;
                continue;
            };

            if current_job.is_some() {
                self.interrupt_worker_job(character_id)?;
            }

            let meal_item_id = if quantity > 1 {
                let split_id = self.id_allocator.allocate()?;
                self.item_world
                    .split_ground_stack(source_item_id, split_id, 1)
                    .map_err(|_| SimulationError::JobInvariantViolation)?;
                split_id
            } else {
                source_item_id
            };
            debug_assert_eq!(
                self.item_world
                    .get(meal_item_id)
                    .and_then(ItemStack::ground_position),
                Some(position)
            );
            let job_id = self.id_allocator.allocate()?;
            self.job_world
                .insert(Job::new(
                    job_id,
                    JobKind::Eat {
                        character_id,
                        item_id: meal_item_id,
                    },
                ))
                .map_err(SimulationError::from_job_world)?;
            self.job_world
                .reserve_worker(job_id, character_id)
                .map_err(SimulationError::from_job_world)?;
            self.apply_navigation_route(character_id, position, route);
        }
        Ok(())
    }

    pub(super) fn ensure_renewable_food_harvest(
        &mut self,
        character_id: EntityId,
    ) -> Result<bool, SimulationError> {
        let character_cell = self
            .characters
            .get(&character_id)
            .ok_or(SimulationError::UnknownCharacter(character_id))?
            .position()
            .containing_cell();
        // Autonomous need satisfaction is deliberately local to the bootstrap
        // settlement. Scanning every explored cell each tick both scales with
        // explored-world size and lets a chain of newly discovered wild bushes
        // become accidental free scouting. Manual Harvest remains unrestricted.
        let mut candidates = Vec::new();
        for y in -AUTONOMOUS_FORAGE_RADIUS_CELLS..=AUTONOMOUS_FORAGE_RADIUS_CELLS {
            for x in -AUTONOMOUS_FORAGE_RADIUS_CELLS..=AUTONOMOUS_FORAGE_RADIUS_CELLS {
                if x.abs() + y.abs() > AUTONOMOUS_FORAGE_RADIUS_CELLS {
                    continue;
                }
                let cell = WorldCell::new(x, y);
                if !self.is_explored(cell) {
                    continue;
                }
                let Some(resource) = self.natural_resource_at(cell)? else {
                    continue;
                };
                if resource.kind() == natural_resource::BERRY_BUSH
                    && self.job_world.harvest_job_for_source(cell).is_none()
                {
                    candidates.push((cell_manhattan_distance(character_cell, cell), cell));
                }
            }
        }
        candidates.sort_unstable();
        for (_, source) in candidates {
            let destination = WorldPosition::from_cell_center(source)?;
            match self.plan_navigation_route(character_id, destination) {
                Ok(_) => {
                    self.designate_harvest(source)?;
                    return Ok(true);
                }
                Err(
                    SimulationError::MoveToDestinationBlocked(_)
                    | SimulationError::MoveToDestinationUndiscovered(_)
                    | SimulationError::MoveToPathNotFound
                    | SimulationError::MoveToSearchBudgetExceeded,
                ) => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(false)
    }

    pub(super) fn try_assign_eat(
        &mut self,
        job_id: EntityId,
        character_id: EntityId,
        item_id: EntityId,
    ) -> Result<(), SimulationError> {
        let Some(character) = self.characters.get(&character_id) else {
            self.cancel_job(job_id)?;
            return Ok(());
        };
        if !character.is_hungry()
            || !character.is_available_for_work()
            || self.job_world.job_for_worker(character_id).is_some()
        {
            return Ok(());
        }
        let Some(item_position) = self.item_world.get(item_id).and_then(|item| {
            (item.kind() == item::BERRIES)
                .then(|| item.ground_position())
                .flatten()
        }) else {
            self.cancel_job(job_id)?;
            return Ok(());
        };
        let route = match self.plan_navigation_route(character_id, item_position) {
            Ok(route) => route,
            Err(
                SimulationError::MoveToDestinationBlocked(_)
                | SimulationError::MoveToDestinationUndiscovered(_)
                | SimulationError::MoveToPathNotFound
                | SimulationError::MoveToSearchBudgetExceeded,
            ) => return Ok(()),
            Err(error) => return Err(error),
        };
        self.job_world
            .reserve_worker(job_id, character_id)
            .map_err(SimulationError::from_job_world)?;
        self.apply_navigation_route(character_id, item_position, route);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::test_support::*;

    #[test]
    fn satiety_decays_only_on_the_global_interval() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cora = cora();
        simulation
            .advance_ticks(SATIETY_DECAY_INTERVAL_TICKS - 1)
            .unwrap();
        assert_eq!(character(&simulation, cora).satiety(), MAX_SATIETY);

        simulation.advance_ticks(1).unwrap();
        assert_eq!(character(&simulation, cora).satiety(), MAX_SATIETY - 1);
    }

    #[test]
    fn hungry_characters_split_exact_physical_meals_and_eat_concurrently() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let ada = EntityId::new(1).unwrap();
        let borin = EntityId::new(2).unwrap();
        set_satiety(&mut simulation, ada, HUNGRY_SATIETY);
        set_satiety(&mut simulation, borin, HUNGRY_SATIETY);
        assert_eq!(total_berries(&simulation), BOOTSTRAP_BERRIES);

        simulation.advance_ticks(1).unwrap();
        let eat_jobs = simulation
            .jobs()
            .filter_map(|job| match job.kind() {
                JobKind::Eat {
                    character_id,
                    item_id,
                } => Some((character_id, item_id, job.state())),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(eat_jobs.len(), 2);
        assert!(eat_jobs.iter().all(|(_, item_id, state)| {
            simulation
                .item_world
                .get(*item_id)
                .is_some_and(|item| item.kind() == item::BERRIES && item.quantity().get() == 1)
                && matches!(state, JobState::Reserved { .. } | JobState::Working { .. })
        }));
        assert_eq!(total_berries(&simulation), BOOTSTRAP_BERRIES);

        for _ in 0..256 {
            simulation.advance_ticks(1).unwrap();
            if simulation.job_world.eat_job_for_character(ada).is_none()
                && simulation.job_world.eat_job_for_character(borin).is_none()
            {
                break;
            }
        }
        assert!(character(&simulation, ada).satiety() > HUNGRY_SATIETY);
        assert!(character(&simulation, borin).satiety() > HUNGRY_SATIETY);
        assert!(character(&simulation, ada).satiety() <= MAX_SATIETY);
        assert!(character(&simulation, borin).satiety() <= MAX_SATIETY);
        assert_eq!(total_berries(&simulation), BOOTSTRAP_BERRIES - 2);
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(simulation.item_world.indexes_are_consistent());
    }

    #[test]
    fn one_berry_restores_exactly_fifty_satiety_at_completion() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cora = cora();
        let food_position = simulation
            .items()
            .find(|item| item.kind() == item::BERRIES)
            .and_then(ItemStack::ground_position)
            .unwrap();
        place_on_grass(&mut simulation, cora, food_position.containing_cell());
        set_satiety(&mut simulation, cora, HUNGRY_SATIETY);

        simulation.advance_ticks(1).unwrap();
        simulation.advance_ticks(1).unwrap();
        assert_eq!(character(&simulation, cora).satiety(), HUNGRY_SATIETY);
        simulation.advance_ticks(1).unwrap();

        assert_eq!(character(&simulation, cora).satiety(), MAX_SATIETY);
        assert_eq!(total_berries(&simulation), BOOTSTRAP_BERRIES - 1);
        assert!(simulation.job_world.eat_job_for_character(cora).is_none());
    }

    #[test]
    fn berry_bush_harvest_produces_physical_food_and_regrows() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let source = WorldCell::new(-3, -3);
        let resource = simulation.natural_resource_at(source).unwrap().unwrap();
        assert_eq!(resource.kind(), natural_resource::BERRY_BUSH);
        let berries_before = total_berries(&simulation);

        let job_id = simulation.designate_harvest(source).unwrap();
        for _ in 0..256 {
            simulation.advance_ticks(1).unwrap();
            if simulation.job_world.get(job_id).is_none() {
                break;
            }
        }
        assert!(simulation.job_world.get(job_id).is_none());
        assert_eq!(simulation.natural_resource_at(source).unwrap(), None);
        assert_eq!(
            total_berries(&simulation),
            berries_before + resource.yield_quantity()
        );
        assert!(simulation.items().any(|item| {
            item.kind() == item::BERRIES
                && item.ground_position() == Some(WorldPosition::from_cell_center(source).unwrap())
        }));

        let ready_tick = simulation.renewable_resource_regrowth[&source];
        assert!(ready_tick > simulation.tick());
        simulation
            .advance_ticks(ready_tick.value() - simulation.tick().value())
            .unwrap();
        let regrown = simulation.natural_resource_at(source).unwrap().unwrap();
        assert_eq!(regrown, resource);
        assert!(!simulation.renewable_resource_regrowth.contains_key(&source));
    }

    #[test]
    fn harvested_berries_use_ordinary_stockpile_logistics() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let initial_berries = simulation
            .items()
            .filter(|item| item.kind() == item::BERRIES)
            .map(|item| (item.id(), item.quantity().get()))
            .collect::<Vec<_>>();
        for (item_id, quantity) in initial_berries {
            simulation.item_world.consume(item_id, quantity).unwrap();
        }
        let stockpile_id = simulation.create_stockpile(WorldCell::new(0, 2)).unwrap();
        for cell in [WorldCell::new(-1, 2), WorldCell::new(1, 2)] {
            simulation
                .set_stockpile_cell(stockpile_id, cell, true)
                .unwrap();
        }
        for kind in [item::WOOD, item::STONE, item::PRIMITIVE_TOOL] {
            simulation
                .set_stockpile_item_allowed(stockpile_id, kind, false)
                .unwrap();
        }

        let source = WorldCell::new(-3, -3);
        simulation.designate_harvest(source).unwrap();
        let mut stockpiled = 0;
        for _ in 0..512 {
            simulation.advance_ticks(1).unwrap();
            stockpiled = simulation
                .items()
                .filter(|item| item.kind() == item::BERRIES)
                .filter_map(ItemStack::ground_position)
                .filter(|position| {
                    simulation.stockpile_at(position.containing_cell()) == Some(stockpile_id)
                })
                .count();
            if stockpiled != 0 {
                break;
            }
        }
        assert!(
            stockpiled > 0,
            "harvested berries never reached the stockpile"
        );
    }

    #[test]
    fn four_regrowing_bushes_sustain_five_characters_long_term() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let initial_berries = simulation
            .items()
            .filter(|item| item.kind() == item::BERRIES)
            .map(|item| (item.id(), item.quantity().get()))
            .collect::<Vec<_>>();
        for (item_id, quantity) in initial_berries {
            simulation.item_world.consume(item_id, quantity).unwrap();
        }

        let stockpile_id = simulation.create_stockpile(WorldCell::new(0, 2)).unwrap();
        for cell in [WorldCell::new(-1, 2), WorldCell::new(1, 2)] {
            simulation
                .set_stockpile_cell(stockpile_id, cell, true)
                .unwrap();
        }
        for kind in [item::WOOD, item::STONE, item::PRIMITIVE_TOOL] {
            simulation
                .set_stockpile_item_allowed(stockpile_id, kind, false)
                .unwrap();
        }

        let mut minimum_satiety = MAX_SATIETY;
        for _ in 0..625 {
            simulation.advance_ticks(16).unwrap();
            for character in simulation.characters() {
                minimum_satiety = minimum_satiety.min(character.satiety());
                assert!(
                    character.satiety() > 0,
                    "character {} starved at tick {}",
                    character.id().value(),
                    simulation.tick().value()
                );
            }
        }
        assert!(minimum_satiety <= HUNGRY_SATIETY);
        assert!(simulation.resource_revision() >= 8);
        assert!(
            simulation.explored_world.cells().all(|cell| {
                cell_manhattan_distance(WorldCell::new(0, 0), cell)
                    <= u128::try_from(AUTONOMOUS_FORAGE_RADIUS_CELLS + IDLE_WANDER_RADIUS_CELLS + 5)
                        .expect("bootstrap forage bound is positive")
            }),
            "autonomous foraging must not turn renewable food into free long-range scouting"
        );
        assert!(
            simulation
                .generator
                .natural_resource_at(WorldCell::new(-3, -3))
                .is_some_and(|resource| resource.kind() == natural_resource::BERRY_BUSH)
        );
    }

    #[test]
    fn manual_interruption_cancels_eat_and_does_not_consume_reserved_meal() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cora = cora();
        set_satiety(&mut simulation, cora, HUNGRY_SATIETY);
        simulation.advance_ticks(1).unwrap();
        let eat_job_id = simulation.job_world.eat_job_for_character(cora).unwrap();
        let JobKind::Eat { item_id, .. } = simulation.job_world.get(eat_job_id).unwrap().kind()
        else {
            unreachable!();
        };
        assert_eq!(
            simulation.item_world.get(item_id).unwrap().quantity().get(),
            1
        );
        let before = character(&simulation, cora).satiety();

        simulation
            .set_movement_direction(cora, Direction::East)
            .unwrap();

        assert!(simulation.job_world.eat_job_for_character(cora).is_none());
        assert_eq!(simulation.job_world.eat_job_for_item(item_id), None);
        assert_eq!(
            simulation.item_world.get(item_id).unwrap().quantity().get(),
            1
        );
        assert_eq!(character(&simulation, cora).satiety(), before);
        assert_eq!(
            character(&simulation, cora).movement(),
            MovementState::ManualDirectional {
                direction: Direction::East
            }
        );
    }

    #[test]
    fn starving_character_without_food_stops_work_and_is_not_reassigned() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let berries = simulation
            .items()
            .find(|item| item.kind() == item::BERRIES)
            .unwrap()
            .id();
        let berry_quantity = simulation.item_world.get(berries).unwrap().quantity().get();
        simulation
            .item_world
            .consume(berries, berry_quantity)
            .unwrap();
        for cell in simulation.explored_world.cells().collect::<Vec<_>>() {
            if simulation
                .generator
                .natural_resource_at(cell)
                .is_some_and(|resource| resource.kind() == natural_resource::BERRY_BUSH)
            {
                simulation
                    .renewable_resource_regrowth
                    .insert(cell, SimulationTick::new(u64::MAX));
            }
        }
        let (source, _) = harvest_fixture(&simulation);
        let job_id = simulation.designate_harvest(source).unwrap();
        for _ in 0..64 {
            simulation.advance_ticks(1).unwrap();
            if simulation
                .job_world
                .get(job_id)
                .unwrap()
                .state()
                .worker()
                .is_some()
            {
                break;
            }
        }
        let worker_id = simulation
            .job_world
            .get(job_id)
            .unwrap()
            .state()
            .worker()
            .unwrap();
        set_satiety(&mut simulation, worker_id, 0);

        simulation.advance_ticks(1).unwrap();

        assert_eq!(simulation.job_world.job_for_worker(worker_id), None);
        assert_eq!(
            character(&simulation, worker_id).movement(),
            MovementState::Idle
        );
        assert!(character(&simulation, worker_id).is_starving());
        assert!(simulation.job_world.get(job_id).is_some());
    }
}
