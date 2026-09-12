//! Construction sites, delivered materials, finished structures and doors.

use progressus_content::structure;

use super::*;

impl Simulation {
    pub(super) fn maintain_doors(&mut self) -> Result<(), SimulationError> {
        let occupied_cells = self
            .characters
            .values()
            .map(|character| character.position().containing_cell())
            .collect::<BTreeSet<_>>();
        self.construction_world
            .maintain_doors(self.clock.tick(), &occupied_cells)
            .map_err(SimulationError::from_construction_world)
    }

    pub fn designate_construction(
        &mut self,
        kind: StructureId,
        cell: WorldCell,
    ) -> Result<EntityId, SimulationError> {
        if kind == structure::DOOR {
            if let Some(site_id) = self.construction_world.site_at(cell)
                && self
                    .construction_world
                    .site(site_id)
                    .is_some_and(|site| site.kind() == structure::STONE_WALL)
            {
                self.cancel_construction(site_id)?;
            }
            if let Some(structure_id) = self.construction_world.structure_at(cell)
                && self
                    .construction_world
                    .structure(structure_id)
                    .is_some_and(|structure| structure.kind() == structure::STONE_WALL)
            {
                self.construction_world
                    .remove_structure(structure_id)
                    .map_err(SimulationError::from_construction_world)?;
            }
        }

        let preparation_resource = self.natural_resource_at(cell)?;
        self.validate_construction_cell(cell)?;
        if self.construction_world.site_at(cell).is_some()
            || self.construction_world.structure_at(cell).is_some()
        {
            return Err(SimulationError::ConstructionCellOccupied(cell));
        }
        let id = self.id_allocator.allocate()?;
        self.construction_world
            .insert_site(
                ConstructionSite::new(id, kind, cell)
                    .with_preparation_resource(preparation_resource.is_some()),
            )
            .map_err(SimulationError::from_construction_world)?;
        self.ensure_construction_job(id)?;
        Ok(id)
    }

    pub fn cancel_construction(&mut self, site_id: EntityId) -> Result<(), SimulationError> {
        let structure_site = self.construction_world.site(site_id).is_some();
        let workstation_site = self.construction_world.workstation_site(site_id).is_some();
        if !structure_site && !workstation_site {
            return Err(SimulationError::UnknownConstructionSite(site_id));
        }
        if let Some(job_id) = self.job_world.construction_delivery_job_for_site(site_id) {
            self.cancel_job(job_id)?;
        }
        if let Some(job_id) = self.job_world.construct_job_for_site(site_id) {
            self.cancel_job(job_id)?;
        }
        if let Some(job_id) = self.job_world.preparation_job_for_site(site_id) {
            self.cancel_job(job_id)?;
        }
        if structure_site {
            self.construction_world
                .release_material(site_id)
                .map_err(SimulationError::from_construction_world)?;
            self.construction_world
                .remove_site(site_id)
                .map_err(SimulationError::from_construction_world)?;
        } else {
            self.construction_world
                .remove_workstation_site(site_id)
                .map_err(SimulationError::from_construction_world)?;
        }
        Ok(())
    }

    pub(super) fn validate_construction_cell(
        &self,
        cell: WorldCell,
    ) -> Result<(), SimulationError> {
        if !self.is_explored(cell) {
            return Err(SimulationError::ConstructionCellUndiscovered(cell));
        }
        if !self.is_walkable(cell)? {
            return Err(SimulationError::ConstructionCellBlocked(cell));
        }
        if self.stockpile_world.stockpile_at(cell).is_some()
            || self.workstation_world.workstation_at(cell).is_some()
            || self.production_logistics_world.zone_at(cell).is_some()
        {
            return Err(SimulationError::ConstructionCellOccupied(cell));
        }
        Ok(())
    }

    pub(super) fn maintain_construction_jobs(&mut self) -> Result<(), SimulationError> {
        let mut site_ids = self
            .construction_world
            .sites()
            .map(ConstructionSite::id)
            .collect::<Vec<_>>();
        site_ids.extend(
            self.construction_world
                .workstation_sites()
                .map(WorkstationConstructionSite::id),
        );
        for site_id in site_ids {
            self.ensure_construction_job(site_id)?;
        }
        Ok(())
    }

    pub(super) fn ensure_construction_job(
        &mut self,
        site_id: EntityId,
    ) -> Result<(), SimulationError> {
        let Some(site_cell) = self.construction_project_cell(site_id) else {
            return Ok(());
        };
        if self.construction_cell_has_removable_occupant(site_cell)?
            && let Some(job_id) = self.job_world.construct_job_for_site(site_id)
        {
            self.cancel_job(job_id)?;
        }
        if self.job_world.preparation_job_for_site(site_id).is_some() {
            return Ok(());
        }
        if let Some(source) = self.natural_resource_at(site_cell)? {
            if self.job_world.harvest_job_for_source(site_cell).is_none() {
                self.create_construction_preparation_job(
                    site_id,
                    ConstructionPreparationTarget::NaturalResource { source: site_cell },
                )?;
            }
            let _ = source;
            return Ok(());
        }
        if let Some(item_id) = self
            .item_world
            .iter()
            .filter(|item| {
                item.ground_position()
                    .is_some_and(|position| position.containing_cell() == site_cell)
            })
            .map(ItemStack::id)
            .min()
        {
            if self.job_world.item_job_for_item(item_id).is_none()
                && self.construction_world.site_for_material(item_id).is_none()
                && let Some(destination) = self.construction_preparation_destination(site_cell)?
            {
                self.create_construction_preparation_job(
                    site_id,
                    ConstructionPreparationTarget::GroundItem {
                        item_id,
                        destination,
                    },
                )?;
            }
            return Ok(());
        }
        if let Some(character_id) = self
            .characters
            .values()
            .filter(|character| character.position().containing_cell() == site_cell)
            .map(Character::id)
            .min()
        {
            if let Some(destination) = self.construction_preparation_destination(site_cell)? {
                self.create_construction_preparation_job(
                    site_id,
                    ConstructionPreparationTarget::Character {
                        character_id,
                        destination,
                    },
                )?;
            }
            return Ok(());
        }
        if self.construction_world.workstation_site(site_id).is_some() {
            self.complete_workstation_placement(site_id)?;
            return Ok(());
        }
        let site = self
            .construction_world
            .site(site_id)
            .cloned()
            .ok_or(SimulationError::UnknownConstructionSite(site_id))?;
        match site.material_state() {
            None => {
                let Some(item_id) = self.select_construction_material(&site) else {
                    return Ok(());
                };
                self.construction_world
                    .reserve_material(site_id, item_id)
                    .map_err(SimulationError::from_construction_world)?;
                self.create_construction_delivery_job(site_id, item_id)?;
            }
            Some(ConstructionMaterialState::Reserved) => {
                let Some(item_id) = site.material_item_id() else {
                    return Err(SimulationError::ConstructionInvariantViolation);
                };
                let valid = self.item_world.get(item_id).is_some_and(|item| {
                    item.kind() == site.kind().definition().material
                        && item.quantity().get() >= site.kind().definition().material_quantity
                });
                if !valid {
                    self.construction_world
                        .release_material(site_id)
                        .map_err(SimulationError::from_construction_world)?;
                    return Ok(());
                }
                if self
                    .job_world
                    .construction_delivery_job_for_site(site_id)
                    .is_none()
                {
                    self.create_construction_delivery_job(site_id, item_id)?;
                }
            }
            Some(ConstructionMaterialState::Delivered) => {
                let Some(item_id) = site.material_item_id() else {
                    return Err(SimulationError::ConstructionInvariantViolation);
                };
                let delivered = self.item_world.get(item_id).is_some_and(|item| {
                    item.kind() == site.kind().definition().material
                        && item.quantity().get() >= site.kind().definition().material_quantity
                        && item.ground_position().is_some_and(|position| {
                            cell_manhattan_distance(position.containing_cell(), site.cell()) <= 1
                        })
                });
                if !delivered {
                    self.construction_world
                        .mark_material_reserved(site_id, item_id)
                        .map_err(SimulationError::from_construction_world)?;
                    return Ok(());
                }
                if self.job_world.construct_job_for_site(site_id).is_none() {
                    let job_id = self.id_allocator.allocate()?;
                    self.job_world
                        .insert(Job::new(job_id, JobKind::Construct { site_id }))
                        .map_err(SimulationError::from_job_world)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn construction_project_cell(&self, site_id: EntityId) -> Option<WorldCell> {
        self.construction_world
            .site(site_id)
            .map(ConstructionSite::cell)
            .or_else(|| {
                self.construction_world
                    .workstation_site(site_id)
                    .map(WorkstationConstructionSite::cell)
            })
    }

    fn create_construction_preparation_job(
        &mut self,
        site_id: EntityId,
        target: ConstructionPreparationTarget,
    ) -> Result<(), SimulationError> {
        let job_id = self.id_allocator.allocate()?;
        self.job_world
            .insert(Job::new(
                job_id,
                JobKind::PrepareConstruction { site_id, target },
            ))
            .map_err(SimulationError::from_job_world)
    }

    pub(super) fn construction_cell_has_removable_occupant(
        &self,
        cell: WorldCell,
    ) -> Result<bool, SimulationError> {
        Ok(self.natural_resource_at(cell)?.is_some()
            || self
                .characters
                .values()
                .any(|character| character.position().containing_cell() == cell)
            || self.item_world.iter().any(|item| {
                item.ground_position()
                    .is_some_and(|position| position.containing_cell() == cell)
            }))
    }

    fn construction_preparation_destination(
        &self,
        source: WorldCell,
    ) -> Result<Option<WorldCell>, SimulationError> {
        for direction in [
            Direction::East,
            Direction::North,
            Direction::South,
            Direction::West,
        ] {
            let Some(cell) = direction.adjacent(source) else {
                continue;
            };
            if self.is_explored(cell)
                && self.is_walkable(cell)?
                && self.natural_resource_at(cell)?.is_none()
                && !self.cell_is_claimed(cell)
                && !self.item_world.iter().any(|item| {
                    item.ground_position()
                        .is_some_and(|position| position.containing_cell() == cell)
                })
            {
                return Ok(Some(cell));
            }
        }
        Ok(None)
    }

    pub(super) fn create_construction_delivery_job(
        &mut self,
        site_id: EntityId,
        item_id: EntityId,
    ) -> Result<(), SimulationError> {
        let job_id = self.id_allocator.allocate()?;
        self.job_world
            .insert(Job::new(
                job_id,
                JobKind::DeliverConstruction { site_id, item_id },
            ))
            .map_err(SimulationError::from_job_world)?;
        Ok(())
    }

    pub(super) fn select_construction_material(&self, site: &ConstructionSite) -> Option<EntityId> {
        let mut candidates = self
            .item_world
            .iter()
            .filter_map(|item| {
                let position = item.ground_position()?;
                (item.kind() == site.kind().definition().material
                    && item.quantity().get() >= site.kind().definition().material_quantity
                    && self.is_explored(position.containing_cell())
                    && self.job_world.item_job_for_item(item.id()).is_none()
                    && self
                        .construction_world
                        .site_for_material(item.id())
                        .is_none())
                .then_some((
                    cell_manhattan_distance(position.containing_cell(), site.cell()),
                    item.id(),
                ))
            })
            .collect::<Vec<_>>();
        candidates.sort_unstable();
        candidates.first().map(|(_, id)| *id)
    }

    pub(super) fn construction_access_cell(
        &self,
        site_cell: WorldCell,
    ) -> Result<Option<WorldCell>, SimulationError> {
        for direction in [
            Direction::East,
            Direction::North,
            Direction::South,
            Direction::West,
        ] {
            let Some(cell) = direction.adjacent(site_cell) else {
                continue;
            };
            if self.is_explored(cell)
                && self.is_walkable(cell)?
                && self.construction_world.site_at(cell).is_none()
                && self.workstation_world.workstation_at(cell).is_none()
            {
                return Ok(Some(cell));
            }
        }
        Ok(None)
    }

    pub(super) fn construction_access_position(
        &self,
        site_cell: WorldCell,
        access_cell: WorldCell,
    ) -> Result<WorldPosition, SimulationError> {
        let center = WorldPosition::from_cell_center(access_cell)?;
        let dx = access_cell.x() - site_cell.x();
        let dy = access_cell.y() - site_cell.y();
        match (dx, dy) {
            (1, 0) => Ok(center.checked_translate(-256, 0)?),
            (-1, 0) => Ok(center.checked_translate(256, 0)?),
            (0, 1) => Ok(center.checked_translate(0, -256)?),
            (0, -1) => Ok(center.checked_translate(0, 256)?),
            _ => Err(SimulationError::ConstructionInvariantViolation),
        }
    }

    pub(super) fn try_assign_construction_delivery(
        &mut self,
        job_id: EntityId,
        site_id: EntityId,
        item_id: EntityId,
    ) -> Result<(), SimulationError> {
        let Some(site) = self.construction_world.site(site_id) else {
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
        let Some(item) = self.item_world.get(item_id) else {
            self.construction_world
                .release_material(site_id)
                .map_err(SimulationError::from_construction_world)?;
            self.job_world
                .remove(job_id)
                .map_err(SimulationError::from_job_world)?;
            return Ok(());
        };
        let Some(item_position) = item.ground_position() else {
            return Ok(());
        };
        if item.kind() != site.kind().definition().material
            || item.quantity().get() < site.kind().definition().material_quantity
        {
            self.construction_world
                .release_material(site_id)
                .map_err(SimulationError::from_construction_world)?;
            self.job_world
                .remove(job_id)
                .map_err(SimulationError::from_job_world)?;
            return Ok(());
        }
        for worker_id in self.available_workers_by_distance(item_position.containing_cell()) {
            let route = match self.plan_navigation_route(worker_id, item_position) {
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
            self.apply_navigation_route(worker_id, item_position, route);
            return Ok(());
        }
        Ok(())
    }

    pub(super) fn try_assign_construct(
        &mut self,
        job_id: EntityId,
        site_id: EntityId,
    ) -> Result<(), SimulationError> {
        let Some(site) = self.construction_world.site(site_id) else {
            self.job_world
                .remove(job_id)
                .map_err(SimulationError::from_job_world)?;
            return Ok(());
        };
        if site.material_state() != Some(ConstructionMaterialState::Delivered) {
            self.job_world
                .remove(job_id)
                .map_err(SimulationError::from_job_world)?;
            return Ok(());
        }
        let Some(access_cell) = self.construction_access_cell(site.cell())? else {
            return Ok(());
        };
        let target = self.construction_access_position(site.cell(), access_cell)?;
        for worker_id in self.available_workers_by_distance(access_cell) {
            let route = match self.plan_navigation_route(worker_id, target) {
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
            self.apply_navigation_route(worker_id, target, route);
            return Ok(());
        }
        Ok(())
    }

    pub(super) fn try_assign_construction_preparation(
        &mut self,
        job_id: EntityId,
        site_id: EntityId,
        target: ConstructionPreparationTarget,
    ) -> Result<(), SimulationError> {
        let Some(site_cell) = self.construction_project_cell(site_id) else {
            self.job_world
                .remove(job_id)
                .map_err(SimulationError::from_job_world)?;
            return Ok(());
        };
        match target {
            ConstructionPreparationTarget::NaturalResource { source } => {
                if source != site_cell {
                    return Err(SimulationError::ConstructionInvariantViolation);
                }
                self.try_assign_harvest(job_id, source)
            }
            ConstructionPreparationTarget::GroundItem {
                item_id,
                destination,
            } => {
                let Some(position) = self
                    .item_world
                    .get(item_id)
                    .and_then(ItemStack::ground_position)
                else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                if position.containing_cell() != site_cell
                    || !self.construction_preparation_destination_is_valid(destination)?
                {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                for worker_id in self.available_workers_by_distance(site_cell) {
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
            ConstructionPreparationTarget::Character {
                character_id,
                destination,
            } => {
                let Some(character) = self.characters.get(&character_id) else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                if character.position().containing_cell() != site_cell {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                if !character.is_available_for_work()
                    || character.is_starving()
                    || self.job_world.job_for_worker(character_id).is_some()
                    || self.character_has_held_payload(character_id)
                    || !self.construction_preparation_destination_is_valid(destination)?
                {
                    return Ok(());
                }
                let position = WorldPosition::from_cell_center(destination)?;
                let route = match self.plan_navigation_route(character_id, position) {
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
                self.apply_navigation_route(character_id, position, route);
                Ok(())
            }
        }
    }

    pub(super) fn construction_preparation_destination_is_valid(
        &self,
        cell: WorldCell,
    ) -> Result<bool, SimulationError> {
        Ok(self.is_explored(cell)
            && self.is_walkable(cell)?
            && self.natural_resource_at(cell)?.is_none()
            && !self.cell_is_claimed(cell)
            && !self.item_world.iter().any(|item| {
                item.ground_position()
                    .is_some_and(|position| position.containing_cell() == cell)
            }))
    }

    pub(super) fn construction_site_ready(&self, site_id: EntityId) -> bool {
        let Some(site) = self.construction_world.site(site_id) else {
            return false;
        };
        if site.material_state() != Some(ConstructionMaterialState::Delivered) {
            return false;
        }
        if self
            .natural_resource_at(site.cell())
            .ok()
            .flatten()
            .is_some()
            || self
                .characters
                .values()
                .any(|character| character.position().containing_cell() == site.cell())
            || self.item_world.iter().any(|item| {
                item.ground_position()
                    .is_some_and(|position| position.containing_cell() == site.cell())
            })
        {
            return false;
        }
        let Some(item_id) = site.material_item_id() else {
            return false;
        };
        self.item_world.get(item_id).is_some_and(|item| {
            item.kind() == site.kind().definition().material
                && item.quantity().get() >= site.kind().definition().material_quantity
                && item.ground_position().is_some_and(|position| {
                    cell_manhattan_distance(position.containing_cell(), site.cell()) <= 1
                })
        })
    }

    pub(super) fn complete_construction(
        &mut self,
        job_id: EntityId,
        worker_id: EntityId,
        site_id: EntityId,
    ) -> Result<(), SimulationError> {
        if !self.construction_site_ready(site_id) {
            return Err(SimulationError::ConstructionInvariantViolation);
        }
        let site = self
            .construction_world
            .site(site_id)
            .cloned()
            .ok_or(SimulationError::UnknownConstructionSite(site_id))?;
        let item_id = site
            .material_item_id()
            .ok_or(SimulationError::ConstructionInvariantViolation)?;
        self.item_world
            .consume(item_id, site.kind().definition().material_quantity)
            .map_err(|_| SimulationError::ConstructionInvariantViolation)?;
        self.job_world
            .remove(job_id)
            .map_err(SimulationError::from_job_world)?;
        self.construction_world
            .complete_site(site_id)
            .map_err(SimulationError::from_construction_world)?;
        if let Some(character) = self.characters.get_mut(&worker_id) {
            character.set_movement(MovementState::Idle);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::test_support::*;
    use progressus_content::{item, natural_resource, terrain};

    #[test]
    fn construction_designation_preserves_a_removable_source_for_preparation() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let (cell, resource) = harvest_fixture(&simulation);

        let site_id = simulation
            .designate_construction(structure::STONE_WALL, cell)
            .unwrap();

        assert_eq!(simulation.construction_site_at(cell), Some(site_id));
        assert_eq!(
            simulation.natural_resource_at(cell).unwrap(),
            Some(resource)
        );
        assert!(simulation.jobs().all(|job| !matches!(
            job.kind(),
            JobKind::DeliverConstruction { site_id: job_site, .. }
                | JobKind::Construct { site_id: job_site }
                if job_site == site_id
        )));
    }

    #[test]
    fn construction_designation_preserves_a_ground_stack_for_preparation() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cell = empty_stockpile_cells(&simulation, 1)[0];
        let item_id = insert_ground_stack(&mut simulation, item::WOOD, 3, cell);

        let site_id = simulation
            .designate_construction(structure::STONE_WALL, cell)
            .unwrap();

        assert_eq!(simulation.construction_site_at(cell), Some(site_id));
        assert_eq!(
            simulation
                .item_world
                .get(item_id)
                .and_then(ItemStack::ground_position)
                .map(WorldPosition::containing_cell),
            Some(cell)
        );
    }

    #[test]
    fn construction_designation_accepts_a_character_who_can_vacate() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cell = empty_stockpile_cells(&simulation, 1)[0];
        let character_id = cora();
        character_mut(&mut simulation, character_id)
            .set_position(WorldPosition::from_cell_center(cell).unwrap());

        let site_id = simulation
            .designate_construction(structure::STONE_WALL, cell)
            .unwrap();

        assert_eq!(simulation.construction_site_at(cell), Some(site_id));
        assert_eq!(
            character(&simulation, character_id)
                .position()
                .containing_cell(),
            cell
        );
    }

    #[test]
    fn construction_waits_for_an_existing_harvest_job_without_replacing_it() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let (cell, _) = harvest_fixture(&simulation);
        let harvest_job = simulation.designate_harvest(cell).unwrap();

        let site_id = simulation
            .designate_construction(structure::STONE_WALL, cell)
            .unwrap();

        assert_eq!(
            simulation.job_world.harvest_job_for_source(cell),
            Some(harvest_job)
        );
        assert_eq!(simulation.job_world.preparation_job_for_site(site_id), None);
        assert!(simulation.job_world.get(harvest_job).is_some());
    }

    #[test]
    fn construction_project_waits_when_no_physical_drop_cell_is_available() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let cell = WorldCell::new(0, 0);
        simulation
            .set_terrain_override(cell, terrain::GRASS)
            .unwrap();
        simulation.depleted_resources.insert(cell);
        for direction in [
            Direction::East,
            Direction::North,
            Direction::South,
            Direction::West,
        ] {
            let adjacent = direction.adjacent(cell).unwrap();
            simulation
                .set_terrain_override(adjacent, terrain::ROCK)
                .unwrap();
        }
        let item_id = insert_ground_stack(&mut simulation, item::WOOD, 3, cell);

        let site_id = simulation
            .designate_construction(structure::STONE_WALL, cell)
            .unwrap();
        simulation.advance_ticks(16).unwrap();

        assert_eq!(simulation.construction_site_at(cell), Some(site_id));
        assert_eq!(simulation.job_world.preparation_job_for_site(site_id), None);
        assert_eq!(
            simulation
                .item_world
                .get(item_id)
                .unwrap()
                .ground_position()
                .unwrap()
                .containing_cell(),
            cell
        );
    }

    #[test]
    fn item_preparation_waits_while_every_worker_has_a_physical_load() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let worker_ids = simulation
            .characters()
            .map(Character::id)
            .collect::<Vec<_>>();
        for worker_id in worker_ids {
            let position = character(&simulation, worker_id).position();
            let load_id = simulation.id_allocator.allocate().unwrap();
            simulation
                .item_world
                .insert_ground(ItemStack::new_ground(
                    load_id,
                    item::WOOD,
                    ItemQuantity::new(item::WOOD.definition().hand_load).unwrap(),
                    position,
                ))
                .unwrap();
            simulation
                .item_world
                .move_to_carried(load_id, worker_id)
                .unwrap();
        }
        let cell = empty_stockpile_cells(&simulation, 1)[0];
        let item_id = insert_ground_stack(&mut simulation, item::STONE, 1, cell);
        let site_id = simulation
            .designate_construction(structure::STONE_WALL, cell)
            .unwrap();

        simulation.advance_ticks(16).unwrap();

        let job_id = simulation
            .job_world
            .preparation_job_for_site(site_id)
            .expect("the project keeps its pending item preparation");
        assert_eq!(
            simulation.job_world.get(job_id).unwrap().state(),
            JobState::Available
        );
        assert_eq!(simulation.item_world.holder_of(item_id), None);
        assert_eq!(
            simulation
                .item_world
                .get(item_id)
                .unwrap()
                .ground_position()
                .unwrap()
                .containing_cell(),
            cell
        );
    }

    #[test]
    fn construction_permanently_clears_a_renewable_source() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cell = WorldCell::new(3, 3);
        assert!(
            simulation
                .natural_resource_at(cell)
                .unwrap()
                .is_some_and(|resource| resource.kind() == natural_resource::BERRY_BUSH)
        );
        let site_id = simulation
            .designate_construction(structure::STONE_WALL, cell)
            .unwrap();

        for _ in 0..2_048 {
            simulation.advance_ticks(1).unwrap();
            if simulation.structure_at(cell) == Some(site_id) {
                break;
            }
        }

        assert_eq!(simulation.structure_at(cell), Some(site_id));
        assert!(!simulation.renewable_resource_regrowth.contains_key(&cell));
        assert!(simulation.depleted_resources.contains(&cell));
        assert_eq!(simulation.natural_resource_at(cell).unwrap(), None);
    }

    #[test]
    fn preparation_physically_clears_a_tree_stack_and_character_before_building() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let tree_cell = (-5..=5)
            .flat_map(|y| (-7..=7).map(move |x| WorldCell::new(x, y)))
            .find(|cell| {
                simulation.is_explored(*cell)
                    && simulation
                        .natural_resource_at(*cell)
                        .unwrap()
                        .is_some_and(|resource| {
                            resource.kind() == progressus_content::natural_resource::TREE
                        })
            })
            .expect("seed 0 exposes an explored tree");
        let extra_id = insert_ground_stack(&mut simulation, item::WOOD, 3, tree_cell);
        let character_id = cora();
        character_mut(&mut simulation, character_id)
            .set_position(WorldPosition::from_cell_center(tree_cell).unwrap());
        let wood_before = total_item_quantity(&simulation, item::WOOD);
        let tree_yield = simulation
            .natural_resource_at(tree_cell)
            .unwrap()
            .unwrap()
            .yield_quantity();

        let site_id = simulation
            .designate_construction(structure::STONE_WALL, tree_cell)
            .unwrap();
        for _ in 0..2_048 {
            simulation.advance_ticks(1).unwrap();
            if simulation.structure_at(tree_cell) == Some(site_id) {
                break;
            }
        }

        assert_eq!(simulation.structure_at(tree_cell), Some(site_id));
        assert_eq!(simulation.natural_resource_at(tree_cell).unwrap(), None);
        assert_ne!(
            character(&simulation, character_id)
                .position()
                .containing_cell(),
            tree_cell
        );
        assert_ne!(
            simulation
                .item_world
                .get(extra_id)
                .unwrap()
                .ground_position()
                .unwrap()
                .containing_cell(),
            tree_cell
        );
        assert_eq!(
            total_item_quantity(&simulation, item::WOOD),
            wood_before + tree_yield
        );
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(simulation.item_world.indexes_are_consistent());
        assert!(simulation.construction_world.indexes_are_consistent());
    }

    #[test]
    fn cancelling_item_preparation_drops_the_stack_and_preserves_quantity() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cell = empty_stockpile_cells(&simulation, 1)[0];
        let item_id = insert_ground_stack(&mut simulation, item::WOOD, 3, cell);
        let site_id = simulation
            .designate_construction(structure::STONE_WALL, cell)
            .unwrap();
        for _ in 0..128 {
            simulation.advance_ticks(1).unwrap();
            if simulation.item_world.holder_of(item_id).is_some() {
                break;
            }
        }
        assert!(simulation.item_world.holder_of(item_id).is_some());

        simulation.cancel_construction(site_id).unwrap();

        let item = simulation.item_world.get(item_id).unwrap();
        assert_eq!(item.quantity().get(), 3);
        assert!(item.ground_position().is_some());
        assert!(simulation.construction_world.site(site_id).is_none());
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(simulation.item_world.indexes_are_consistent());
    }

    #[test]
    fn active_preparation_round_trips_and_finishes_deterministically() {
        let mut original = Simulation::new(WorldSeed::new(0)).unwrap();
        let (cell, _) = harvest_fixture(&original);
        let site_id = original
            .designate_construction(structure::STONE_WALL, cell)
            .unwrap();
        original.advance_ticks(2).unwrap();
        assert!(
            original
                .job_world
                .preparation_job_for_site(site_id)
                .is_some()
        );

        let encoded = original.save_json().unwrap();
        let mut restored = Simulation::load_json(&encoded).unwrap();
        assert_eq!(restored.save_json().unwrap(), encoded);

        for _ in 0..1_024 {
            original.advance_ticks(1).unwrap();
            restored.advance_ticks(1).unwrap();
            if original.structure_at(cell) == Some(site_id) {
                break;
            }
        }
        assert_eq!(original.structure_at(cell), Some(site_id));
        assert_eq!(restored.save_json().unwrap(), original.save_json().unwrap());
    }

    #[test]
    fn a_late_character_occupant_is_vacated_before_completion() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cell = empty_stockpile_cells(&simulation, 1)[0];
        let site_id = simulation
            .designate_construction(structure::STONE_WALL, cell)
            .unwrap();
        let mut construct_worker = None;
        for _ in 0..512 {
            simulation.advance_ticks(1).unwrap();
            construct_worker = simulation.jobs().find_map(|job| {
                matches!(job.kind(), JobKind::Construct { site_id: id } if id == site_id)
                    .then(|| job.state().worker())
                    .flatten()
            });
            if construct_worker.is_some() {
                break;
            }
        }
        let construct_worker = construct_worker.expect("construction work starts");
        let occupant = simulation
            .characters()
            .map(Character::id)
            .find(|id| {
                *id != construct_worker && simulation.job_world.job_for_worker(*id).is_none()
            })
            .expect("another idle character is available");
        let occupant_state = character_mut(&mut simulation, occupant);
        occupant_state.set_position(WorldPosition::from_cell_center(cell).unwrap());
        occupant_state.set_movement(MovementState::Idle);

        simulation.advance_ticks(1).unwrap();
        assert!(simulation.structure_at(cell).is_none());
        assert!(
            simulation
                .job_world
                .preparation_job_for_site(site_id)
                .is_some()
        );

        for _ in 0..256 {
            simulation.advance_ticks(1).unwrap();
            if simulation.structure_at(cell) == Some(site_id) {
                break;
            }
        }
        assert_eq!(simulation.structure_at(cell), Some(site_id));
        assert_ne!(
            character(&simulation, occupant)
                .position()
                .containing_cell(),
            cell
        );
    }

    #[test]
    fn a_late_ground_stack_is_moved_before_completion() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cell = empty_stockpile_cells(&simulation, 1)[0];
        let site_id = simulation
            .designate_construction(structure::STONE_WALL, cell)
            .unwrap();
        for _ in 0..512 {
            simulation.advance_ticks(1).unwrap();
            if simulation.jobs().any(|job| {
                matches!(job.kind(), JobKind::Construct { site_id: id } if id == site_id)
                    && matches!(job.state(), JobState::Working { .. })
            }) {
                break;
            }
        }
        assert!(simulation.jobs().any(|job| {
            matches!(job.kind(), JobKind::Construct { site_id: id } if id == site_id)
                && matches!(job.state(), JobState::Working { .. })
        }));
        let item_id = insert_ground_stack(&mut simulation, item::WOOD, 1, cell);

        simulation.advance_ticks(1).unwrap();
        assert!(simulation.structure_at(cell).is_none());
        assert!(
            simulation
                .job_world
                .preparation_job_for_site(site_id)
                .is_some()
        );

        for _ in 0..256 {
            simulation.advance_ticks(1).unwrap();
            if simulation.structure_at(cell) == Some(site_id) {
                break;
            }
        }
        assert_eq!(simulation.structure_at(cell), Some(site_id));
        assert_ne!(
            simulation
                .item_world
                .get(item_id)
                .unwrap()
                .ground_position()
                .unwrap()
                .containing_cell(),
            cell
        );
    }

    #[test]
    fn cancelling_construction_during_delivery_drops_material_and_cleans_reservations() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let wall_cell = empty_stockpile_cells(&simulation, 1)[0];
        let site_id = simulation
            .designate_construction(structure::STONE_WALL, wall_cell)
            .unwrap();

        let mut carried = None;
        for _ in 0..128 {
            simulation.advance_ticks(1).unwrap();
            if let Some(item) = simulation.items().find(|item| {
                simulation.construction_world.site_for_material(item.id()) == Some(site_id)
                    && item.carrier().is_some()
            }) {
                carried = Some(item.id());
                break;
            }
        }
        let item_id =
            carried.expect("construction material must enter Carried before cancellation");
        simulation.cancel_construction(site_id).unwrap();

        assert!(simulation.construction_world.site(site_id).is_none());
        assert_eq!(
            simulation.construction_world.site_for_material(item_id),
            None
        );
        assert!(
            simulation
                .item_world
                .get(item_id)
                .unwrap()
                .ground_position()
                .is_some()
        );
        assert!(simulation.structure_at(wall_cell).is_none());
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(simulation.construction_world.indexes_are_consistent());
        assert!(simulation.item_world.indexes_are_consistent());
    }

    #[test]
    fn stone_wall_construction_delivers_physical_material_consumes_two_and_blocks_navigation() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let wall_cell = empty_stockpile_cells(&simulation, 1)[0];
        let stone_before = simulation
            .items()
            .filter(|item| item.kind() == item::STONE)
            .map(|item| item.quantity().get())
            .sum::<u32>();

        let site_id = simulation
            .designate_construction(structure::STONE_WALL, wall_cell)
            .unwrap();
        let mut saw_carried_material = false;
        let mut saw_delivered_material = false;
        for _ in 0..768 {
            simulation.advance_ticks(1).unwrap();
            saw_carried_material |= simulation.items().any(|item| {
                simulation.construction_world.site_for_material(item.id()) == Some(site_id)
                    && item.carrier().is_some()
            });
            saw_delivered_material |=
                simulation
                    .construction_world
                    .site(site_id)
                    .is_some_and(|site| {
                        site.material_state() == Some(ConstructionMaterialState::Delivered)
                    });
            if simulation.structure_at(wall_cell) == Some(site_id) {
                break;
            }
        }

        assert!(
            saw_carried_material,
            "construction material must pass through Carried"
        );
        assert!(
            saw_delivered_material,
            "material must be delivered before construction work"
        );
        assert_eq!(simulation.structure_at(wall_cell), Some(site_id));
        assert!(simulation.construction_site_at(wall_cell).is_none());
        assert_eq!(
            simulation
                .items()
                .filter(|item| item.kind() == item::STONE)
                .map(|item| item.quantity().get())
                .sum::<u32>(),
            stone_before - structure::STONE_WALL.definition().material_quantity
        );
        assert!(!simulation.is_walkable(wall_cell).unwrap());
        let cora = EntityId::new(3).unwrap();

        for y in 0..=2 {
            for x in -1..=1 {
                simulation
                    .set_terrain_override(WorldCell::new(x, y), terrain::GRASS)
                    .unwrap();
            }
        }
        let start = WorldCell::new(0, 0);
        let goal = WorldCell::new(0, 2);
        let cora_state = simulation.characters.get_mut(&cora).unwrap();
        cora_state.set_position(WorldPosition::from_cell_center(start).unwrap());
        cora_state.set_movement(MovementState::Idle);
        simulation
            .move_to(cora, WorldPosition::from_cell_center(goal).unwrap())
            .unwrap();
        assert!(
            simulation
                .characters
                .get(&cora)
                .unwrap()
                .navigation_waypoints()
                .all(|position| position.containing_cell() != wall_cell),
            "A* route must go around the completed wall rather than through it"
        );
        assert!(simulation.construction_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(simulation.item_world.indexes_are_consistent());
    }

    #[test]
    fn door_designation_replaces_planned_stone_wall_without_leaving_old_site() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cell = empty_stockpile_cells(&simulation, 1)[0];
        let wall_id = simulation
            .designate_construction(structure::STONE_WALL, cell)
            .unwrap();
        assert_eq!(simulation.construction_site_at(cell), Some(wall_id));

        let door_id = simulation
            .designate_construction(structure::DOOR, cell)
            .unwrap();
        assert_ne!(door_id, wall_id);
        assert!(simulation.construction_world.site(wall_id).is_none());
        assert_eq!(simulation.construction_site_at(cell), Some(door_id));
        assert_eq!(
            simulation.construction_world.site(door_id).unwrap().kind(),
            structure::DOOR
        );
        assert!(simulation.construction_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(simulation.item_world.indexes_are_consistent());
    }

    #[test]
    fn door_designation_replaces_completed_stone_wall_with_new_door_site() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cell = empty_stockpile_cells(&simulation, 1)[0];
        let wall_id = simulation.id_allocator.allocate().unwrap();
        simulation
            .construction_world
            .insert_site(ConstructionSite::new(wall_id, structure::STONE_WALL, cell))
            .unwrap();
        simulation
            .construction_world
            .complete_site(wall_id)
            .unwrap();
        assert_eq!(simulation.structure_at(cell), Some(wall_id));
        assert!(!simulation.is_walkable(cell).unwrap());

        let door_id = simulation
            .designate_construction(structure::DOOR, cell)
            .unwrap();
        assert_ne!(door_id, wall_id);
        assert!(simulation.construction_world.structure(wall_id).is_none());
        assert_eq!(simulation.structure_at(cell), None);
        assert_eq!(simulation.construction_site_at(cell), Some(door_id));
        assert_eq!(
            simulation.construction_world.site(door_id).unwrap().kind(),
            structure::DOOR
        );
        assert!(simulation.is_walkable(cell).unwrap());
        assert!(simulation.construction_world.indexes_are_consistent());
    }

    #[test]
    fn door_is_passable_opens_for_a_character_and_closes_after_the_hold_window() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let start = WorldCell::new(0, 1);
        let door_cell = WorldCell::new(1, 1);
        let goal = WorldCell::new(2, 1);
        for cell in [start, door_cell, goal] {
            simulation
                .set_terrain_override(cell, terrain::GRASS)
                .unwrap();
        }
        let door_id = simulation.id_allocator.allocate().unwrap();
        simulation
            .construction_world
            .insert_site(ConstructionSite::new(door_id, structure::DOOR, door_cell))
            .unwrap();
        simulation
            .construction_world
            .complete_site(door_id)
            .unwrap();

        assert!(simulation.is_walkable(door_cell).unwrap());
        assert_eq!(
            simulation
                .construction_world
                .structure(door_id)
                .unwrap()
                .door_state(),
            Some(crate::DoorState::Closed)
        );

        let cora = cora();
        let cora_state = simulation.characters.get_mut(&cora).unwrap();
        cora_state.set_position(WorldPosition::from_cell_center(start).unwrap());
        cora_state.set_movement(MovementState::Idle);
        simulation
            .move_to(cora, WorldPosition::from_cell_center(goal).unwrap())
            .unwrap();

        let mut saw_open = false;
        for _ in 0..16 {
            simulation.advance_ticks(1).unwrap();
            if simulation
                .construction_world
                .structure(door_id)
                .unwrap()
                .door_state()
                == Some(crate::DoorState::Open)
            {
                saw_open = true;
            }
            if character(&simulation, cora).position()
                == WorldPosition::from_cell_center(goal).unwrap()
            {
                break;
            }
        }
        assert!(
            saw_open,
            "door must visibly open while a character passes through it"
        );
        assert_eq!(
            character(&simulation, cora).position(),
            WorldPosition::from_cell_center(goal).unwrap()
        );

        simulation
            .advance_ticks(crate::DOOR_HOLD_OPEN_TICKS + 1)
            .unwrap();
        assert_eq!(
            simulation
                .construction_world
                .structure(door_id)
                .unwrap()
                .door_state(),
            Some(crate::DoorState::Closed)
        );
    }
}
