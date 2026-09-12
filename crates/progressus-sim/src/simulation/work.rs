//! Generic job lifecycle: designation, assignment, advancement and interruption.

use super::*;

impl Simulation {
    pub fn designate_harvest(&mut self, source: WorldCell) -> Result<EntityId, SimulationError> {
        if !self.is_explored(source) {
            return Err(SimulationError::HarvestSourceUndiscovered(source));
        }
        if self.natural_resource_at(source)?.is_none() {
            return Err(SimulationError::NaturalResourceMissing(source));
        }
        if self.job_world.harvest_job_for_source(source).is_some() {
            return Err(SimulationError::HarvestAlreadyDesignated(source));
        }
        let id = self.id_allocator.allocate()?;
        self.job_world
            .insert(Job::new(id, JobKind::Harvest { source }))
            .map_err(SimulationError::from_job_world)?;
        Ok(id)
    }

    /// Orders one named character to walk to a ground item and equip it.
    /// Navigation, reservation, pickup and equipment all remain authoritative.
    pub fn designate_equipment_fetch(
        &mut self,
        character_id: EntityId,
        item_id: EntityId,
    ) -> Result<EntityId, SimulationError> {
        let item = self
            .item_world
            .get(item_id)
            .ok_or(SimulationError::UnknownItem(item_id))?;
        let position = item
            .ground_position()
            .ok_or(SimulationError::ItemNotOnGround(item_id))?;
        if item.quantity().get() != 1 {
            return Err(SimulationError::EquipmentStackMustBeSingle(item_id));
        }
        let slot = item
            .kind()
            .definition()
            .equip_slot
            .ok_or(SimulationError::ItemNotEquippable(item_id))?;
        if self.carried_load(character_id) + item.kind().load_cost(1) > HAND_LOAD_UNITS {
            return Err(SimulationError::CarryCapacityExceeded {
                character_id,
                item_id,
            });
        }
        self.ensure_item_tree_unreserved(item_id)?;
        if self
            .item_world
            .equipped_by(character_id)
            .any(|(filled, _)| filled == slot)
        {
            return Err(SimulationError::SlotAlreadyOccupied {
                character_id,
                item_id,
            });
        }
        let route = self.plan_navigation_route(character_id, position)?;
        self.interrupt_worker_job(character_id)?;
        let id = self.id_allocator.allocate()?;
        self.job_world
            .insert(Job::new(
                id,
                JobKind::EquipTool {
                    item_id,
                    requested_worker_id: Some(character_id),
                },
            ))
            .map_err(SimulationError::from_job_world)?;
        self.job_world
            .reserve_worker(id, character_id)
            .map_err(SimulationError::from_job_world)?;
        self.apply_navigation_route(character_id, position, route);
        Ok(id)
    }

    pub fn cancel_job(&mut self, job_id: EntityId) -> Result<(), SimulationError> {
        let job = self
            .job_world
            .get(job_id)
            .cloned()
            .ok_or(SimulationError::UnknownJob(job_id))?;
        if let JobState::Transporting { worker_id } = job.state() {
            let item_id = match job.kind() {
                JobKind::Haul { item_id, .. }
                | JobKind::SupplyProduction { item_id, .. }
                | JobKind::DeliverConstruction { item_id, .. } => Some(item_id),
                JobKind::PrepareConstruction {
                    target: ConstructionPreparationTarget::GroundItem { item_id, .. },
                    ..
                } => Some(item_id),
                JobKind::Harvest { .. }
                | JobKind::PrepareConstruction { .. }
                | JobKind::Eat { .. }
                | JobKind::Craft { .. }
                | JobKind::Construct { .. }
                | JobKind::EquipTool { .. } => None,
            };
            if let Some(item_id) = item_id {
                let position = self
                    .characters
                    .get(&worker_id)
                    .ok_or(SimulationError::UnknownCharacter(worker_id))?
                    .position();
                self.drop_item_for_job(worker_id, item_id, position)?;
                if let JobKind::DeliverConstruction { site_id, .. } = job.kind() {
                    self.construction_world
                        .mark_material_reserved(site_id, item_id)
                        .map_err(SimulationError::from_construction_world)?;
                }
            }
        }
        let worker = job.state().worker();
        self.job_world
            .remove(job_id)
            .map_err(SimulationError::from_job_world)?;
        if let Some(worker_id) = worker
            && let Some(character) = self.characters.get_mut(&worker_id)
        {
            character.set_movement(MovementState::Idle);
        }
        Ok(())
    }

    pub(super) fn interrupt_worker_job(
        &mut self,
        worker_id: EntityId,
    ) -> Result<(), SimulationError> {
        let Some(job_id) = self.job_world.job_for_worker(worker_id) else {
            return Ok(());
        };
        let job = self
            .job_world
            .get(job_id)
            .cloned()
            .ok_or(SimulationError::UnknownJob(job_id))?;
        if matches!(job.kind(), JobKind::Eat { .. }) {
            self.cancel_job(job_id)?;
            return Ok(());
        }
        if let JobState::Transporting { .. } = job.state() {
            let item_id = match job.kind() {
                JobKind::Haul { item_id, .. }
                | JobKind::SupplyProduction { item_id, .. }
                | JobKind::DeliverConstruction { item_id, .. } => Some(item_id),
                JobKind::PrepareConstruction {
                    target: ConstructionPreparationTarget::GroundItem { item_id, .. },
                    ..
                } => Some(item_id),
                JobKind::Harvest { .. }
                | JobKind::PrepareConstruction { .. }
                | JobKind::Eat { .. }
                | JobKind::Craft { .. }
                | JobKind::Construct { .. }
                | JobKind::EquipTool { .. } => None,
            };
            if let Some(item_id) = item_id {
                let position = self
                    .characters
                    .get(&worker_id)
                    .ok_or(SimulationError::UnknownCharacter(worker_id))?
                    .position();
                self.drop_item_for_job(worker_id, item_id, position)?;
                if let JobKind::DeliverConstruction { site_id, .. } = job.kind() {
                    self.construction_world
                        .mark_material_reserved(site_id, item_id)
                        .map_err(SimulationError::from_construction_world)?;
                }
            }
        }
        self.job_world
            .release_worker(job_id)
            .map_err(SimulationError::from_job_world)?;
        Ok(())
    }

    pub(super) fn advance_jobs_one_tick(&mut self) -> Result<(), SimulationError> {
        let job_ids = self.job_world.iter().map(Job::id).collect::<Vec<_>>();
        for job_id in job_ids {
            let Some(job) = self.job_world.get(job_id).cloned() else {
                continue;
            };
            match job.state() {
                JobState::Available => self.try_assign_job(job_id, job.kind())?,
                JobState::Reserved { worker_id } => {
                    self.advance_reserved_job(job_id, job.kind(), worker_id)?
                }
                JobState::Transporting { worker_id } => {
                    self.advance_transporting_job(job_id, job.kind(), worker_id)?
                }
                JobState::Working {
                    worker_id,
                    remaining_ticks,
                } => self.advance_working_job(job_id, job.kind(), worker_id, remaining_ticks)?,
            }
        }
        Ok(())
    }

    pub(super) fn try_assign_job(
        &mut self,
        job_id: EntityId,
        kind: JobKind,
    ) -> Result<(), SimulationError> {
        match kind {
            JobKind::EquipTool {
                item_id,
                requested_worker_id,
            } => self.try_assign_equip(job_id, item_id, requested_worker_id),
            JobKind::Harvest { source } => self.try_assign_harvest(job_id, source),
            JobKind::Eat {
                character_id,
                item_id,
            } => self.try_assign_eat(job_id, character_id, item_id),
            JobKind::Haul {
                item_id,
                stockpile_id,
                destination,
            } => self.try_assign_haul(job_id, item_id, stockpile_id, destination),
            JobKind::Craft {
                workstation_id,
                order_id,
                recipe_id,
            } => self.try_assign_craft(job_id, workstation_id, order_id, recipe_id),
            JobKind::SupplyProduction {
                workstation_id,
                item_id,
                destination,
            } => self.try_assign_production_supply(job_id, workstation_id, item_id, destination),
            JobKind::DeliverConstruction { site_id, item_id } => {
                self.try_assign_construction_delivery(job_id, site_id, item_id)
            }
            JobKind::Construct { site_id } => self.try_assign_construct(job_id, site_id),
            JobKind::PrepareConstruction { site_id, target } => {
                self.try_assign_construction_preparation(job_id, site_id, target)
            }
        }
    }

    pub(super) fn available_workers_by_distance(&self, target: WorldCell) -> Vec<EntityId> {
        let mut candidates = self
            .characters
            .values()
            .filter(|character| {
                character.is_available_for_work()
                    && !character.is_starving()
                    && self.job_world.job_for_worker(character.id()).is_none()
                    && !self.character_has_held_payload(character.id())
            })
            .map(|character| {
                (
                    cell_manhattan_distance(character.position().containing_cell(), target),
                    character.id(),
                )
            })
            .collect::<Vec<_>>();
        candidates.sort_unstable();
        candidates.into_iter().map(|(_, id)| id).collect()
    }

    /// Sends someone to fetch a tool when work is waiting that nobody is
    /// equipped to do. Without this the requirement would simply stall the
    /// job forever, since nothing else ever puts a tool in a slot.
    pub(super) fn maintain_equip_jobs(&mut self) -> Result<(), SimulationError> {
        let mut wanted: BTreeSet<CapabilityId> = BTreeSet::new();
        for job in self.job_world.iter() {
            let source = match (job.kind(), job.state()) {
                (JobKind::Harvest { source }, JobState::Available)
                | (
                    JobKind::PrepareConstruction {
                        target: ConstructionPreparationTarget::NaturalResource { source },
                        ..
                    },
                    JobState::Available,
                ) => source,
                _ => continue,
            };
            let Some(resource) = self.natural_resource_at(source)? else {
                continue;
            };
            for capability in resource.kind().definition().requires {
                if !self
                    .characters
                    .keys()
                    .any(|id| self.can_perform(*id, *capability))
                {
                    wanted.insert(*capability);
                }
            }
        }
        if wanted.is_empty() {
            return Ok(());
        }

        for capability in wanted {
            // A tool already on its way is enough; do not send a second worker.
            if self.job_world.iter().any(|job| {
                let JobKind::EquipTool { item_id, .. } = job.kind() else {
                    return false;
                };
                self.item_world
                    .get(item_id)
                    .is_some_and(|item| item.kind().provides(capability))
            }) {
                continue;
            }
            let Some(item_id) = self
                .item_world
                .iter()
                .filter(|item| {
                    item.kind().provides(capability)
                        && item.ground_position().is_some()
                        && self.job_world.item_job_for_item(item.id()).is_none()
                })
                .map(ItemStack::id)
                .next()
            else {
                continue;
            };
            let job_id = self.id_allocator.allocate()?;
            self.job_world
                .insert(Job::new(
                    job_id,
                    JobKind::EquipTool {
                        item_id,
                        requested_worker_id: None,
                    },
                ))
                .map_err(SimulationError::from_job_world)?;
        }
        Ok(())
    }

    /// Sends the nearest worker with a free tool slot to fetch this tool.
    pub(super) fn try_assign_equip(
        &mut self,
        job_id: EntityId,
        item_id: EntityId,
        requested_worker_id: Option<EntityId>,
    ) -> Result<(), SimulationError> {
        let Some(item) = self.item_world.get(item_id) else {
            self.job_world
                .remove(job_id)
                .map_err(SimulationError::from_job_world)?;
            return Ok(());
        };
        let Some(position) = item.ground_position() else {
            self.cancel_job(job_id)?;
            return Ok(());
        };
        let Some(slot) = item.kind().definition().equip_slot else {
            self.cancel_job(job_id)?;
            return Ok(());
        };
        let cell = position.containing_cell();
        let workers = match requested_worker_id {
            Some(worker_id) => vec![worker_id],
            None => self.available_workers_by_distance(cell),
        };
        for worker_id in workers {
            let Some(character) = self.characters.get(&worker_id) else {
                self.cancel_job(job_id)?;
                return Ok(());
            };
            if !character.is_available_for_work()
                || character.is_starving()
                || self.job_world.job_for_worker(worker_id).is_some()
                || self.character_has_held_payload(worker_id)
            {
                continue;
            }
            if self
                .item_world
                .equipped_by(worker_id)
                .any(|(filled, _)| filled == slot)
            {
                continue;
            }
            let route = match self.plan_navigation_route(worker_id, position) {
                Ok(route) => route,
                Err(
                    SimulationError::MoveToDestinationBlocked(_)
                    | SimulationError::MoveToDestinationUndiscovered(_)
                    | SimulationError::MoveToPathNotFound
                    | SimulationError::MoveToSearchBudgetExceeded,
                ) => continue,
                Err(error) => return Err(error),
            };
            self.job_world
                .reserve_worker(job_id, worker_id)
                .map_err(SimulationError::from_job_world)?;
            self.apply_navigation_route(worker_id, position, route);
            return Ok(());
        }
        Ok(())
    }

    pub(super) fn try_assign_harvest(
        &mut self,
        job_id: EntityId,
        source: WorldCell,
    ) -> Result<(), SimulationError> {
        if self.natural_resource_at(source)?.is_none() {
            self.job_world
                .remove(job_id)
                .map_err(SimulationError::from_job_world)?;
            return Ok(());
        }
        let required = self
            .natural_resource_at(source)?
            .map(|resource| resource.kind().definition().requires)
            .unwrap_or_default();
        let destination = WorldPosition::from_cell_center(source)?;
        for worker_id in self.available_workers_by_distance(source) {
            if !self.meets_requirements(worker_id, required) {
                continue;
            }
            let route = match self.plan_navigation_route(worker_id, destination) {
                Ok(route) => route,
                Err(
                    SimulationError::MoveToDestinationBlocked(_)
                    | SimulationError::MoveToDestinationUndiscovered(_)
                    | SimulationError::MoveToPathNotFound
                    | SimulationError::MoveToSearchBudgetExceeded,
                ) => continue,
                Err(error) => return Err(error),
            };
            self.job_world
                .reserve_worker(job_id, worker_id)
                .map_err(SimulationError::from_job_world)?;
            self.apply_navigation_route(worker_id, destination, route);
            return Ok(());
        }
        Ok(())
    }

    pub(super) fn advance_reserved_job(
        &mut self,
        job_id: EntityId,
        kind: JobKind,
        worker_id: EntityId,
    ) -> Result<(), SimulationError> {
        match kind {
            JobKind::EquipTool { item_id, .. } => {
                let Some(item) = self.item_world.get(item_id) else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                let Some(item_position) = item.ground_position() else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                let Some(slot) = item.kind().definition().equip_slot else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                if self.carried_load(worker_id) + item.kind().load_cost(1) > HAND_LOAD_UNITS
                    || self
                        .item_world
                        .equipped_by(worker_id)
                        .any(|(filled, _)| filled == slot)
                {
                    return Ok(());
                }
                let Some(character) = self.characters.get(&worker_id) else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    item_position,
                    InteractionRadius::zero(),
                ) {
                    // Take it and put it away in one step: a tool never
                    // travels in the hands, so it never occupies them.
                    self.pick_up_within_capacity(worker_id, item_id)?;
                    self.equip_item_for_job(worker_id, item_id)?;
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    if let Some(character) = self.characters.get_mut(&worker_id) {
                        character.set_movement(MovementState::Idle);
                    }
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::Harvest { source }
            | JobKind::PrepareConstruction {
                target: ConstructionPreparationTarget::NaturalResource { source },
                ..
            } => {
                if self.natural_resource_at(source)?.is_none() {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                let Some(character) = self.characters.get(&worker_id) else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                let target = WorldPosition::from_cell_center(source)?;
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    target,
                    InteractionRadius::zero(),
                ) {
                    self.characters
                        .get_mut(&worker_id)
                        .expect("worker was checked above")
                        .set_movement(MovementState::Idle);
                    self.job_world
                        .start_working(job_id, HARVEST_WORK_TICKS)
                        .map_err(SimulationError::from_job_world)?;
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::Eat {
                character_id,
                item_id,
            } => {
                if character_id != worker_id
                    || !self
                        .characters
                        .get(&character_id)
                        .is_some_and(Character::is_hungry)
                {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                let Some((item_position, _nutrition)) =
                    self.item_world.get(item_id).and_then(|item| {
                        let nutrition = item.kind().definition().nutrition;
                        (nutrition > 0)
                            .then(|| item.ground_position())
                            .flatten()
                            .map(|position| (position, nutrition))
                    })
                else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                let character = self
                    .characters
                    .get(&worker_id)
                    .expect("eat worker was validated above");
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    item_position,
                    InteractionRadius::zero(),
                ) {
                    self.characters
                        .get_mut(&worker_id)
                        .expect("eat worker is still present")
                        .set_movement(MovementState::Idle);
                    self.job_world
                        .start_working(job_id, EAT_WORK_TICKS)
                        .map_err(SimulationError::from_job_world)?;
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::Haul {
                item_id,
                stockpile_id,
                destination,
            } => {
                if self.stockpile_world.stockpile_at(destination) != Some(stockpile_id) {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                let Some(item_position) = self
                    .item_world
                    .get(item_id)
                    .and_then(ItemStack::ground_position)
                else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                let Some(character) = self.characters.get(&worker_id) else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    item_position,
                    InteractionRadius::zero(),
                ) {
                    let target = WorldPosition::from_cell_center(destination)?;
                    let route = match self.plan_navigation_route(worker_id, target) {
                        Ok(route) => route,
                        Err(
                            SimulationError::MoveToDestinationBlocked(_)
                            | SimulationError::MoveToDestinationUndiscovered(_)
                            | SimulationError::MoveToPathNotFound
                            | SimulationError::MoveToSearchBudgetExceeded,
                        ) => {
                            self.job_world
                                .release_worker(job_id)
                                .map_err(SimulationError::from_job_world)?;
                            return Ok(());
                        }
                        Err(error) => return Err(error),
                    };
                    self.pick_up_within_capacity(worker_id, item_id)?;
                    self.job_world
                        .start_transporting(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    self.apply_navigation_route(worker_id, target, route);
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::SupplyProduction {
                workstation_id,
                item_id,
                destination,
            } => {
                if self.production_logistics_world.zone_at(destination)
                    != Some((workstation_id, ProductionZoneKind::Input))
                {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                let Some(item_position) = self
                    .item_world
                    .get(item_id)
                    .and_then(ItemStack::ground_position)
                else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                let Some(character) = self.characters.get(&worker_id) else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    item_position,
                    InteractionRadius::zero(),
                ) {
                    let target = WorldPosition::from_cell_center(destination)?;
                    let route = match self.plan_navigation_route(worker_id, target) {
                        Ok(route) => route,
                        Err(
                            SimulationError::MoveToDestinationBlocked(_)
                            | SimulationError::MoveToDestinationUndiscovered(_)
                            | SimulationError::MoveToPathNotFound
                            | SimulationError::MoveToSearchBudgetExceeded,
                        ) => {
                            self.job_world
                                .release_worker(job_id)
                                .map_err(SimulationError::from_job_world)?;
                            return Ok(());
                        }
                        Err(error) => return Err(error),
                    };
                    self.pick_up_within_capacity(worker_id, item_id)?;
                    self.job_world
                        .start_transporting(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    self.apply_navigation_route(worker_id, target, route);
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::Craft {
                workstation_id,
                order_id,
                recipe_id,
            } => {
                if self.production_world.get(order_id).is_none_or(|order| {
                    order.workstation_id() != workstation_id
                        || order.recipe_id() != recipe_id
                        || !order.is_pending()
                }) {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                if !self.craft_reserved_inputs_valid(job_id, workstation_id, recipe_id) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                let Some(workstation) = self.workstation_world.get(workstation_id) else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                if !self.is_walkable(workstation.cell())? {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                let Some(character) = self.characters.get(&worker_id) else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                let target = WorldPosition::from_cell_center(workstation.cell())?;
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    target,
                    InteractionRadius::zero(),
                ) {
                    self.characters
                        .get_mut(&worker_id)
                        .expect("worker was checked above")
                        .set_movement(MovementState::Idle);
                    self.job_world
                        .start_working(job_id, recipe_id.definition().work_ticks)
                        .map_err(SimulationError::from_job_world)?;
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::DeliverConstruction { site_id, item_id } => {
                let Some(site) = self.construction_world.site(site_id).cloned() else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                if site.material_item_id() != Some(item_id)
                    || site.material_state() != Some(ConstructionMaterialState::Reserved)
                {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                let Some(item_position) = self
                    .item_world
                    .get(item_id)
                    .and_then(ItemStack::ground_position)
                else {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                let Some(character) = self.characters.get(&worker_id) else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    item_position,
                    InteractionRadius::zero(),
                ) {
                    let Some(access_cell) = self.construction_access_cell(site.cell())? else {
                        self.job_world
                            .release_worker(job_id)
                            .map_err(SimulationError::from_job_world)?;
                        return Ok(());
                    };
                    let target = self.construction_access_position(site.cell(), access_cell)?;
                    let route = match self.plan_navigation_route(worker_id, target) {
                        Ok(route) => route,
                        Err(
                            SimulationError::MoveToDestinationBlocked(_)
                            | SimulationError::MoveToDestinationUndiscovered(_)
                            | SimulationError::MoveToPathNotFound
                            | SimulationError::MoveToSearchBudgetExceeded,
                        ) => {
                            self.job_world
                                .release_worker(job_id)
                                .map_err(SimulationError::from_job_world)?;
                            return Ok(());
                        }
                        Err(error) => return Err(error),
                    };
                    self.pick_up_within_capacity(worker_id, item_id)?;
                    self.job_world
                        .start_transporting(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    self.apply_navigation_route(worker_id, target, route);
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::Construct { site_id } => {
                let Some(site) = self.construction_world.site(site_id).cloned() else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                if site.material_state() != Some(ConstructionMaterialState::Delivered) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                let Some(access_cell) = self.construction_access_cell(site.cell())? else {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                let target = self.construction_access_position(site.cell(), access_cell)?;
                let Some(character) = self.characters.get(&worker_id) else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    WorldPosition::from_cell_center(site.cell())?,
                    InteractionRadius::zero(),
                ) {
                    self.characters
                        .get_mut(&worker_id)
                        .expect("worker was checked above")
                        .set_movement(MovementState::Idle);
                    self.job_world
                        .start_working(job_id, site.kind().definition().work_ticks)
                        .map_err(SimulationError::from_job_world)?;
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    let route = self.plan_navigation_route(worker_id, target)?;
                    self.apply_navigation_route(worker_id, target, route);
                }
            }
            JobKind::PrepareConstruction {
                site_id,
                target:
                    ConstructionPreparationTarget::GroundItem {
                        item_id,
                        destination,
                    },
            } => {
                let Some(site_cell) = self.construction_project_cell(site_id) else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                let Some(item_position) = self
                    .item_world
                    .get(item_id)
                    .and_then(ItemStack::ground_position)
                else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                if item_position.containing_cell() != site_cell
                    || !self.construction_preparation_destination_is_valid(destination)?
                {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                let Some(character) = self.characters.get(&worker_id) else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    item_position,
                    InteractionRadius::zero(),
                ) {
                    let target = WorldPosition::from_cell_center(destination)?;
                    let route = match self.plan_navigation_route(worker_id, target) {
                        Ok(route) => route,
                        Err(
                            SimulationError::MoveToDestinationBlocked(_)
                            | SimulationError::MoveToDestinationUndiscovered(_)
                            | SimulationError::MoveToPathNotFound
                            | SimulationError::MoveToSearchBudgetExceeded,
                        ) => {
                            self.job_world
                                .release_worker(job_id)
                                .map_err(SimulationError::from_job_world)?;
                            return Ok(());
                        }
                        Err(error) => return Err(error),
                    };
                    self.pick_up_within_capacity(worker_id, item_id)?;
                    self.job_world
                        .start_transporting(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    self.apply_navigation_route(worker_id, target, route);
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::PrepareConstruction {
                site_id,
                target:
                    ConstructionPreparationTarget::Character {
                        character_id,
                        destination,
                    },
            } => {
                if character_id != worker_id || self.construction_project_cell(site_id).is_none() {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                let Some(character) = self.characters.get(&worker_id) else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                if character.position().containing_cell()
                    != self.construction_project_cell(site_id).unwrap()
                {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    self.characters
                        .get_mut(&worker_id)
                        .expect("worker was checked above")
                        .set_movement(MovementState::Idle);
                    self.ensure_construction_job(site_id)?;
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    let target = WorldPosition::from_cell_center(destination)?;
                    let route = self.plan_navigation_route(worker_id, target)?;
                    self.apply_navigation_route(worker_id, target, route);
                }
            }
        }
        Ok(())
    }

    pub(super) fn advance_transporting_job(
        &mut self,
        job_id: EntityId,
        kind: JobKind,
        worker_id: EntityId,
    ) -> Result<(), SimulationError> {
        match kind {
            // An equip job finishes the moment the tool is in its slot, so it
            // never reaches the transporting state.
            JobKind::EquipTool { .. } => return Err(SimulationError::JobInvariantViolation),
            JobKind::Haul {
                item_id,
                stockpile_id,
                destination,
            } => {
                if self.stockpile_world.stockpile_at(destination) != Some(stockpile_id) {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                let Some(character) = self.characters.get(&worker_id) else {
                    return Err(SimulationError::JobInvariantViolation);
                };
                if self.item_world.holder_of(item_id) != Some(worker_id) {
                    return Err(SimulationError::JobInvariantViolation);
                }
                let target = WorldPosition::from_cell_center(destination)?;
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    target,
                    InteractionRadius::zero(),
                ) {
                    let merge_target = self.stockpile_merge_target(item_id, destination);
                    self.drop_item_for_job(worker_id, item_id, target)?;
                    if let Some(target_id) = merge_target {
                        self.item_world
                            .merge_ground_stacks(target_id, item_id)
                            .expect("haul destination capacity was validated before delivery");
                    }
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    self.characters
                        .get_mut(&worker_id)
                        .expect("worker was checked above")
                        .set_movement(MovementState::Idle);
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    let position = character.position();
                    self.drop_item_for_job(worker_id, item_id, position)?;
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::SupplyProduction {
                workstation_id,
                item_id,
                destination,
            } => {
                if self.production_logistics_world.zone_at(destination)
                    != Some((workstation_id, ProductionZoneKind::Input))
                {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                let Some(character) = self.characters.get(&worker_id) else {
                    return Err(SimulationError::JobInvariantViolation);
                };
                if self.item_world.holder_of(item_id) != Some(worker_id) {
                    return Err(SimulationError::JobInvariantViolation);
                }
                let target = WorldPosition::from_cell_center(destination)?;
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    target,
                    InteractionRadius::zero(),
                ) {
                    let merge_target = self.stockpile_merge_target(item_id, destination);
                    self.drop_item_for_job(worker_id, item_id, target)?;
                    if let Some(target_id) = merge_target {
                        self.item_world
                            .merge_ground_stacks(target_id, item_id)
                            .map_err(|_| SimulationError::JobInvariantViolation)?;
                    }
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    self.characters
                        .get_mut(&worker_id)
                        .expect("worker was checked above")
                        .set_movement(MovementState::Idle);
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    let position = character.position();
                    self.drop_item_for_job(worker_id, item_id, position)?;
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::DeliverConstruction { site_id, item_id } => {
                let Some(site) = self.construction_world.site(site_id).cloned() else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                if self.item_world.holder_of(item_id) != Some(worker_id) {
                    return Err(SimulationError::ConstructionInvariantViolation);
                }
                let Some(character) = self.characters.get(&worker_id) else {
                    return Err(SimulationError::ConstructionInvariantViolation);
                };
                let Some(access_cell) = self.construction_access_cell(site.cell())? else {
                    let position = character.position();
                    self.drop_item_for_job(worker_id, item_id, position)?;
                    self.construction_world
                        .mark_material_reserved(site_id, item_id)
                        .map_err(SimulationError::from_construction_world)?;
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                let target = self.construction_access_position(site.cell(), access_cell)?;
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    target,
                    InteractionRadius::zero(),
                ) {
                    self.drop_item_for_job(worker_id, item_id, target)?;
                    self.construction_world
                        .mark_material_delivered(site_id, item_id)
                        .map_err(SimulationError::from_construction_world)?;
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    self.characters
                        .get_mut(&worker_id)
                        .expect("worker was checked above")
                        .set_movement(MovementState::Idle);
                    self.ensure_construction_job(site_id)?;
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    let position = character.position();
                    self.drop_item_for_job(worker_id, item_id, position)?;
                    self.construction_world
                        .mark_material_reserved(site_id, item_id)
                        .map_err(SimulationError::from_construction_world)?;
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::PrepareConstruction {
                site_id,
                target:
                    ConstructionPreparationTarget::GroundItem {
                        item_id,
                        destination,
                    },
            } => {
                if self.construction_project_cell(site_id).is_none() {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                let Some(character) = self.characters.get(&worker_id) else {
                    return Err(SimulationError::ConstructionInvariantViolation);
                };
                if self.item_world.holder_of(item_id) != Some(worker_id) {
                    return Err(SimulationError::ConstructionInvariantViolation);
                }
                let target = WorldPosition::from_cell_center(destination)?;
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    target,
                    InteractionRadius::zero(),
                ) && self.construction_preparation_destination_is_valid(destination)?
                {
                    self.drop_item_for_job(worker_id, item_id, target)?;
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    self.characters
                        .get_mut(&worker_id)
                        .expect("worker was checked above")
                        .set_movement(MovementState::Idle);
                    self.ensure_construction_job(site_id)?;
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    let position = character.position();
                    self.drop_item_for_job(worker_id, item_id, position)?;
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::Harvest { .. }
            | JobKind::Eat { .. }
            | JobKind::Craft { .. }
            | JobKind::Construct { .. }
            | JobKind::PrepareConstruction { .. } => {
                return Err(SimulationError::JobInvariantViolation);
            }
        }
        Ok(())
    }

    pub(super) fn advance_working_job(
        &mut self,
        job_id: EntityId,
        kind: JobKind,
        worker_id: EntityId,
        remaining_ticks: u32,
    ) -> Result<(), SimulationError> {
        match kind {
            // An equip job ends when the tool is stowed; it has no work phase.
            JobKind::EquipTool { .. } => return Err(SimulationError::JobInvariantViolation),
            JobKind::Harvest { source } => {
                let Some(resource) = self.natural_resource_at(source)? else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                if !self.characters.contains_key(&worker_id) {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                if remaining_ticks > 1 {
                    self.job_world
                        .set_remaining_work(job_id, remaining_ticks - 1)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                self.complete_harvest(job_id, worker_id, source, resource)?;
            }
            JobKind::PrepareConstruction {
                site_id,
                target: ConstructionPreparationTarget::NaturalResource { source },
            } => {
                let Some(resource) = self.natural_resource_at(source)? else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                if !self.characters.contains_key(&worker_id) {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                if remaining_ticks > 1 {
                    self.job_world
                        .set_remaining_work(job_id, remaining_ticks - 1)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                self.complete_harvest(job_id, worker_id, source, resource)?;
                if self.renewable_resource_regrowth.remove(&source).is_some() {
                    self.depleted_resources.insert(source);
                }
                self.ensure_construction_job(site_id)?;
            }
            JobKind::Eat {
                character_id,
                item_id,
            } => {
                if character_id != worker_id {
                    return Err(SimulationError::JobInvariantViolation);
                }
                let Some(character) = self.characters.get(&character_id) else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                let Some((item_position, nutrition)) =
                    self.item_world.get(item_id).and_then(|item| {
                        let nutrition = item.kind().definition().nutrition;
                        (nutrition > 0)
                            .then(|| item.ground_position())
                            .flatten()
                            .map(|position| (position, nutrition))
                    })
                else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                if !within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    item_position,
                    InteractionRadius::zero(),
                ) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                if remaining_ticks > 1 {
                    self.job_world
                        .set_remaining_work(job_id, remaining_ticks - 1)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                self.item_world
                    .consume(item_id, 1)
                    .map_err(|_| SimulationError::JobInvariantViolation)?;
                self.characters
                    .get_mut(&character_id)
                    .expect("eat worker is still present")
                    .restore_satiety(nutrition);
                self.job_world
                    .remove(job_id)
                    .map_err(SimulationError::from_job_world)?;
                self.characters
                    .get_mut(&character_id)
                    .expect("eat worker is still present")
                    .set_movement(MovementState::Idle);
            }
            JobKind::Haul { .. } | JobKind::SupplyProduction { .. } => {
                return Err(SimulationError::JobInvariantViolation);
            }
            JobKind::PrepareConstruction { .. } => {
                return Err(SimulationError::ConstructionInvariantViolation);
            }
            JobKind::Craft {
                workstation_id,
                order_id,
                recipe_id,
            } => {
                if self.production_world.get(order_id).is_none_or(|order| {
                    order.workstation_id() != workstation_id
                        || order.recipe_id() != recipe_id
                        || !order.is_pending()
                }) {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                if !self.craft_reserved_inputs_valid(job_id, workstation_id, recipe_id) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                if !self.characters.contains_key(&worker_id) {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                if remaining_ticks > 1 {
                    self.job_world
                        .set_remaining_work(job_id, remaining_ticks - 1)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                self.complete_craft(job_id, worker_id, workstation_id, order_id, recipe_id)?;
            }
            JobKind::DeliverConstruction { .. } => {
                return Err(SimulationError::ConstructionInvariantViolation);
            }
            JobKind::Construct { site_id } => {
                if !self.construction_site_ready(site_id) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                if !self.characters.contains_key(&worker_id) {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                if remaining_ticks > 1 {
                    self.job_world
                        .set_remaining_work(job_id, remaining_ticks - 1)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                self.complete_construction(job_id, worker_id, site_id)?;
            }
        }
        Ok(())
    }

    pub(super) fn complete_harvest(
        &mut self,
        job_id: EntityId,
        worker_id: EntityId,
        source: WorldCell,
        resource: NaturalResource,
    ) -> Result<(), SimulationError> {
        let next_resource_revision = self
            .resource_revision
            .checked_add(1)
            .ok_or(SimulationError::ResourceRevisionOverflow)?;
        let renewable_ready_tick = match resource.kind().definition().regrow_ticks {
            Some(delay) => Some(SimulationTick::new(
                self.clock
                    .tick()
                    .value()
                    .checked_add(delay)
                    .ok_or(SimulationError::TickOverflow)?,
            )),
            None => None,
        };
        let item_id = self.id_allocator.allocate()?;
        let kind = resource.kind().definition().yields;
        let quantity = ItemQuantity::new(resource.yield_quantity())
            .expect("worldgen natural-resource yields are positive");
        let position = WorldPosition::from_cell_center(source)?;
        self.item_world
            .insert_ground(ItemStack::new_ground(item_id, kind, quantity, position))
            .expect("allocated item IDs are unique and harvested outputs start on the ground");
        match renewable_ready_tick {
            Some(ready_tick) => {
                if self
                    .renewable_resource_regrowth
                    .insert(source, ready_tick)
                    .is_some()
                {
                    return Err(SimulationError::JobInvariantViolation);
                }
            }
            None => {
                if !self.depleted_resources.insert(source) {
                    return Err(SimulationError::JobInvariantViolation);
                }
            }
        }
        self.resource_revision = next_resource_revision;
        self.job_world
            .remove(job_id)
            .map_err(SimulationError::from_job_world)?;
        if let Some(character) = self.characters.get_mut(&worker_id) {
            character.set_movement(MovementState::Idle);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use progressus_content::{capability, item, natural_resource, slot};

    /// Copper needs a pick. Nobody starts with one, so the settlement must
    /// fetch and equip the tool it crafted before it can mine at all — which
    /// is the first time the game's only production chain has a purpose.
    #[test]
    fn mining_waits_for_a_tool_and_then_proceeds() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        // Copper is deliberately kept clear of the starting clearing, so the
        // settlement must reach it the way a player would: by going there.
        let vein = (-40..40)
            .flat_map(|y| (-40..40).map(move |x| WorldCell::new(x, y)))
            .find(|cell| {
                simulation
                    .generator
                    .natural_resource_at(*cell)
                    .is_some_and(|r| r.kind() == natural_resource::COPPER_VEIN)
                    && simulation.is_walkable(*cell).unwrap_or(false)
            })
            .expect("the copper layer places a vein within reach of the start");
        let scout = cora();
        let approach = Direction::East
            .adjacent(vein)
            .filter(|cell| simulation.is_walkable(*cell).unwrap_or(false))
            .unwrap_or(vein);
        place_on_grass(&mut simulation, scout, approach);
        simulation.advance_ticks(1).unwrap();
        assert!(
            simulation.is_explored(vein),
            "the scout did not reveal the vein"
        );

        let requires = natural_resource::COPPER_VEIN.definition().requires;
        assert_eq!(requires, [capability::MINE]);

        // Nobody is equipped, so nobody qualifies for the work.
        for id in simulation.characters.keys() {
            assert!(!simulation.meets_requirements(*id, requires));
        }

        // Put a pick within reach and let the settlement notice it.
        let ground = WorldPosition::from_cell_center(approach).unwrap();
        let tool = simulation.id_allocator.allocate().unwrap();
        simulation
            .item_world
            .insert_ground(ItemStack::new_ground(
                tool,
                item::PRIMITIVE_TOOL,
                ItemQuantity::new(1).unwrap(),
                ground,
            ))
            .unwrap();
        simulation
            .designate_harvest(vein)
            .unwrap_or_else(|_| panic!("the fixture needs a designatable vein at {vein:?}"));

        let mut equipped_by = None;
        for _ in 0..2048 {
            simulation.advance_ticks(1).unwrap();
            if let Some(id) = simulation
                .characters
                .keys()
                .copied()
                .find(|id| simulation.can_perform(*id, capability::MINE))
            {
                equipped_by = Some(id);
                break;
            }
        }
        let equipped_by =
            equipped_by.expect("nobody ever fetched the pick, so mining could never start");
        assert_eq!(simulation.equipment(equipped_by), vec![(slot::TOOL, tool)]);
        assert_eq!(
            simulation.carried_load(equipped_by),
            0,
            "the pick was still weighing on its bearer's hands"
        );
        assert!(simulation.item_world.indexes_are_consistent());
    }

    /// One tool, one fetcher: a second worker must not be sent after a pick
    /// that is already on its way to someone's belt.
    #[test]
    fn only_one_worker_is_sent_for_the_same_tool() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let ground = WorldPosition::from_cell_center(WorldCell::new(1, 0)).unwrap();
        let tool = simulation.id_allocator.allocate().unwrap();
        simulation
            .item_world
            .insert_ground(ItemStack::new_ground(
                tool,
                item::PRIMITIVE_TOOL,
                ItemQuantity::new(1).unwrap(),
                ground,
            ))
            .unwrap();
        simulation.designate_harvest(WorldCell::new(4, 0)).ok();

        for _ in 0..64 {
            simulation.advance_ticks(1).unwrap();
            let equip_jobs = simulation
                .job_world
                .iter()
                .filter(|job| matches!(job.kind(), JobKind::EquipTool { .. }))
                .count();
            assert!(equip_jobs <= 1, "{equip_jobs} workers chased one pick");
        }
    }

    #[test]
    fn player_order_sends_the_named_character_to_fetch_and_equip_a_cart() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let character = cora();
        let target = (-5..=5)
            .flat_map(|y| (-7..=7).map(move |x| WorldCell::new(x, y)))
            .filter(|cell| simulation.is_explored(*cell))
            .filter(|cell| simulation.is_walkable(*cell).unwrap_or(false))
            .find(|cell| {
                simulation
                    .plan_navigation_route(
                        character,
                        WorldPosition::from_cell_center(*cell).unwrap(),
                    )
                    .is_ok()
            })
            .expect("seed 0 must expose a reachable ground cell");
        let cart = insert_ground_stack(&mut simulation, item::CART, 1, target);

        let job_id = simulation
            .designate_equipment_fetch(character, cart)
            .unwrap();
        assert_eq!(simulation.job_for_worker(character), Some(job_id));
        assert_eq!(
            simulation.job_world.get(job_id).unwrap().kind(),
            JobKind::EquipTool {
                item_id: cart,
                requested_worker_id: Some(character),
            }
        );

        let bytes = simulation.save_json().unwrap();
        let mut simulation = Simulation::load_json(&bytes).unwrap();
        for _ in 0..2048 {
            simulation.advance_ticks(1).unwrap();
            if simulation.equipment(character) == vec![(slot::TOOL, cart)] {
                break;
            }
        }

        assert_eq!(simulation.equipment(character), vec![(slot::TOOL, cart)]);
        assert!(
            simulation
                .item_world
                .get(cart)
                .unwrap()
                .ground_position()
                .is_none()
        );
        assert_eq!(simulation.job_for_worker(character), None);
        assert!(simulation.item_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
    }

    #[test]
    fn equipment_order_rejects_full_hands_before_reserving_the_item() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let character = cora();
        let cell = simulation.characters[&character]
            .position()
            .containing_cell();
        let wood = insert_ground_stack(&mut simulation, item::WOOD, 10, cell);
        simulation.pick_up_item(character, wood).unwrap();
        let tool = insert_ground_stack(&mut simulation, item::PRIMITIVE_TOOL, 1, cell);
        let position = simulation.item_world.get(tool).unwrap().ground_position();

        assert_eq!(
            simulation.designate_equipment_fetch(character, tool),
            Err(SimulationError::CarryCapacityExceeded {
                character_id: character,
                item_id: tool,
            })
        );
        assert_eq!(
            simulation.item_world.get(tool).unwrap().ground_position(),
            position
        );
        assert!(!simulation.item_is_reserved(tool));
        assert!(simulation.job_world.indexes_are_consistent());
    }

    #[test]
    fn hands_filled_after_an_equipment_order_do_not_fail_the_simulation_tick() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let character = cora();
        let cell = simulation.characters[&character]
            .position()
            .containing_cell();
        let tool = insert_ground_stack(&mut simulation, item::PRIMITIVE_TOOL, 1, cell);
        let job = simulation
            .designate_equipment_fetch(character, tool)
            .unwrap();
        let wood = insert_ground_stack(&mut simulation, item::WOOD, 10, cell);
        simulation.pick_up_item(character, wood).unwrap();

        simulation.advance_ticks(1).unwrap();
        assert_eq!(simulation.job_for_worker(character), Some(job));
        assert!(
            simulation
                .item_world
                .get(tool)
                .unwrap()
                .ground_position()
                .is_some()
        );
        simulation
            .drop_item(
                character,
                wood,
                simulation.characters[&character].position(),
            )
            .unwrap();
        simulation.advance_ticks(1).unwrap();
        assert_eq!(simulation.equipment(character), vec![(slot::TOOL, tool)]);
        assert!(simulation.job_world.indexes_are_consistent());
    }
    use super::*;
    use crate::simulation::test_support::*;

    #[test]
    fn harvest_job_completes_into_one_physical_stack_and_cleans_reservation() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let (source, resource) = harvest_fixture(&simulation);
        let item_revision = simulation.item_revision();
        let resource_revision = simulation.resource_revision();

        let job_id = simulation.designate_harvest(source).unwrap();
        let output_id = simulation.next_entity_id().unwrap();
        assert_eq!(simulation.jobs().count(), 1);
        assert_eq!(
            simulation.natural_resource_at(source).unwrap(),
            Some(resource)
        );

        for _ in 0..256 {
            if simulation.jobs().next().is_none() {
                break;
            }
            simulation.advance_ticks(1).unwrap();
        }

        assert_eq!(simulation.jobs().count(), 0, "harvest job did not finish");
        assert_eq!(simulation.natural_resource_at(source).unwrap(), None);
        assert_eq!(simulation.resource_revision(), resource_revision + 1);
        assert_eq!(simulation.item_revision(), item_revision + 1);
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(
            simulation
                .characters()
                .all(|c| simulation.job_for_worker(c.id()).is_none())
        );

        let output = simulation
            .items()
            .find(|item| item.id() == output_id)
            .unwrap();
        let expected_kind = match resource.kind().name() {
            "tree" => item::WOOD,
            "stone_outcrop" => item::STONE,
            "berry_bush" => item::BERRIES,
            other => panic!("harvest fixture uses unexpected resource {other}"),
        };
        assert_eq!(output.kind(), expected_kind);
        assert_eq!(output.quantity().get(), resource.yield_quantity());
        assert_eq!(
            output.ground_position(),
            Some(WorldPosition::from_cell_center(source).unwrap())
        );
        assert_ne!(job_id, output_id);
    }

    #[test]
    fn harvest_assignment_is_deterministic_and_exclusive() {
        let mut first = Simulation::new(WorldSeed::new(0)).unwrap();
        let mut second = first.clone();
        let (source, _) = harvest_fixture(&first);
        let first_job = first.designate_harvest(source).unwrap();
        let second_job = second.designate_harvest(source).unwrap();
        assert_eq!(first_job, second_job);

        first.advance_ticks(1).unwrap();
        second.advance_ticks(1).unwrap();

        let first_state = first.job_world.get(first_job).unwrap().state();
        let second_state = second.job_world.get(second_job).unwrap().state();
        assert_eq!(first_state, second_state);
        let worker = first_state
            .worker()
            .expect("reachable harvest should reserve a worker");
        assert_eq!(first.job_for_worker(worker), Some(first_job));
        assert!(first.job_world.indexes_are_consistent());
    }

    #[test]
    fn harvest_designation_rejects_invalid_sources_without_allocating_jobs() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let (source, _) = harvest_fixture(&simulation);
        let before_next = simulation.next_entity_id();
        let job = simulation.designate_harvest(source).unwrap();
        assert_eq!(
            simulation.designate_harvest(source),
            Err(SimulationError::HarvestAlreadyDesignated(source))
        );
        assert_eq!(simulation.jobs().count(), 1);

        simulation.cancel_job(job).unwrap();
        let empty = (0..=7)
            .flat_map(|y| (-7..=7).map(move |x| WorldCell::new(x, y)))
            .find(|cell| {
                simulation.is_explored(*cell)
                    && simulation.natural_resource_at(*cell).unwrap().is_none()
            })
            .unwrap();
        assert_eq!(
            simulation.designate_harvest(empty),
            Err(SimulationError::NaturalResourceMissing(empty))
        );
        let unknown = WorldCell::new(100, 100);
        assert_eq!(
            simulation.designate_harvest(unknown),
            Err(SimulationError::HarvestSourceUndiscovered(unknown))
        );
        assert_eq!(
            before_next.unwrap().value() + 1,
            simulation.next_entity_id().unwrap().value()
        );
    }

    #[test]
    fn cancelling_or_manually_interrupting_harvest_releases_worker_reservation() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let (source, _) = harvest_fixture(&simulation);
        let job_id = simulation.designate_harvest(source).unwrap();
        simulation.advance_ticks(1).unwrap();
        let worker = simulation
            .job_world
            .get(job_id)
            .unwrap()
            .state()
            .worker()
            .unwrap();

        simulation.stop_movement(worker).unwrap();
        assert_eq!(
            simulation.job_world.get(job_id).unwrap().state(),
            JobState::Available
        );
        assert_eq!(simulation.job_for_worker(worker), None);
        assert!(simulation.job_world.indexes_are_consistent());

        simulation.advance_ticks(1).unwrap();
        let worker = simulation
            .job_world
            .get(job_id)
            .unwrap()
            .state()
            .worker()
            .unwrap();
        simulation.cancel_job(job_id).unwrap();
        assert_eq!(simulation.jobs().count(), 0);
        assert_eq!(simulation.job_for_worker(worker), None);
        assert_eq!(
            character(&simulation, worker).movement(),
            MovementState::Idle
        );
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(simulation.natural_resource_at(source).unwrap().is_some());
    }

    #[test]
    fn harvested_output_becomes_a_physical_haul_candidate_and_reaches_stockpile() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let initial_item_ids = simulation
            .items()
            .map(ItemStack::id)
            .collect::<BTreeSet<_>>();
        let stockpile_id = simulation.create_stockpile(WorldCell::new(-2, 0)).unwrap();
        for x in -1..=2 {
            simulation
                .set_stockpile_cell(stockpile_id, WorldCell::new(x, 0), true)
                .unwrap();
        }
        let (source, resource) = harvest_fixture(&simulation);
        let output_kind = match resource.kind().name() {
            "tree" => item::WOOD,
            "stone_outcrop" => item::STONE,
            "berry_bush" => item::BERRIES,
            other => panic!("harvest fixture uses unexpected resource {other}"),
        };
        let initial_stockpile_quantity = simulation
            .items()
            .filter(|item| {
                item.kind() == output_kind
                    && item.ground_position().is_some_and(|position| {
                        simulation.stockpile_at(position.containing_cell()) == Some(stockpile_id)
                    })
            })
            .map(|item| item.quantity().get())
            .sum::<u32>();
        simulation.designate_harvest(source).unwrap();

        let mut saw_new_carried = false;
        for _ in 0..768 {
            simulation.advance_ticks(1).unwrap();
            saw_new_carried |= simulation
                .items()
                .any(|item| !initial_item_ids.contains(&item.id()) && item.carrier().is_some());
            let stockpile_quantity = simulation
                .items()
                .filter(|item| {
                    item.kind() == output_kind
                        && item.ground_position().is_some_and(|position| {
                            simulation.stockpile_at(position.containing_cell())
                                == Some(stockpile_id)
                        })
                })
                .map(|item| item.quantity().get())
                .sum::<u32>();
            if simulation.natural_resource_at(source).unwrap().is_none()
                && stockpile_quantity == initial_stockpile_quantity + resource.yield_quantity()
            {
                break;
            }
        }

        let final_stockpile_quantity = simulation
            .items()
            .filter(|item| {
                item.kind() == output_kind
                    && item.ground_position().is_some_and(|position| {
                        simulation.stockpile_at(position.containing_cell()) == Some(stockpile_id)
                    })
            })
            .map(|item| item.quantity().get())
            .sum::<u32>();
        assert!(
            saw_new_carried,
            "harvested output must pass through Carried during haul before merge/delivery"
        );
        assert_eq!(
            final_stockpile_quantity,
            initial_stockpile_quantity + resource.yield_quantity()
        );
        assert_eq!(simulation.natural_resource_at(source).unwrap(), None);
        assert!(simulation.item_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
    }
}
