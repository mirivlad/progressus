use progressus_sim::{
    Character, ConstructionMaterialState, ConstructionSite, ItemKind, ItemStack, Job, JobKind,
    JobState, MovementState, NaturalResource, NaturalResourceKind, ProductionLogistics,
    ProductionOrder, ProductionTarget, ProductionZoneKind, RecipeId, Stockpile, Structure,
    StructureKind, Workstation, WorkstationKind, WorldPosition,
};

use crate::{ChunkCoord, EntityId, LocalCell, SimulationTick, Terrain, WorldCell, WorldgenVersion};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientSnapshot {
    pub tick: SimulationTick,
    pub worldgen_version: WorldgenVersion,
    pub exploration_revision: u64,
    pub item_revision: u64,
    pub resource_revision: u64,
    pub job_revision: u64,
    pub stockpile_revision: u64,
    pub workstation_revision: u64,
    pub production_revision: u64,
    pub production_logistics_revision: u64,
    pub construction_revision: u64,
    pub residency_revision: u64,
    pub resident_chunks: Vec<ChunkCoord>,
    pub chunks: Vec<ChunkSnapshot>,
    pub ground_items: Vec<GroundItemSnapshot>,
    pub carried_items: Vec<CarriedItemSnapshot>,
    pub natural_resources: Vec<NaturalResourceSnapshot>,
    pub jobs: Vec<JobSnapshot>,
    pub stockpiles: Vec<StockpileSnapshot>,
    pub workstations: Vec<WorkstationSnapshot>,
    pub production_orders: Vec<ProductionOrderSnapshot>,
    pub production_logistics: Vec<ProductionLogisticsSnapshot>,
    pub construction_sites: Vec<ConstructionSiteSnapshot>,
    pub structures: Vec<StructureSnapshot>,
    pub characters: Vec<CharacterSnapshot>,
    pub navigation: Option<NavigationSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroundItemSnapshot {
    pub id: EntityId,
    pub kind: ItemKind,
    pub quantity: u32,
    pub position: WorldPosition,
}

impl GroundItemSnapshot {
    pub(crate) fn from_ground_item(item: &ItemStack) -> Self {
        Self {
            id: item.id(),
            kind: item.kind(),
            quantity: item.quantity().get(),
            position: item
                .ground_position()
                .expect("ground item snapshots are built only from ground items"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CarriedItemSnapshot {
    pub id: EntityId,
    pub kind: ItemKind,
    pub quantity: u32,
    pub character_id: EntityId,
}

impl CarriedItemSnapshot {
    pub(crate) fn from_carried_item(item: &ItemStack) -> Self {
        Self {
            id: item.id(),
            kind: item.kind(),
            quantity: item.quantity().get(),
            character_id: item
                .carrier()
                .expect("carried item snapshots are built only from carried items"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NaturalResourceSnapshot {
    pub cell: WorldCell,
    pub kind: NaturalResourceKind,
    pub yield_quantity: u32,
}

impl NaturalResourceSnapshot {
    pub(crate) fn new(cell: WorldCell, resource: NaturalResource) -> Self {
        Self {
            cell,
            kind: resource.kind(),
            yield_quantity: resource.yield_quantity(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobSnapshot {
    pub id: EntityId,
    pub kind: JobKind,
    pub state: JobState,
}

impl From<&Job> for JobSnapshot {
    fn from(job: &Job) -> Self {
        Self {
            id: job.id(),
            kind: job.kind(),
            state: job.state(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockpileSnapshot {
    pub id: EntityId,
    pub cells: Vec<WorldCell>,
}

impl From<&Stockpile> for StockpileSnapshot {
    fn from(stockpile: &Stockpile) -> Self {
        Self {
            id: stockpile.id(),
            cells: stockpile.cells().collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkstationSnapshot {
    pub id: EntityId,
    pub kind: WorkstationKind,
    pub cell: WorldCell,
}

impl From<&Workstation> for WorkstationSnapshot {
    fn from(workstation: &Workstation) -> Self {
        Self {
            id: workstation.id(),
            kind: workstation.kind(),
            cell: workstation.cell(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProductionOrderSnapshot {
    pub id: EntityId,
    pub workstation_id: EntityId,
    pub recipe_id: RecipeId,
    pub target: ProductionTarget,
}

impl From<&ProductionOrder> for ProductionOrderSnapshot {
    fn from(order: &ProductionOrder) -> Self {
        Self {
            id: order.id(),
            workstation_id: order.workstation_id(),
            recipe_id: order.recipe_id(),
            target: order.target(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductionLogisticsSnapshot {
    pub workstation_id: EntityId,
    pub input_cells: Vec<WorldCell>,
    pub output_cells: Vec<WorldCell>,
}

impl From<&ProductionLogistics> for ProductionLogisticsSnapshot {
    fn from(logistics: &ProductionLogistics) -> Self {
        Self {
            workstation_id: logistics.workstation_id(),
            input_cells: logistics.cells(ProductionZoneKind::Input).collect(),
            output_cells: logistics.cells(ProductionZoneKind::Output).collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConstructionSiteSnapshot {
    pub id: EntityId,
    pub kind: StructureKind,
    pub cell: WorldCell,
    pub material_item_id: Option<EntityId>,
    pub material_state: Option<ConstructionMaterialState>,
}

impl From<&ConstructionSite> for ConstructionSiteSnapshot {
    fn from(site: &ConstructionSite) -> Self {
        Self {
            id: site.id(),
            kind: site.kind(),
            cell: site.cell(),
            material_item_id: site.material_item_id(),
            material_state: site.material_state(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StructureSnapshot {
    pub id: EntityId,
    pub kind: StructureKind,
    pub cell: WorldCell,
}

impl From<&Structure> for StructureSnapshot {
    fn from(structure: &Structure) -> Self {
        Self {
            id: structure.id(),
            kind: structure.kind(),
            cell: structure.cell(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NavigationSnapshot {
    pub character_id: EntityId,
    pub destination: Option<WorldPosition>,
    pub remaining_waypoints: Vec<WorldPosition>,
    pub last_tick_motion_trace: Vec<WorldPosition>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnownTerrain {
    Unknown,
    Known(Terrain),
}

impl From<&Character> for NavigationSnapshot {
    fn from(character: &Character) -> Self {
        Self {
            character_id: character.id(),
            destination: character.navigation_destination(),
            remaining_waypoints: character.navigation_waypoints().collect(),
            last_tick_motion_trace: character.last_tick_motion_trace().to_vec(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChunkSnapshot {
    pub coordinate: ChunkCoord,
    pub side: u16,
    /// Terrain in row-major order: `index = local_y * side + local_x`.
    pub cells: Vec<KnownTerrain>,
}

impl ChunkSnapshot {
    pub fn terrain_at(&self, local: LocalCell) -> Option<KnownTerrain> {
        if local.x() >= self.side || local.y() >= self.side {
            return None;
        }

        let index = usize::from(local.y()) * usize::from(self.side) + usize::from(local.x());
        self.cells.get(index).copied()
    }

    pub fn known_terrain_at(&self, local: LocalCell) -> Option<Terrain> {
        match self.terrain_at(local)? {
            KnownTerrain::Unknown => None,
            KnownTerrain::Known(terrain) => Some(terrain),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterSnapshot {
    pub id: EntityId,
    pub name: String,
    pub position: WorldPosition,
    pub containing_cell: WorldCell,
    pub satiety: u8,
    pub movement: MovementState,
    pub last_tick_motion_trace: Vec<WorldPosition>,
}

impl From<&Character> for CharacterSnapshot {
    fn from(character: &Character) -> Self {
        Self {
            id: character.id(),
            name: character.name().to_owned(),
            position: character.position(),
            containing_cell: character.position().containing_cell(),
            satiety: character.satiety(),
            movement: character.movement(),
            last_tick_motion_trace: character.last_tick_motion_trace().to_vec(),
        }
    }
}
