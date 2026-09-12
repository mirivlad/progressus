//! The authoritative simulation's error type and its conversions.

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SimulationError {
    TickOverflow,
    EntityIdExhausted,
    DuplicateEntityId(EntityId),
    SpawnNotWalkable(WorldCell),
    NoBootstrapFoodCell,
    UnknownCharacter(EntityId),
    UnknownItem(EntityId),
    UnknownJob(EntityId),
    UnknownStockpile(EntityId),
    StockpileCellUndiscovered(WorldCell),
    StockpileCellBlocked(WorldCell),
    StockpileCellOccupiedByResource(WorldCell),
    StockpileCellOccupiedByWorkstation(WorldCell),
    StockpileCellAlreadyOwned {
        cell: WorldCell,
        stockpile_id: EntityId,
    },
    StockpileRevisionOverflow,
    StockpileInvariantViolation,
    UnknownWorkstation(EntityId),
    WorkstationCellUndiscovered(WorldCell),
    WorkstationCellBlocked(WorldCell),
    WorkstationCellOccupiedByResource(WorldCell),
    WorkstationCellOccupiedByStockpile(WorldCell),
    WorkstationCellOccupiedByItem(WorldCell),
    WorkstationCellAlreadyOccupied {
        cell: WorldCell,
        workstation_id: EntityId,
    },
    WorkstationRevisionOverflow,
    WorkstationInvariantViolation,
    WorkstationPortLayoutUnavailable(WorldCell),
    RecipeWorkstationMismatch {
        workstation_id: EntityId,
        recipe_id: RecipeId,
    },
    CraftAlreadyDesignated(EntityId),
    UnknownProductionOrder(EntityId),
    ProductionOrderQuantityTooLarge(u32),
    ProductionRevisionOverflow,
    ProductionInvariantViolation,
    WorkbenchInputPortsFixed(EntityId),
    WorkbenchOutputPortsFixed(EntityId),
    ProductionZoneCellOutOfRange {
        workstation_id: EntityId,
        workstation_cell: WorldCell,
        cell: WorldCell,
    },
    ProductionZoneCellUndiscovered(WorldCell),
    ProductionZoneCellBlocked(WorldCell),
    ProductionZoneCellOccupied(WorldCell),
    ProductionZoneCellAlreadyOwned {
        cell: WorldCell,
        workstation_id: EntityId,
        kind: ProductionZoneKind,
    },
    ProductionLogisticsRevisionOverflow,
    ProductionLogisticsInvariantViolation,
    ProductionOutputBlocked(EntityId),
    UnknownConstructionSite(EntityId),
    ConstructionCellUndiscovered(WorldCell),
    ConstructionCellBlocked(WorldCell),
    ConstructionCellOccupied(WorldCell),
    ConstructionRevisionOverflow,
    ConstructionInvariantViolation,
    HarvestSourceUndiscovered(WorldCell),
    NaturalResourceMissing(WorldCell),
    HarvestAlreadyDesignated(WorldCell),
    JobRevisionOverflow,
    ResourceRevisionOverflow,
    JobInvariantViolation,
    ItemNotOnGround(EntityId),
    ItemReserved(EntityId),
    ItemNotEquippable(EntityId),
    ItemNotAContainer(EntityId),
    EquipmentStackMustBeSingle(EntityId),
    ContainerStackMustBeSingle(EntityId),
    ContainerNestingNotAllowed {
        item_id: EntityId,
        container_id: EntityId,
    },
    SlotAlreadyOccupied {
        character_id: EntityId,
        item_id: EntityId,
    },
    CarryCapacityExceeded {
        character_id: EntityId,
        item_id: EntityId,
    },
    ItemNotCarriedByCharacter {
        character_id: EntityId,
        item_id: EntityId,
    },
    ItemOutOfReach {
        character_id: EntityId,
        item_id: EntityId,
    },
    ItemDropBlocked(WorldCell),
    MovementCoordinateOverflow(WorldCell),
    MovementDestinationBlocked(WorldCell),
    MoveToDestinationBlocked(WorldCell),
    MoveToDestinationUndiscovered(WorldCell),
    MoveToPathNotFound,
    MoveToSearchBudgetExceeded,
    Position(WorldPositionError),
    Worldgen(WorldgenError),
}

impl SimulationError {
    pub(super) fn from_job_world(error: JobWorldError) -> Self {
        match error {
            JobWorldError::UnknownJob(id) => Self::UnknownJob(id),
            JobWorldError::HarvestSourceAlreadyDesignated(source) => {
                Self::HarvestAlreadyDesignated(source)
            }
            JobWorldError::CraftWorkstationAlreadyDesignated(workstation_id) => {
                Self::CraftAlreadyDesignated(workstation_id)
            }
            JobWorldError::RevisionOverflow => Self::JobRevisionOverflow,
            JobWorldError::DuplicateJob(_)
            | JobWorldError::EatCharacterAlreadyDesignated(_)
            | JobWorldError::EatItemAlreadyReserved(_)
            | JobWorldError::HaulItemAlreadyReserved(_)
            | JobWorldError::HaulDestinationAlreadyReserved(_)
            | JobWorldError::ProductionSupplyItemAlreadyReserved(_)
            | JobWorldError::ProductionSupplyDestinationAlreadyReserved(_)
            | JobWorldError::CraftItemAlreadyReserved(_)
            | JobWorldError::CraftInputsAlreadyReserved(_)
            | JobWorldError::CraftOrderAlreadyDesignated(_)
            | JobWorldError::ConstructionDeliveryAlreadyDesignated(_)
            | JobWorldError::ConstructionAlreadyDesignated(_)
            | JobWorldError::ConstructionPreparationAlreadyDesignated(_)
            | JobWorldError::ConstructionPreparationItemAlreadyReserved(_)
            | JobWorldError::EquipItemAlreadyReserved(_)
            | JobWorldError::JobNotCraft(_)
            | JobWorldError::WorkerAlreadyReserved(_)
            | JobWorldError::JobNotAvailable(_)
            | JobWorldError::JobNotReserved(_)
            | JobWorldError::JobNotWorking(_)
            | JobWorldError::IndexCorruption => Self::JobInvariantViolation,
        }
    }

    pub(super) fn from_production_world(error: ProductionWorldError) -> Self {
        match error {
            ProductionWorldError::UnknownOrder(id) => Self::UnknownProductionOrder(id),
            ProductionWorldError::QuantityTooLarge(quantity) => {
                Self::ProductionOrderQuantityTooLarge(quantity)
            }
            ProductionWorldError::RevisionOverflow => Self::ProductionRevisionOverflow,
            ProductionWorldError::DuplicateOrder(_)
            | ProductionWorldError::OrderAlreadyComplete(_)
            | ProductionWorldError::IndexCorruption => Self::ProductionInvariantViolation,
        }
    }

    pub(super) fn from_production_logistics_world(error: ProductionLogisticsWorldError) -> Self {
        match error {
            ProductionLogisticsWorldError::UnknownWorkstation(id) => Self::UnknownWorkstation(id),
            ProductionLogisticsWorldError::CellAlreadyOwned {
                cell,
                workstation_id,
                kind,
            } => Self::ProductionZoneCellAlreadyOwned {
                cell,
                workstation_id,
                kind,
            },
            ProductionLogisticsWorldError::RevisionOverflow => {
                Self::ProductionLogisticsRevisionOverflow
            }
            ProductionLogisticsWorldError::DuplicateWorkstation(_)
            | ProductionLogisticsWorldError::IndexCorruption => {
                Self::ProductionLogisticsInvariantViolation
            }
        }
    }

    pub(super) fn from_stockpile_world(error: StockpileWorldError) -> Self {
        match error {
            StockpileWorldError::UnknownStockpile(id) => Self::UnknownStockpile(id),
            StockpileWorldError::CellAlreadyOwned { cell, stockpile_id } => {
                Self::StockpileCellAlreadyOwned { cell, stockpile_id }
            }
            StockpileWorldError::RevisionOverflow => Self::StockpileRevisionOverflow,
            StockpileWorldError::DuplicateStockpile(_) => Self::StockpileInvariantViolation,
        }
    }

    pub(super) fn from_workstation_world(error: WorkstationWorldError) -> Self {
        match error {
            WorkstationWorldError::UnknownWorkstation(id) => Self::UnknownWorkstation(id),
            WorkstationWorldError::CellAlreadyOccupied {
                cell,
                workstation_id,
            } => Self::WorkstationCellAlreadyOccupied {
                cell,
                workstation_id,
            },
            WorkstationWorldError::RevisionOverflow => Self::WorkstationRevisionOverflow,
            WorkstationWorldError::DuplicateWorkstation(_)
            | WorkstationWorldError::IndexCorruption => Self::WorkstationInvariantViolation,
        }
    }

    pub(super) fn from_construction_world(error: ConstructionWorldError) -> Self {
        match error {
            ConstructionWorldError::UnknownSite(id) => Self::UnknownConstructionSite(id),
            ConstructionWorldError::CellAlreadyOccupied(cell) => {
                Self::ConstructionCellOccupied(cell)
            }
            ConstructionWorldError::RevisionOverflow => Self::ConstructionRevisionOverflow,
            ConstructionWorldError::DuplicateConstructionId(_)
            | ConstructionWorldError::UnknownStructure(_)
            | ConstructionWorldError::NotADoor(_)
            | ConstructionWorldError::SiteAlreadyHasMaterial(_)
            | ConstructionWorldError::MaterialAlreadyReserved { .. }
            | ConstructionWorldError::MaterialReservationMismatch { .. }
            | ConstructionWorldError::IndexCorruption => Self::ConstructionInvariantViolation,
        }
    }
}

impl Display for SimulationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::TickOverflow => formatter.write_str("simulation tick overflow"),
            Self::EntityIdExhausted => formatter.write_str("stable entity ID space exhausted"),
            Self::DuplicateEntityId(id) => {
                write!(formatter, "duplicate stable entity ID {}", id.value())
            }
            Self::SpawnNotWalkable(position) => write!(
                formatter,
                "initial character position ({}, {}) is not walkable",
                position.x(),
                position.y()
            ),
            Self::NoBootstrapFoodCell => {
                formatter.write_str("no free explored grass cell is available for bootstrap food")
            }
            Self::UnknownCharacter(id) => write!(formatter, "unknown character ID {}", id.value()),
            Self::UnknownItem(id) => write!(formatter, "unknown item ID {}", id.value()),
            Self::UnknownJob(id) => write!(formatter, "unknown job ID {}", id.value()),
            Self::UnknownStockpile(id) => {
                write!(formatter, "unknown stockpile ID {}", id.value())
            }
            Self::StockpileCellUndiscovered(cell) => write!(
                formatter,
                "stockpile cell ({}, {}) is undiscovered",
                cell.x(),
                cell.y()
            ),
            Self::StockpileCellBlocked(cell) => write!(
                formatter,
                "stockpile cell ({}, {}) is not walkable",
                cell.x(),
                cell.y()
            ),
            Self::StockpileCellOccupiedByResource(cell) => write!(
                formatter,
                "stockpile cell ({}, {}) contains a natural resource",
                cell.x(),
                cell.y()
            ),
            Self::StockpileCellOccupiedByWorkstation(cell) => write!(
                formatter,
                "stockpile cell ({}, {}) contains a workstation",
                cell.x(),
                cell.y()
            ),
            Self::StockpileCellAlreadyOwned { cell, stockpile_id } => write!(
                formatter,
                "stockpile cell ({}, {}) already belongs to stockpile ID {}",
                cell.x(),
                cell.y(),
                stockpile_id.value()
            ),
            Self::StockpileRevisionOverflow => formatter.write_str("stockpile revision overflow"),
            Self::StockpileInvariantViolation => {
                formatter.write_str("stockpile ownership invariant violated")
            }
            Self::UnknownWorkstation(id) => {
                write!(formatter, "unknown workstation ID {}", id.value())
            }
            Self::WorkstationCellUndiscovered(cell) => write!(
                formatter,
                "workstation cell ({}, {}) is undiscovered",
                cell.x(),
                cell.y()
            ),
            Self::WorkstationCellBlocked(cell) => write!(
                formatter,
                "workstation cell ({}, {}) is not walkable",
                cell.x(),
                cell.y()
            ),
            Self::WorkstationCellOccupiedByResource(cell) => write!(
                formatter,
                "workstation cell ({}, {}) contains a natural resource",
                cell.x(),
                cell.y()
            ),
            Self::WorkstationCellOccupiedByStockpile(cell) => write!(
                formatter,
                "workstation cell ({}, {}) belongs to a stockpile",
                cell.x(),
                cell.y()
            ),
            Self::WorkstationCellOccupiedByItem(cell) => write!(
                formatter,
                "workstation cell ({}, {}) contains a ground item",
                cell.x(),
                cell.y()
            ),
            Self::WorkstationCellAlreadyOccupied {
                cell,
                workstation_id,
            } => write!(
                formatter,
                "workstation cell ({}, {}) already belongs to workstation ID {}",
                cell.x(),
                cell.y(),
                workstation_id.value()
            ),
            Self::WorkstationRevisionOverflow => {
                formatter.write_str("workstation revision overflow")
            }
            Self::WorkstationInvariantViolation => {
                formatter.write_str("workstation ownership invariant violated")
            }
            Self::WorkstationPortLayoutUnavailable(cell) => write!(
                formatter,
                "workstation at ({}, {}) cannot provide two input and two output ports",
                cell.x(),
                cell.y()
            ),
            Self::RecipeWorkstationMismatch {
                workstation_id,
                recipe_id,
            } => write!(
                formatter,
                "recipe {:?} cannot run at workstation ID {}",
                recipe_id,
                workstation_id.value()
            ),
            Self::CraftAlreadyDesignated(workstation_id) => write!(
                formatter,
                "workstation ID {} already has a craft designation",
                workstation_id.value()
            ),
            Self::UnknownProductionOrder(id) => {
                write!(formatter, "unknown production order ID {}", id.value())
            }
            Self::ProductionOrderQuantityTooLarge(quantity) => write!(
                formatter,
                "production order quantity {quantity} exceeds the supported limit"
            ),
            Self::ProductionRevisionOverflow => formatter.write_str("production revision overflow"),
            Self::ProductionInvariantViolation => {
                formatter.write_str("production order invariant violated")
            }
            Self::WorkbenchInputPortsFixed(workstation_id) => write!(
                formatter,
                "workbench ID {} has two fixed cardinal input ports; rotate them instead of editing input cells directly",
                workstation_id.value()
            ),
            Self::WorkbenchOutputPortsFixed(workstation_id) => write!(
                formatter,
                "workbench ID {} has two fixed diagonal output ports; rotate them instead of editing output cells directly",
                workstation_id.value()
            ),
            Self::ProductionZoneCellOutOfRange {
                workstation_id,
                workstation_cell,
                cell,
            } => write!(
                formatter,
                "production zone cell ({}, {}) must border workstation {} at ({}, {})",
                cell.x(),
                cell.y(),
                workstation_id.value(),
                workstation_cell.x(),
                workstation_cell.y()
            ),
            Self::ProductionZoneCellUndiscovered(cell) => write!(
                formatter,
                "production zone cell ({}, {}) is undiscovered",
                cell.x(),
                cell.y()
            ),
            Self::ProductionZoneCellBlocked(cell) => write!(
                formatter,
                "production zone cell ({}, {}) is not walkable",
                cell.x(),
                cell.y()
            ),
            Self::ProductionZoneCellOccupied(cell) => write!(
                formatter,
                "production zone cell ({}, {}) is occupied",
                cell.x(),
                cell.y()
            ),
            Self::ProductionZoneCellAlreadyOwned {
                cell,
                workstation_id,
                kind,
            } => write!(
                formatter,
                "production zone cell ({}, {}) already belongs to workstation ID {} as {:?}",
                cell.x(),
                cell.y(),
                workstation_id.value(),
                kind
            ),
            Self::ProductionLogisticsRevisionOverflow => {
                formatter.write_str("production logistics revision overflow")
            }
            Self::ProductionLogisticsInvariantViolation => {
                formatter.write_str("production logistics invariant violated")
            }
            Self::ProductionOutputBlocked(workstation_id) => write!(
                formatter,
                "workstation ID {} has no available production output cell",
                workstation_id.value()
            ),
            Self::UnknownConstructionSite(id) => {
                write!(formatter, "unknown construction site ID {}", id.value())
            }
            Self::ConstructionCellUndiscovered(cell) => write!(
                formatter,
                "construction cell ({}, {}) is undiscovered",
                cell.x(),
                cell.y()
            ),
            Self::ConstructionCellBlocked(cell) => write!(
                formatter,
                "construction cell ({}, {}) is not walkable",
                cell.x(),
                cell.y()
            ),
            Self::ConstructionCellOccupied(cell) => write!(
                formatter,
                "construction cell ({}, {}) is occupied",
                cell.x(),
                cell.y()
            ),
            Self::ConstructionRevisionOverflow => {
                formatter.write_str("construction revision overflow")
            }
            Self::ConstructionInvariantViolation => {
                formatter.write_str("construction material/site invariant violated")
            }
            Self::HarvestSourceUndiscovered(source) => write!(
                formatter,
                "harvest source ({}, {}) is undiscovered",
                source.x(),
                source.y()
            ),
            Self::NaturalResourceMissing(source) => write!(
                formatter,
                "no natural resource exists at ({}, {})",
                source.x(),
                source.y()
            ),
            Self::HarvestAlreadyDesignated(source) => write!(
                formatter,
                "natural resource at ({}, {}) is already designated for harvest",
                source.x(),
                source.y()
            ),
            Self::JobRevisionOverflow => formatter.write_str("job revision overflow"),
            Self::ResourceRevisionOverflow => formatter.write_str("resource revision overflow"),
            Self::JobInvariantViolation => {
                formatter.write_str("job reservation invariant violated")
            }
            Self::CarryCapacityExceeded {
                character_id,
                item_id,
            } => write!(
                formatter,
                "character {} cannot carry item {} on top of what is already in their hands",
                character_id.value(),
                item_id.value()
            ),
            Self::ItemNotAContainer(id) => {
                write!(formatter, "item {} holds nothing", id.value())
            }
            Self::EquipmentStackMustBeSingle(id) => write!(
                formatter,
                "item {} must be a single physical item to occupy an equipment slot",
                id.value()
            ),
            Self::ContainerStackMustBeSingle(id) => write!(
                formatter,
                "container item {} must be a single physical item",
                id.value()
            ),
            Self::ContainerNestingNotAllowed {
                item_id,
                container_id,
            } => write!(
                formatter,
                "item {} cannot be placed inside container {}",
                item_id.value(),
                container_id.value()
            ),
            Self::ItemNotEquippable(id) => write!(
                formatter,
                "item {} does not belong in any equipment slot",
                id.value()
            ),
            Self::SlotAlreadyOccupied {
                character_id,
                item_id,
            } => write!(
                formatter,
                "character {} already has that slot filled, so item {} cannot be equipped",
                character_id.value(),
                item_id.value()
            ),
            Self::ItemNotOnGround(id) => {
                write!(formatter, "item ID {} is not on the ground", id.value())
            }
            Self::ItemReserved(id) => {
                write!(
                    formatter,
                    "item ID {} is reserved by an active job",
                    id.value()
                )
            }
            Self::ItemNotCarriedByCharacter {
                character_id,
                item_id,
            } => write!(
                formatter,
                "item ID {} is not carried by character ID {}",
                item_id.value(),
                character_id.value()
            ),
            Self::ItemOutOfReach {
                character_id,
                item_id,
            } => write!(
                formatter,
                "item ID {} is outside interaction reach of character ID {}",
                item_id.value(),
                character_id.value()
            ),
            Self::ItemDropBlocked(position) => write!(
                formatter,
                "item drop destination ({}, {}) is not walkable",
                position.x(),
                position.y()
            ),
            Self::MovementCoordinateOverflow(position) => write!(
                formatter,
                "movement from ({}, {}) exceeds the world-cell coordinate range",
                position.x(),
                position.y()
            ),
            Self::MovementDestinationBlocked(position) => write!(
                formatter,
                "movement destination ({}, {}) is not walkable",
                position.x(),
                position.y()
            ),
            Self::MoveToDestinationBlocked(position) => write!(
                formatter,
                "move-to destination ({}, {}) is not walkable",
                position.x(),
                position.y()
            ),
            Self::MoveToDestinationUndiscovered(position) => write!(
                formatter,
                "move-to destination ({}, {}) is undiscovered",
                position.x(),
                position.y()
            ),
            Self::MoveToPathNotFound => formatter.write_str("move-to path not found"),
            Self::MoveToSearchBudgetExceeded => {
                formatter.write_str("move-to search budget exceeded")
            }
            Self::Position(_) => {
                formatter.write_str("world position is outside the representable world-cell range")
            }
            Self::Worldgen(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for SimulationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Worldgen(error) => Some(error),
            _ => None,
        }
    }
}

impl From<WorldgenError> for SimulationError {
    fn from(error: WorldgenError) -> Self {
        Self::Worldgen(error)
    }
}

impl From<WorldPositionError> for SimulationError {
    fn from(error: WorldPositionError) -> Self {
        Self::Position(error)
    }
}
