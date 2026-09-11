use std::error::Error;
use std::fmt::{self, Display, Formatter};

mod read_model;

use progressus_sim::{ItemStack, Simulation, SimulationError};

pub use progressus_sim::{
    CHUNK_SIDE, CURRENT_WORLDGEN_VERSION, ChunkCoord, ConstructionMaterialState, ConstructionSite,
    DEFAULT_CHARACTER_INTERACTION_RADIUS, DEFAULT_CHARACTER_SPEED, Direction, DoorState, EntityId,
    InteractionRadius, ItemCategory, ItemId, ItemLocation, ItemQuantity, JobKind, JobState,
    LocalCell, MAX_PRODUCTION_ORDER_RUNS, MAX_SATIETY, MovementSpeed, MovementState,
    NaturalResource, NaturalResourceId, ProductionLogistics, ProductionOrder, ProductionTarget,
    ProductionZoneKind, RESIDENT_CHUNK_RADIUS, RESIDENT_CHUNKS_PER_CENTER, RecipeId,
    SAVE_FORMAT_VERSION, SUBUNITS_PER_CELL, SaveError, SaveMetadata, SimulationTick, SlotId,
    Stockpile, Structure, StructureId, TerrainId, Workstation, WorkstationId, WorldCell,
    WorldPosition, WorldSeed, WorldgenVersion, item, natural_resource, recipe, slot, structure,
    terrain, workstation,
};
pub use read_model::{
    CarriedItemSnapshot, CharacterSnapshot, ChunkSnapshot, ClientSnapshot,
    ConstructionSiteSnapshot, GroundItemSnapshot, InventoryItemSnapshot, JobSnapshot, KnownTerrain,
    NaturalResourceSnapshot, NavigationSnapshot, ProductionLogisticsSnapshot,
    ProductionOrderSnapshot, StockpileSnapshot, StructureSnapshot, WorkstationSnapshot,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NewGameOptions {
    pub seed: WorldSeed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    AdvanceTicks {
        count: u64,
    },
    SetMovementDirection {
        character_id: EntityId,
        direction: Direction,
    },
    MoveTo {
        character_id: EntityId,
        destination: WorldPosition,
    },
    StopMovement {
        character_id: EntityId,
    },
    DesignateHarvest {
        source: WorldCell,
    },
    CancelJob {
        job_id: EntityId,
    },
    CreateStockpile {
        cell: WorldCell,
    },
    SetStockpileCell {
        stockpile_id: EntityId,
        cell: WorldCell,
        enabled: bool,
    },
    SetStockpileItemAllowed {
        stockpile_id: EntityId,
        kind: ItemId,
        allowed: bool,
    },
    PlaceWorkstation {
        kind: WorkstationId,
        cell: WorldCell,
    },
    RemoveWorkstation {
        workstation_id: EntityId,
    },
    DesignateCraft {
        workstation_id: EntityId,
        recipe_id: RecipeId,
    },
    AddProductionOrder {
        workstation_id: EntityId,
        recipe_id: RecipeId,
        target: ProductionTarget,
    },
    SetProductionOrderTarget {
        order_id: EntityId,
        target: ProductionTarget,
    },
    SetProductionZoneCell {
        workstation_id: EntityId,
        kind: ProductionZoneKind,
        cell: WorldCell,
        enabled: bool,
    },
    CycleWorkstationInputs {
        workstation_id: EntityId,
    },
    CycleWorkstationOutputs {
        workstation_id: EntityId,
    },
    RemoveProductionOrder {
        order_id: EntityId,
    },
    DesignateConstruction {
        kind: StructureId,
        cell: WorldCell,
    },
    CancelConstruction {
        site_id: EntityId,
    },
    /// Take a reachable ground stack into the hands.
    PickUpItem {
        character_id: EntityId,
        item_id: EntityId,
    },
    /// Put a borne stack down where the character stands. This is also how a
    /// loaded cart is parked: it keeps what it holds.
    DropItem {
        character_id: EntityId,
        item_id: EntityId,
    },
    EquipItem {
        character_id: EntityId,
        item_id: EntityId,
    },
    UnequipItem {
        character_id: EntityId,
        item_id: EntityId,
    },
    /// Move a reachable ground stack into a container the character bears or
    /// can reach.
    LoadContainer {
        character_id: EntityId,
        item_id: EntityId,
        container_id: EntityId,
    },
    UnloadContainer {
        character_id: EntityId,
        item_id: EntityId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotQuery {
    pub chunks: Vec<ChunkCoord>,
    pub navigation_for: Option<EntityId>,
    pub include_terrain: bool,
    pub include_ground_items: bool,
    pub include_natural_resources: bool,
}

impl Default for SnapshotQuery {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            navigation_for: None,
            include_terrain: true,
            include_ground_items: true,
            include_natural_resources: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Application {
    simulation: Simulation,
}

impl Application {
    pub fn new_game(options: NewGameOptions) -> Result<Self, ApplicationError> {
        Ok(Self {
            simulation: Simulation::new(options.seed)?,
        })
    }

    pub fn save_json(&self) -> Result<Vec<u8>, ApplicationError> {
        Ok(self.simulation.save_json()?)
    }

    pub fn from_save_json(bytes: &[u8]) -> Result<Self, ApplicationError> {
        Ok(Self {
            simulation: Simulation::load_json(bytes)?,
        })
    }

    pub fn save_metadata(bytes: &[u8]) -> Result<SaveMetadata, ApplicationError> {
        Ok(Simulation::save_metadata(bytes)?)
    }

    pub fn execute(&mut self, command: Command) -> Result<(), ApplicationError> {
        match command {
            Command::AdvanceTicks { count } => self.simulation.advance_ticks(count)?,
            Command::SetMovementDirection {
                character_id,
                direction,
            } => self
                .simulation
                .set_movement_direction(character_id, direction)?,
            Command::MoveTo {
                character_id,
                destination,
            } => self.simulation.move_to(character_id, destination)?,
            Command::StopMovement { character_id } => {
                self.simulation.stop_movement(character_id)?
            }
            Command::DesignateHarvest { source } => {
                self.simulation.designate_harvest(source)?;
            }
            Command::CancelJob { job_id } => self.simulation.cancel_job(job_id)?,
            Command::CreateStockpile { cell } => {
                self.simulation.create_stockpile(cell)?;
            }
            Command::SetStockpileCell {
                stockpile_id,
                cell,
                enabled,
            } => self
                .simulation
                .set_stockpile_cell(stockpile_id, cell, enabled)?,
            Command::SetStockpileItemAllowed {
                stockpile_id,
                kind,
                allowed,
            } => self
                .simulation
                .set_stockpile_item_allowed(stockpile_id, kind, allowed)?,
            Command::PlaceWorkstation { kind, cell } => {
                self.simulation.place_workstation(kind, cell)?;
            }
            Command::RemoveWorkstation { workstation_id } => {
                self.simulation.remove_workstation(workstation_id)?;
            }
            Command::DesignateCraft {
                workstation_id,
                recipe_id,
            } => {
                self.simulation.designate_craft(workstation_id, recipe_id)?;
            }
            Command::AddProductionOrder {
                workstation_id,
                recipe_id,
                target,
            } => {
                self.simulation
                    .add_production_order(workstation_id, recipe_id, target)?;
            }
            Command::SetProductionOrderTarget { order_id, target } => {
                self.simulation
                    .set_production_order_target(order_id, target)?;
            }
            Command::SetProductionZoneCell {
                workstation_id,
                kind,
                cell,
                enabled,
            } => self
                .simulation
                .set_production_zone_cell(workstation_id, kind, cell, enabled)?,
            Command::CycleWorkstationInputs { workstation_id } => {
                self.simulation.cycle_workstation_inputs(workstation_id)?;
            }
            Command::CycleWorkstationOutputs { workstation_id } => {
                self.simulation.cycle_workstation_outputs(workstation_id)?;
            }
            Command::RemoveProductionOrder { order_id } => {
                self.simulation.remove_production_order(order_id)?;
            }
            Command::DesignateConstruction { kind, cell } => {
                self.simulation.designate_construction(kind, cell)?;
            }
            Command::CancelConstruction { site_id } => {
                self.simulation.cancel_construction(site_id)?;
            }
            Command::PickUpItem {
                character_id,
                item_id,
            } => {
                self.simulation.pick_up_item(character_id, item_id)?;
            }
            Command::DropItem {
                character_id,
                item_id,
            } => {
                // Where the character stands: putting something down is not a
                // way to place it anywhere else.
                let position = self
                    .simulation
                    .characters()
                    .find(|character| character.id() == character_id)
                    .ok_or(SimulationError::UnknownCharacter(character_id))?
                    .position();
                self.simulation.drop_item(character_id, item_id, position)?;
            }
            Command::EquipItem {
                character_id,
                item_id,
            } => {
                self.simulation.equip_item(character_id, item_id)?;
            }
            Command::UnequipItem {
                character_id,
                item_id,
            } => {
                self.simulation.unequip_item(character_id, item_id)?;
            }
            Command::LoadContainer {
                character_id,
                item_id,
                container_id,
            } => {
                self.simulation
                    .load_into_container(character_id, item_id, container_id)?;
            }
            Command::UnloadContainer {
                character_id,
                item_id,
            } => {
                self.simulation
                    .unload_from_container(character_id, item_id)?;
            }
        }
        Ok(())
    }

    /// Returns effective terrain only when the player has already explored the cell.
    /// This is a bounded point read and must not reveal undiscovered worldgen state.
    pub fn known_terrain_at(&self, cell: WorldCell) -> Result<Option<TerrainId>, ApplicationError> {
        if !self.simulation.is_explored(cell) {
            return Ok(None);
        }
        Ok(Some(self.simulation.effective_terrain_at(cell)?))
    }

    pub fn snapshot(&self, mut query: SnapshotQuery) -> Result<ClientSnapshot, ApplicationError> {
        query.chunks.sort_unstable();
        query.chunks.dedup();
        let requested_chunks = query.chunks.clone();
        let explored_requested_chunks = requested_chunks
            .iter()
            .copied()
            .filter(|coordinate| self.simulation.has_explored_cells_in_chunk(*coordinate))
            .collect::<Vec<_>>();

        let chunks = if query.include_terrain {
            explored_requested_chunks
                .iter()
                .copied()
                .map(|coordinate| {
                    let effective = self.simulation.effective_chunk(coordinate)?;
                    let cells = (0..CHUNK_SIDE)
                        .flat_map(|y| (0..CHUNK_SIDE).map(move |x| LocalCell::new(x, y)))
                        .map(|local| {
                            let cell = coordinate
                                .world_cell(local)
                                .expect("valid chunk-local cells produce world cells");
                            if self.simulation.is_explored(cell) {
                                KnownTerrain::Known(
                                    effective
                                        .terrain_at(local)
                                        .expect("effective chunks contain every local cell"),
                                )
                            } else {
                                KnownTerrain::Unknown
                            }
                        })
                        .collect();
                    Ok(ChunkSnapshot {
                        coordinate,
                        side: CHUNK_SIDE,
                        cells,
                    })
                })
                .collect::<Result<Vec<_>, SimulationError>>()?
        } else {
            Vec::new()
        };
        let carried_items = self
            .simulation
            .items()
            .filter(|item| item.carrier().is_some())
            .map(CarriedItemSnapshot::from_carried_item)
            .collect();
        let ground_items = if query.include_ground_items {
            explored_requested_chunks
                .iter()
                .copied()
                .flat_map(|coordinate| self.simulation.ground_items_in_chunk(coordinate))
                .filter(|item| {
                    item.ground_position().is_some_and(|position| {
                        self.simulation.is_explored(position.containing_cell())
                    })
                })
                .map(GroundItemSnapshot::from_ground_item)
                .collect()
        } else {
            Vec::new()
        };
        // A stack is visible where it physically rests. Borne goods follow
        // their bearer, as carried stacks already do; goods standing on the
        // ground — including whatever a parked container holds — stay subject
        // to exploration, so nothing is revealed by being packed away.
        let inventory_items = self
            .simulation
            .items()
            .filter(|item| {
                match self
                    .simulation
                    .item_root(item.id())
                    .map(ItemStack::location)
                {
                    Some(ItemLocation::Carried { .. } | ItemLocation::Equipped { .. }) => true,
                    Some(ItemLocation::Ground { position }) => {
                        let cell = position.containing_cell();
                        query.include_ground_items
                            && self.simulation.is_explored(cell)
                            && explored_requested_chunks.contains(&cell.split().0)
                    }
                    Some(ItemLocation::Contained { .. }) | None => false,
                }
            })
            .map(|item| {
                let contained_load = item
                    .kind()
                    .capacity()
                    .map(|_| self.simulation.container_load(item.id()));
                InventoryItemSnapshot::new(
                    item,
                    self.simulation.item_is_reserved(item.id()),
                    contained_load,
                )
            })
            .collect();

        let natural_resources = if query.include_natural_resources {
            explored_requested_chunks
                .into_iter()
                .map(|coordinate| self.simulation.natural_resources_in_chunk(coordinate))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .filter(|(cell, _)| self.simulation.is_explored(*cell))
                .map(|(cell, resource)| NaturalResourceSnapshot::new(cell, resource))
                .collect()
        } else {
            Vec::new()
        };
        let characters = self
            .simulation
            .characters()
            .map(CharacterSnapshot::from)
            .collect();
        let jobs = self.simulation.jobs().map(JobSnapshot::from).collect();
        let stockpiles = self
            .simulation
            .stockpiles()
            .map(StockpileSnapshot::from)
            .collect();
        let workstations = self
            .simulation
            .workstations()
            .map(WorkstationSnapshot::from)
            .collect();
        let production_orders = self
            .simulation
            .production_orders()
            .map(ProductionOrderSnapshot::from)
            .collect();
        let production_logistics = self
            .simulation
            .production_logistics()
            .map(ProductionLogisticsSnapshot::from)
            .collect();
        let construction_sites = self
            .simulation
            .construction_sites()
            .map(ConstructionSiteSnapshot::from)
            .collect();
        let structures = self
            .simulation
            .structures()
            .map(StructureSnapshot::from)
            .collect();

        Ok(ClientSnapshot {
            tick: self.simulation.tick(),
            worldgen_version: self.simulation.worldgen_version(),
            exploration_revision: self.simulation.exploration_revision(),
            item_revision: self.simulation.item_revision(),
            resource_revision: self.simulation.resource_revision(),
            job_revision: self.simulation.job_revision(),
            stockpile_revision: self.simulation.stockpile_revision(),
            workstation_revision: self.simulation.workstation_revision(),
            production_revision: self.simulation.production_revision(),
            production_logistics_revision: self.simulation.production_logistics_revision(),
            construction_revision: self.simulation.construction_revision(),
            residency_revision: self.simulation.residency_revision(),
            resident_chunks: self.simulation.resident_chunks().collect(),
            chunks,
            ground_items,
            carried_items,
            inventory_items,
            natural_resources,
            jobs,
            stockpiles,
            workstations,
            production_orders,
            production_logistics,
            construction_sites,
            structures,
            characters,
            navigation: query.navigation_for.and_then(|id| {
                self.simulation
                    .characters()
                    .find(|character| character.id() == id)
                    .map(NavigationSnapshot::from)
            }),
        })
    }
}

#[cfg(test)]
impl Application {
    fn from_simulation_for_test(simulation: Simulation) -> Self {
        Self { simulation }
    }
}

#[derive(Debug)]
pub enum ApplicationError {
    Simulation(SimulationError),
    Save(SaveError),
}

impl Display for ApplicationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Simulation(error) => Display::fmt(error, formatter),
            Self::Save(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for ApplicationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Simulation(error) => Some(error),
            Self::Save(error) => Some(error),
        }
    }
}

impl From<SimulationError> for ApplicationError {
    fn from(error: SimulationError) -> Self {
        Self::Simulation(error)
    }
}

impl From<SaveError> for ApplicationError {
    fn from(error: SaveError) -> Self {
        Self::Save(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cart needs a tool to build, so it is the first thing in the game that
    /// cannot be made in one step. Ordering it has to work through the whole
    /// chain — craft the tool, supply eight wood and that tool, build the cart —
    /// or the recipe exists only on paper.
    #[test]
    fn a_cart_can_be_ordered_and_actually_built() {
        let mut application = Application::new_game(NewGameOptions {
            seed: WorldSeed::new(0),
        })
        .unwrap();
        application
            .execute(Command::CreateStockpile {
                cell: WorldCell::new(-2, 1),
            })
            .unwrap();
        let stockpile_id = application
            .snapshot(SnapshotQuery::default())
            .unwrap()
            .stockpiles[0]
            .id;
        for x in -1..=3 {
            application
                .execute(Command::SetStockpileCell {
                    stockpile_id,
                    cell: WorldCell::new(x, 1),
                    enabled: true,
                })
                .unwrap();
        }
        application
            .execute(Command::AdvanceTicks { count: 400 })
            .unwrap();
        let workbench_cell = (2..=5)
            .flat_map(|y| (-4..=4).map(move |x| WorldCell::new(x, y)))
            .find(|cell| {
                application
                    .execute(Command::PlaceWorkstation {
                        kind: workstation::WORKBENCH,
                        cell: *cell,
                    })
                    .is_ok()
            });
        assert!(workbench_cell.is_some(), "nowhere to stand a workbench");
        let workstation_id = application
            .snapshot(SnapshotQuery::default())
            .unwrap()
            .workstations[0]
            .id;

        for recipe_id in [recipe::PRIMITIVE_TOOL, recipe::CART] {
            application
                .execute(Command::AddProductionOrder {
                    workstation_id,
                    recipe_id,
                    target: ProductionTarget::Finite { remaining_runs: 1 },
                })
                .unwrap();
            application
                .execute(Command::AdvanceTicks { count: 1500 })
                .unwrap();
        }

        let built = application
            .snapshot(SnapshotQuery {
                chunks: nearby_chunks(),
                ..SnapshotQuery::default()
            })
            .unwrap();
        assert!(
            built
                .inventory_items
                .iter()
                .any(|row| row.kind == item::CART),
            "the settlement never finished a cart"
        );
    }

    fn nearby_chunks() -> Vec<ChunkCoord> {
        (-2..=2)
            .flat_map(|x| (-2..=2).map(move |y| ChunkCoord::new(x, y)))
            .collect()
    }

    /// A stack standing on the ground is visible where it lies, so it obeys
    /// exploration and the requested chunks. Once someone picks it up it
    /// travels with them, and asking about no chunk at all still shows it —
    /// otherwise a character's own hands would empty whenever the camera
    /// looked elsewhere.
    #[test]
    fn inventory_rows_follow_borne_goods_and_leave_ground_stacks_to_exploration() {
        let mut application = Application::new_game(NewGameOptions {
            seed: WorldSeed::new(0),
        })
        .unwrap();

        let unasked = application.snapshot(SnapshotQuery::default()).unwrap();
        assert!(
            unasked.inventory_items.is_empty(),
            "ground stacks appeared without their chunk being asked for"
        );

        let asked = application
            .snapshot(SnapshotQuery {
                chunks: nearby_chunks(),
                ..SnapshotQuery::default()
            })
            .unwrap();
        let wood = asked
            .inventory_items
            .iter()
            .find(|row| row.kind == item::WOOD)
            .expect("the starting world puts wood on the ground");
        assert_eq!(wood.load, item::WOOD.load_cost(wood.quantity));
        assert_eq!(wood.capacity, None);
        assert_eq!(wood.contained_load, None);
        assert!(!wood.reserved);

        let item_id = wood.id;
        let character_id = asked
            .characters
            .iter()
            .map(|character| character.id)
            .find(|character_id| {
                application
                    .execute(Command::PickUpItem {
                        character_id: *character_id,
                        item_id,
                    })
                    .is_ok()
            })
            .expect("someone stands within reach of a starting stack");

        let borne = application.snapshot(SnapshotQuery::default()).unwrap();
        let row = borne
            .inventory_items
            .iter()
            .find(|row| row.id == item_id)
            .expect("a stack in someone's hands is theirs to see");
        assert_eq!(row.location, ItemLocation::Carried { character_id });

        application
            .execute(Command::DropItem {
                character_id,
                item_id,
            })
            .unwrap();
        let put_down = application.snapshot(SnapshotQuery::default()).unwrap();
        assert!(
            !put_down.inventory_items.iter().any(|row| row.id == item_id),
            "a stack put back on the ground stayed visible through unasked chunks"
        );
    }

    /// Each command has to reach its own operation. Sharing a shape with its
    /// neighbours is exactly what makes a mis-wired arm survive review, so
    /// this drives a real tool through every state it can occupy.
    #[test]
    fn item_commands_each_reach_their_own_operation() {
        let mut application = Application::new_game(NewGameOptions {
            seed: WorldSeed::new(0),
        })
        .unwrap();
        application
            .execute(Command::CreateStockpile {
                cell: WorldCell::new(-2, 1),
            })
            .unwrap();
        let stockpile_id = application
            .snapshot(SnapshotQuery::default())
            .unwrap()
            .stockpiles[0]
            .id;
        for x in -1..=2 {
            application
                .execute(Command::SetStockpileCell {
                    stockpile_id,
                    cell: WorldCell::new(x, 1),
                    enabled: true,
                })
                .unwrap();
        }
        application
            .execute(Command::AdvanceTicks { count: 400 })
            .unwrap();
        let workbench_cell = (2..=5)
            .flat_map(|y| (-4..=4).map(move |x| WorldCell::new(x, y)))
            .find(|cell| {
                application
                    .execute(Command::PlaceWorkstation {
                        kind: workstation::WORKBENCH,
                        cell: *cell,
                    })
                    .is_ok()
            });
        assert!(workbench_cell.is_some(), "nowhere to stand a workbench");
        let workstation_id = application
            .snapshot(SnapshotQuery::default())
            .unwrap()
            .workstations[0]
            .id;
        application
            .execute(Command::AddProductionOrder {
                workstation_id,
                recipe_id: recipe::PRIMITIVE_TOOL,
                target: ProductionTarget::Finite { remaining_runs: 1 },
            })
            .unwrap();

        let mut tool = None;
        for _ in 0..40 {
            application
                .execute(Command::AdvanceTicks { count: 100 })
                .unwrap();
            let snapshot = application
                .snapshot(SnapshotQuery {
                    chunks: nearby_chunks(),
                    ..SnapshotQuery::default()
                })
                .unwrap();
            tool = snapshot
                .inventory_items
                .iter()
                .find(|row| row.kind == item::PRIMITIVE_TOOL && !row.reserved)
                .map(|row| row.id);
            if tool.is_some() {
                break;
            }
        }
        let item_id = tool.expect("the workbench never produced a free tool");

        let snapshot = application
            .snapshot(SnapshotQuery {
                chunks: nearby_chunks(),
                ..SnapshotQuery::default()
            })
            .unwrap();
        let character_id = snapshot
            .characters
            .iter()
            .map(|character| character.id)
            .find(|character_id| {
                application
                    .execute(Command::PickUpItem {
                        character_id: *character_id,
                        item_id,
                    })
                    .is_ok()
            })
            .expect("nobody could reach the finished tool");
        let location = |application: &Application| {
            application
                .snapshot(SnapshotQuery::default())
                .unwrap()
                .inventory_items
                .iter()
                .find(|row| row.id == item_id)
                .map(|row| row.location)
        };
        assert_eq!(
            location(&application),
            Some(ItemLocation::Carried { character_id })
        );

        application
            .execute(Command::EquipItem {
                character_id,
                item_id,
            })
            .unwrap();
        assert_eq!(
            location(&application),
            Some(ItemLocation::Equipped {
                character_id,
                slot: slot::TOOL,
            })
        );

        // Unloading is not unequipping: nothing contains this tool.
        assert!(
            application
                .execute(Command::UnloadContainer {
                    character_id,
                    item_id,
                })
                .is_err()
        );
        assert_eq!(
            location(&application),
            Some(ItemLocation::Equipped {
                character_id,
                slot: slot::TOOL,
            })
        );

        application
            .execute(Command::UnequipItem {
                character_id,
                item_id,
            })
            .unwrap();
        assert_eq!(
            location(&application),
            Some(ItemLocation::Carried { character_id })
        );

        // A tool is not a container, so nothing can be loaded into it.
        assert!(
            application
                .execute(Command::LoadContainer {
                    character_id,
                    item_id,
                    container_id: item_id,
                })
                .is_err()
        );

        application
            .execute(Command::DropItem {
                character_id,
                item_id,
            })
            .unwrap();
        let ground = application
            .snapshot(SnapshotQuery {
                chunks: nearby_chunks(),
                ..SnapshotQuery::default()
            })
            .unwrap();
        assert!(
            ground
                .inventory_items
                .iter()
                .any(|row| row.id == item_id
                    && matches!(row.location, ItemLocation::Ground { .. })),
            "the tool never reached the ground"
        );
    }

    #[test]
    fn snapshot_returns_effective_terrain_without_mutating_raw_worldgen() {
        let mut simulation = Simulation::new(WorldSeed::new(42)).unwrap();
        let position = WorldCell::new(0, 0);
        let (coordinate, local) = position.split();

        assert_eq!(
            simulation
                .generated_chunk(coordinate)
                .unwrap()
                .terrain_at(local),
            Some(terrain::GRASS),
        );
        simulation
            .set_terrain_override(position, terrain::ROCK)
            .unwrap();
        assert_eq!(
            simulation
                .generated_chunk(coordinate)
                .unwrap()
                .terrain_at(local),
            Some(terrain::GRASS),
        );

        let application = Application::from_simulation_for_test(simulation);
        let snapshot = application
            .snapshot(SnapshotQuery {
                chunks: vec![coordinate],
                ..SnapshotQuery::default()
            })
            .unwrap();

        assert_eq!(
            snapshot.chunks[0].known_terrain_at(local),
            Some(terrain::ROCK)
        );
    }

    #[test]
    fn chunk_query_publishes_only_explored_ground_items_in_deterministic_order() {
        let application = Application::new_game(NewGameOptions {
            seed: WorldSeed::new(42),
        })
        .unwrap();

        let lightweight = application.snapshot(SnapshotQuery::default()).unwrap();
        assert!(lightweight.ground_items.is_empty());

        let snapshot = application
            .snapshot(SnapshotQuery {
                chunks: vec![
                    ChunkCoord::new(0, 0),
                    ChunkCoord::new(-1, 0),
                    ChunkCoord::new(0, 0),
                ],
                ..SnapshotQuery::default()
            })
            .unwrap();

        assert_eq!(
            snapshot
                .ground_items
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            (6..=9)
                .map(|value| EntityId::new(value).unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(snapshot.ground_items[0].kind, item::WOOD);
        assert_eq!(snapshot.ground_items[0].quantity, 8);
        assert_eq!(
            snapshot.ground_items[0].position,
            WorldPosition::from_cell_origin(WorldCell::new(-2, 0))
                .unwrap()
                .checked_translate(160, 180)
                .unwrap()
        );
    }

    #[test]
    fn item_revision_and_ground_snapshot_follow_authoritative_transfer() {
        let mut application = Application::new_game(NewGameOptions {
            seed: WorldSeed::new(42),
        })
        .unwrap();
        let query = SnapshotQuery {
            chunks: vec![ChunkCoord::new(-1, 0)],
            ..SnapshotQuery::default()
        };
        let before = application.snapshot(query.clone()).unwrap();
        assert!(
            before
                .ground_items
                .iter()
                .any(|item| item.id == EntityId::new(6).unwrap())
        );

        application
            .simulation
            .pick_up_item(EntityId::new(1).unwrap(), EntityId::new(6).unwrap())
            .unwrap();
        let carried = application.snapshot(query.clone()).unwrap();
        assert_eq!(carried.item_revision, before.item_revision + 1);
        assert!(
            !carried
                .ground_items
                .iter()
                .any(|item| item.id == EntityId::new(6).unwrap())
        );

        let destination = WorldPosition::from_cell_center(WorldCell::new(-2, 0)).unwrap();
        application
            .simulation
            .drop_item(
                EntityId::new(1).unwrap(),
                EntityId::new(6).unwrap(),
                destination,
            )
            .unwrap();
        let dropped = application.snapshot(query).unwrap();
        assert_eq!(dropped.item_revision, before.item_revision + 2);
        assert_eq!(
            dropped
                .ground_items
                .iter()
                .find(|item| item.id == EntityId::new(6).unwrap())
                .unwrap()
                .position,
            destination
        );
    }

    #[test]
    fn snapshot_publishes_newly_explored_terrain_only_after_authoritative_movement() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        for x in 0..=3 {
            simulation
                .set_terrain_override(WorldCell::new(x, 0), terrain::GRASS)
                .unwrap();
        }
        let mut application = Application::from_simulation_for_test(simulation);
        let coordinate = ChunkCoord::new(0, 0);
        let newly_visible = LocalCell::new(8, 0);

        assert_eq!(
            application
                .snapshot(SnapshotQuery {
                    chunks: vec![coordinate],
                    ..SnapshotQuery::default()
                })
                .unwrap()
                .chunks[0]
                .terrain_at(newly_visible),
            Some(KnownTerrain::Unknown)
        );

        application
            .execute(Command::SetMovementDirection {
                character_id: EntityId::new(3).unwrap(),
                direction: Direction::East,
            })
            .unwrap();
        application
            .execute(Command::AdvanceTicks { count: 12 })
            .unwrap();

        assert!(
            application
                .snapshot(SnapshotQuery {
                    chunks: vec![coordinate],
                    ..SnapshotQuery::default()
                })
                .unwrap()
                .chunks[0]
                .known_terrain_at(newly_visible)
                .is_some()
        );
    }
}
