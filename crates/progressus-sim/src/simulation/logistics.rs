//! Stockpile floor areas and the physical Haul jobs that fill them.

use super::*;

impl Simulation {
    pub fn create_stockpile(&mut self, cell: WorldCell) -> Result<EntityId, SimulationError> {
        self.validate_stockpile_cell(cell)?;
        if let Some(existing) = self.stockpile_world.stockpile_at(cell) {
            return Err(SimulationError::StockpileCellAlreadyOwned {
                cell,
                stockpile_id: existing,
            });
        }
        let id = self.id_allocator.allocate()?;
        self.stockpile_world
            .insert(Stockpile::new(id, cell))
            .map_err(SimulationError::from_stockpile_world)?;
        Ok(id)
    }

    pub fn set_stockpile_item_allowed(
        &mut self,
        stockpile_id: EntityId,
        kind: ItemId,
        allowed: bool,
    ) -> Result<(), SimulationError> {
        if self.stockpile_world.get(stockpile_id).is_none() {
            return Err(SimulationError::UnknownStockpile(stockpile_id));
        }
        if !allowed {
            let jobs = self
                .job_world
                .iter()
                .filter_map(|job| match job.kind() {
                    JobKind::Haul {
                        item_id,
                        stockpile_id: destination_stockpile,
                        ..
                    } if destination_stockpile == stockpile_id
                        && self
                            .item_world
                            .get(item_id)
                            .is_some_and(|item| item.kind() == kind) =>
                    {
                        Some(job.id())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            for job_id in jobs {
                self.cancel_job(job_id)?;
            }
        }
        self.stockpile_world
            .set_item_allowed(stockpile_id, kind, allowed)
            .map_err(SimulationError::from_stockpile_world)?;
        Ok(())
    }

    pub fn set_stockpile_cell(
        &mut self,
        stockpile_id: EntityId,
        cell: WorldCell,
        enabled: bool,
    ) -> Result<(), SimulationError> {
        if self.stockpile_world.get(stockpile_id).is_none() {
            return Err(SimulationError::UnknownStockpile(stockpile_id));
        }
        if enabled {
            self.validate_stockpile_cell(cell)?;
        } else {
            let jobs = self
                .job_world
                .iter()
                .filter_map(|job| match job.kind() {
                    JobKind::Haul { destination, .. } if destination == cell => Some(job.id()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            for job_id in jobs {
                self.cancel_job(job_id)?;
            }
        }
        self.stockpile_world
            .set_cell(stockpile_id, cell, enabled)
            .map_err(SimulationError::from_stockpile_world)?;
        Ok(())
    }

    pub(super) fn validate_stockpile_cell(&self, cell: WorldCell) -> Result<(), SimulationError> {
        if !self.is_explored(cell) {
            return Err(SimulationError::StockpileCellUndiscovered(cell));
        }
        if !self.is_walkable(cell)? {
            return Err(SimulationError::StockpileCellBlocked(cell));
        }
        if self.natural_resource_at(cell)?.is_some() {
            return Err(SimulationError::StockpileCellOccupiedByResource(cell));
        }
        if self.workstation_world.workstation_at(cell).is_some() {
            return Err(SimulationError::StockpileCellOccupiedByWorkstation(cell));
        }
        if self.production_logistics_world.zone_at(cell).is_some()
            || self.construction_world.site_at(cell).is_some()
            || self.construction_world.structure_at(cell).is_some()
        {
            return Err(SimulationError::StockpileCellBlocked(cell));
        }
        Ok(())
    }

    pub(super) fn stockpile_destination_accepts_item(
        &self,
        item_id: EntityId,
        destination: WorldCell,
    ) -> bool {
        let Some(item) = self.item_world.get(item_id) else {
            return false;
        };
        let Some(stockpile_id) = self.stockpile_world.stockpile_at(destination) else {
            return false;
        };
        if !self
            .stockpile_world
            .get(stockpile_id)
            .is_some_and(|stockpile| stockpile.accepts(item.kind()))
        {
            return false;
        }
        let mut occupants = self.item_world.iter().filter(|other| {
            other.id() != item_id
                && other
                    .ground_position()
                    .is_some_and(|position| position.containing_cell() == destination)
        });
        let Some(existing) = occupants.next() else {
            return true;
        };
        if occupants.next().is_some() || existing.kind() != item.kind() {
            return false;
        }
        existing
            .quantity()
            .get()
            .checked_add(item.quantity().get())
            .is_some_and(|combined| combined <= MAX_STACK_QUANTITY)
    }

    pub(super) fn stockpile_merge_capacity(
        &self,
        item_id: EntityId,
        destination: WorldCell,
    ) -> Option<(EntityId, u32)> {
        let item = self.item_world.get(item_id)?;
        let mut occupants = self.item_world.iter().filter(|other| {
            other.id() != item_id
                && other
                    .ground_position()
                    .is_some_and(|position| position.containing_cell() == destination)
        });
        let existing = occupants.next()?;
        if occupants.next().is_some() || existing.kind() != item.kind() {
            return None;
        }
        let capacity = MAX_STACK_QUANTITY.saturating_sub(existing.quantity().get());
        (capacity > 0).then_some((existing.id(), capacity))
    }

    pub(super) fn stockpile_merge_target(
        &self,
        item_id: EntityId,
        destination: WorldCell,
    ) -> Option<EntityId> {
        let item = self.item_world.get(item_id)?;
        let (target_id, capacity) = self.stockpile_merge_capacity(item_id, destination)?;
        (item.quantity().get() <= capacity).then_some(target_id)
    }

    pub(super) fn stockpile_destination_for_item(
        &self,
        item_id: EntityId,
        allow_empty: bool,
    ) -> Result<Option<(EntityId, WorldCell, u32)>, SimulationError> {
        let Some(item) = self.item_world.get(item_id) else {
            return Ok(None);
        };
        let quantity = item.quantity().get();

        // First pass always prefers filling an existing compatible stack,
        // even when only part of the source fits. The caller splits that
        // exact amount before creating the physical Haul job.
        for stockpile in self.stockpile_world.iter() {
            if !stockpile.accepts(item.kind()) {
                continue;
            }
            for cell in stockpile.cells() {
                if self.job_world.haul_job_for_destination(cell).is_some()
                    || !self.is_walkable(cell)?
                {
                    continue;
                }
                if let Some((_target_id, capacity)) = self.stockpile_merge_capacity(item_id, cell) {
                    return Ok(Some((stockpile.id(), cell, quantity.min(capacity))));
                }
            }
        }

        if !allow_empty {
            return Ok(None);
        }
        for stockpile in self.stockpile_world.iter() {
            if !stockpile.accepts(item.kind()) {
                continue;
            }
            for cell in stockpile.cells() {
                if self.job_world.haul_job_for_destination(cell).is_some()
                    || !self.is_walkable(cell)?
                {
                    continue;
                }
                let occupied = self.item_world.iter().any(|other| {
                    other.id() != item_id
                        && other
                            .ground_position()
                            .is_some_and(|position| position.containing_cell() == cell)
                });
                if !occupied {
                    return Ok(Some((stockpile.id(), cell, quantity)));
                }
            }
        }
        Ok(None)
    }

    pub(super) fn stockpile_consolidation_destination(
        &self,
        item_id: EntityId,
    ) -> Result<Option<(EntityId, WorldCell, u32)>, SimulationError> {
        let Some(item) = self.item_world.get(item_id) else {
            return Ok(None);
        };
        let Some(source_position) = item.ground_position() else {
            return Ok(None);
        };
        let source_cell = source_position.containing_cell();
        let Some(stockpile_id) = self.stockpile_world.stockpile_at(source_cell) else {
            return Ok(None);
        };
        let Some(stockpile) = self.stockpile_world.get(stockpile_id) else {
            return Ok(None);
        };

        // Only move a higher stable ID into a lower one. This canonical
        // direction prevents underfilled stacks from ping-ponging between
        // cells on successive maintenance ticks.
        for cell in stockpile.cells() {
            if cell == source_cell
                || self.job_world.haul_job_for_destination(cell).is_some()
                || !self.is_walkable(cell)?
            {
                continue;
            }
            let Some((target_id, capacity)) = self.stockpile_merge_capacity(item_id, cell) else {
                continue;
            };
            if target_id < item_id {
                return Ok(Some((
                    stockpile_id,
                    cell,
                    item.quantity().get().min(capacity),
                )));
            }
        }
        Ok(None)
    }

    pub(super) fn maintain_haul_jobs(&mut self) -> Result<(), SimulationError> {
        let existing_haul_jobs = self
            .job_world
            .iter()
            .filter_map(|job| match job.kind() {
                JobKind::Haul { .. } => Some(job.id()),
                JobKind::Harvest { .. }
                | JobKind::Eat { .. }
                | JobKind::Craft { .. }
                | JobKind::SupplyProduction { .. }
                | JobKind::DeliverConstruction { .. }
                | JobKind::Construct { .. } => None,
            })
            .collect::<Vec<_>>();
        for job_id in existing_haul_jobs {
            let Some(job) = self.job_world.get(job_id).cloned() else {
                continue;
            };
            let JobKind::Haul {
                item_id,
                stockpile_id,
                destination,
            } = job.kind()
            else {
                continue;
            };
            let destination_valid = self.stockpile_world.stockpile_at(destination)
                == Some(stockpile_id)
                && self.is_walkable(destination)?
                && self.stockpile_destination_accepts_item(item_id, destination);
            let item_valid = match job.state() {
                JobState::Transporting { worker_id } => self
                    .item_world
                    .get(item_id)
                    .is_some_and(|item| item.carrier() == Some(worker_id)),
                _ => self.item_world.get(item_id).is_some_and(|item| {
                    item.ground_position().is_some_and(|position| {
                        self.is_explored(position.containing_cell())
                            && position.containing_cell() != destination
                    })
                }),
            };
            if !destination_valid || !item_valid {
                self.cancel_job(job_id)?;
            }
        }

        let available_ground_items = self
            .item_world
            .iter()
            .filter_map(|item| {
                let position = item.ground_position()?;
                (self.is_explored(position.containing_cell())
                    && self.job_world.item_job_for_item(item.id()).is_none()
                    && !self.item_is_local_input_for_waiting_craft(item.id())
                    && self
                        .construction_world
                        .site_for_material(item.id())
                        .is_none())
                .then_some((item.id(), position.containing_cell()))
            })
            .collect::<Vec<_>>();

        for (item_id, source_cell) in available_ground_items {
            let source_stockpile = self.stockpile_world.stockpile_at(source_cell);
            let source_accepts_item = source_stockpile
                .and_then(|id| self.stockpile_world.get(id))
                .zip(self.item_world.get(item_id))
                .is_some_and(|(stockpile, item)| stockpile.accepts(item.kind()));
            let destination = if source_accepts_item {
                self.stockpile_consolidation_destination(item_id)?
            } else {
                self.stockpile_destination_for_item(item_id, true)?
            };
            let Some((stockpile_id, destination, move_quantity)) = destination else {
                continue;
            };
            let source_quantity = self
                .item_world
                .get(item_id)
                .expect("candidate item is still live")
                .quantity()
                .get();
            let haul_item_id = if move_quantity < source_quantity {
                let split_id = self.id_allocator.allocate()?;
                self.item_world
                    .split_ground_stack(item_id, split_id, move_quantity)
                    .map_err(|_| SimulationError::JobInvariantViolation)?;
                split_id
            } else {
                item_id
            };
            let job_id = self.id_allocator.allocate()?;
            self.job_world
                .insert(Job::new(
                    job_id,
                    JobKind::Haul {
                        item_id: haul_item_id,
                        stockpile_id,
                        destination,
                    },
                ))
                .map_err(SimulationError::from_job_world)?;
        }
        Ok(())
    }

    pub(super) fn try_assign_haul(
        &mut self,
        job_id: EntityId,
        item_id: EntityId,
        stockpile_id: EntityId,
        destination: WorldCell,
    ) -> Result<(), SimulationError> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::test_support::*;
    use progressus_content::{item, terrain};

    #[test]
    fn stockpile_cells_are_unique_validated_and_remove_when_empty() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let first = simulation.create_stockpile(WorldCell::new(0, 0)).unwrap();
        assert_eq!(simulation.stockpile_at(WorldCell::new(0, 0)), Some(first));
        assert_eq!(simulation.stockpiles().count(), 1);
        assert!(simulation.stockpile_world.indexes_are_consistent());

        let second = simulation.create_stockpile(WorldCell::new(1, 0)).unwrap();
        assert_eq!(
            simulation.set_stockpile_cell(first, WorldCell::new(1, 0), true),
            Err(SimulationError::StockpileCellAlreadyOwned {
                cell: WorldCell::new(1, 0),
                stockpile_id: second,
            })
        );
        let (resource_cell, _) = harvest_fixture(&simulation);
        assert_eq!(
            simulation.create_stockpile(resource_cell),
            Err(SimulationError::StockpileCellOccupiedByResource(
                resource_cell
            ))
        );
        let blocked = (-7..=7)
            .flat_map(|x| (-5..=5).map(move |y| WorldCell::new(x, y)))
            .find(|cell| {
                simulation.is_explored(*cell)
                    && simulation.effective_terrain_at(*cell).unwrap() != terrain::GRASS
            })
            .unwrap();
        assert_eq!(
            simulation.create_stockpile(blocked),
            Err(SimulationError::StockpileCellBlocked(blocked))
        );
        let unknown = WorldCell::new(100, 100);
        assert_eq!(
            simulation.create_stockpile(unknown),
            Err(SimulationError::StockpileCellUndiscovered(unknown))
        );

        simulation
            .set_stockpile_cell(first, WorldCell::new(0, 0), false)
            .unwrap();
        assert_eq!(simulation.stockpile_at(WorldCell::new(0, 0)), None);
        assert!(simulation.stockpile_world.get(first).is_none());
        assert!(simulation.stockpile_world.indexes_are_consistent());
    }

    #[test]
    fn haul_job_physically_carries_one_stack_into_the_stockpile() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let destination = WorldCell::new(0, 0);
        let stockpile_id = simulation.create_stockpile(destination).unwrap();
        let item_id = EntityId::new(6).unwrap();
        let before = simulation.item_world.get(item_id).unwrap().clone();
        let mut saw_carried = false;

        for _ in 0..256 {
            simulation.advance_ticks(1).unwrap();
            let item = simulation.item_world.get(item_id).unwrap();
            saw_carried |= item.carrier().is_some();
            if item
                .ground_position()
                .is_some_and(|position| position.containing_cell() == destination)
            {
                break;
            }
        }

        let item = simulation.item_world.get(item_id).unwrap();
        assert!(
            saw_carried,
            "haul must use the canonical Carried item state"
        );
        assert_eq!(item.id(), before.id());
        assert_eq!(item.kind(), before.kind());
        assert_eq!(item.quantity(), before.quantity());
        assert_eq!(
            item.ground_position(),
            Some(WorldPosition::from_cell_center(destination).unwrap())
        );
        assert_eq!(simulation.stockpile_at(destination), Some(stockpile_id));
        assert_eq!(simulation.jobs().count(), 0);
        assert!(simulation.item_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
    }

    #[test]
    fn items_already_inside_a_stockpile_do_not_generate_haul_jobs() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        simulation.create_stockpile(WorldCell::new(-2, 0)).unwrap();
        simulation.advance_ticks(8).unwrap();

        assert_eq!(
            simulation
                .item_world
                .get(EntityId::new(6).unwrap())
                .unwrap()
                .ground_position()
                .unwrap()
                .containing_cell(),
            WorldCell::new(-2, 0)
        );
        assert!(!simulation.jobs().any(|job| matches!(
            job.kind(),
            JobKind::Haul { item_id, .. } if item_id == EntityId::new(6).unwrap()
        )));
    }

    #[test]
    fn haul_merges_same_kind_stacks_into_one_physical_stockpile_stack() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let target_id = EntityId::new(6).unwrap();
        let source_id = EntityId::new(8).unwrap();
        let destination = simulation
            .item_world
            .get(target_id)
            .unwrap()
            .ground_position()
            .unwrap()
            .containing_cell();
        simulation.create_stockpile(destination).unwrap();

        for _ in 0..512 {
            simulation.advance_ticks(1).unwrap();
            if simulation.item_world.get(source_id).is_none() {
                break;
            }
        }

        let merged = simulation.item_world.get(target_id).unwrap();
        assert_eq!(merged.kind(), item::WOOD);
        assert_eq!(merged.quantity().get(), 18);
        assert_eq!(
            merged.ground_position().unwrap().containing_cell(),
            destination
        );
        assert!(simulation.item_world.get(source_id).is_none());
        assert!(simulation.item_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
    }

    #[test]
    fn interrupting_transport_drops_the_item_and_releases_the_worker() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        simulation.create_stockpile(WorldCell::new(0, 0)).unwrap();
        let (job_id, item_id, worker_id) = transporting_haul_fixture(&mut simulation);
        let worker_position = character(&simulation, worker_id).position();

        simulation.stop_movement(worker_id).unwrap();

        assert_eq!(
            simulation
                .item_world
                .get(item_id)
                .unwrap()
                .ground_position(),
            Some(worker_position)
        );
        assert_eq!(simulation.job_for_worker(worker_id), None);
        assert_eq!(
            simulation.job_world.get(job_id).unwrap().state(),
            JobState::Available
        );
        assert_eq!(
            character(&simulation, worker_id).movement(),
            MovementState::Idle
        );
        assert!(simulation.item_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
    }

    #[test]
    fn removing_an_active_stockpile_cell_cancels_haul_and_drops_carried_item() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let destination = WorldCell::new(0, 0);
        let stockpile_id = simulation.create_stockpile(destination).unwrap();
        let (_job_id, item_id, worker_id) = transporting_haul_fixture(&mut simulation);
        let worker_position = character(&simulation, worker_id).position();

        simulation
            .set_stockpile_cell(stockpile_id, destination, false)
            .unwrap();

        assert!(simulation.stockpiles().next().is_none());
        assert!(simulation.jobs().next().is_none());
        assert_eq!(
            simulation
                .item_world
                .get(item_id)
                .unwrap()
                .ground_position(),
            Some(worker_position)
        );
        assert_eq!(simulation.job_for_worker(worker_id), None);
        assert!(simulation.item_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
    }

    #[test]
    fn haul_prefers_a_compatible_partial_stack_before_an_empty_stockpile_cell() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let target_id = EntityId::new(6).unwrap();
        let source_id = EntityId::new(8).unwrap();
        let target_cell = simulation
            .item_world
            .get(target_id)
            .unwrap()
            .ground_position()
            .unwrap()
            .containing_cell();
        let empty_cell = empty_stockpile_cells(&simulation, 1)[0];
        let stockpile_id = simulation.create_stockpile(empty_cell).unwrap();
        simulation
            .set_stockpile_cell(stockpile_id, target_cell, true)
            .unwrap();

        assert_eq!(
            simulation
                .stockpile_destination_for_item(source_id, true)
                .unwrap(),
            Some((stockpile_id, target_cell, 10)),
        );
    }

    #[test]
    fn stockpile_item_policy_filters_haul_destinations_and_moves_disallowed_contents_out() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let cells = empty_stockpile_cells(&simulation, 3);
        let rejected = simulation.create_stockpile(cells[0]).unwrap();
        let accepted = simulation.create_stockpile(cells[1]).unwrap();
        simulation
            .set_stockpile_item_allowed(rejected, item::WOOD, false)
            .unwrap();
        let item_id = insert_ground_stack(&mut simulation, item::WOOD, 4, cells[2]);

        assert_eq!(
            simulation
                .stockpile_destination_for_item(item_id, true)
                .unwrap()
                .map(|(stockpile_id, cell, _)| (stockpile_id, cell)),
            Some((accepted, cells[1]))
        );

        simulation
            .item_world
            .move_to_carried(item_id, cora())
            .unwrap();
        simulation
            .item_world
            .move_to_ground(
                item_id,
                cora(),
                WorldPosition::from_cell_center(cells[0]).unwrap(),
            )
            .unwrap();
        simulation.maintain_haul_jobs().unwrap();
        assert!(simulation.jobs().any(|job| {
            matches!(
                job.kind(),
                JobKind::Haul {
                    item_id: hauled,
                    stockpile_id,
                    destination,
                } if hauled == item_id && stockpile_id == accepted && destination == cells[1]
            )
        }));
    }

    #[test]
    fn stockpile_fills_partial_capacity_before_leaving_a_remainder_stack() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let bootstrap_items = simulation
            .items()
            .map(|item| (item.id(), item.quantity().get()))
            .collect::<Vec<_>>();
        for (item_id, quantity) in bootstrap_items {
            simulation.item_world.consume(item_id, quantity).unwrap();
        }
        let cells = empty_stockpile_cells(&simulation, 2);
        let stockpile_id = simulation.create_stockpile(cells[0]).unwrap();
        simulation
            .set_stockpile_cell(stockpile_id, cells[1], true)
            .unwrap();
        let target_id = simulation.id_allocator.allocate().unwrap();
        let source_id = simulation.id_allocator.allocate().unwrap();
        simulation
            .item_world
            .insert_ground(ItemStack::new_ground(
                target_id,
                item::WOOD,
                ItemQuantity::new(1020).unwrap(),
                WorldPosition::from_cell_center(cells[0]).unwrap(),
            ))
            .unwrap();
        simulation
            .item_world
            .insert_ground(ItemStack::new_ground(
                source_id,
                item::WOOD,
                ItemQuantity::new(10).unwrap(),
                WorldPosition::from_cell_center(cells[1]).unwrap(),
            ))
            .unwrap();

        for _ in 0..256 {
            simulation.advance_ticks(1).unwrap();
            if simulation
                .item_world
                .get(target_id)
                .is_some_and(|item| item.quantity().get() == MAX_STACK_QUANTITY)
            {
                break;
            }
        }

        assert_eq!(
            simulation
                .item_world
                .get(target_id)
                .unwrap()
                .quantity()
                .get(),
            MAX_STACK_QUANTITY
        );
        assert_eq!(
            simulation
                .item_world
                .get(source_id)
                .unwrap()
                .quantity()
                .get(),
            6
        );
        assert_eq!(
            simulation
                .items()
                .filter(|item| item.kind() == item::WOOD)
                .map(|item| item.quantity().get())
                .sum::<u32>(),
            1030
        );
        assert!(simulation.item_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
    }

    #[test]
    fn stockpile_compacts_same_kind_stacks_until_one_stack_holds_the_total() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cells = [
            WorldCell::new(-2, 0),
            WorldCell::new(-1, 0),
            WorldCell::new(1, 0),
            WorldCell::new(2, 0),
        ];
        let stockpile_id = simulation.create_stockpile(cells[0]).unwrap();
        for cell in cells.into_iter().skip(1) {
            simulation
                .set_stockpile_cell(stockpile_id, cell, true)
                .unwrap();
        }

        for _ in 0..512 {
            simulation.advance_ticks(1).unwrap();
            let wood = simulation
                .items()
                .filter(|item| item.kind() == item::WOOD)
                .count();
            let stone = simulation
                .items()
                .filter(|item| item.kind() == item::STONE)
                .count();
            if wood == 1 && stone == 1 {
                break;
            }
        }

        let wood = simulation
            .items()
            .filter(|item| item.kind() == item::WOOD)
            .collect::<Vec<_>>();
        let stone = simulation
            .items()
            .filter(|item| item.kind() == item::STONE)
            .collect::<Vec<_>>();
        assert_eq!(wood.len(), 1);
        assert_eq!(wood[0].quantity().get(), 18);
        assert_eq!(stone.len(), 1);
        assert_eq!(stone[0].quantity().get(), 14);
        assert!(simulation.item_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
    }

    #[test]
    fn multiple_haul_jobs_reserve_distinct_items_and_destinations() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let destinations = empty_stockpile_cells(&simulation, 2);
        let stockpile_id = simulation.create_stockpile(destinations[0]).unwrap();
        simulation
            .set_stockpile_cell(stockpile_id, destinations[1], true)
            .unwrap();

        simulation.advance_ticks(1).unwrap();

        let hauls = simulation
            .jobs()
            .filter_map(|job| match job.kind() {
                JobKind::Haul {
                    item_id,
                    destination,
                    ..
                } => Some((item_id, destination)),
                JobKind::Harvest { .. }
                | JobKind::Eat { .. }
                | JobKind::Craft { .. }
                | JobKind::SupplyProduction { .. }
                | JobKind::DeliverConstruction { .. }
                | JobKind::Construct { .. } => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(hauls.len(), 2);
        assert_ne!(hauls[0].0, hauls[1].0);
        assert_ne!(hauls[0].1, hauls[1].1);
        assert!(destinations.contains(&hauls[0].1));
        assert!(destinations.contains(&hauls[1].1));
        assert!(simulation.job_world.indexes_are_consistent());
    }
}
