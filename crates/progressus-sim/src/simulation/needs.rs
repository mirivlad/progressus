//! Authoritative food and rest needs with physical Eat and Sleep jobs.

use super::*;
use progressus_content::structure;

impl Simulation {
    pub(super) fn decay_rest_if_due(&mut self) {
        if !self
            .clock
            .tick()
            .value()
            .is_multiple_of(REST_DECAY_INTERVAL_TICKS)
        {
            return;
        }
        for character in self.characters.values_mut() {
            character.decay_rest();
        }
    }

    pub(super) fn maintain_sleep_jobs(&mut self) -> Result<(), SimulationError> {
        let ids = self.characters.keys().copied().collect::<Vec<_>>();
        for character_id in ids {
            let character = self.characters.get(&character_id).expect("known character");
            if !character.is_tired()
                || character.is_hungry()
                || self
                    .job_world
                    .sleep_job_for_character(character_id)
                    .is_some()
            {
                continue;
            }
            let current_job = self.job_world.job_for_worker(character_id);
            if current_job.is_none() && !character.is_available_for_work() {
                continue;
            }
            let character_cell = character.position().containing_cell();
            let mut beds = self
                .construction_world
                .structures()
                .filter(|bed| bed.kind() == structure::BED)
                .filter(|bed| self.job_world.sleep_job_for_bed(bed.id()).is_none())
                .map(|bed| {
                    (
                        cell_manhattan_distance(character_cell, bed.cell()),
                        bed.id(),
                        bed.cell(),
                    )
                })
                .collect::<Vec<_>>();
            beds.sort_unstable();
            let mut selected = None;
            for (_, bed_id, cell) in beds {
                let destination = WorldPosition::from_cell_center(cell)?;
                match self.plan_navigation_route(character_id, destination) {
                    Ok(route) => {
                        selected = Some((bed_id, destination, route));
                        break;
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
            if current_job.is_some() {
                self.interrupt_worker_job(character_id)?;
            }
            let bed_id = selected.as_ref().map(|(id, _, _)| *id);
            let job_id = self.id_allocator.allocate()?;
            self.job_world
                .insert(Job::new(
                    job_id,
                    JobKind::Sleep {
                        character_id,
                        bed_id,
                    },
                ))
                .map_err(SimulationError::from_job_world)?;
            self.job_world
                .reserve_worker(job_id, character_id)
                .map_err(SimulationError::from_job_world)?;
            if let Some((_, destination, route)) = selected {
                self.apply_navigation_route(character_id, destination, route);
            } else {
                self.characters
                    .get_mut(&character_id)
                    .expect("known character")
                    .set_movement(MovementState::Idle);
            }
        }
        Ok(())
    }

    pub(super) fn try_assign_sleep(&mut self, job_id: EntityId) -> Result<(), SimulationError> {
        // A Sleep job becomes available only after an interruption. It is
        // cancelled, not reassigned, so its bed reservation cannot linger.
        self.cancel_job(job_id)
    }

    pub(super) fn advance_reserved_sleep(
        &mut self,
        job_id: EntityId,
        character_id: EntityId,
        bed_id: Option<EntityId>,
        worker_id: EntityId,
    ) -> Result<(), SimulationError> {
        if worker_id != character_id {
            return Err(SimulationError::JobInvariantViolation);
        }
        let Some(character) = self.characters.get(&character_id) else {
            self.cancel_job(job_id)?;
            return Ok(());
        };
        if let Some(bed_id) = bed_id {
            let Some(bed) = self
                .construction_world
                .structure(bed_id)
                .filter(|structure| structure.kind() == structure::BED)
            else {
                self.cancel_job(job_id)?;
                return Ok(());
            };
            if character.position() != WorldPosition::from_cell_center(bed.cell())? {
                if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    self.cancel_job(job_id)?;
                }
                return Ok(());
            }
        }
        self.characters
            .get_mut(&character_id)
            .expect("sleep worker is present")
            .set_movement(MovementState::Idle);
        self.job_world
            .start_working(job_id, SLEEP_WORK_TICKS)
            .map_err(SimulationError::from_job_world)
    }

    pub(super) fn advance_working_sleep(
        &mut self,
        job_id: EntityId,
        character_id: EntityId,
        bed_id: Option<EntityId>,
        worker_id: EntityId,
        remaining_ticks: u32,
    ) -> Result<(), SimulationError> {
        if worker_id != character_id {
            return Err(SimulationError::JobInvariantViolation);
        }
        let Some(character) = self.characters.get(&character_id) else {
            self.cancel_job(job_id)?;
            return Ok(());
        };
        if let Some(bed_id) = bed_id {
            let Some(bed) = self
                .construction_world
                .structure(bed_id)
                .filter(|structure| structure.kind() == structure::BED)
            else {
                self.cancel_job(job_id)?;
                return Ok(());
            };
            if character.position() != WorldPosition::from_cell_center(bed.cell())? {
                self.cancel_job(job_id)?;
                return Ok(());
            }
        }
        if remaining_ticks > 1 {
            return self
                .job_world
                .set_remaining_work(job_id, remaining_ticks - 1)
                .map_err(SimulationError::from_job_world);
        }
        let sheltered =
            bed_id.is_some() && self.is_enclosed(character.position().containing_cell())?;
        let restoration = if bed_id.is_some() {
            if sheltered { crate::MAX_REST } else { 50 }
        } else {
            25
        };
        self.characters
            .get_mut(&character_id)
            .expect("sleep worker is present")
            .restore_rest(restoration);
        self.characters
            .get_mut(&character_id)
            .expect("sleep worker is present")
            .record_sleep_shelter(sheltered);
        self.job_world
            .remove(job_id)
            .map_err(SimulationError::from_job_world)?;
        Ok(())
    }

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
            let valid_food = self
                .item_world
                .get(item_id)
                .is_some_and(|item| item.kind().is_food() && item.ground_position().is_some());
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
                    (item.kind().is_food()
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
                if resource.kind().definition().yields.is_food()
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
            item.kind()
                .is_food()
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
    use progressus_content::{item, natural_resource};

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
    fn sleep_on_ground_is_an_explicit_job_that_restores_without_a_bed() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let sleeper = cora();
        set_rest(&mut simulation, sleeper, crate::TIRED_REST);
        simulation.advance_ticks(1).unwrap();
        let job_id = simulation
            .job_world
            .sleep_job_for_character(sleeper)
            .unwrap();
        assert_eq!(
            simulation.job_world.get(job_id).unwrap().kind(),
            JobKind::Sleep {
                character_id: sleeper,
                bed_id: None,
            }
        );
        let saved = simulation.save_json().unwrap();
        let mut restored = Simulation::load_json(&saved).unwrap();
        simulation
            .advance_ticks(u64::from(SLEEP_WORK_TICKS))
            .unwrap();
        restored.advance_ticks(u64::from(SLEEP_WORK_TICKS)).unwrap();
        assert_eq!(
            restored.save_json().unwrap(),
            simulation.save_json().unwrap()
        );
        assert!(
            simulation
                .job_world
                .sleep_job_for_character(sleeper)
                .is_none()
        );
        assert!(character(&simulation, sleeper).rest() > crate::TIRED_REST);
        assert!(simulation.job_world.indexes_are_consistent());
    }

    #[test]
    fn one_finished_bed_is_exclusive_and_other_tired_character_sleeps_on_ground() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let bed_cell = WorldCell::new(0, 1);
        simulation
            .set_terrain_override(bed_cell, terrain::GRASS)
            .unwrap();
        let bed_id = simulation.id_allocator.allocate().unwrap();
        simulation
            .construction_world
            .insert_site(ConstructionSite::new(bed_id, structure::BED, bed_cell))
            .unwrap();
        simulation.construction_world.complete_site(bed_id).unwrap();
        let ada = EntityId::new(1).unwrap();
        let borin = EntityId::new(2).unwrap();
        set_rest(&mut simulation, ada, crate::TIRED_REST);
        set_rest(&mut simulation, borin, crate::TIRED_REST);
        simulation.advance_ticks(1).unwrap();
        let bed_job = simulation.job_world.sleep_job_for_bed(bed_id).unwrap();
        assert!(matches!(simulation.job_world.get(bed_job).unwrap().kind(),
            JobKind::Sleep { bed_id: Some(id), .. } if id == bed_id));
        let other = if simulation.job_world.get(bed_job).unwrap().kind()
            == (JobKind::Sleep {
                character_id: ada,
                bed_id: Some(bed_id),
            }) {
            borin
        } else {
            ada
        };
        assert!(matches!(
            simulation
                .job_world
                .get(simulation.job_world.sleep_job_for_character(other).unwrap())
                .unwrap()
                .kind(),
            JobKind::Sleep { bed_id: None, .. }
        ));
        assert!(simulation.job_world.indexes_are_consistent());
    }

    #[test]
    fn sleep_job_survives_save_load_and_interrupt_releases_bed() {
        let mut original = Simulation::new(WorldSeed::new(0)).unwrap();
        let cell = WorldCell::new(0, 1);
        original.set_terrain_override(cell, terrain::GRASS).unwrap();
        let bed_id = original.id_allocator.allocate().unwrap();
        original
            .construction_world
            .insert_site(ConstructionSite::new(bed_id, structure::BED, cell))
            .unwrap();
        original.construction_world.complete_site(bed_id).unwrap();
        let sleeper = cora();
        set_rest(&mut original, sleeper, crate::TIRED_REST);
        original.advance_ticks(1).unwrap();
        let job_id = original.job_world.sleep_job_for_bed(bed_id).unwrap();
        let saved = original.save_json().unwrap();
        let mut restored = Simulation::load_json(&saved).unwrap();
        assert_eq!(restored.save_json().unwrap(), saved);
        original.advance_ticks(80).unwrap();
        restored.advance_ticks(80).unwrap();
        assert_eq!(restored.save_json().unwrap(), original.save_json().unwrap());

        let mut interrupted = Simulation::load_json(&saved).unwrap();
        interrupted.cancel_job(job_id).unwrap();
        assert_eq!(interrupted.job_world.sleep_job_for_bed(bed_id), None);
        assert_eq!(interrupted.job_world.sleep_job_for_character(sleeper), None);
        assert!(interrupted.job_world.indexes_are_consistent());

        let mut invalid_bed: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        let job = invalid_bed["jobs"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|job| job["id"] == job_id.value())
            .unwrap();
        job["job"]["bed_id"] = serde_json::Value::from(999_999_u64);
        assert!(Simulation::load_json(&serde_json::to_vec(&invalid_bed).unwrap()).is_err());

        let mut wrong_worker: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        let job = wrong_worker["jobs"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|job| job["id"] == job_id.value())
            .unwrap();
        job["state"]["worker_id"] = serde_json::Value::from(1_u64);
        assert!(Simulation::load_json(&serde_json::to_vec(&wrong_worker).unwrap()).is_err());
    }

    #[test]
    fn hungry_character_preempts_sleep_and_eats_first() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let sleeper = cora();
        set_rest(&mut simulation, sleeper, crate::TIRED_REST);
        simulation.advance_ticks(1).unwrap();
        assert!(
            simulation
                .job_world
                .sleep_job_for_character(sleeper)
                .is_some()
        );
        set_satiety(&mut simulation, sleeper, HUNGRY_SATIETY);
        simulation.advance_ticks(1).unwrap();
        assert!(
            simulation
                .job_world
                .sleep_job_for_character(sleeper)
                .is_none()
        );
        assert!(
            simulation
                .job_world
                .eat_job_for_character(sleeper)
                .is_some()
        );
        assert!(simulation.job_world.indexes_are_consistent());
    }

    #[test]
    fn sheltered_bed_restores_full_rest_while_open_bed_restores_fifty() {
        for enclosed in [false, true] {
            let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
            let center = WorldCell::new(0, 0);
            simulation
                .set_terrain_override(center, terrain::GRASS)
                .unwrap();
            let bed_id = simulation.id_allocator.allocate().unwrap();
            simulation
                .construction_world
                .insert_site(ConstructionSite::new(bed_id, structure::BED, center))
                .unwrap();
            simulation.construction_world.complete_site(bed_id).unwrap();
            if enclosed {
                for y in -1_i64..=1 {
                    for x in -1_i64..=1 {
                        if x.abs() != 1 && y.abs() != 1 {
                            continue;
                        }
                        let cell = WorldCell::new(x, y);
                        simulation
                            .set_terrain_override(cell, terrain::GRASS)
                            .unwrap();
                        let wall_id = simulation.id_allocator.allocate().unwrap();
                        simulation
                            .construction_world
                            .insert_site(ConstructionSite::new(
                                wall_id,
                                structure::STONE_WALL,
                                cell,
                            ))
                            .unwrap();
                        simulation
                            .construction_world
                            .complete_site(wall_id)
                            .unwrap();
                    }
                }
            } else {
                for y in -20..=20 {
                    for x in -20..=20 {
                        simulation
                            .set_terrain_override(WorldCell::new(x, y), terrain::GRASS)
                            .unwrap();
                    }
                }
            }
            let sleeper = cora();
            set_rest(&mut simulation, sleeper, crate::TIRED_REST);
            simulation.advance_ticks(1).unwrap();
            assert!(simulation.job_world.sleep_job_for_bed(bed_id).is_some());
            simulation
                .advance_ticks(u64::from(SLEEP_WORK_TICKS))
                .unwrap();
            assert_eq!(
                character(&simulation, sleeper).rest(),
                if enclosed { 100 } else { 79 }
            );
            let saved: serde_json::Value =
                serde_json::from_slice(&simulation.save_json().unwrap()).unwrap();
            let sleeper_save = saved["characters"]
                .as_array()
                .unwrap()
                .iter()
                .find(|value| value["id"] == sleeper.value())
                .unwrap();
            assert_eq!(sleeper_save["last_sleep_sheltered"], enclosed);
            let restored = Simulation::load_json(&simulation.save_json().unwrap()).unwrap();
            assert_eq!(
                character(&restored, sleeper).last_sleep_sheltered(),
                Some(enclosed)
            );
        }
    }

    #[test]
    fn manual_move_cancels_sleep_and_releases_bed_immediately() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let center = WorldCell::new(0, 0);
        simulation
            .set_terrain_override(center, terrain::GRASS)
            .unwrap();
        let bed_id = simulation.id_allocator.allocate().unwrap();
        simulation
            .construction_world
            .insert_site(ConstructionSite::new(bed_id, structure::BED, center))
            .unwrap();
        simulation.construction_world.complete_site(bed_id).unwrap();
        let sleeper = cora();
        set_rest(&mut simulation, sleeper, crate::TIRED_REST);
        simulation.advance_ticks(1).unwrap();
        assert!(simulation.job_world.sleep_job_for_bed(bed_id).is_some());
        simulation
            .set_movement_direction(sleeper, Direction::East)
            .unwrap();
        assert_eq!(simulation.job_world.sleep_job_for_bed(bed_id), None);
        assert_eq!(simulation.job_world.job_for_worker(sleeper), None);
        assert!(simulation.job_world.indexes_are_consistent());
    }

    #[test]
    fn blocked_sleep_route_releases_bed_reservation() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let bed_cell = WorldCell::new(0, 2);
        for cell in [WorldCell::new(0, 0), WorldCell::new(0, 1), bed_cell] {
            simulation
                .set_terrain_override(cell, terrain::GRASS)
                .unwrap();
        }
        let bed_id = simulation.id_allocator.allocate().unwrap();
        simulation
            .construction_world
            .insert_site(ConstructionSite::new(bed_id, structure::BED, bed_cell))
            .unwrap();
        simulation.construction_world.complete_site(bed_id).unwrap();
        let sleeper = cora();
        set_rest(&mut simulation, sleeper, crate::TIRED_REST);
        simulation.advance_ticks(1).unwrap();
        assert!(simulation.job_world.sleep_job_for_bed(bed_id).is_some());
        simulation
            .set_terrain_override(WorldCell::new(0, 1), terrain::ROCK)
            .unwrap();
        for _ in 0..8 {
            simulation.advance_ticks(1).unwrap();
            if simulation.job_world.sleep_job_for_bed(bed_id).is_none() {
                break;
            }
        }
        assert_eq!(simulation.job_world.sleep_job_for_bed(bed_id), None);
        assert!(simulation.job_world.indexes_are_consistent());
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
        let mut minimum_rest = crate::MAX_REST;
        let mut saw_sleep = false;
        for _ in 0..625 {
            simulation.advance_ticks(16).unwrap();
            saw_sleep |= simulation
                .jobs()
                .any(|job| matches!(job.kind(), JobKind::Sleep { .. }));
            for character in simulation.characters() {
                minimum_satiety = minimum_satiety.min(character.satiety());
                minimum_rest = minimum_rest.min(character.rest());
                assert!(
                    character.satiety() > 0,
                    "character {} starved at tick {}",
                    character.id().value(),
                    simulation.tick().value()
                );
                assert!(
                    character.rest() > 0,
                    "character {} became exhausted at tick {}",
                    character.id().value(),
                    simulation.tick().value()
                );
            }
            assert!(simulation.job_world.indexes_are_consistent());
            assert!(simulation.item_world.indexes_are_consistent());
        }
        assert!(minimum_satiety <= HUNGRY_SATIETY);
        assert!(minimum_rest <= crate::TIRED_REST);
        assert!(
            saw_sleep,
            "no character slept in the 10,000-tick settlement run"
        );
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
