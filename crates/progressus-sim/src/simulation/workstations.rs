//! Player commands for workstations, production orders and their logistics zones.

use super::*;

impl Simulation {
    pub fn set_production_zone_cell(
        &mut self,
        workstation_id: EntityId,
        kind: ProductionZoneKind,
        cell: WorldCell,
        enabled: bool,
    ) -> Result<(), SimulationError> {
        let workstation = self
            .workstation_world
            .get(workstation_id)
            .ok_or(SimulationError::UnknownWorkstation(workstation_id))?;
        if workstation.kind() == WorkstationKind::Workbench {
            return match kind {
                ProductionZoneKind::Input => {
                    Err(SimulationError::WorkbenchInputPortsFixed(workstation_id))
                }
                ProductionZoneKind::Output => {
                    Err(SimulationError::WorkbenchOutputPortsFixed(workstation_id))
                }
            };
        }
        if enabled {
            let workstation_cell = workstation.cell();
            if !is_production_zone_neighbour(workstation_cell, cell) {
                return Err(SimulationError::ProductionZoneCellOutOfRange {
                    workstation_id,
                    workstation_cell,
                    cell,
                });
            }
            self.validate_production_zone_cell(cell)?;
        }
        if let Some(job_id) = self.job_world.craft_job_for_workstation(workstation_id) {
            self.cancel_job(job_id)?;
        }
        let supply_jobs = self
            .job_world
            .iter()
            .filter_map(|job| match job.kind() {
                JobKind::SupplyProduction {
                    workstation_id: job_workstation,
                    destination,
                    ..
                } if job_workstation == workstation_id && (!enabled || destination == cell) => {
                    Some(job.id())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for job_id in supply_jobs {
            self.cancel_job(job_id)?;
        }
        self.production_logistics_world
            .set_cell(workstation_id, kind, cell, enabled)
            .map_err(SimulationError::from_production_logistics_world)?;
        self.ensure_craft_job_for_workstation(workstation_id)
    }

    pub fn cycle_workstation_inputs(
        &mut self,
        workstation_id: EntityId,
    ) -> Result<(), SimulationError> {
        let workstation_cell = self
            .workstation_world
            .get(workstation_id)
            .ok_or(SimulationError::UnknownWorkstation(workstation_id))?
            .cell();
        self.cycle_workstation_ports(
            workstation_id,
            ProductionZoneKind::Input,
            production_input_layouts(workstation_cell),
            true,
        )
    }

    pub fn cycle_workstation_outputs(
        &mut self,
        workstation_id: EntityId,
    ) -> Result<(), SimulationError> {
        let workstation_cell = self
            .workstation_world
            .get(workstation_id)
            .ok_or(SimulationError::UnknownWorkstation(workstation_id))?
            .cell();
        self.cycle_workstation_ports(
            workstation_id,
            ProductionZoneKind::Output,
            production_output_layouts(workstation_cell),
            false,
        )
    }

    pub(super) fn cycle_workstation_ports(
        &mut self,
        workstation_id: EntityId,
        kind: ProductionZoneKind,
        layouts: Vec<[WorldCell; 2]>,
        cancel_supply: bool,
    ) -> Result<(), SimulationError> {
        let current = self
            .production_logistics_world
            .get(workstation_id)
            .ok_or(SimulationError::UnknownWorkstation(workstation_id))?
            .cells(kind)
            .collect::<BTreeSet<_>>();
        if layouts.is_empty() {
            return Err(SimulationError::ProductionOutputBlocked(workstation_id));
        }
        let current_index = layouts
            .iter()
            .position(|pair| pair.iter().copied().collect::<BTreeSet<_>>() == current);
        let next_index = current_index.map_or(0, |index| (index + 1) % layouts.len());
        let pair = layouts[next_index];
        for cell in pair {
            if current.contains(&cell) {
                continue;
            }
            if let Some((owner, owner_kind)) = self.production_logistics_world.zone_at(cell) {
                return Err(SimulationError::ProductionZoneCellAlreadyOwned {
                    cell,
                    workstation_id: owner,
                    kind: owner_kind,
                });
            }
            self.validate_production_zone_cell(cell)?;
        }
        if let Some(job_id) = self.job_world.craft_job_for_workstation(workstation_id) {
            self.cancel_job(job_id)?;
        }
        if cancel_supply {
            let supply_jobs = self
                .job_world
                .iter()
                .filter_map(|job| match job.kind() {
                    JobKind::SupplyProduction {
                        workstation_id: id, ..
                    } if id == workstation_id => Some(job.id()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            for job_id in supply_jobs {
                self.cancel_job(job_id)?;
            }
        }
        for cell in current {
            self.production_logistics_world
                .set_cell(workstation_id, kind, cell, false)
                .map_err(SimulationError::from_production_logistics_world)?;
        }
        for cell in pair {
            self.production_logistics_world
                .set_cell(workstation_id, kind, cell, true)
                .map_err(SimulationError::from_production_logistics_world)?;
        }
        self.ensure_craft_job_for_workstation(workstation_id)
    }

    pub(super) fn validate_production_zone_cell(
        &self,
        cell: WorldCell,
    ) -> Result<(), SimulationError> {
        if !self.is_explored(cell) {
            return Err(SimulationError::ProductionZoneCellUndiscovered(cell));
        }
        if !self.is_walkable(cell)? {
            return Err(SimulationError::ProductionZoneCellBlocked(cell));
        }
        if self.natural_resource_at(cell)?.is_some()
            || self.workstation_world.workstation_at(cell).is_some()
            || self.stockpile_world.stockpile_at(cell).is_some()
            || self.construction_world.site_at(cell).is_some()
            || self.construction_world.structure_at(cell).is_some()
            || self.item_world.iter().any(|item| {
                item.ground_position()
                    .is_some_and(|position| position.containing_cell() == cell)
            })
        {
            return Err(SimulationError::ProductionZoneCellOccupied(cell));
        }
        Ok(())
    }

    pub fn add_production_order(
        &mut self,
        workstation_id: EntityId,
        recipe_id: RecipeId,
        target: ProductionTarget,
    ) -> Result<EntityId, SimulationError> {
        let workstation = self
            .workstation_world
            .get(workstation_id)
            .ok_or(SimulationError::UnknownWorkstation(workstation_id))?;
        if workstation.kind() != recipe_definition(recipe_id).workstation {
            return Err(SimulationError::RecipeWorkstationMismatch {
                workstation_id,
                recipe_id,
            });
        }
        let id = self.id_allocator.allocate()?;
        self.production_world
            .insert(ProductionOrder::new(id, workstation_id, recipe_id, target))
            .map_err(SimulationError::from_production_world)?;
        self.ensure_craft_job_for_workstation(workstation_id)?;
        Ok(id)
    }

    pub fn set_production_order_target(
        &mut self,
        order_id: EntityId,
        target: ProductionTarget,
    ) -> Result<(), SimulationError> {
        let workstation_id = self
            .production_world
            .get(order_id)
            .ok_or(SimulationError::UnknownProductionOrder(order_id))?
            .workstation_id();
        if !target.is_pending()
            && let Some(job_id) = self.job_world.craft_job_for_order(order_id)
        {
            self.cancel_job(job_id)?;
        }
        self.production_world
            .set_target(order_id, target)
            .map_err(SimulationError::from_production_world)?;
        self.ensure_craft_job_for_workstation(workstation_id)
    }

    pub fn remove_production_order(&mut self, order_id: EntityId) -> Result<(), SimulationError> {
        let workstation_id = self
            .production_world
            .get(order_id)
            .ok_or(SimulationError::UnknownProductionOrder(order_id))?
            .workstation_id();
        if let Some(job_id) = self.job_world.craft_job_for_order(order_id) {
            self.cancel_job(job_id)?;
        }
        self.production_world
            .remove(order_id)
            .map_err(SimulationError::from_production_world)?;
        self.ensure_craft_job_for_workstation(workstation_id)
    }

    pub fn place_workstation(
        &mut self,
        kind: WorkstationKind,
        cell: WorldCell,
    ) -> Result<EntityId, SimulationError> {
        self.validate_workstation_cell(cell)?;
        if let Some(existing) = self.workstation_world.workstation_at(cell) {
            return Err(SimulationError::WorkstationCellAlreadyOccupied {
                cell,
                workstation_id: existing,
            });
        }
        let (inputs, outputs) = match kind {
            WorkstationKind::Workbench => self.default_workbench_ports(cell)?,
        };
        let id = self.id_allocator.allocate()?;
        self.workstation_world
            .insert(Workstation::new(id, kind, cell))
            .map_err(SimulationError::from_workstation_world)?;
        self.production_logistics_world
            .insert_workstation(id)
            .map_err(SimulationError::from_production_logistics_world)?;
        for input in inputs {
            self.production_logistics_world
                .set_cell(id, ProductionZoneKind::Input, input, true)
                .map_err(SimulationError::from_production_logistics_world)?;
        }
        for output in outputs {
            self.production_logistics_world
                .set_cell(id, ProductionZoneKind::Output, output, true)
                .map_err(SimulationError::from_production_logistics_world)?;
        }
        Ok(id)
    }

    pub(super) fn default_workbench_ports(
        &self,
        workstation_cell: WorldCell,
    ) -> Result<([WorldCell; 2], [WorldCell; 2]), SimulationError> {
        let available = |cell: &WorldCell| {
            self.production_logistics_world.zone_at(*cell).is_none()
                && self.validate_production_zone_cell(*cell).is_ok()
        };
        let inputs = production_input_layouts(workstation_cell)
            .into_iter()
            .find(|pair| pair.iter().all(&available))
            .ok_or(SimulationError::WorkstationPortLayoutUnavailable(
                workstation_cell,
            ))?;
        let outputs = production_output_layouts(workstation_cell)
            .into_iter()
            .find(|pair| pair.iter().all(&available))
            .ok_or(SimulationError::WorkstationPortLayoutUnavailable(
                workstation_cell,
            ))?;
        Ok((inputs, outputs))
    }

    pub fn remove_workstation(&mut self, workstation_id: EntityId) -> Result<(), SimulationError> {
        if self.workstation_world.get(workstation_id).is_none() {
            return Err(SimulationError::UnknownWorkstation(workstation_id));
        }
        if let Some(job_id) = self.job_world.craft_job_for_workstation(workstation_id) {
            self.cancel_job(job_id)?;
        }
        let supply_jobs = self
            .job_world
            .iter()
            .filter_map(|job| match job.kind() {
                JobKind::SupplyProduction {
                    workstation_id: job_workstation,
                    ..
                } if job_workstation == workstation_id => Some(job.id()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for job_id in supply_jobs {
            self.cancel_job(job_id)?;
        }
        self.production_world
            .remove_for_workstation(workstation_id)
            .map_err(SimulationError::from_production_world)?;
        self.production_logistics_world
            .remove_workstation(workstation_id)
            .map_err(SimulationError::from_production_logistics_world)?;
        self.workstation_world
            .remove(workstation_id)
            .map_err(SimulationError::from_workstation_world)?;
        Ok(())
    }

    pub fn designate_craft(
        &mut self,
        workstation_id: EntityId,
        recipe_id: RecipeId,
    ) -> Result<EntityId, SimulationError> {
        let order_id =
            self.add_production_order(workstation_id, recipe_id, ProductionTarget::finite(1))?;
        self.job_world
            .craft_job_for_order(order_id)
            .ok_or(SimulationError::JobInvariantViolation)
    }

    pub(super) fn validate_workstation_cell(&self, cell: WorldCell) -> Result<(), SimulationError> {
        if !self.is_explored(cell) {
            return Err(SimulationError::WorkstationCellUndiscovered(cell));
        }
        if !self.is_walkable(cell)? {
            return Err(SimulationError::WorkstationCellBlocked(cell));
        }
        if self.natural_resource_at(cell)?.is_some() {
            return Err(SimulationError::WorkstationCellOccupiedByResource(cell));
        }
        if self.stockpile_world.stockpile_at(cell).is_some() {
            return Err(SimulationError::WorkstationCellOccupiedByStockpile(cell));
        }
        if self.item_world.iter().any(|item| {
            item.ground_position()
                .is_some_and(|position| position.containing_cell() == cell)
        }) {
            return Err(SimulationError::WorkstationCellOccupiedByItem(cell));
        }
        if self.construction_world.site_at(cell).is_some()
            || self.construction_world.structure_at(cell).is_some()
        {
            return Err(SimulationError::WorkstationCellBlocked(cell));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::test_support::*;

    #[test]
    fn blocked_workbench_port_layout_rejects_placement_before_allocating_id() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let center = WorldCell::new(0, 0);
        for dy in -1_i64..=1 {
            for dx in -1_i64..=1 {
                let cell = WorldCell::new(center.x() + dx, center.y() + dy);
                simulation
                    .set_terrain_override(cell, Terrain::Grass)
                    .unwrap();
                simulation.depleted_resources.insert(cell);
            }
        }
        let diagonals = [
            WorldCell::new(-1, 1),
            WorldCell::new(1, 1),
            WorldCell::new(1, -1),
            WorldCell::new(-1, -1),
        ];
        let stockpile_id = simulation.create_stockpile(diagonals[0]).unwrap();
        for cell in diagonals.into_iter().skip(1) {
            simulation
                .set_stockpile_cell(stockpile_id, cell, true)
                .unwrap();
        }
        let next_id = simulation.next_entity_id();

        assert_eq!(
            simulation.place_workstation(WorkstationKind::Workbench, center),
            Err(SimulationError::WorkstationPortLayoutUnavailable(center))
        );
        assert_eq!(simulation.next_entity_id(), next_id);
        assert_eq!(simulation.workstation_at(center), None);
    }

    #[test]
    fn workbench_ports_are_fixed_local_and_exclusive_from_stockpiles() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let workstation_cell = WorldCell::new(0, 0);
        let workstation_id = simulation
            .place_workstation(WorkstationKind::Workbench, workstation_cell)
            .unwrap();
        let inputs = production_zone_cells(&simulation, workstation_id, ProductionZoneKind::Input);
        let outputs =
            production_zone_cells(&simulation, workstation_id, ProductionZoneKind::Output);
        assert_eq!(inputs.len(), 2);
        assert_eq!(outputs.len(), 2);

        assert_eq!(
            simulation.set_production_zone_cell(
                workstation_id,
                ProductionZoneKind::Input,
                inputs[0],
                false,
            ),
            Err(SimulationError::WorkbenchInputPortsFixed(workstation_id))
        );
        assert_eq!(
            simulation.set_production_zone_cell(
                workstation_id,
                ProductionZoneKind::Output,
                outputs[0],
                false,
            ),
            Err(SimulationError::WorkbenchOutputPortsFixed(workstation_id))
        );
        assert!(matches!(
            simulation.create_stockpile(inputs[0]),
            Err(SimulationError::StockpileCellBlocked(cell)) if cell == inputs[0]
        ));
        assert!(matches!(
            simulation.create_stockpile(outputs[0]),
            Err(SimulationError::StockpileCellBlocked(cell)) if cell == outputs[0]
        ));
        assert!(
            simulation
                .production_logistics_world
                .indexes_are_consistent()
        );
    }

    #[test]
    fn workbench_input_and_output_cycles_are_independent() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let center = WorldCell::new(0, 0);
        for dy in -1_i64..=1 {
            for dx in -1_i64..=1 {
                let cell = WorldCell::new(center.x() + dx, center.y() + dy);
                simulation
                    .set_terrain_override(cell, Terrain::Grass)
                    .unwrap();
                simulation.depleted_resources.insert(cell);
            }
        }
        let workstation_id = simulation
            .place_workstation(WorkstationKind::Workbench, center)
            .unwrap();
        let original_inputs =
            production_zone_cells(&simulation, workstation_id, ProductionZoneKind::Input);
        let original_outputs =
            production_zone_cells(&simulation, workstation_id, ProductionZoneKind::Output);
        assert_eq!(original_inputs.len(), 2);
        assert_eq!(original_outputs.len(), 2);

        let mut input_layouts = vec![original_inputs.clone()];
        for _ in 0..5 {
            simulation.cycle_workstation_inputs(workstation_id).unwrap();
            input_layouts.push(production_zone_cells(
                &simulation,
                workstation_id,
                ProductionZoneKind::Input,
            ));
            assert_eq!(
                production_zone_cells(&simulation, workstation_id, ProductionZoneKind::Output),
                original_outputs
            );
        }
        input_layouts.sort();
        input_layouts.dedup();
        assert_eq!(input_layouts.len(), 6);
        simulation.cycle_workstation_inputs(workstation_id).unwrap();
        assert_eq!(
            production_zone_cells(&simulation, workstation_id, ProductionZoneKind::Input),
            original_inputs
        );

        let mut output_layouts = vec![original_outputs.clone()];
        for _ in 0..5 {
            simulation
                .cycle_workstation_outputs(workstation_id)
                .unwrap();
            output_layouts.push(production_zone_cells(
                &simulation,
                workstation_id,
                ProductionZoneKind::Output,
            ));
            assert_eq!(
                production_zone_cells(&simulation, workstation_id, ProductionZoneKind::Input),
                original_inputs
            );
        }
        output_layouts.sort();
        output_layouts.dedup();
        assert_eq!(output_layouts.len(), 6);
        simulation
            .cycle_workstation_outputs(workstation_id)
            .unwrap();
        assert_eq!(
            production_zone_cells(&simulation, workstation_id, ProductionZoneKind::Output),
            original_outputs
        );
        assert!(
            simulation
                .production_logistics_world
                .indexes_are_consistent()
        );
    }

    #[test]
    fn workbench_port_layouts_follow_the_two_independent_six_pair_cycles() {
        let center = WorldCell::new(10, 20);
        assert_eq!(
            production_input_layouts(center),
            vec![
                [WorldCell::new(10, 21), WorldCell::new(10, 19)],
                [WorldCell::new(9, 20), WorldCell::new(11, 20)],
                [WorldCell::new(10, 21), WorldCell::new(11, 20)],
                [WorldCell::new(11, 20), WorldCell::new(10, 19)],
                [WorldCell::new(10, 19), WorldCell::new(9, 20)],
                [WorldCell::new(9, 20), WorldCell::new(10, 21)],
            ]
        );
        assert_eq!(
            production_output_layouts(center),
            vec![
                [WorldCell::new(9, 21), WorldCell::new(11, 19)],
                [WorldCell::new(11, 21), WorldCell::new(9, 19)],
                [WorldCell::new(9, 21), WorldCell::new(11, 21)],
                [WorldCell::new(11, 21), WorldCell::new(11, 19)],
                [WorldCell::new(11, 19), WorldCell::new(9, 19)],
                [WorldCell::new(9, 19), WorldCell::new(9, 21)],
            ]
        );
    }
}
