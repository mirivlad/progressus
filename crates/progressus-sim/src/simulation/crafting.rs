//! Craft jobs: input selection, physical supply, execution and output.

use super::*;

impl Simulation {
    pub(super) fn craft_local_quantity(&self, workstation_id: EntityId, kind: ItemId) -> u32 {
        let Some(logistics) = self.production_logistics_world.get(workstation_id) else {
            return 0;
        };
        self.item_world
            .iter()
            .filter(|item| item.kind() == kind)
            .filter(|item| self.job_world.item_job_for_item(item.id()).is_none())
            .filter(|item| {
                self.construction_world
                    .site_for_material(item.id())
                    .is_none()
            })
            .filter(|item| {
                item.ground_position().is_some_and(|position| {
                    logistics.contains(ProductionZoneKind::Input, position.containing_cell())
                })
            })
            .map(|item| item.quantity().get())
            .fold(0_u32, u32::saturating_add)
    }

    pub(super) fn craft_incoming_quantity(&self, workstation_id: EntityId, kind: ItemId) -> u32 {
        self.job_world
            .iter()
            .filter_map(|job| match job.kind() {
                JobKind::SupplyProduction {
                    workstation_id: job_workstation,
                    item_id,
                    ..
                } if job_workstation == workstation_id => self.item_world.get(item_id),
                _ => None,
            })
            .filter(|item| item.kind() == kind)
            .map(|item| item.quantity().get())
            .fold(0_u32, u32::saturating_add)
    }

    pub(super) fn production_zone_destination(
        &self,
        workstation_id: EntityId,
        zone_kind: ProductionZoneKind,
        item_kind: ItemId,
        quantity: u32,
    ) -> Result<Option<WorldCell>, SimulationError> {
        let Some(logistics) = self.production_logistics_world.get(workstation_id) else {
            return Ok(None);
        };
        for merge_only in [true, false] {
            for cell in logistics.cells(zone_kind) {
                if self.job_world.logistics_job_for_destination(cell).is_some()
                    || !self.is_walkable(cell)?
                {
                    continue;
                }
                let occupants = self
                    .item_world
                    .iter()
                    .filter(|item| {
                        item.ground_position()
                            .is_some_and(|position| position.containing_cell() == cell)
                    })
                    .collect::<Vec<_>>();
                let acceptable = match occupants.as_slice() {
                    [] => !merge_only,
                    [existing] => {
                        existing.kind() == item_kind
                            && self.job_world.item_job_for_item(existing.id()).is_none()
                            && self
                                .construction_world
                                .site_for_material(existing.id())
                                .is_none()
                            && existing
                                .quantity()
                                .get()
                                .checked_add(quantity)
                                .is_some_and(|combined| combined <= MAX_STACK_QUANTITY)
                    }
                    _ => false,
                };
                if acceptable {
                    return Ok(Some(cell));
                }
            }
        }
        Ok(None)
    }

    pub(super) fn craft_supply_source(
        &self,
        workstation_id: EntityId,
        kind: ItemId,
    ) -> Option<(EntityId, u32)> {
        let workstation_cell = self.workstation_world.get(workstation_id)?.cell();
        let mut candidates = self
            .item_world
            .iter()
            .filter_map(|item| {
                if item.kind() != kind
                    || self.job_world.item_job_for_item(item.id()).is_some()
                    || self
                        .construction_world
                        .site_for_material(item.id())
                        .is_some()
                {
                    return None;
                }
                let position = item.ground_position()?;
                let cell = position.containing_cell();
                if self.stockpile_world.stockpile_at(cell).is_none()
                    || matches!(
                        self.production_logistics_world.zone_at(cell),
                        Some((_, ProductionZoneKind::Input))
                    )
                {
                    return None;
                }
                Some((
                    cell_manhattan_distance(cell, workstation_cell),
                    item.id(),
                    item.quantity().get(),
                ))
            })
            .collect::<Vec<_>>();
        candidates.sort_unstable();
        candidates
            .into_iter()
            .next()
            .map(|(_, item_id, quantity)| (item_id, quantity))
    }

    pub(super) fn maintain_craft_supply_jobs(&mut self) -> Result<(), SimulationError> {
        let waiting = self
            .job_world
            .iter()
            .filter_map(|job| match (job.kind(), job.state()) {
                (
                    JobKind::Craft {
                        workstation_id,
                        recipe_id,
                        ..
                    },
                    JobState::Available,
                ) => Some((job.id(), workstation_id, recipe_id)),
                _ => None,
            })
            .collect::<Vec<_>>();

        for (_job_id, workstation_id, recipe_id) in waiting {
            let recipe = recipe_id.definition();
            for requirement in recipe.inputs {
                let local = self.craft_local_quantity(workstation_id, requirement.item);
                let incoming = self.craft_incoming_quantity(workstation_id, requirement.item);
                let mut missing = requirement
                    .quantity
                    .saturating_sub(local.saturating_add(incoming));
                while missing > 0 {
                    let Some((source_id, source_quantity)) =
                        self.craft_supply_source(workstation_id, requirement.item)
                    else {
                        break;
                    };
                    let move_quantity = missing.min(source_quantity);
                    let Some(destination) = self.production_zone_destination(
                        workstation_id,
                        ProductionZoneKind::Input,
                        requirement.item,
                        move_quantity,
                    )?
                    else {
                        break;
                    };
                    let item_id = if move_quantity < source_quantity {
                        let split_id = self.id_allocator.allocate()?;
                        self.item_world
                            .split_ground_stack(source_id, split_id, move_quantity)
                            .map_err(|_| SimulationError::JobInvariantViolation)?;
                        split_id
                    } else {
                        source_id
                    };
                    let supply_id = self.id_allocator.allocate()?;
                    self.job_world
                        .insert(Job::new(
                            supply_id,
                            JobKind::SupplyProduction {
                                workstation_id,
                                item_id,
                                destination,
                            },
                        ))
                        .map_err(SimulationError::from_job_world)?;
                    missing -= move_quantity;
                }
            }
        }
        Ok(())
    }

    pub(super) fn item_is_local_input_for_waiting_craft(&self, item_id: EntityId) -> bool {
        let Some(item) = self.item_world.get(item_id) else {
            return false;
        };
        let Some(position) = item.ground_position() else {
            return false;
        };
        let cell = position.containing_cell();
        let Some((workstation_id, ProductionZoneKind::Input)) =
            self.production_logistics_world.zone_at(cell)
        else {
            return false;
        };
        self.job_world.iter().any(|job| {
            let JobKind::Craft {
                workstation_id: job_workstation,
                recipe_id,
                ..
            } = job.kind()
            else {
                return false;
            };
            job_workstation == workstation_id
                && job.state() == JobState::Available
                && recipe_id
                    .definition()
                    .inputs
                    .iter()
                    .any(|requirement| requirement.item == item.kind())
        })
    }

    pub(super) fn maintain_craft_jobs(&mut self) -> Result<(), SimulationError> {
        let workstation_ids = self
            .workstation_world
            .iter()
            .map(Workstation::id)
            .collect::<Vec<_>>();
        for workstation_id in workstation_ids {
            self.ensure_craft_job_for_workstation(workstation_id)?;
        }
        Ok(())
    }

    pub(super) fn ensure_craft_job_for_workstation(
        &mut self,
        workstation_id: EntityId,
    ) -> Result<(), SimulationError> {
        if self
            .job_world
            .craft_job_for_workstation(workstation_id)
            .is_some()
        {
            return Ok(());
        }
        let Some(order) = self
            .production_world
            .first_pending_for_workstation(workstation_id)
            .copied()
        else {
            return Ok(());
        };
        let workstation = self
            .workstation_world
            .get(workstation_id)
            .ok_or(SimulationError::UnknownWorkstation(workstation_id))?;
        if workstation.kind() != order.recipe_id().definition().workstation {
            return Err(SimulationError::RecipeWorkstationMismatch {
                workstation_id,
                recipe_id: order.recipe_id(),
            });
        }
        let job_id = self.id_allocator.allocate()?;
        self.job_world
            .insert(Job::new(
                job_id,
                JobKind::Craft {
                    workstation_id,
                    order_id: order.id(),
                    recipe_id: order.recipe_id(),
                },
            ))
            .map_err(SimulationError::from_job_world)?;
        Ok(())
    }

    pub(super) fn try_assign_production_supply(
        &mut self,
        job_id: EntityId,
        workstation_id: EntityId,
        item_id: EntityId,
        destination: WorldCell,
    ) -> Result<(), SimulationError> {
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

    pub(super) fn try_assign_craft(
        &mut self,
        job_id: EntityId,
        workstation_id: EntityId,
        order_id: EntityId,
        recipe_id: RecipeId,
    ) -> Result<(), SimulationError> {
        let Some(order) = self.production_world.get(order_id) else {
            self.cancel_job(job_id)?;
            return Ok(());
        };
        if order.workstation_id() != workstation_id
            || order.recipe_id() != recipe_id
            || !order.is_pending()
        {
            self.cancel_job(job_id)?;
            return Ok(());
        }
        let Some(workstation) = self.workstation_world.get(workstation_id) else {
            self.cancel_job(job_id)?;
            return Ok(());
        };
        let recipe = recipe_id.definition();
        if workstation.kind() != recipe.workstation || !self.is_walkable(workstation.cell())? {
            self.cancel_job(job_id)?;
            return Ok(());
        }
        let workstation_cell = workstation.cell();
        let Some(input_ids) = self.select_craft_inputs(workstation_id, recipe_id) else {
            return Ok(());
        };
        if self
            .production_zone_destination(
                workstation_id,
                ProductionZoneKind::Output,
                recipe.output,
                recipe.output_quantity,
            )?
            .is_none()
        {
            return Ok(());
        }
        let target = WorldPosition::from_cell_center(workstation_cell)?;
        for worker_id in self.available_workers_by_distance(workstation_cell) {
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
            if let Err(error) = self.job_world.reserve_craft_items(job_id, &input_ids) {
                self.job_world
                    .release_worker(job_id)
                    .map_err(SimulationError::from_job_world)?;
                return Err(SimulationError::from_job_world(error));
            }
            self.apply_navigation_route(worker_id, target, route);
            return Ok(());
        }
        Ok(())
    }

    pub(super) fn select_craft_inputs(
        &self,
        workstation_id: EntityId,
        recipe_id: RecipeId,
    ) -> Option<Vec<EntityId>> {
        let logistics = self.production_logistics_world.get(workstation_id)?;
        let recipe = recipe_id.definition();
        let mut selected = BTreeSet::new();
        for requirement in recipe.inputs {
            let mut remaining = requirement.quantity;
            for item in self.item_world.iter() {
                if selected.contains(&item.id())
                    || item.kind() != requirement.item
                    || self.job_world.item_job_for_item(item.id()).is_some()
                    || self
                        .construction_world
                        .site_for_material(item.id())
                        .is_some()
                {
                    continue;
                }
                let Some(position) = item.ground_position() else {
                    continue;
                };
                if !logistics.contains(ProductionZoneKind::Input, position.containing_cell()) {
                    continue;
                }
                selected.insert(item.id());
                remaining = remaining.saturating_sub(item.quantity().get());
                if remaining == 0 {
                    break;
                }
            }
            if remaining != 0 {
                return None;
            }
        }
        Some(selected.into_iter().collect())
    }

    pub(super) fn craft_reserved_inputs_valid(
        &self,
        job_id: EntityId,
        workstation_id: EntityId,
        recipe_id: RecipeId,
    ) -> bool {
        let Some(workstation) = self.workstation_world.get(workstation_id) else {
            return false;
        };
        let Some(logistics) = self.production_logistics_world.get(workstation_id) else {
            return false;
        };
        let recipe = recipe_id.definition();
        if workstation.kind() != recipe.workstation {
            return false;
        }
        let Some(reserved) = self.job_world.craft_reserved_items(job_id) else {
            return false;
        };
        for requirement in recipe.inputs {
            let available = reserved
                .iter()
                .filter_map(|item_id| {
                    if self.job_world.craft_job_for_item(*item_id) != Some(job_id) {
                        return None;
                    }
                    let item = self.item_world.get(*item_id)?;
                    if item.kind() != requirement.item {
                        return None;
                    }
                    let position = item.ground_position()?;
                    logistics
                        .contains(ProductionZoneKind::Input, position.containing_cell())
                        .then_some(item.quantity().get())
                })
                .fold(0_u32, u32::saturating_add);
            if available < requirement.quantity {
                return false;
            }
        }
        true
    }

    pub(super) fn craft_consumption_plan(
        &self,
        job_id: EntityId,
        recipe_id: RecipeId,
    ) -> Option<Vec<(EntityId, u32)>> {
        let reserved = self.job_world.craft_reserved_items(job_id)?;
        let recipe = recipe_id.definition();
        let mut plan = Vec::new();
        for requirement in recipe.inputs {
            let mut remaining = requirement.quantity;
            for item_id in reserved {
                let item = self.item_world.get(*item_id)?;
                if item.kind() != requirement.item || item.ground_position().is_none() {
                    continue;
                }
                let amount = remaining.min(item.quantity().get());
                if amount != 0 {
                    plan.push((*item_id, amount));
                    remaining -= amount;
                }
                if remaining == 0 {
                    break;
                }
            }
            if remaining != 0 {
                return None;
            }
        }
        Some(plan)
    }

    pub(super) fn complete_craft(
        &mut self,
        job_id: EntityId,
        worker_id: EntityId,
        workstation_id: EntityId,
        order_id: EntityId,
        recipe_id: RecipeId,
    ) -> Result<(), SimulationError> {
        if !self.craft_reserved_inputs_valid(job_id, workstation_id, recipe_id) {
            return Err(SimulationError::JobInvariantViolation);
        }
        self.workstation_world
            .get(workstation_id)
            .ok_or(SimulationError::UnknownWorkstation(workstation_id))?;
        let recipe = recipe_id.definition();
        let plan = self
            .craft_consumption_plan(job_id, recipe_id)
            .ok_or(SimulationError::JobInvariantViolation)?;
        let Some(output_cell) = self.production_zone_destination(
            workstation_id,
            ProductionZoneKind::Output,
            recipe.output,
            recipe.output_quantity,
        )?
        else {
            // Completed work waits for physical output capacity without consuming
            // its reserved inputs or aborting the rest of the simulation tick.
            return Ok(());
        };
        let merge_target = self.item_world.iter().find_map(|item| {
            (item.kind() == recipe.output
                && item
                    .ground_position()
                    .is_some_and(|position| position.containing_cell() == output_cell)
                && item
                    .quantity()
                    .get()
                    .checked_add(recipe.output_quantity)
                    .is_some_and(|combined| combined <= MAX_STACK_QUANTITY))
            .then_some(item.id())
        });
        let output_id = self.id_allocator.allocate()?;
        let output_position = WorldPosition::from_cell_center(output_cell)?;
        let output_quantity = ItemQuantity::new(recipe.output_quantity)
            .expect("recipe outputs are defined with positive quantities");

        for (item_id, amount) in plan {
            self.item_world
                .consume(item_id, amount)
                .map_err(|_| SimulationError::JobInvariantViolation)?;
        }
        self.item_world
            .insert_ground(ItemStack::new_ground(
                output_id,
                recipe.output,
                output_quantity,
                output_position,
            ))
            .map_err(|_| SimulationError::JobInvariantViolation)?;
        if let Some(target_id) = merge_target {
            self.item_world
                .merge_ground_stacks(target_id, output_id)
                .map_err(|_| SimulationError::JobInvariantViolation)?;
        }
        self.production_world
            .complete_one(order_id)
            .map_err(SimulationError::from_production_world)?;
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
    use super::*;
    use crate::simulation::test_support::*;
    use progressus_content::{item, recipe, terrain, workstation};

    #[test]
    fn craft_consumes_exact_quantities_from_input_zone_and_outputs_to_output_zone() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let workstation_id = simulation
            .place_workstation(workstation::WORKBENCH, WorldCell::new(0, 0))
            .unwrap();
        let (wood_id, stone_id) = seed_recipe_inputs(&mut simulation, workstation_id, 5, 3);
        let output_cell =
            production_zone_cells(&simulation, workstation_id, ProductionZoneKind::Output)[0];
        let job_id = simulation
            .designate_craft(workstation_id, recipe::PRIMITIVE_TOOL)
            .unwrap();
        let mut saw_working = false;

        for _ in 0..128 {
            simulation.advance_ticks(1).unwrap();
            saw_working |= simulation
                .job_world
                .get(job_id)
                .is_some_and(|job| matches!(job.state(), JobState::Working { .. }));
            if simulation.job_world.get(job_id).is_none() {
                break;
            }
        }

        assert!(saw_working);
        assert!(simulation.job_world.get(job_id).is_none());
        assert_eq!(
            simulation.item_world.get(wood_id).unwrap().quantity().get(),
            3
        );
        assert_eq!(
            simulation
                .item_world
                .get(stone_id)
                .unwrap()
                .quantity()
                .get(),
            2
        );
        let tools = simulation
            .items()
            .filter(|item| item.kind() == item::PRIMITIVE_TOOL)
            .collect::<Vec<_>>();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].quantity().get(), 1);
        assert_eq!(
            tools[0].ground_position().unwrap().containing_cell(),
            output_cell
        );
        assert!(simulation.item_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(simulation.workstation_world.indexes_are_consistent());
        assert!(
            simulation
                .production_logistics_world
                .indexes_are_consistent()
        );
    }

    #[test]
    fn infinite_orders_share_common_stockpile_without_double_supply_or_starvation() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let (shared, first_bench_cell, second_bench_cell, _, _) =
            shared_workbench_fixture_cells(&simulation);
        let stockpile_id = simulation.create_stockpile(shared).unwrap();
        let first_workstation = simulation
            .place_workstation(workstation::WORKBENCH, first_bench_cell)
            .unwrap();
        let second_workstation = simulation
            .place_workstation(workstation::WORKBENCH, second_bench_cell)
            .unwrap();
        let first_input =
            production_zone_cells(&simulation, first_workstation, ProductionZoneKind::Input)[0];
        let second_input =
            production_zone_cells(&simulation, second_workstation, ProductionZoneKind::Input)[0];
        let first_stone = insert_ground_stack(&mut simulation, item::STONE, 1, first_input);
        let second_stone = insert_ground_stack(&mut simulation, item::STONE, 1, second_input);
        let shared_wood = insert_ground_stack(&mut simulation, item::WOOD, 2, shared);

        simulation
            .add_production_order(
                first_workstation,
                recipe::PRIMITIVE_TOOL,
                ProductionTarget::Infinite,
            )
            .unwrap();
        simulation
            .add_production_order(
                second_workstation,
                recipe::PRIMITIVE_TOOL,
                ProductionTarget::Infinite,
            )
            .unwrap();
        simulation.advance_ticks(1).unwrap();

        assert_eq!(
            simulation
                .jobs()
                .filter(|job| matches!(
                    job.kind(),
                    JobKind::SupplyProduction { item_id, .. } if item_id == shared_wood
                ))
                .count(),
            1,
            "one physical stockpile stack can belong to only one supply job",
        );
        assert_eq!(
            simulation.item_world.get(shared_wood).unwrap().carrier(),
            None
        );

        for _ in 0..256 {
            simulation.advance_ticks(1).unwrap();
            if total_item_quantity(&simulation, item::PRIMITIVE_TOOL) >= 1 {
                break;
            }
        }
        assert!(simulation.item_world.get(shared_wood).is_none());
        assert!(
            simulation.item_world.get(first_stone).is_none()
                ^ simulation.item_world.get(second_stone).is_none(),
            "exactly one workstation must consume its private Stone input first",
        );

        let second_wood = insert_ground_stack(&mut simulation, item::WOOD, 2, shared);
        for _ in 0..256 {
            simulation.advance_ticks(1).unwrap();
            if total_item_quantity(&simulation, item::PRIMITIVE_TOOL) >= 2 {
                break;
            }
        }

        assert!(simulation.item_world.get(second_wood).is_none());
        assert!(simulation.item_world.get(first_stone).is_none());
        assert!(simulation.item_world.get(second_stone).is_none());
        assert_eq!(total_item_quantity(&simulation, item::PRIMITIVE_TOOL), 2);
        simulation.advance_ticks(32).unwrap();
        assert_eq!(
            total_item_quantity(&simulation, item::PRIMITIVE_TOOL),
            2,
            "infinite orders wait when physical inputs are exhausted",
        );
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(simulation.production_world.indexes_are_consistent());
        assert!(
            simulation
                .production_logistics_world
                .indexes_are_consistent()
        );
        assert_eq!(
            simulation.stockpile_world.stockpile_at(shared),
            Some(stockpile_id)
        );
    }

    #[test]
    fn production_physically_supplies_exact_recipe_amounts_from_distant_stockpile_cells() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let workstation_id = simulation
            .place_workstation(workstation::WORKBENCH, WorldCell::new(0, 0))
            .unwrap();
        let input_cells =
            production_zone_cells(&simulation, workstation_id, ProductionZoneKind::Input);
        let output_cell =
            production_zone_cells(&simulation, workstation_id, ProductionZoneKind::Output)[0];
        let source_cells = distant_stockpile_cells(&simulation, WorldCell::new(0, 0), 2);
        let stockpile_id = simulation.create_stockpile(source_cells[0]).unwrap();
        simulation
            .set_stockpile_cell(stockpile_id, source_cells[1], true)
            .unwrap();
        let wood_source = insert_ground_stack(&mut simulation, item::WOOD, 20, source_cells[0]);
        let stone_source = insert_ground_stack(&mut simulation, item::STONE, 20, source_cells[1]);
        simulation
            .add_production_order(
                workstation_id,
                recipe::PRIMITIVE_TOOL,
                ProductionTarget::finite(1),
            )
            .unwrap();
        let mut saw_exact_wood_supply = false;
        let mut saw_exact_stone_supply = false;
        let mut saw_output = false;

        for _ in 0..1024 {
            simulation.advance_ticks(1).unwrap();
            for job in simulation.jobs() {
                if let JobKind::SupplyProduction {
                    workstation_id: job_workstation,
                    item_id,
                    destination,
                } = job.kind()
                {
                    if job_workstation != workstation_id || !input_cells.contains(&destination) {
                        continue;
                    }
                    if let Some(item) = simulation.item_world.get(item_id) {
                        saw_exact_wood_supply |=
                            item.kind() == item::WOOD && item.quantity().get() == 2;
                        saw_exact_stone_supply |=
                            item.kind() == item::STONE && item.quantity().get() == 1;
                    }
                }
            }
            saw_output |= simulation.items().any(|item| {
                item.kind() == item::PRIMITIVE_TOOL
                    && item
                        .ground_position()
                        .is_some_and(|position| position.containing_cell() == output_cell)
            });
            if total_item_quantity(&simulation, item::PRIMITIVE_TOOL) >= 1 && saw_output {
                break;
            }
        }

        assert!(saw_exact_wood_supply);
        assert!(saw_exact_stone_supply);
        assert!(
            saw_output,
            "crafted output must physically appear in Output zone"
        );
        assert_eq!(total_item_quantity(&simulation, item::PRIMITIVE_TOOL), 1);
        assert_eq!(
            simulation
                .item_world
                .get(wood_source)
                .unwrap()
                .quantity()
                .get(),
            18
        );
        assert_eq!(
            simulation
                .item_world
                .get(stone_source)
                .unwrap()
                .quantity()
                .get(),
            19
        );
        assert!(simulation.item_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(
            simulation
                .production_logistics_world
                .indexes_are_consistent()
        );
    }

    #[test]
    fn craft_job_stays_available_without_physical_inputs_near_workbench() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cell = empty_stockpile_cells(&simulation, 1)[0];
        let workstation_id = simulation
            .place_workstation(workstation::WORKBENCH, cell)
            .unwrap();
        let job_id = simulation
            .designate_craft(workstation_id, recipe::PRIMITIVE_TOOL)
            .unwrap();

        simulation.advance_ticks(16).unwrap();

        assert_eq!(
            simulation.job_world.get(job_id).unwrap().state(),
            JobState::Available
        );
        assert!(simulation.job_world.craft_reserved_items(job_id).is_none());
        assert!(
            simulation
                .items()
                .all(|item| item.kind() != item::PRIMITIVE_TOOL)
        );
    }

    #[test]
    fn production_order_repeats_craft_until_remaining_runs_reaches_zero() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let workstation_id = simulation
            .place_workstation(workstation::WORKBENCH, WorldCell::new(0, 0))
            .unwrap();
        seed_recipe_inputs(&mut simulation, workstation_id, 6, 3);
        let order_id = simulation
            .add_production_order(
                workstation_id,
                recipe::PRIMITIVE_TOOL,
                ProductionTarget::finite(3),
            )
            .unwrap();

        for _ in 0..512 {
            simulation.advance_ticks(1).unwrap();
            if simulation
                .production_world
                .get(order_id)
                .is_some_and(|order| order.remaining_runs() == Some(0))
            {
                break;
            }
        }

        assert_eq!(
            simulation
                .production_world
                .get(order_id)
                .unwrap()
                .remaining_runs(),
            Some(0)
        );
        assert_eq!(total_item_quantity(&simulation, item::PRIMITIVE_TOOL), 3);
        assert!(
            simulation
                .job_world
                .craft_job_for_workstation(workstation_id)
                .is_none()
        );
        assert!(simulation.production_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
    }

    #[test]
    fn blocked_craft_output_preserves_completed_work_across_save_and_cancellation() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let workstation_id = simulation
            .place_workstation(workstation::WORKBENCH, WorldCell::new(0, 0))
            .unwrap();
        let (wood_id, stone_id) = seed_recipe_inputs(&mut simulation, workstation_id, 2, 1);
        let output_cells =
            production_zone_cells(&simulation, workstation_id, ProductionZoneKind::Output);
        let order_id = simulation
            .add_production_order(
                workstation_id,
                recipe::PRIMITIVE_TOOL,
                ProductionTarget::finite(1),
            )
            .unwrap();
        let job_id = simulation.job_world.craft_job_for_order(order_id).unwrap();

        for _ in 0..128 {
            simulation.advance_ticks(1).unwrap();
            if matches!(
                simulation.job_world.get(job_id).map(Job::state),
                Some(JobState::Working {
                    remaining_ticks: 1,
                    ..
                })
            ) {
                break;
            }
        }
        assert!(matches!(
            simulation.job_world.get(job_id).map(Job::state),
            Some(JobState::Working {
                remaining_ticks: 1,
                ..
            })
        ));
        let worker_id = simulation
            .job_world
            .get(job_id)
            .unwrap()
            .state()
            .worker()
            .unwrap();
        let reserved = simulation
            .job_world
            .craft_reserved_items(job_id)
            .unwrap()
            .clone();

        for cell in &output_cells {
            simulation
                .set_terrain_override(*cell, terrain::ROCK)
                .unwrap();
        }
        simulation.advance_ticks(3).unwrap();

        assert!(matches!(
            simulation.job_world.get(job_id).map(Job::state),
            Some(JobState::Working {
                worker_id: active_worker,
                remaining_ticks: 1,
            }) if active_worker == worker_id
        ));
        assert_eq!(
            simulation.job_world.craft_reserved_items(job_id),
            Some(&reserved)
        );
        assert_eq!(
            simulation.item_world.get(wood_id).unwrap().quantity().get(),
            2
        );
        assert_eq!(
            simulation
                .item_world
                .get(stone_id)
                .unwrap()
                .quantity()
                .get(),
            1
        );
        assert_eq!(total_item_quantity(&simulation, item::PRIMITIVE_TOOL), 0);

        let saved = simulation.save_json().unwrap();
        let mut completed = Simulation::load_json(&saved).unwrap();
        let mut cancelled = Simulation::load_json(&saved).unwrap();

        for cell in &output_cells {
            completed
                .set_terrain_override(*cell, terrain::GRASS)
                .unwrap();
        }
        completed.advance_ticks(1).unwrap();
        assert_eq!(total_item_quantity(&completed, item::PRIMITIVE_TOOL), 1);
        assert_eq!(
            completed
                .production_world
                .get(order_id)
                .unwrap()
                .remaining_runs(),
            Some(0)
        );
        assert!(completed.job_world.get(job_id).is_none());
        completed.advance_ticks(16).unwrap();
        assert_eq!(total_item_quantity(&completed, item::PRIMITIVE_TOOL), 1);

        cancelled.remove_production_order(order_id).unwrap();
        assert!(cancelled.job_world.get(job_id).is_none());
        assert!(cancelled.job_world.craft_reserved_items(job_id).is_none());
        for item_id in reserved {
            assert_eq!(cancelled.job_world.craft_job_for_item(item_id), None);
        }
        assert_eq!(
            cancelled.item_world.get(wood_id).unwrap().quantity().get(),
            2
        );
        assert_eq!(
            cancelled.item_world.get(stone_id).unwrap().quantity().get(),
            1
        );
        assert!(cancelled.job_world.indexes_are_consistent());
    }

    #[test]
    fn manual_interruption_releases_craft_inputs_without_consuming_them() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let workstation_id = simulation
            .place_workstation(workstation::WORKBENCH, WorldCell::new(0, 0))
            .unwrap();
        seed_recipe_inputs(&mut simulation, workstation_id, 2, 1);
        let job_id = simulation
            .designate_craft(workstation_id, recipe::PRIMITIVE_TOOL)
            .unwrap();
        simulation.advance_ticks(1).unwrap();
        let worker_id = simulation
            .job_world
            .get(job_id)
            .unwrap()
            .state()
            .worker()
            .unwrap();
        let before = simulation
            .job_world
            .craft_reserved_items(job_id)
            .unwrap()
            .iter()
            .map(|id| (*id, simulation.item_world.get(*id).unwrap().quantity()))
            .collect::<Vec<_>>();

        simulation.stop_movement(worker_id).unwrap();

        assert_eq!(
            simulation.job_world.get(job_id).unwrap().state(),
            JobState::Available
        );
        assert!(simulation.job_world.craft_reserved_items(job_id).is_none());
        for (item_id, quantity) in before {
            assert_eq!(
                simulation.item_world.get(item_id).unwrap().quantity(),
                quantity
            );
            assert_eq!(simulation.job_world.craft_job_for_item(item_id), None);
        }
        assert!(simulation.job_world.indexes_are_consistent());
    }

    #[test]
    fn removing_workstation_cancels_craft_and_releases_input_reservations() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let workstation_id = simulation
            .place_workstation(workstation::WORKBENCH, WorldCell::new(0, 0))
            .unwrap();
        seed_recipe_inputs(&mut simulation, workstation_id, 2, 1);
        let job_id = simulation
            .designate_craft(workstation_id, recipe::PRIMITIVE_TOOL)
            .unwrap();
        simulation.advance_ticks(1).unwrap();
        assert!(simulation.job_world.craft_reserved_items(job_id).is_some());

        simulation.remove_workstation(workstation_id).unwrap();

        assert!(simulation.workstation_world.get(workstation_id).is_none());
        assert!(
            simulation
                .production_logistics_world
                .get(workstation_id)
                .is_none()
        );
        assert!(simulation.job_world.get(job_id).is_none());
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(simulation.workstation_world.indexes_are_consistent());
        assert!(
            simulation
                .production_logistics_world
                .indexes_are_consistent()
        );
    }

    #[test]
    fn crafted_output_leaves_output_zone_and_reaches_stockpile() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let workstation_id = simulation
            .place_workstation(workstation::WORKBENCH, WorldCell::new(0, 0))
            .unwrap();
        seed_recipe_inputs(&mut simulation, workstation_id, 2, 1);
        let stock_cells = distant_stockpile_cells(&simulation, WorldCell::new(0, 0), 1);
        let stockpile_id = simulation.create_stockpile(stock_cells[0]).unwrap();
        simulation
            .add_production_order(
                workstation_id,
                recipe::PRIMITIVE_TOOL,
                ProductionTarget::finite(1),
            )
            .unwrap();
        let mut saw_output_zone = false;
        let mut reached_stockpile = false;

        for _ in 0..512 {
            simulation.advance_ticks(1).unwrap();
            for item in simulation
                .items()
                .filter(|item| item.kind() == item::PRIMITIVE_TOOL)
            {
                let Some(cell) = item.ground_position().map(WorldPosition::containing_cell) else {
                    continue;
                };
                saw_output_zone |= simulation.production_logistics_world.zone_at(cell)
                    == Some((workstation_id, ProductionZoneKind::Output));
                if simulation.stockpile_world.stockpile_at(cell) == Some(stockpile_id) {
                    reached_stockpile = true;
                }
            }
            if reached_stockpile {
                break;
            }
        }

        assert!(saw_output_zone);
        assert!(reached_stockpile);
        assert_eq!(total_item_quantity(&simulation, item::PRIMITIVE_TOOL), 1);
        assert!(simulation.job_world.indexes_are_consistent());
    }
}
