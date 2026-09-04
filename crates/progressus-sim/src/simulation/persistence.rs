use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};

use super::*;
use crate::construction::ConstructionWorld;
use crate::entity::{CharacterRestoreState, EntityIdAllocator, NavigationRoute};
use crate::exploration::ExploredWorld;
use crate::item::ItemWorld;
use crate::job::JobWorld;
use crate::production::ProductionWorld;
use crate::production_logistics::ProductionLogisticsWorld;
use crate::residency::ChunkResidency;
use crate::stockpile::StockpileWorld;
use crate::workstation::WorkstationWorld;
use crate::world_state::ModifiedWorld;
use crate::{MAX_SATIETY, MovementSpeed};

pub const SAVE_FORMAT_VERSION: u32 = 1;
const SAVE_FORMAT_NAME: &str = "progressus-save";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SaveMetadata {
    pub format_version: u32,
    pub world_seed: WorldSeed,
    pub worldgen_version: WorldgenVersion,
    pub tick: SimulationTick,
}

#[derive(Debug)]
pub enum SaveError {
    Encode(serde_json::Error),
    Decode(serde_json::Error),
    UnsupportedFormat { name: String, version: u32 },
    UnsupportedWorldgen(WorldgenError),
    InvalidData(String),
}

impl Display for SaveError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode(error) => write!(formatter, "failed to encode save: {error}"),
            Self::Decode(error) => write!(formatter, "failed to decode save: {error}"),
            Self::UnsupportedFormat { name, version } => write!(
                formatter,
                "unsupported save format {name:?} version {version}; expected {SAVE_FORMAT_NAME:?} version {SAVE_FORMAT_VERSION}"
            ),
            Self::UnsupportedWorldgen(error) => {
                write!(formatter, "save uses unsupported worldgen: {error}")
            }
            Self::InvalidData(message) => write!(formatter, "invalid save data: {message}"),
        }
    }
}

impl Error for SaveError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Encode(error) | Self::Decode(error) => Some(error),
            Self::UnsupportedWorldgen(error) => Some(error),
            Self::UnsupportedFormat { .. } | Self::InvalidData(_) => None,
        }
    }
}

impl Simulation {
    pub fn save_json(&self) -> Result<Vec<u8>, SaveError> {
        serde_json::to_vec_pretty(&SaveV1::from_simulation(self)).map_err(SaveError::Encode)
    }

    pub fn load_json(bytes: &[u8]) -> Result<Self, SaveError> {
        let save: SaveV1 = serde_json::from_slice(bytes).map_err(SaveError::Decode)?;
        save.into_simulation()
    }

    pub fn save_metadata(bytes: &[u8]) -> Result<SaveMetadata, SaveError> {
        let header: SaveHeader = serde_json::from_slice(bytes).map_err(SaveError::Decode)?;
        validate_header(&header)?;
        Ok(SaveMetadata {
            format_version: header.version,
            world_seed: WorldSeed::new(header.world_seed),
            worldgen_version: WorldgenVersion::new(header.worldgen_version),
            tick: SimulationTick::new(header.tick),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SaveHeader {
    format: String,
    version: u32,
    world_seed: u64,
    worldgen_version: u32,
    tick: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SaveV1 {
    #[serde(flatten)]
    header: SaveHeader,
    next_entity_id: Option<u64>,
    characters: Vec<CharacterSave>,
    terrain_overrides: Vec<TerrainOverrideSave>,
    explored_cells: Vec<CellSave>,
    depleted_resources: Vec<CellSave>,
    items: Vec<ItemSave>,
    stockpiles: Vec<StockpileSave>,
    workstations: Vec<WorkstationSave>,
    production_orders: Vec<ProductionOrderSave>,
    production_logistics: Vec<ProductionLogisticsSave>,
    construction_sites: Vec<ConstructionSiteSave>,
    structures: Vec<StructureSave>,
    jobs: Vec<JobSave>,
}

impl SaveV1 {
    fn from_simulation(simulation: &Simulation) -> Self {
        Self {
            header: SaveHeader {
                format: SAVE_FORMAT_NAME.to_owned(),
                version: SAVE_FORMAT_VERSION,
                world_seed: simulation.generator.seed().value(),
                worldgen_version: simulation.generator.version().value(),
                tick: simulation.clock.tick().value(),
            },
            next_entity_id: simulation.id_allocator.peek().map(EntityId::value),
            characters: simulation
                .characters
                .values()
                .map(CharacterSave::from_character)
                .collect(),
            terrain_overrides: simulation
                .modified_world
                .overrides()
                .map(|(chunk, local, terrain)| TerrainOverrideSave {
                    chunk: ChunkSave::from(chunk),
                    local: LocalSave::from(local),
                    terrain: TerrainSave::from(terrain),
                })
                .collect(),
            explored_cells: simulation
                .explored_world
                .cells()
                .map(CellSave::from)
                .collect(),
            depleted_resources: simulation
                .depleted_resources
                .iter()
                .copied()
                .map(CellSave::from)
                .collect(),
            items: simulation
                .item_world
                .iter()
                .map(ItemSave::from_item)
                .collect(),
            stockpiles: simulation
                .stockpile_world
                .iter()
                .map(StockpileSave::from_stockpile)
                .collect(),
            workstations: simulation
                .workstation_world
                .iter()
                .map(WorkstationSave::from_workstation)
                .collect(),
            production_orders: simulation
                .production_world
                .iter()
                .map(ProductionOrderSave::from_order)
                .collect(),
            production_logistics: simulation
                .production_logistics_world
                .iter()
                .map(ProductionLogisticsSave::from_logistics)
                .collect(),
            construction_sites: simulation
                .construction_world
                .sites()
                .map(ConstructionSiteSave::from_site)
                .collect(),
            structures: simulation
                .construction_world
                .structures()
                .map(StructureSave::from_structure)
                .collect(),
            jobs: simulation
                .job_world
                .iter()
                .map(|job| JobSave::from_job(job, &simulation.job_world))
                .collect(),
        }
    }

    fn into_simulation(self) -> Result<Simulation, SaveError> {
        validate_header(&self.header)?;
        validate_collection_uniqueness(&self)?;
        let seed = WorldSeed::new(self.header.world_seed);
        let version = WorldgenVersion::new(self.header.worldgen_version);
        let generator =
            WorldGenerator::new(seed, version).map_err(SaveError::UnsupportedWorldgen)?;

        let characters = restore_characters(self.characters)?;
        if characters.is_empty() {
            return invalid("save contains no characters");
        }
        let modified_world = restore_modified_world(&generator, self.terrain_overrides)?;
        let explored_world = restore_exploration(self.explored_cells)?;
        let depleted_resources = restore_depleted_resources(&generator, self.depleted_resources)?;
        let item_world = restore_items(&characters, self.items)?;
        let stockpile_world = restore_stockpiles(self.stockpiles)?;
        let workstation_world = restore_workstations(self.workstations)?;
        let production_world =
            restore_production_orders(&workstation_world, self.production_orders)?;
        let production_logistics_world = restore_production_logistics(
            &workstation_world,
            &stockpile_world,
            self.production_logistics,
        )?;
        let construction_world = restore_construction(self.construction_sites, self.structures)?;
        let job_world = restore_jobs(
            &characters,
            &item_world,
            &stockpile_world,
            &workstation_world,
            &production_world,
            &production_logistics_world,
            &construction_world,
            self.jobs,
        )?;

        let max_id = max_owned_entity_id(
            &characters,
            &item_world,
            &job_world,
            &stockpile_world,
            &workstation_world,
            &production_world,
            &construction_world,
        );
        validate_next_entity_id(self.next_entity_id, max_id)?;
        let last_discovery_cells = characters
            .iter()
            .map(|(id, character)| (*id, character.position().containing_cell()))
            .collect();
        let mut chunk_residency = ChunkResidency::default();
        chunk_residency
            .reconcile(
                generator,
                characters
                    .values()
                    .map(|character| character.position().containing_cell().split().0),
            )
            .map_err(SaveError::UnsupportedWorldgen)?;

        let simulation = Simulation {
            generator,
            clock: SimulationClock::new(self.header.tick),
            id_allocator: EntityIdAllocator::restore_next(self.next_entity_id),
            characters,
            modified_world,
            item_world,
            job_world,
            production_world,
            production_logistics_world,
            stockpile_world,
            workstation_world,
            construction_world,
            chunk_residency,
            depleted_resources,
            resource_revision: 0,
            explored_world,
            last_discovery_cells,
            #[cfg(test)]
            base_terrain_query_count: Cell::new(0),
        };
        validate_restored_simulation(&simulation)?;
        Ok(simulation)
    }
}

fn validate_header(header: &SaveHeader) -> Result<(), SaveError> {
    if header.format != SAVE_FORMAT_NAME || header.version != SAVE_FORMAT_VERSION {
        return Err(SaveError::UnsupportedFormat {
            name: header.format.clone(),
            version: header.version,
        });
    }
    WorldGenerator::new(
        WorldSeed::new(header.world_seed),
        WorldgenVersion::new(header.worldgen_version),
    )
    .map_err(SaveError::UnsupportedWorldgen)?;
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> Result<T, SaveError> {
    Err(SaveError::InvalidData(message.into()))
}

fn entity_id(value: u64, field: &str) -> Result<EntityId, SaveError> {
    EntityId::new(value).ok_or_else(|| SaveError::InvalidData(format!("{field} cannot be zero")))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct CellSave {
    x: i64,
    y: i64,
}

impl From<WorldCell> for CellSave {
    fn from(cell: WorldCell) -> Self {
        Self {
            x: cell.x(),
            y: cell.y(),
        }
    }
}

impl CellSave {
    const fn into_cell(self) -> WorldCell {
        WorldCell::new(self.x, self.y)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ChunkSave {
    x: i64,
    y: i64,
}

impl From<ChunkCoord> for ChunkSave {
    fn from(chunk: ChunkCoord) -> Self {
        Self {
            x: chunk.x(),
            y: chunk.y(),
        }
    }
}

impl ChunkSave {
    const fn into_chunk(self) -> ChunkCoord {
        ChunkCoord::new(self.x, self.y)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct LocalSave {
    x: u16,
    y: u16,
}

impl From<LocalCell> for LocalSave {
    fn from(local: LocalCell) -> Self {
        Self {
            x: local.x(),
            y: local.y(),
        }
    }
}

impl LocalSave {
    fn into_local(self) -> Result<LocalCell, SaveError> {
        if self.x >= CHUNK_SIDE || self.y >= CHUNK_SIDE {
            return invalid(format!(
                "local cell ({}, {}) is outside a {}x{} chunk",
                self.x, self.y, CHUNK_SIDE, CHUNK_SIDE
            ));
        }
        Ok(LocalCell::new(self.x, self.y))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct PositionSave {
    x_subunits: String,
    y_subunits: String,
}

impl From<WorldPosition> for PositionSave {
    fn from(position: WorldPosition) -> Self {
        Self {
            x_subunits: position.x_subunits().to_string(),
            y_subunits: position.y_subunits().to_string(),
        }
    }
}

impl PositionSave {
    fn into_position(self) -> Result<WorldPosition, SaveError> {
        let x = self
            .x_subunits
            .parse::<i128>()
            .map_err(|_| SaveError::InvalidData("invalid x_subunits i128 string".to_owned()))?;
        let y = self
            .y_subunits
            .parse::<i128>()
            .map_err(|_| SaveError::InvalidData("invalid y_subunits i128 string".to_owned()))?;
        WorldPosition::from_subunits(x, y).map_err(|_| {
            SaveError::InvalidData("world position is outside WorldCell range".to_owned())
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DirectionSave {
    East,
    West,
    North,
    South,
}

impl From<Direction> for DirectionSave {
    fn from(direction: Direction) -> Self {
        match direction {
            Direction::East => Self::East,
            Direction::West => Self::West,
            Direction::North => Self::North,
            Direction::South => Self::South,
        }
    }
}

impl From<DirectionSave> for Direction {
    fn from(direction: DirectionSave) -> Self {
        match direction {
            DirectionSave::East => Self::East,
            DirectionSave::West => Self::West,
            DirectionSave::North => Self::North,
            DirectionSave::South => Self::South,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum MovementSave {
    Idle,
    ManualDirectional { direction: DirectionSave },
    Navigating { destination: PositionSave },
}

impl From<MovementState> for MovementSave {
    fn from(movement: MovementState) -> Self {
        match movement {
            MovementState::Idle => Self::Idle,
            MovementState::ManualDirectional { direction } => Self::ManualDirectional {
                direction: direction.into(),
            },
            MovementState::Navigating { destination } => Self::Navigating {
                destination: destination.into(),
            },
        }
    }
}

impl MovementSave {
    fn into_movement(self) -> Result<MovementState, SaveError> {
        Ok(match self {
            Self::Idle => MovementState::Idle,
            Self::ManualDirectional { direction } => MovementState::ManualDirectional {
                direction: direction.into(),
            },
            Self::Navigating { destination } => MovementState::Navigating {
                destination: destination.into_position()?,
            },
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct NavigationSave {
    destination: PositionSave,
    waypoints: Vec<PositionSave>,
}

impl NavigationSave {
    fn from_route(route: &NavigationRoute) -> Self {
        Self {
            destination: route.destination.into(),
            waypoints: route.waypoints.iter().copied().map(Into::into).collect(),
        }
    }

    fn into_route(self) -> Result<NavigationRoute, SaveError> {
        Ok(NavigationRoute {
            destination: self.destination.into_position()?,
            waypoints: self
                .waypoints
                .into_iter()
                .map(PositionSave::into_position)
                .collect::<Result<VecDeque<_>, _>>()?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct CharacterSave {
    id: u64,
    name: String,
    position: PositionSave,
    speed_subunits_per_tick: u32,
    interaction_radius_subunits: u32,
    #[serde(default = "default_satiety")]
    satiety: u8,
    movement: MovementSave,
    navigation: Option<NavigationSave>,
}

const fn default_satiety() -> u8 {
    MAX_SATIETY
}

impl CharacterSave {
    fn from_character(character: &Character) -> Self {
        Self {
            id: character.id().value(),
            name: character.name().to_owned(),
            position: character.position().into(),
            speed_subunits_per_tick: character.speed().subunits_per_tick(),
            interaction_radius_subunits: character.interaction_radius().subunits(),
            satiety: character.satiety(),
            movement: character.movement().into(),
            navigation: character.navigation_route().map(NavigationSave::from_route),
        }
    }

    fn into_character(self) -> Result<Character, SaveError> {
        let id = entity_id(self.id, "character id")?;
        let position = self.position.into_position()?;
        let speed = MovementSpeed::new(self.speed_subunits_per_tick).ok_or_else(|| {
            SaveError::InvalidData(format!("character {} has zero movement speed", id.value()))
        })?;
        if self.satiety > MAX_SATIETY {
            return invalid(format!(
                "character {} has satiety {} above maximum {}",
                id.value(),
                self.satiety,
                MAX_SATIETY
            ));
        }
        let movement = self.movement.into_movement()?;
        let route = self
            .navigation
            .map(NavigationSave::into_route)
            .transpose()?;
        match (movement, route.as_ref()) {
            (MovementState::Navigating { destination }, Some(route))
                if route.destination == destination && !route.waypoints.is_empty() => {}
            (MovementState::Navigating { .. }, _) => {
                return invalid(format!(
                    "navigating character {} has no matching non-empty route",
                    id.value()
                ));
            }
            (_, Some(_)) => {
                return invalid(format!(
                    "non-navigating character {} unexpectedly has a route",
                    id.value()
                ));
            }
            _ => {}
        }
        Ok(Character::restore(
            id,
            self.name,
            position,
            CharacterRestoreState {
                speed,
                interaction_radius: InteractionRadius::new(self.interaction_radius_subunits),
                satiety: self.satiety,
                movement,
                route,
            },
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TerrainSave {
    Grass,
    Water,
    Rock,
}

impl From<Terrain> for TerrainSave {
    fn from(terrain: Terrain) -> Self {
        match terrain {
            Terrain::Grass => Self::Grass,
            Terrain::Water => Self::Water,
            Terrain::Rock => Self::Rock,
        }
    }
}

impl From<TerrainSave> for Terrain {
    fn from(terrain: TerrainSave) -> Self {
        match terrain {
            TerrainSave::Grass => Self::Grass,
            TerrainSave::Water => Self::Water,
            TerrainSave::Rock => Self::Rock,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct TerrainOverrideSave {
    chunk: ChunkSave,
    local: LocalSave,
    terrain: TerrainSave,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ItemKindSave {
    Wood,
    Stone,
    PrimitiveTool,
    Berries,
}

impl From<ItemKind> for ItemKindSave {
    fn from(kind: ItemKind) -> Self {
        match kind {
            ItemKind::Wood => Self::Wood,
            ItemKind::Stone => Self::Stone,
            ItemKind::PrimitiveTool => Self::PrimitiveTool,
            ItemKind::Berries => Self::Berries,
        }
    }
}

impl From<ItemKindSave> for ItemKind {
    fn from(kind: ItemKindSave) -> Self {
        match kind {
            ItemKindSave::Wood => Self::Wood,
            ItemKindSave::Stone => Self::Stone,
            ItemKindSave::PrimitiveTool => Self::PrimitiveTool,
            ItemKindSave::Berries => Self::Berries,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ItemLocationSave {
    Ground { position: PositionSave },
    Carried { character_id: u64 },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ItemSave {
    id: u64,
    kind: ItemKindSave,
    quantity: u32,
    location: ItemLocationSave,
}

impl ItemSave {
    fn from_item(item: &ItemStack) -> Self {
        let location = match item.location() {
            ItemLocation::Ground { position } => ItemLocationSave::Ground {
                position: position.into(),
            },
            ItemLocation::Carried { character_id } => ItemLocationSave::Carried {
                character_id: character_id.value(),
            },
        };
        Self {
            id: item.id().value(),
            kind: item.kind().into(),
            quantity: item.quantity().get(),
            location,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct StockpileSave {
    id: u64,
    cells: Vec<CellSave>,
}

impl StockpileSave {
    fn from_stockpile(stockpile: &Stockpile) -> Self {
        Self {
            id: stockpile.id().value(),
            cells: stockpile.cells().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WorkstationKindSave {
    Workbench,
}

impl From<WorkstationKind> for WorkstationKindSave {
    fn from(kind: WorkstationKind) -> Self {
        match kind {
            WorkstationKind::Workbench => Self::Workbench,
        }
    }
}

impl From<WorkstationKindSave> for WorkstationKind {
    fn from(kind: WorkstationKindSave) -> Self {
        match kind {
            WorkstationKindSave::Workbench => Self::Workbench,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct WorkstationSave {
    id: u64,
    kind: WorkstationKindSave,
    cell: CellSave,
}

impl WorkstationSave {
    fn from_workstation(workstation: &Workstation) -> Self {
        Self {
            id: workstation.id().value(),
            kind: workstation.kind().into(),
            cell: workstation.cell().into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RecipeIdSave {
    PrimitiveTool,
}

impl From<RecipeId> for RecipeIdSave {
    fn from(recipe: RecipeId) -> Self {
        match recipe {
            RecipeId::PrimitiveTool => Self::PrimitiveTool,
        }
    }
}

impl From<RecipeIdSave> for RecipeId {
    fn from(recipe: RecipeIdSave) -> Self {
        match recipe {
            RecipeIdSave::PrimitiveTool => Self::PrimitiveTool,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ProductionTargetSave {
    Finite { remaining_runs: u32 },
    Infinite,
}

impl From<ProductionTarget> for ProductionTargetSave {
    fn from(target: ProductionTarget) -> Self {
        match target {
            ProductionTarget::Finite { remaining_runs } => Self::Finite { remaining_runs },
            ProductionTarget::Infinite => Self::Infinite,
        }
    }
}

impl From<ProductionTargetSave> for ProductionTarget {
    fn from(target: ProductionTargetSave) -> Self {
        match target {
            ProductionTargetSave::Finite { remaining_runs } => Self::Finite { remaining_runs },
            ProductionTargetSave::Infinite => Self::Infinite,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ProductionOrderSave {
    id: u64,
    workstation_id: u64,
    recipe_id: RecipeIdSave,
    target: ProductionTargetSave,
}

impl ProductionOrderSave {
    fn from_order(order: &ProductionOrder) -> Self {
        Self {
            id: order.id().value(),
            workstation_id: order.workstation_id().value(),
            recipe_id: order.recipe_id().into(),
            target: order.target().into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProductionZoneKindSave {
    Input,
    Output,
}

impl From<ProductionZoneKind> for ProductionZoneKindSave {
    fn from(kind: ProductionZoneKind) -> Self {
        match kind {
            ProductionZoneKind::Input => Self::Input,
            ProductionZoneKind::Output => Self::Output,
        }
    }
}

impl From<ProductionZoneKindSave> for ProductionZoneKind {
    fn from(kind: ProductionZoneKindSave) -> Self {
        match kind {
            ProductionZoneKindSave::Input => Self::Input,
            ProductionZoneKindSave::Output => Self::Output,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ProductionLogisticsSave {
    workstation_id: u64,
    input_cells: Vec<CellSave>,
    output_cells: Vec<CellSave>,
}

impl ProductionLogisticsSave {
    fn from_logistics(logistics: &ProductionLogistics) -> Self {
        Self {
            workstation_id: logistics.workstation_id().value(),
            input_cells: logistics
                .cells(ProductionZoneKind::Input)
                .map(Into::into)
                .collect(),
            output_cells: logistics
                .cells(ProductionZoneKind::Output)
                .map(Into::into)
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StructureKindSave {
    StoneWall,
}

impl From<StructureKind> for StructureKindSave {
    fn from(kind: StructureKind) -> Self {
        match kind {
            StructureKind::StoneWall => Self::StoneWall,
        }
    }
}

impl From<StructureKindSave> for StructureKind {
    fn from(kind: StructureKindSave) -> Self {
        match kind {
            StructureKindSave::StoneWall => Self::StoneWall,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ConstructionMaterialStateSave {
    Reserved,
    Delivered,
}

impl From<ConstructionMaterialState> for ConstructionMaterialStateSave {
    fn from(state: ConstructionMaterialState) -> Self {
        match state {
            ConstructionMaterialState::Reserved => Self::Reserved,
            ConstructionMaterialState::Delivered => Self::Delivered,
        }
    }
}

impl From<ConstructionMaterialStateSave> for ConstructionMaterialState {
    fn from(state: ConstructionMaterialStateSave) -> Self {
        match state {
            ConstructionMaterialStateSave::Reserved => Self::Reserved,
            ConstructionMaterialStateSave::Delivered => Self::Delivered,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ConstructionMaterialSave {
    item_id: u64,
    state: ConstructionMaterialStateSave,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ConstructionSiteSave {
    id: u64,
    kind: StructureKindSave,
    cell: CellSave,
    material: Option<ConstructionMaterialSave>,
}

impl ConstructionSiteSave {
    fn from_site(site: &ConstructionSite) -> Self {
        let material =
            site.material_item_id()
                .zip(site.material_state())
                .map(|(item_id, state)| ConstructionMaterialSave {
                    item_id: item_id.value(),
                    state: state.into(),
                });
        Self {
            id: site.id().value(),
            kind: site.kind().into(),
            cell: site.cell().into(),
            material,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct StructureSave {
    id: u64,
    kind: StructureKindSave,
    cell: CellSave,
}

impl StructureSave {
    fn from_structure(structure: &Structure) -> Self {
        Self {
            id: structure.id().value(),
            kind: structure.kind().into(),
            cell: structure.cell().into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum JobKindSave {
    Harvest {
        source: CellSave,
    },
    Eat {
        character_id: u64,
        item_id: u64,
    },
    Haul {
        item_id: u64,
        stockpile_id: u64,
        destination: CellSave,
    },
    Craft {
        workstation_id: u64,
        order_id: u64,
        recipe_id: RecipeIdSave,
    },
    SupplyProduction {
        workstation_id: u64,
        item_id: u64,
        destination: CellSave,
    },
    DeliverConstruction {
        site_id: u64,
        item_id: u64,
    },
    Construct {
        site_id: u64,
    },
}

impl From<JobKind> for JobKindSave {
    fn from(kind: JobKind) -> Self {
        match kind {
            JobKind::Harvest { source } => Self::Harvest {
                source: source.into(),
            },
            JobKind::Eat {
                character_id,
                item_id,
            } => Self::Eat {
                character_id: character_id.value(),
                item_id: item_id.value(),
            },
            JobKind::Haul {
                item_id,
                stockpile_id,
                destination,
            } => Self::Haul {
                item_id: item_id.value(),
                stockpile_id: stockpile_id.value(),
                destination: destination.into(),
            },
            JobKind::Craft {
                workstation_id,
                order_id,
                recipe_id,
            } => Self::Craft {
                workstation_id: workstation_id.value(),
                order_id: order_id.value(),
                recipe_id: recipe_id.into(),
            },
            JobKind::SupplyProduction {
                workstation_id,
                item_id,
                destination,
            } => Self::SupplyProduction {
                workstation_id: workstation_id.value(),
                item_id: item_id.value(),
                destination: destination.into(),
            },
            JobKind::DeliverConstruction { site_id, item_id } => Self::DeliverConstruction {
                site_id: site_id.value(),
                item_id: item_id.value(),
            },
            JobKind::Construct { site_id } => Self::Construct {
                site_id: site_id.value(),
            },
        }
    }
}

impl JobKindSave {
    fn into_kind(self) -> Result<JobKind, SaveError> {
        Ok(match self {
            Self::Harvest { source } => JobKind::Harvest {
                source: source.into_cell(),
            },
            Self::Eat {
                character_id,
                item_id,
            } => JobKind::Eat {
                character_id: entity_id(character_id, "eat character_id")?,
                item_id: entity_id(item_id, "eat item_id")?,
            },
            Self::Haul {
                item_id,
                stockpile_id,
                destination,
            } => JobKind::Haul {
                item_id: entity_id(item_id, "haul item_id")?,
                stockpile_id: entity_id(stockpile_id, "haul stockpile_id")?,
                destination: destination.into_cell(),
            },
            Self::Craft {
                workstation_id,
                order_id,
                recipe_id,
            } => JobKind::Craft {
                workstation_id: entity_id(workstation_id, "craft workstation_id")?,
                order_id: entity_id(order_id, "craft order_id")?,
                recipe_id: recipe_id.into(),
            },
            Self::SupplyProduction {
                workstation_id,
                item_id,
                destination,
            } => JobKind::SupplyProduction {
                workstation_id: entity_id(workstation_id, "supply workstation_id")?,
                item_id: entity_id(item_id, "supply item_id")?,
                destination: destination.into_cell(),
            },
            Self::DeliverConstruction { site_id, item_id } => JobKind::DeliverConstruction {
                site_id: entity_id(site_id, "construction delivery site_id")?,
                item_id: entity_id(item_id, "construction delivery item_id")?,
            },
            Self::Construct { site_id } => JobKind::Construct {
                site_id: entity_id(site_id, "construct site_id")?,
            },
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum JobStateSave {
    Available,
    Reserved {
        worker_id: u64,
    },
    Transporting {
        worker_id: u64,
    },
    Working {
        worker_id: u64,
        remaining_ticks: u32,
    },
}

impl From<JobState> for JobStateSave {
    fn from(state: JobState) -> Self {
        match state {
            JobState::Available => Self::Available,
            JobState::Reserved { worker_id } => Self::Reserved {
                worker_id: worker_id.value(),
            },
            JobState::Transporting { worker_id } => Self::Transporting {
                worker_id: worker_id.value(),
            },
            JobState::Working {
                worker_id,
                remaining_ticks,
            } => Self::Working {
                worker_id: worker_id.value(),
                remaining_ticks,
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct JobSave {
    id: u64,
    job: JobKindSave,
    state: JobStateSave,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    craft_reserved_items: Vec<u64>,
}

impl JobSave {
    fn from_job(job: &Job, world: &JobWorld) -> Self {
        Self {
            id: job.id().value(),
            job: job.kind().into(),
            state: job.state().into(),
            craft_reserved_items: world
                .craft_reserved_items(job.id())
                .into_iter()
                .flat_map(|items| items.iter().map(|id| id.value()))
                .collect(),
        }
    }
}

fn validate_collection_uniqueness(save: &SaveV1) -> Result<(), SaveError> {
    let mut owned = BTreeSet::new();
    let mut add = |id: u64, label: &str| -> Result<(), SaveError> {
        if id == 0 {
            return invalid(format!("{label} ID cannot be zero"));
        }
        if !owned.insert(id) {
            return invalid(format!("duplicate global entity ID {id} ({label})"));
        }
        Ok(())
    };
    for value in &save.characters {
        add(value.id, "character")?;
    }
    for value in &save.items {
        add(value.id, "item")?;
    }
    for value in &save.stockpiles {
        add(value.id, "stockpile")?;
    }
    for value in &save.workstations {
        add(value.id, "workstation")?;
    }
    for value in &save.production_orders {
        add(value.id, "production order")?;
    }
    for value in &save.construction_sites {
        add(value.id, "construction site")?;
    }
    for value in &save.structures {
        add(value.id, "structure")?;
    }
    for value in &save.jobs {
        add(value.id, "job")?;
    }
    Ok(())
}

fn restore_characters(
    saved: Vec<CharacterSave>,
) -> Result<BTreeMap<EntityId, Character>, SaveError> {
    let mut characters = BTreeMap::new();
    for value in saved {
        let character = value.into_character()?;
        if characters.insert(character.id(), character).is_some() {
            return invalid("duplicate character ID");
        }
    }
    Ok(characters)
}

fn restore_modified_world(
    generator: &WorldGenerator,
    saved: Vec<TerrainOverrideSave>,
) -> Result<ModifiedWorld, SaveError> {
    let mut world = ModifiedWorld::default();
    let mut seen = BTreeSet::new();
    for value in saved {
        let chunk = value.chunk.into_chunk();
        let local = value.local.into_local()?;
        if !seen.insert((chunk, local)) {
            return invalid(format!(
                "duplicate terrain override at chunk ({}, {}) local ({}, {})",
                chunk.x(),
                chunk.y(),
                local.x(),
                local.y()
            ));
        }
        let cell = chunk.world_cell(local).ok_or_else(|| {
            SaveError::InvalidData("terrain override world coordinate overflows".to_owned())
        })?;
        let terrain: Terrain = value.terrain.into();
        if generator.terrain_at(cell) == terrain {
            return invalid(format!(
                "terrain override at ({}, {}) redundantly equals generated terrain",
                cell.x(),
                cell.y()
            ));
        }
        world.restore_override(chunk, local, terrain);
    }
    Ok(world)
}

fn restore_exploration(saved: Vec<CellSave>) -> Result<ExploredWorld, SaveError> {
    let cells = saved
        .into_iter()
        .map(CellSave::into_cell)
        .collect::<Vec<_>>();
    if cells.iter().copied().collect::<BTreeSet<_>>().len() != cells.len() {
        return invalid("explored_cells contains duplicates");
    }
    Ok(ExploredWorld::restore_cells(cells))
}

fn restore_depleted_resources(
    generator: &WorldGenerator,
    saved: Vec<CellSave>,
) -> Result<BTreeSet<WorldCell>, SaveError> {
    let mut cells = BTreeSet::new();
    for value in saved {
        let cell = value.into_cell();
        if !cells.insert(cell) {
            return invalid(format!(
                "duplicate depleted resource cell ({}, {})",
                cell.x(),
                cell.y()
            ));
        }
        if generator.natural_resource_at(cell).is_none() {
            return invalid(format!(
                "depleted resource cell ({}, {}) has no generated natural resource",
                cell.x(),
                cell.y()
            ));
        }
    }
    Ok(cells)
}

fn restore_items(
    characters: &BTreeMap<EntityId, Character>,
    saved: Vec<ItemSave>,
) -> Result<ItemWorld, SaveError> {
    let mut world = ItemWorld::default();
    for value in saved {
        let id = entity_id(value.id, "item id")?;
        let quantity = ItemQuantity::new(value.quantity).ok_or_else(|| {
            SaveError::InvalidData(format!(
                "item {} quantity {} is outside 1..={MAX_STACK_QUANTITY}",
                id.value(),
                value.quantity
            ))
        })?;
        let kind: ItemKind = value.kind.into();
        match value.location {
            ItemLocationSave::Ground { position } => world
                .insert_ground(ItemStack::new_ground(
                    id,
                    kind,
                    quantity,
                    position.into_position()?,
                ))
                .map_err(|error| invalid_world_error("item", error))?,
            ItemLocationSave::Carried { character_id } => {
                let carrier = entity_id(character_id, "item carrier")?;
                let character = characters.get(&carrier).ok_or_else(|| {
                    SaveError::InvalidData(format!(
                        "item {} references missing carrier {}",
                        id.value(),
                        carrier.value()
                    ))
                })?;
                world
                    .insert_ground(ItemStack::new_ground(
                        id,
                        kind,
                        quantity,
                        character.position(),
                    ))
                    .map_err(|error| invalid_world_error("item", error))?;
                world
                    .move_to_carried(id, carrier)
                    .map_err(|error| invalid_world_error("item carrier", error))?;
            }
        }
    }
    Ok(world)
}

fn restore_stockpiles(saved: Vec<StockpileSave>) -> Result<StockpileWorld, SaveError> {
    let mut world = StockpileWorld::default();
    for value in saved {
        let id = entity_id(value.id, "stockpile id")?;
        let mut cells = value
            .cells
            .into_iter()
            .map(CellSave::into_cell)
            .collect::<Vec<_>>();
        if cells.is_empty() {
            return invalid(format!("stockpile {} has no cells", id.value()));
        }
        cells.sort_unstable();
        if cells.windows(2).any(|pair| pair[0] == pair[1]) {
            return invalid(format!("stockpile {} contains duplicate cells", id.value()));
        }
        let first = cells[0];
        world
            .insert(Stockpile::new(id, first))
            .map_err(|error| invalid_world_error("stockpile", error))?;
        for cell in cells.into_iter().skip(1) {
            world
                .set_cell(id, cell, true)
                .map_err(|error| invalid_world_error("stockpile cell", error))?;
        }
    }
    Ok(world)
}

fn restore_workstations(saved: Vec<WorkstationSave>) -> Result<WorkstationWorld, SaveError> {
    let mut world = WorkstationWorld::default();
    for value in saved {
        let id = entity_id(value.id, "workstation id")?;
        world
            .insert(Workstation::new(
                id,
                value.kind.into(),
                value.cell.into_cell(),
            ))
            .map_err(|error| invalid_world_error("workstation", error))?;
    }
    Ok(world)
}

fn restore_production_orders(
    workstations: &WorkstationWorld,
    saved: Vec<ProductionOrderSave>,
) -> Result<ProductionWorld, SaveError> {
    let mut world = ProductionWorld::default();
    for value in saved {
        let id = entity_id(value.id, "production order id")?;
        let workstation_id = entity_id(value.workstation_id, "production order workstation_id")?;
        let workstation = workstations.get(workstation_id).ok_or_else(|| {
            SaveError::InvalidData(format!(
                "production order {} references missing workstation {}",
                id.value(),
                workstation_id.value()
            ))
        })?;
        let recipe_id: RecipeId = value.recipe_id.into();
        if recipe_definition(recipe_id).workstation != workstation.kind() {
            return invalid(format!(
                "production order {} recipe is incompatible with workstation {}",
                id.value(),
                workstation_id.value()
            ));
        }
        world
            .insert(ProductionOrder::new(
                id,
                workstation_id,
                recipe_id,
                value.target.into(),
            ))
            .map_err(|error| invalid_world_error("production order", error))?;
    }
    Ok(world)
}

fn restore_production_logistics(
    workstations: &WorkstationWorld,
    stockpiles: &StockpileWorld,
    saved: Vec<ProductionLogisticsSave>,
) -> Result<ProductionLogisticsWorld, SaveError> {
    let workstation_ids = workstations
        .iter()
        .map(Workstation::id)
        .collect::<BTreeSet<_>>();
    let saved_ids = saved
        .iter()
        .map(|value| entity_id(value.workstation_id, "production logistics workstation_id"))
        .collect::<Result<BTreeSet<_>, _>>()?;
    if saved_ids != workstation_ids || saved.len() != saved_ids.len() {
        return invalid("production_logistics must contain exactly one record per workstation");
    }
    let mut world = ProductionLogisticsWorld::default();
    for workstation_id in &workstation_ids {
        world
            .insert_workstation(*workstation_id)
            .map_err(|error| invalid_world_error("production logistics", error))?;
    }
    for value in saved {
        let workstation_id =
            entity_id(value.workstation_id, "production logistics workstation_id")?;
        let workstation_cell = workstations
            .get(workstation_id)
            .expect("workstation set equality was checked above")
            .cell();
        for (kind, cells) in [
            (ProductionZoneKind::Input, value.input_cells),
            (ProductionZoneKind::Output, value.output_cells),
        ] {
            let mut seen = BTreeSet::new();
            for value in cells {
                let cell = value.into_cell();
                if !seen.insert(cell) {
                    return invalid(format!(
                        "production zone for workstation {} contains duplicate cell ({}, {})",
                        workstation_id.value(),
                        cell.x(),
                        cell.y()
                    ));
                }
                if !is_saved_production_zone_neighbour(workstation_cell, cell) {
                    return invalid(format!(
                        "production zone cell ({}, {}) is not adjacent to workstation {}",
                        cell.x(),
                        cell.y(),
                        workstation_id.value()
                    ));
                }
                if stockpiles.stockpile_at(cell).is_some() {
                    return invalid(format!(
                        "production zone cell ({}, {}) overlaps a stockpile",
                        cell.x(),
                        cell.y()
                    ));
                }
                world
                    .set_cell(workstation_id, kind, cell, true)
                    .map_err(|error| invalid_world_error("production zone", error))?;
            }
        }
    }
    Ok(world)
}

fn is_saved_production_zone_neighbour(center: WorldCell, cell: WorldCell) -> bool {
    let dx = i128::from(cell.x()) - i128::from(center.x());
    let dy = i128::from(cell.y()) - i128::from(center.y());
    dx.abs() <= 1 && dy.abs() <= 1 && (dx != 0 || dy != 0)
}

fn restore_construction(
    sites: Vec<ConstructionSiteSave>,
    structures: Vec<StructureSave>,
) -> Result<ConstructionWorld, SaveError> {
    let mut world = ConstructionWorld::default();
    for value in sites {
        let id = entity_id(value.id, "construction site id")?;
        let kind: StructureKind = value.kind.into();
        world
            .insert_site(ConstructionSite::new(id, kind, value.cell.into_cell()))
            .map_err(|error| invalid_world_error("construction site", error))?;
        if let Some(material) = value.material {
            let item_id = entity_id(material.item_id, "construction material item_id")?;
            world
                .reserve_material(id, item_id)
                .map_err(|error| invalid_world_error("construction material", error))?;
            if ConstructionMaterialState::from(material.state)
                == ConstructionMaterialState::Delivered
            {
                world
                    .mark_material_delivered(id, item_id)
                    .map_err(|error| invalid_world_error("construction material", error))?;
            }
        }
    }
    for value in structures {
        let id = entity_id(value.id, "structure id")?;
        let site = ConstructionSite::new(id, value.kind.into(), value.cell.into_cell());
        world
            .insert_site(site)
            .map_err(|error| invalid_world_error("structure staging", error))?;
        world
            .complete_site(id)
            .map_err(|error| invalid_world_error("structure", error))?;
    }
    Ok(world)
}

fn invalid_world_error<T: fmt::Debug>(context: &str, error: T) -> SaveError {
    SaveError::InvalidData(format!("{context} restore failed: {error:?}"))
}

#[allow(clippy::too_many_arguments)]
fn restore_jobs(
    characters: &BTreeMap<EntityId, Character>,
    items: &ItemWorld,
    stockpiles: &StockpileWorld,
    workstations: &WorkstationWorld,
    production: &ProductionWorld,
    production_logistics: &ProductionLogisticsWorld,
    construction: &ConstructionWorld,
    saved: Vec<JobSave>,
) -> Result<JobWorld, SaveError> {
    let mut world = JobWorld::default();
    for value in saved {
        let id = entity_id(value.id, "job id")?;
        let kind = value.job.into_kind()?;
        validate_job_references(
            id,
            kind,
            characters,
            items,
            stockpiles,
            workstations,
            production,
            production_logistics,
            construction,
        )?;
        let reserved_items = value
            .craft_reserved_items
            .into_iter()
            .map(|value| entity_id(value, "craft reserved item_id"))
            .collect::<Result<Vec<_>, _>>()?;
        if reserved_items
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .len()
            != reserved_items.len()
        {
            return invalid(format!("craft job {} repeats a reserved item", id.value()));
        }
        if !matches!(kind, JobKind::Craft { .. }) && !reserved_items.is_empty() {
            return invalid(format!(
                "non-craft job {} unexpectedly stores craft_reserved_items",
                id.value()
            ));
        }
        for item_id in &reserved_items {
            if items.get(*item_id).is_none() {
                return invalid(format!(
                    "job {} reserves missing item {}",
                    id.value(),
                    item_id.value()
                ));
            }
        }

        world
            .insert(Job::new(id, kind))
            .map_err(|error| invalid_world_error("job", error))?;
        if !reserved_items.is_empty() {
            world
                .reserve_craft_items(id, &reserved_items)
                .map_err(|error| invalid_world_error("craft reservation", error))?;
        }

        match value.state {
            JobStateSave::Available => {
                if !reserved_items.is_empty() {
                    return invalid(format!(
                        "available craft job {} cannot retain item reservations",
                        id.value()
                    ));
                }
            }
            JobStateSave::Reserved { worker_id } => {
                let worker = restore_worker_id(characters, worker_id, id)?;
                if let JobKind::Eat { character_id, .. } = kind
                    && worker != character_id
                {
                    return invalid(format!(
                        "eat job {} is reserved by character {} instead of {}",
                        id.value(),
                        worker.value(),
                        character_id.value()
                    ));
                }
                world
                    .reserve_worker(id, worker)
                    .map_err(|error| invalid_world_error("job worker reservation", error))?;
            }
            JobStateSave::Transporting { worker_id } => {
                if !matches!(
                    kind,
                    JobKind::Haul { .. }
                        | JobKind::SupplyProduction { .. }
                        | JobKind::DeliverConstruction { .. }
                ) {
                    return invalid(format!(
                        "job {} has transporting state for a non-transport job",
                        id.value()
                    ));
                }
                let worker = restore_worker_id(characters, worker_id, id)?;
                if let JobKind::Eat { character_id, .. } = kind
                    && worker != character_id
                {
                    return invalid(format!(
                        "eat job {} is reserved by character {} instead of {}",
                        id.value(),
                        worker.value(),
                        character_id.value()
                    ));
                }
                world
                    .reserve_worker(id, worker)
                    .map_err(|error| invalid_world_error("job worker reservation", error))?;
                world
                    .start_transporting(id)
                    .map_err(|error| invalid_world_error("transporting job", error))?;
            }
            JobStateSave::Working {
                worker_id,
                remaining_ticks,
            } => {
                if remaining_ticks == 0 {
                    return invalid(format!(
                        "working job {} has zero remaining_ticks",
                        id.value()
                    ));
                }
                if !matches!(
                    kind,
                    JobKind::Harvest { .. }
                        | JobKind::Eat { .. }
                        | JobKind::Craft { .. }
                        | JobKind::Construct { .. }
                ) {
                    return invalid(format!(
                        "job {} has working state for a non-work job",
                        id.value()
                    ));
                }
                let worker = restore_worker_id(characters, worker_id, id)?;
                if let JobKind::Eat { character_id, .. } = kind
                    && worker != character_id
                {
                    return invalid(format!(
                        "eat job {} is reserved by character {} instead of {}",
                        id.value(),
                        worker.value(),
                        character_id.value()
                    ));
                }
                world
                    .reserve_worker(id, worker)
                    .map_err(|error| invalid_world_error("job worker reservation", error))?;
                world
                    .start_working(id, remaining_ticks)
                    .map_err(|error| invalid_world_error("working job", error))?;
            }
        }
    }
    Ok(world)
}

fn restore_worker_id(
    characters: &BTreeMap<EntityId, Character>,
    value: u64,
    job_id: EntityId,
) -> Result<EntityId, SaveError> {
    let worker = entity_id(value, "job worker_id")?;
    if !characters.contains_key(&worker) {
        return invalid(format!(
            "job {} references missing worker {}",
            job_id.value(),
            worker.value()
        ));
    }
    Ok(worker)
}

#[allow(clippy::too_many_arguments)]
fn validate_job_references(
    job_id: EntityId,
    kind: JobKind,
    characters: &BTreeMap<EntityId, Character>,
    items: &ItemWorld,
    stockpiles: &StockpileWorld,
    workstations: &WorkstationWorld,
    production: &ProductionWorld,
    production_logistics: &ProductionLogisticsWorld,
    construction: &ConstructionWorld,
) -> Result<(), SaveError> {
    match kind {
        JobKind::Harvest { .. } => {}
        JobKind::Eat {
            character_id,
            item_id,
        } => {
            if !characters.contains_key(&character_id) {
                return invalid(format!(
                    "eat job {} references missing character {}",
                    job_id.value(),
                    character_id.value()
                ));
            }
            require_item(items, item_id, job_id)?;
            if items.get(item_id).is_none_or(|item| {
                item.kind() != ItemKind::Berries || item.ground_position().is_none()
            }) {
                return invalid(format!(
                    "eat job {} references non-food item {}",
                    job_id.value(),
                    item_id.value()
                ));
            }
        }
        JobKind::Haul {
            item_id,
            stockpile_id,
            destination,
        } => {
            require_item(items, item_id, job_id)?;
            if stockpiles.stockpile_at(destination) != Some(stockpile_id) {
                return invalid(format!(
                    "haul job {} destination is not owned by stockpile {}",
                    job_id.value(),
                    stockpile_id.value()
                ));
            }
        }
        JobKind::SupplyProduction {
            workstation_id,
            item_id,
            destination,
        } => {
            require_item(items, item_id, job_id)?;
            if workstations.get(workstation_id).is_none() {
                return invalid(format!(
                    "supply job {} references missing workstation {}",
                    job_id.value(),
                    workstation_id.value()
                ));
            }
            if production_logistics.zone_at(destination)
                != Some((workstation_id, ProductionZoneKind::Input))
            {
                return invalid(format!(
                    "supply job {} destination is not an Input cell of workstation {}",
                    job_id.value(),
                    workstation_id.value()
                ));
            }
        }
        JobKind::Craft {
            workstation_id,
            order_id,
            recipe_id,
        } => {
            let workstation = workstations.get(workstation_id).ok_or_else(|| {
                SaveError::InvalidData(format!(
                    "craft job {} references missing workstation {}",
                    job_id.value(),
                    workstation_id.value()
                ))
            })?;
            let order = production.get(order_id).ok_or_else(|| {
                SaveError::InvalidData(format!(
                    "craft job {} references missing production order {}",
                    job_id.value(),
                    order_id.value()
                ))
            })?;
            if order.workstation_id() != workstation_id
                || order.recipe_id() != recipe_id
                || recipe_definition(recipe_id).workstation != workstation.kind()
            {
                return invalid(format!(
                    "craft job {} references incompatible order",
                    job_id.value()
                ));
            }
        }
        JobKind::DeliverConstruction { site_id, item_id } => {
            require_item(items, item_id, job_id)?;
            let site = construction.site(site_id).ok_or_else(|| {
                SaveError::InvalidData(format!(
                    "construction delivery job {} references missing site {}",
                    job_id.value(),
                    site_id.value()
                ))
            })?;
            if site.material_item_id() != Some(item_id) {
                return invalid(format!(
                    "construction delivery job {} item does not match site material",
                    job_id.value()
                ));
            }
        }
        JobKind::Construct { site_id } => {
            if construction.site(site_id).is_none() {
                return invalid(format!(
                    "construct job {} references missing site {}",
                    job_id.value(),
                    site_id.value()
                ));
            }
        }
    }
    Ok(())
}

fn require_item(items: &ItemWorld, item_id: EntityId, job_id: EntityId) -> Result<(), SaveError> {
    if items.get(item_id).is_none() {
        return invalid(format!(
            "job {} references missing item {}",
            job_id.value(),
            item_id.value()
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn max_owned_entity_id(
    characters: &BTreeMap<EntityId, Character>,
    items: &ItemWorld,
    jobs: &JobWorld,
    stockpiles: &StockpileWorld,
    workstations: &WorkstationWorld,
    production: &ProductionWorld,
    construction: &ConstructionWorld,
) -> Option<u64> {
    characters
        .keys()
        .copied()
        .chain(items.iter().map(ItemStack::id))
        .chain(jobs.iter().map(Job::id))
        .chain(stockpiles.iter().map(Stockpile::id))
        .chain(workstations.iter().map(Workstation::id))
        .chain(production.iter().map(ProductionOrder::id))
        .chain(construction.sites().map(ConstructionSite::id))
        .chain(construction.structures().map(Structure::id))
        .map(EntityId::value)
        .max()
}

fn validate_next_entity_id(next: Option<u64>, max_id: Option<u64>) -> Result<(), SaveError> {
    if next == Some(0) {
        return invalid("next_entity_id cannot be zero");
    }
    if let (Some(next), Some(max_id)) = (next, max_id)
        && next <= max_id
    {
        return invalid(format!(
            "next_entity_id {next} must be greater than maximum owned entity ID {max_id}"
        ));
    }
    if next.is_none() && max_id != Some(u64::MAX) {
        return invalid("exhausted entity allocator requires an owned u64::MAX entity ID");
    }
    Ok(())
}

fn validate_restored_simulation(simulation: &Simulation) -> Result<(), SaveError> {
    for character in simulation.characters.values() {
        if !simulation
            .explored_world
            .contains(character.position().containing_cell())
        {
            return invalid(format!(
                "character {} stands in an unexplored cell",
                character.id().value()
            ));
        }
    }

    for logistics in simulation.production_logistics_world.iter() {
        let workstation_id = logistics.workstation_id();
        let workstation = simulation
            .workstation_world
            .get(workstation_id)
            .ok_or_else(|| SaveError::InvalidData("orphan production logistics".to_owned()))?;
        for kind in [ProductionZoneKind::Input, ProductionZoneKind::Output] {
            for cell in logistics.cells(kind) {
                if !is_saved_production_zone_neighbour(workstation.cell(), cell) {
                    return invalid(format!(
                        "production zone cell ({}, {}) is outside workstation {} perimeter",
                        cell.x(),
                        cell.y(),
                        workstation_id.value()
                    ));
                }
                if simulation.stockpile_world.stockpile_at(cell).is_some() {
                    return invalid("production zone overlaps stockpile after restore");
                }
            }
        }
    }

    for site in simulation.construction_world.sites() {
        if let Some(item_id) = site.material_item_id() {
            let item = simulation.item_world.get(item_id).ok_or_else(|| {
                SaveError::InvalidData(format!(
                    "construction site {} references missing material {}",
                    site.id().value(),
                    item_id.value()
                ))
            })?;
            if item.kind() != site.kind().material_kind()
                || item.quantity().get() < site.kind().material_quantity()
            {
                return invalid(format!(
                    "construction site {} has incompatible material stack",
                    site.id().value()
                ));
            }
            if site.material_state() == Some(ConstructionMaterialState::Delivered)
                && item.ground_position().is_none()
            {
                return invalid(format!(
                    "construction site {} marks carried material as delivered",
                    site.id().value()
                ));
            }
        } else if site.material_state().is_some() {
            return invalid(format!(
                "construction site {} has material state without material item",
                site.id().value()
            ));
        }
    }

    for job in simulation.job_world.iter() {
        validate_restored_job_state(simulation, job)?;
    }

    for item in simulation.item_world.iter() {
        if let Some(carrier) = item.carrier()
            && !simulation.characters.contains_key(&carrier)
        {
            return invalid(format!(
                "item {} has missing carrier {}",
                item.id().value(),
                carrier.value()
            ));
        }
    }
    Ok(())
}

fn validate_restored_job_state(simulation: &Simulation, job: &Job) -> Result<(), SaveError> {
    if let JobKind::Harvest { source } = job.kind()
        && (simulation.depleted_resources.contains(&source)
            || simulation.generator.natural_resource_at(source).is_none())
    {
        return invalid(format!(
            "harvest job {} targets a missing natural resource",
            job.id().value()
        ));
    }

    if let JobKind::Craft {
        workstation_id,
        recipe_id,
        ..
    } = job.kind()
    {
        match job.state() {
            JobState::Available => {
                if simulation
                    .job_world
                    .craft_reserved_items(job.id())
                    .is_some()
                {
                    return invalid(format!(
                        "available craft job {} retained input reservations",
                        job.id().value()
                    ));
                }
            }
            JobState::Reserved { .. } | JobState::Working { .. } => {
                if !simulation.craft_reserved_inputs_valid(job.id(), workstation_id, recipe_id) {
                    return invalid(format!(
                        "craft job {} has invalid physical input reservations",
                        job.id().value()
                    ));
                }
            }
            JobState::Transporting { .. } => {
                return invalid(format!(
                    "craft job {} cannot be transporting",
                    job.id().value()
                ));
            }
        }
    }

    if let JobState::Transporting { worker_id } = job.state() {
        let item_id = match job.kind() {
            JobKind::Haul { item_id, .. }
            | JobKind::SupplyProduction { item_id, .. }
            | JobKind::DeliverConstruction { item_id, .. } => item_id,
            _ => {
                return invalid(format!(
                    "non-logistics job {} cannot be transporting",
                    job.id().value()
                ));
            }
        };
        if simulation
            .item_world
            .get(item_id)
            .and_then(ItemStack::carrier)
            != Some(worker_id)
        {
            return invalid(format!(
                "transporting job {} item is not carried by its worker",
                job.id().value()
            ));
        }
    }

    if let JobKind::Construct { site_id } = job.kind()
        && matches!(
            job.state(),
            JobState::Reserved { .. } | JobState::Working { .. }
        )
        && simulation
            .construction_world
            .site(site_id)
            .is_none_or(|site| site.material_state() != Some(ConstructionMaterialState::Delivered))
    {
        return invalid(format!(
            "construct job {} is active before material delivery",
            job.id().value()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::HUNGRY_SATIETY;

    #[test]
    fn pristine_save_round_trips_canonically_and_metadata_is_readable() {
        let simulation = Simulation::new(WorldSeed::new(42)).unwrap();
        let resident_before = simulation.resident_chunks().collect::<Vec<_>>();
        let encoded = simulation.save_json().unwrap();
        let metadata = Simulation::save_metadata(&encoded).unwrap();
        assert_eq!(metadata.format_version, SAVE_FORMAT_VERSION);
        assert_eq!(metadata.world_seed, WorldSeed::new(42));
        assert_eq!(metadata.worldgen_version, CURRENT_WORLDGEN_VERSION);
        assert_eq!(metadata.tick, SimulationTick::new(0));

        let restored = Simulation::load_json(&encoded).unwrap();
        assert_eq!(restored.save_json().unwrap(), encoded);
        assert_eq!(
            restored.resident_chunks().collect::<Vec<_>>(),
            resident_before
        );
        assert!(
            restored
                .characters()
                .all(|character| character.last_tick_motion_trace() == [character.position()])
        );
    }

    #[test]
    fn save_v1_without_satiety_defaults_existing_characters_to_full() {
        let simulation = Simulation::new(WorldSeed::new(42)).unwrap();
        let encoded = simulation.save_json().unwrap();
        let mut json: Value = serde_json::from_slice(&encoded).unwrap();
        for character in json["characters"].as_array_mut().unwrap() {
            character.as_object_mut().unwrap().remove("satiety");
        }
        let restored = Simulation::load_json(&serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(
            restored
                .characters()
                .all(|character| character.satiety() == MAX_SATIETY)
        );
    }

    #[test]
    fn active_production_and_navigation_continue_deterministically_after_load() {
        let mut original = Simulation::new(WorldSeed::new(0)).unwrap();
        let workstation_id = original
            .place_workstation(WorkstationKind::Workbench, WorldCell::new(0, 1))
            .unwrap();
        let stockpile_id = original.create_stockpile(WorldCell::new(-2, 0)).unwrap();
        original
            .set_stockpile_cell(stockpile_id, WorldCell::new(2, 0), true)
            .unwrap();
        original
            .add_production_order(
                workstation_id,
                RecipeId::PrimitiveTool,
                ProductionTarget::Infinite,
            )
            .unwrap();
        original
            .set_terrain_override(WorldCell::new(7, 7), Terrain::Grass)
            .unwrap();

        let mut saw_active = false;
        for _ in 0..128 {
            original.advance_ticks(1).unwrap();
            saw_active |= original.jobs().any(|job| {
                matches!(
                    job.state(),
                    JobState::Reserved { .. }
                        | JobState::Transporting { .. }
                        | JobState::Working { .. }
                )
            });
            if saw_active
                && original
                    .jobs()
                    .any(|job| matches!(job.kind(), JobKind::SupplyProduction { .. }))
            {
                break;
            }
        }
        assert!(saw_active, "fixture must save non-trivial active authority");

        let encoded = original.save_json().unwrap();
        let mut restored = Simulation::load_json(&encoded).unwrap();
        assert_eq!(restored.save_json().unwrap(), encoded);

        for _ in 0..96 {
            original.advance_ticks(1).unwrap();
            restored.advance_ticks(1).unwrap();
        }
        assert_eq!(restored.save_json().unwrap(), original.save_json().unwrap());
    }

    #[test]
    fn active_eat_job_and_satiety_continue_deterministically_after_load() {
        let mut original = Simulation::new(WorldSeed::new(0)).unwrap();
        let cora = EntityId::new(3).unwrap();
        while original.characters.get(&cora).unwrap().satiety() > HUNGRY_SATIETY {
            original.characters.get_mut(&cora).unwrap().decay_satiety();
        }
        original.advance_ticks(1).unwrap();
        let eat_job = original
            .jobs()
            .find(|job| matches!(job.kind(), JobKind::Eat { character_id, .. } if character_id == cora))
            .cloned()
            .expect("hungry Cora must receive an Eat job");
        let JobKind::Eat { item_id, .. } = eat_job.kind() else {
            unreachable!();
        };
        assert_eq!(
            original.item_world.get(item_id).unwrap().quantity().get(),
            1
        );
        assert!(matches!(
            eat_job.state(),
            JobState::Reserved { .. } | JobState::Working { .. }
        ));

        let encoded = original.save_json().unwrap();
        let mut restored = Simulation::load_json(&encoded).unwrap();
        assert_eq!(restored.save_json().unwrap(), encoded);

        for _ in 0..128 {
            original.advance_ticks(1).unwrap();
            restored.advance_ticks(1).unwrap();
        }
        assert_eq!(restored.save_json().unwrap(), original.save_json().unwrap());
        assert_eq!(
            restored.characters.get(&cora).unwrap().satiety(),
            original.characters.get(&cora).unwrap().satiety()
        );
    }

    #[test]
    fn sparse_distant_override_and_depletion_round_trip_without_chunk_payloads() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let distant_override = WorldCell::new(50_000, -80_000);
        let base = simulation.generator.terrain_at(distant_override);
        let replacement = match base {
            Terrain::Grass => Terrain::Rock,
            Terrain::Water | Terrain::Rock => Terrain::Grass,
        };
        simulation
            .set_terrain_override(distant_override, replacement)
            .unwrap();
        let depleted = (-200..=200)
            .flat_map(|y| (-200..=200).map(move |x| WorldCell::new(x, y)))
            .find(|cell| simulation.generator.natural_resource_at(*cell).is_some())
            .unwrap();
        simulation.depleted_resources.insert(depleted);
        simulation.resource_revision += 1;

        let encoded = simulation.save_json().unwrap();
        let json: Value = serde_json::from_slice(&encoded).unwrap();
        assert!(json.get("chunks").is_none());
        assert_eq!(json["terrain_overrides"].as_array().unwrap().len(), 1);
        assert_eq!(json["depleted_resources"].as_array().unwrap().len(), 1);

        let restored = Simulation::load_json(&encoded).unwrap();
        assert_eq!(
            restored.effective_terrain_at(distant_override).unwrap(),
            replacement
        );
        assert!(restored.depleted_resources.contains(&depleted));
        assert_eq!(restored.save_json().unwrap(), encoded);
    }

    #[test]
    fn malformed_version_duplicate_ids_and_broken_references_are_rejected() {
        let simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let encoded = simulation.save_json().unwrap();
        let mut json: Value = serde_json::from_slice(&encoded).unwrap();

        json["version"] = Value::from(99_u64);
        let unsupported = serde_json::to_vec(&json).unwrap();
        assert!(matches!(
            Simulation::load_json(&unsupported),
            Err(SaveError::UnsupportedFormat { version: 99, .. })
        ));

        let mut duplicate: Value = serde_json::from_slice(&encoded).unwrap();
        let character_id = duplicate["characters"][0]["id"].clone();
        duplicate["items"][0]["id"] = character_id;
        assert!(matches!(
            Simulation::load_json(&serde_json::to_vec(&duplicate).unwrap()),
            Err(SaveError::InvalidData(_))
        ));

        let mut broken: Value = serde_json::from_slice(&encoded).unwrap();
        broken["items"][0]["location"] = serde_json::json!({
            "kind": "carried",
            "character_id": 999999u64
        });
        assert!(matches!(
            Simulation::load_json(&serde_json::to_vec(&broken).unwrap()),
            Err(SaveError::InvalidData(_))
        ));
    }
}
