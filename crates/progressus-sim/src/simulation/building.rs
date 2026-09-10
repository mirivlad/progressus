//! Construction sites, delivered materials, finished structures and doors.

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
        kind: StructureKind,
        cell: WorldCell,
    ) -> Result<EntityId, SimulationError> {
        if kind == StructureKind::Door {
            if let Some(site_id) = self.construction_world.site_at(cell)
                && self
                    .construction_world
                    .site(site_id)
                    .is_some_and(|site| site.kind() == StructureKind::StoneWall)
            {
                self.cancel_construction(site_id)?;
            }
            if let Some(structure_id) = self.construction_world.structure_at(cell)
                && self
                    .construction_world
                    .structure(structure_id)
                    .is_some_and(|structure| structure.kind() == StructureKind::StoneWall)
            {
                self.construction_world
                    .remove_structure(structure_id)
                    .map_err(SimulationError::from_construction_world)?;
            }
        }

        self.validate_construction_cell(cell)?;
        if self.construction_world.site_at(cell).is_some()
            || self.construction_world.structure_at(cell).is_some()
        {
            return Err(SimulationError::ConstructionCellOccupied(cell));
        }
        let id = self.id_allocator.allocate()?;
        self.construction_world
            .insert_site(ConstructionSite::new(id, kind, cell))
            .map_err(SimulationError::from_construction_world)?;
        self.ensure_construction_job(id)?;
        Ok(id)
    }

    pub fn cancel_construction(&mut self, site_id: EntityId) -> Result<(), SimulationError> {
        if self.construction_world.site(site_id).is_none() {
            return Err(SimulationError::UnknownConstructionSite(site_id));
        }
        if let Some(job_id) = self.job_world.construction_delivery_job_for_site(site_id) {
            self.cancel_job(job_id)?;
        }
        if let Some(job_id) = self.job_world.construct_job_for_site(site_id) {
            self.cancel_job(job_id)?;
        }
        self.construction_world
            .release_material(site_id)
            .map_err(SimulationError::from_construction_world)?;
        self.construction_world
            .remove_site(site_id)
            .map_err(SimulationError::from_construction_world)?;
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
        if self.natural_resource_at(cell)?.is_some()
            || self.stockpile_world.stockpile_at(cell).is_some()
            || self.workstation_world.workstation_at(cell).is_some()
            || self.production_logistics_world.zone_at(cell).is_some()
            || self
                .characters
                .values()
                .any(|character| character.position().containing_cell() == cell)
            || self.item_world.iter().any(|item| {
                item.ground_position()
                    .is_some_and(|position| position.containing_cell() == cell)
            })
        {
            return Err(SimulationError::ConstructionCellOccupied(cell));
        }
        Ok(())
    }

    pub(super) fn maintain_construction_jobs(&mut self) -> Result<(), SimulationError> {
        let site_ids = self
            .construction_world
            .sites()
            .map(ConstructionSite::id)
            .collect::<Vec<_>>();
        for site_id in site_ids {
            self.ensure_construction_job(site_id)?;
        }
        Ok(())
    }

    pub(super) fn ensure_construction_job(
        &mut self,
        site_id: EntityId,
    ) -> Result<(), SimulationError> {
        let Some(site) = self.construction_world.site(site_id).cloned() else {
            return Ok(());
        };
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
                    item.kind() == site.kind().material_kind()
                        && item.quantity().get() >= site.kind().material_quantity()
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
                    item.kind() == site.kind().material_kind()
                        && item.quantity().get() >= site.kind().material_quantity()
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
                (item.kind() == site.kind().material_kind()
                    && item.quantity().get() >= site.kind().material_quantity()
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
        if item.kind() != site.kind().material_kind()
            || item.quantity().get() < site.kind().material_quantity()
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

    pub(super) fn construction_site_ready(&self, site_id: EntityId) -> bool {
        let Some(site) = self.construction_world.site(site_id) else {
            return false;
        };
        if site.material_state() != Some(ConstructionMaterialState::Delivered) {
            return false;
        }
        let Some(item_id) = site.material_item_id() else {
            return false;
        };
        self.item_world.get(item_id).is_some_and(|item| {
            item.kind() == site.kind().material_kind()
                && item.quantity().get() >= site.kind().material_quantity()
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
            .consume(item_id, site.kind().material_quantity())
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

    #[test]
    fn cancelling_construction_during_delivery_drops_material_and_cleans_reservations() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let wall_cell = empty_stockpile_cells(&simulation, 1)[0];
        let site_id = simulation
            .designate_construction(StructureKind::StoneWall, wall_cell)
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
            .filter(|item| item.kind() == ItemKind::Stone)
            .map(|item| item.quantity().get())
            .sum::<u32>();

        let site_id = simulation
            .designate_construction(StructureKind::StoneWall, wall_cell)
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
                .filter(|item| item.kind() == ItemKind::Stone)
                .map(|item| item.quantity().get())
                .sum::<u32>(),
            stone_before - StructureKind::StoneWall.material_quantity()
        );
        assert!(!simulation.is_walkable(wall_cell).unwrap());
        let cora = EntityId::new(3).unwrap();

        for y in 0..=2 {
            for x in -1..=1 {
                simulation
                    .set_terrain_override(WorldCell::new(x, y), Terrain::Grass)
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
            .designate_construction(StructureKind::StoneWall, cell)
            .unwrap();
        assert_eq!(simulation.construction_site_at(cell), Some(wall_id));

        let door_id = simulation
            .designate_construction(StructureKind::Door, cell)
            .unwrap();
        assert_ne!(door_id, wall_id);
        assert!(simulation.construction_world.site(wall_id).is_none());
        assert_eq!(simulation.construction_site_at(cell), Some(door_id));
        assert_eq!(
            simulation.construction_world.site(door_id).unwrap().kind(),
            StructureKind::Door
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
            .insert_site(ConstructionSite::new(
                wall_id,
                StructureKind::StoneWall,
                cell,
            ))
            .unwrap();
        simulation
            .construction_world
            .complete_site(wall_id)
            .unwrap();
        assert_eq!(simulation.structure_at(cell), Some(wall_id));
        assert!(!simulation.is_walkable(cell).unwrap());

        let door_id = simulation
            .designate_construction(StructureKind::Door, cell)
            .unwrap();
        assert_ne!(door_id, wall_id);
        assert!(simulation.construction_world.structure(wall_id).is_none());
        assert_eq!(simulation.structure_at(cell), None);
        assert_eq!(simulation.construction_site_at(cell), Some(door_id));
        assert_eq!(
            simulation.construction_world.site(door_id).unwrap().kind(),
            StructureKind::Door
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
                .set_terrain_override(cell, Terrain::Grass)
                .unwrap();
        }
        let door_id = simulation.id_allocator.allocate().unwrap();
        simulation
            .construction_world
            .insert_site(ConstructionSite::new(
                door_id,
                StructureKind::Door,
                door_cell,
            ))
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
