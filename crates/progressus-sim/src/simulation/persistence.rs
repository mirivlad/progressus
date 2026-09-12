use progressus_content::item;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};

use super::*;
use crate::construction::ConstructionWorld;
use crate::entity::{CharacterRestoreState, EntityIdAllocator, NavigationRoute};
use crate::exploration::ExploredWorld;
use crate::item_world::ItemWorld;
use crate::job::JobWorld;
use crate::production::ProductionWorld;
use crate::production_logistics::ProductionLogisticsWorld;
use crate::residency::ChunkResidency;
use crate::stockpile::StockpileWorld;
use crate::workstation_world::WorkstationWorld;
use crate::world_state::ModifiedWorld;
use crate::{MAX_CONTAINER_DEPTH, MAX_SATIETY, MovementSpeed, ResourceLayerId, SlotId};

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
    UnknownContent { kind: &'static str, name: String },
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
            Self::UnknownContent { kind, name } => write!(
                formatter,
                "save names {kind} {name:?}, which this build does not define"
            ),
        }
    }
}

impl Error for SaveError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Encode(error) | Self::Decode(error) => Some(error),
            Self::UnsupportedWorldgen(error) => Some(error),
            Self::UnsupportedFormat { .. } | Self::InvalidData(_) | Self::UnknownContent { .. } => {
                None
            }
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
    /// Resource layers active when this world was written. Loading applies
    /// every layer this build knows, so a newer build's layer is additive; a
    /// layer recorded here that this build lacks is an error. See ADR-0022.
    #[serde(default)]
    worldgen_layers: Vec<String>,
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
    #[serde(default)]
    renewable_resource_regrowth: Vec<RenewableResourceRegrowthSave>,
    items: Vec<ItemSave>,
    stockpiles: Vec<StockpileSave>,
    workstations: Vec<WorkstationSave>,
    production_orders: Vec<ProductionOrderSave>,
    production_logistics: Vec<ProductionLogisticsSave>,
    construction_sites: Vec<ConstructionSiteSave>,
    #[serde(default)]
    workstation_construction_sites: Vec<WorkstationConstructionSiteSave>,
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
                worldgen_layers: simulation
                    .generator
                    .layers()
                    .iter()
                    .map(|layer| layer.name().to_owned())
                    .collect(),
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
            renewable_resource_regrowth: simulation
                .renewable_resource_regrowth
                .iter()
                .map(|(cell, ready_tick)| RenewableResourceRegrowthSave {
                    cell: (*cell).into(),
                    ready_tick: ready_tick.value(),
                })
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
            workstation_construction_sites: simulation
                .construction_world
                .workstation_sites()
                .map(WorkstationConstructionSiteSave::from_site)
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
        recorded_layers_are_known(&self.header)?;
        let generator =
            WorldGenerator::new(seed, version).map_err(SaveError::UnsupportedWorldgen)?;

        let characters = restore_characters(self.characters)?;
        if characters.is_empty() {
            return invalid("save contains no characters");
        }
        let modified_world = restore_modified_world(&generator, self.terrain_overrides)?;
        let explored_world = restore_exploration(self.explored_cells)?;
        let depleted_resources = restore_depleted_resources(&generator, self.depleted_resources)?;
        let renewable_resource_regrowth = restore_renewable_resource_regrowth(
            &generator,
            SimulationTick::new(self.header.tick),
            self.renewable_resource_regrowth,
        )?;
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
        let construction_world = restore_construction(
            self.construction_sites,
            self.workstation_construction_sites,
            self.structures,
        )?;
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
            renewable_resource_regrowth,
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
    recorded_layers_are_known(header)?;
    Ok(())
}

/// A world may gain layers it did not have, but never silently lose one: a
/// recorded layer this build does not define means the save came from a newer
/// build whose resources this build cannot reproduce.
fn recorded_layers_are_known(header: &SaveHeader) -> Result<(), SaveError> {
    for name in &header.worldgen_layers {
        if ResourceLayerId::from_name(name).is_none() {
            return Err(SaveError::UnknownContent {
                kind: "worldgen layer",
                name: name.clone(),
            });
        }
    }
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
struct RenewableResourceRegrowthSave {
    cell: CellSave,
    ready_tick: u64,
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

/// Content is persisted by its stable name. A name this build does not define
/// is an error, never a silently substituted default.
macro_rules! content_name_save {
    ($save:ident, $handle:ty, $kind:literal) => {
        #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
        #[serde(transparent)]
        struct $save(String);

        impl From<$handle> for $save {
            fn from(id: $handle) -> Self {
                Self(id.name().to_owned())
            }
        }

        impl $save {
            fn id(&self) -> Result<$handle, SaveError> {
                <$handle>::from_name(&self.0).ok_or_else(|| SaveError::UnknownContent {
                    kind: $kind,
                    name: self.0.clone(),
                })
            }
        }
    };
}

content_name_save!(TerrainSave, TerrainId, "terrain");
content_name_save!(ItemKindSave, ItemId, "item");
content_name_save!(StructureKindSave, StructureId, "structure");
content_name_save!(WorkstationKindSave, WorkstationId, "workstation");
content_name_save!(RecipeIdSave, RecipeId, "recipe");

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
    Wandering { destination: PositionSave },
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
            MovementState::Wandering { destination } => Self::Wandering {
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
            Self::Wandering { destination } => MovementState::Wandering {
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
    #[serde(default)]
    idle_anchor: Option<CellSave>,
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
            idle_anchor: Some(character.idle_anchor().into()),
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
        let idle_anchor = self
            .idle_anchor
            .map(CellSave::into_cell)
            .unwrap_or_else(|| position.containing_cell());
        let movement = self.movement.into_movement()?;
        let route = self
            .navigation
            .map(NavigationSave::into_route)
            .transpose()?;
        match (movement, route.as_ref()) {
            (MovementState::Navigating { destination }, Some(route))
            | (MovementState::Wandering { destination }, Some(route))
                if route.destination == destination && !route.waypoints.is_empty() => {}
            (MovementState::Navigating { .. } | MovementState::Wandering { .. }, _) => {
                return invalid(format!(
                    "moving character {} has no matching non-empty route",
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
                idle_anchor,
                movement,
                route,
            },
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct TerrainOverrideSave {
    chunk: ChunkSave,
    local: LocalSave,
    terrain: TerrainSave,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ItemLocationSave {
    Ground { position: PositionSave },
    Carried { character_id: u64 },
    Equipped { character_id: u64, slot: String },
    Contained { container_id: u64 },
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
            ItemLocation::Equipped { character_id, slot } => ItemLocationSave::Equipped {
                character_id: character_id.value(),
                slot: slot.name().to_owned(),
            },
            ItemLocation::Contained { container_id } => ItemLocationSave::Contained {
                container_id: container_id.value(),
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
    #[serde(default)]
    disallowed_items: Vec<ItemKindSave>,
}

impl StockpileSave {
    fn from_stockpile(stockpile: &Stockpile) -> Self {
        Self {
            id: stockpile.id().value(),
            cells: stockpile.cells().map(Into::into).collect(),
            disallowed_items: stockpile.disallowed_items().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ConstructionSiteSave {
    id: u64,
    kind: StructureKindSave,
    cell: CellSave,
    material: Option<ConstructionMaterialSave>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    preparation_resource_present: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct WorkstationConstructionSiteSave {
    id: u64,
    kind: WorkstationKindSave,
    cell: CellSave,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    preparation_resource_present: bool,
}

impl WorkstationConstructionSiteSave {
    fn from_site(site: &WorkstationConstructionSite) -> Self {
        Self {
            id: site.id().value(),
            kind: site.kind().into(),
            cell: site.cell().into(),
            preparation_resource_present: site.preparation_resource_present(),
        }
    }
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
            preparation_resource_present: site.preparation_resource_present(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct StructureSave {
    id: u64,
    kind: StructureKindSave,
    cell: CellSave,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    door_open_until_tick: Option<u64>,
}

impl StructureSave {
    fn from_structure(structure: &Structure) -> Self {
        Self {
            id: structure.id().value(),
            kind: structure.kind().into(),
            cell: structure.cell().into(),
            door_open_until_tick: structure.door_open_until().map(SimulationTick::value),
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
    PrepareConstruction {
        site_id: u64,
        target: ConstructionPreparationTargetSave,
    },
    EquipTool {
        item_id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        requested_worker_id: Option<u64>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ConstructionPreparationTargetSave {
    NaturalResource {
        source: CellSave,
    },
    GroundItem {
        item_id: u64,
        destination: CellSave,
    },
    Character {
        character_id: u64,
        destination: CellSave,
    },
}

impl From<ConstructionPreparationTarget> for ConstructionPreparationTargetSave {
    fn from(target: ConstructionPreparationTarget) -> Self {
        match target {
            ConstructionPreparationTarget::NaturalResource { source } => Self::NaturalResource {
                source: source.into(),
            },
            ConstructionPreparationTarget::GroundItem {
                item_id,
                destination,
            } => Self::GroundItem {
                item_id: item_id.value(),
                destination: destination.into(),
            },
            ConstructionPreparationTarget::Character {
                character_id,
                destination,
            } => Self::Character {
                character_id: character_id.value(),
                destination: destination.into(),
            },
        }
    }
}

impl ConstructionPreparationTargetSave {
    fn into_target(self) -> Result<ConstructionPreparationTarget, SaveError> {
        Ok(match self {
            Self::NaturalResource { source } => ConstructionPreparationTarget::NaturalResource {
                source: source.into_cell(),
            },
            Self::GroundItem {
                item_id,
                destination,
            } => ConstructionPreparationTarget::GroundItem {
                item_id: entity_id(item_id, "construction preparation item_id")?,
                destination: destination.into_cell(),
            },
            Self::Character {
                character_id,
                destination,
            } => ConstructionPreparationTarget::Character {
                character_id: entity_id(character_id, "construction preparation character_id")?,
                destination: destination.into_cell(),
            },
        })
    }
}

impl From<JobKind> for JobKindSave {
    fn from(kind: JobKind) -> Self {
        match kind {
            JobKind::EquipTool {
                item_id,
                requested_worker_id,
            } => Self::EquipTool {
                item_id: item_id.value(),
                requested_worker_id: requested_worker_id.map(EntityId::value),
            },
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
            JobKind::PrepareConstruction { site_id, target } => Self::PrepareConstruction {
                site_id: site_id.value(),
                target: target.into(),
            },
        }
    }
}

impl JobKindSave {
    fn into_kind(self) -> Result<JobKind, SaveError> {
        Ok(match self {
            Self::EquipTool {
                item_id,
                requested_worker_id,
            } => JobKind::EquipTool {
                item_id: entity_id(item_id, "equip item_id")?,
                requested_worker_id: requested_worker_id
                    .map(|id| entity_id(id, "equip requested_worker_id"))
                    .transpose()?,
            },
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
                recipe_id: recipe_id.id()?,
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
            Self::PrepareConstruction { site_id, target } => JobKind::PrepareConstruction {
                site_id: entity_id(site_id, "construction preparation site_id")?,
                target: target.into_target()?,
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
    for value in &save.workstation_construction_sites {
        add(value.id, "workstation construction site")?;
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
        let terrain = value.terrain.id()?;
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
        let Some(resource) = generator.natural_resource_at(cell) else {
            return invalid(format!(
                "depleted resource cell ({}, {}) has no generated natural resource",
                cell.x(),
                cell.y()
            ));
        };
        if resource.kind().is_renewable() {
            return invalid(format!(
                "renewable {} at ({}, {}) cannot be permanently depleted",
                resource.kind().name(),
                cell.x(),
                cell.y()
            ));
        }
    }
    Ok(cells)
}

fn restore_renewable_resource_regrowth(
    generator: &WorldGenerator,
    current_tick: SimulationTick,
    saved: Vec<RenewableResourceRegrowthSave>,
) -> Result<BTreeMap<WorldCell, SimulationTick>, SaveError> {
    let mut regrowth = BTreeMap::new();
    for value in saved {
        let cell = value.cell.into_cell();
        if value.ready_tick <= current_tick.value() {
            return invalid(format!(
                "renewable resource at ({}, {}) has non-future regrowth tick {}",
                cell.x(),
                cell.y(),
                value.ready_tick
            ));
        }
        let Some(resource) = generator.natural_resource_at(cell) else {
            return invalid(format!(
                "renewable resource cell ({}, {}) has no generated natural resource",
                cell.x(),
                cell.y()
            ));
        };
        if !resource.kind().is_renewable() {
            return invalid(format!(
                "resource cell ({}, {}) holds {}, which does not regrow",
                cell.x(),
                cell.y(),
                resource.kind().name()
            ));
        }
        if regrowth
            .insert(cell, SimulationTick::new(value.ready_tick))
            .is_some()
        {
            return invalid(format!(
                "duplicate renewable resource cell ({}, {})",
                cell.x(),
                cell.y()
            ));
        }
    }
    Ok(regrowth)
}

fn restore_items(
    characters: &BTreeMap<EntityId, Character>,
    saved: Vec<ItemSave>,
) -> Result<ItemWorld, SaveError> {
    let mut world = ItemWorld::default();
    // Contained stacks wait for their container to exist. Depth is bounded, so
    // a handful of passes always settles it, and anything still waiting after
    // that names a container that is missing or circular.
    let mut deferred: Vec<(EntityId, ItemId, ItemQuantity, EntityId)> = Vec::new();
    for value in saved {
        let id = entity_id(value.id, "item id")?;
        let quantity = ItemQuantity::new(value.quantity).ok_or_else(|| {
            SaveError::InvalidData(format!(
                "item {} quantity {} is outside 1..={MAX_STACK_QUANTITY}",
                id.value(),
                value.quantity
            ))
        })?;
        let kind = value.kind.id()?;
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
            ItemLocationSave::Equipped { character_id, slot } => {
                let bearer = entity_id(character_id, "item bearer")?;
                let character = characters.get(&bearer).ok_or_else(|| {
                    SaveError::InvalidData(format!(
                        "item {} references missing bearer {}",
                        id.value(),
                        bearer.value()
                    ))
                })?;
                let slot = SlotId::from_name(&slot).ok_or(SaveError::UnknownContent {
                    kind: "equipment slot",
                    name: slot.clone(),
                })?;
                // Equipment restores through the ordinary ground and hands
                // transitions, so it cannot bypass their index bookkeeping.
                world
                    .insert_ground(ItemStack::new_ground(
                        id,
                        kind,
                        quantity,
                        character.position(),
                    ))
                    .map_err(|error| invalid_world_error("item", error))?;
                world
                    .move_to_carried(id, bearer)
                    .map_err(|error| invalid_world_error("item bearer", error))?;
                world
                    .equip_carried(id, bearer, slot)
                    .map_err(|error| invalid_world_error("item equipment", error))?;
            }
            ItemLocationSave::Contained { container_id } => {
                deferred.push((
                    id,
                    kind,
                    quantity,
                    entity_id(container_id, "item container")?,
                ));
            }
        }
    }

    for _ in 0..=MAX_CONTAINER_DEPTH {
        let mut remaining = Vec::new();
        for (id, kind, quantity, container_id) in deferred {
            if world.get(container_id).is_none() {
                remaining.push((id, kind, quantity, container_id));
                continue;
            }
            // Contents restore through the ordinary ground transition, so they
            // cannot bypass the index bookkeeping that keeps a stack in exactly
            // one place.
            let anchor = world
                .get(container_id)
                .and_then(|container| container.ground_position())
                .unwrap_or_else(|| {
                    WorldPosition::from_cell_center(WorldCell::new(0, 0))
                        .expect("the world origin is a valid position")
                });
            world
                .insert_ground(ItemStack::new_ground(id, kind, quantity, anchor))
                .map_err(|error| invalid_world_error("item", error))?;
            world
                .move_to_container(id, container_id)
                .map_err(|error| invalid_world_error("item container", error))?;
        }
        deferred = remaining;
        if deferred.is_empty() {
            break;
        }
    }
    if let Some((id, _, _, container_id)) = deferred.first() {
        return invalid(format!(
            "item {} names container {}, which is missing or nested too deeply",
            id.value(),
            container_id.value()
        ));
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
        let mut disallowed_items = value
            .disallowed_items
            .iter()
            .map(ItemKindSave::id)
            .collect::<Result<Vec<_>, _>>()?;
        disallowed_items.sort_unstable();
        if disallowed_items.windows(2).any(|pair| pair[0] == pair[1]) {
            return invalid(format!(
                "stockpile {} contains duplicate item filters",
                id.value()
            ));
        }
        for kind in disallowed_items {
            world
                .set_item_allowed(id, kind, false)
                .map_err(|error| invalid_world_error("stockpile policy", error))?;
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
                value.kind.id()?,
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
        let recipe_id = value.recipe_id.id()?;
        if recipe_id.definition().workstation != workstation.kind() {
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
    workstation_sites: Vec<WorkstationConstructionSiteSave>,
    structures: Vec<StructureSave>,
) -> Result<ConstructionWorld, SaveError> {
    let mut world = ConstructionWorld::default();
    for value in sites {
        let id = entity_id(value.id, "construction site id")?;
        let kind = value.kind.id()?;
        world
            .insert_site(
                ConstructionSite::new(id, kind, value.cell.into_cell())
                    .with_preparation_resource(value.preparation_resource_present),
            )
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
    for value in workstation_sites {
        let id = entity_id(value.id, "workstation construction site id")?;
        world
            .insert_workstation_site(
                WorkstationConstructionSite::new(id, value.kind.id()?, value.cell.into_cell())
                    .with_preparation_resource(value.preparation_resource_present),
            )
            .map_err(|error| invalid_world_error("workstation construction site", error))?;
    }
    for value in structures {
        let id = entity_id(value.id, "structure id")?;
        let site = ConstructionSite::new(id, value.kind.id()?, value.cell.into_cell());
        world
            .insert_site(site)
            .map_err(|error| invalid_world_error("structure staging", error))?;
        world
            .complete_site(id)
            .map_err(|error| invalid_world_error("structure", error))?;
        if let Some(open_until_tick) = value.door_open_until_tick {
            world
                .set_door_open_until(id, Some(SimulationTick::new(open_until_tick)))
                .map_err(|error| invalid_world_error("door state", error))?;
        }
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
                if let JobKind::PrepareConstruction {
                    target: ConstructionPreparationTarget::Character { character_id, .. },
                    ..
                } = kind
                    && worker != character_id
                {
                    return invalid(format!(
                        "construction preparation job {} is reserved by character {} instead of {}",
                        id.value(),
                        worker.value(),
                        character_id.value()
                    ));
                }
                if let JobKind::EquipTool {
                    requested_worker_id: Some(character_id),
                    ..
                } = kind
                    && worker != character_id
                {
                    return invalid(format!(
                        "requested equip job {} is reserved by character {} instead of {}",
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
                        | JobKind::PrepareConstruction {
                            target: ConstructionPreparationTarget::GroundItem { .. },
                            ..
                        }
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
                        | JobKind::PrepareConstruction {
                            target: ConstructionPreparationTarget::NaturalResource { .. },
                            ..
                        }
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
        JobKind::EquipTool {
            item_id,
            requested_worker_id,
        } => {
            if items.get(item_id).is_none() {
                return invalid(format!(
                    "equip job references missing item {}",
                    item_id.value()
                ));
            }
            if let Some(character_id) = requested_worker_id
                && !characters.contains_key(&character_id)
            {
                return invalid(format!(
                    "equip job {} references missing requested character {}",
                    job_id.value(),
                    character_id.value()
                ));
            }
        }
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
            if items
                .get(item_id)
                .is_none_or(|item| item.kind() != item::BERRIES || item.ground_position().is_none())
            {
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
                || recipe_id.definition().workstation != workstation.kind()
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
        JobKind::PrepareConstruction { site_id, target } => {
            let site_cell = construction
                .site(site_id)
                .map(ConstructionSite::cell)
                .or_else(|| {
                    construction
                        .workstation_site(site_id)
                        .map(WorkstationConstructionSite::cell)
                })
                .ok_or_else(|| {
                    SaveError::InvalidData(format!(
                        "construction preparation job {} references missing site {}",
                        job_id.value(),
                        site_id.value()
                    ))
                })?;
            match target {
                ConstructionPreparationTarget::NaturalResource { source } => {
                    if source != site_cell {
                        return invalid(format!(
                            "construction preparation job {} targets a different source cell",
                            job_id.value()
                        ));
                    }
                }
                ConstructionPreparationTarget::GroundItem { item_id, .. } => {
                    require_item(items, item_id, job_id)?;
                }
                ConstructionPreparationTarget::Character { character_id, .. } => {
                    if !characters.contains_key(&character_id) {
                        return invalid(format!(
                            "construction preparation job {} references missing character {}",
                            job_id.value(),
                            character_id.value()
                        ));
                    }
                }
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
        .chain(
            construction
                .workstation_sites()
                .map(WorkstationConstructionSite::id),
        )
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
            if item.kind() != site.kind().definition().material
                || item.quantity().get() < site.kind().definition().material_quantity
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

    for site in simulation.construction_world.workstation_sites() {
        if simulation
            .workstation_world
            .workstation_at(site.cell())
            .is_some()
            || simulation
                .stockpile_world
                .stockpile_at(site.cell())
                .is_some()
            || simulation
                .production_logistics_world
                .zone_at(site.cell())
                .is_some()
        {
            return invalid(format!(
                "workstation construction site {} overlaps a permanent claim",
                site.id().value()
            ));
        }
        let walkable = simulation
            .is_walkable(site.cell())
            .map_err(|error| invalid_world_error("workstation construction site", error))?;
        if !simulation.is_explored(site.cell()) || !walkable {
            return invalid(format!(
                "workstation construction site {} occupies an unavailable cell",
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
            JobKind::PrepareConstruction {
                target: ConstructionPreparationTarget::GroundItem { item_id, .. },
                ..
            } => item_id,
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
    use progressus_content::{
        item, natural_resource, recipe, slot, structure, terrain, workstation,
    };

    /// A loaded cart must survive a save with its goods still inside it and
    /// still counted exactly once — a container is the easiest place for
    /// quantity to quietly appear or vanish.
    #[test]
    fn a_loaded_container_round_trips_without_gaining_or_losing_anything() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let bearer = crate::simulation::test_support::cora();
        let position = simulation.characters[&bearer].position();
        let place = |simulation: &mut Simulation, kind, quantity| {
            let id = simulation.id_allocator.allocate().unwrap();
            simulation
                .item_world
                .insert_ground(ItemStack::new_ground(
                    id,
                    kind,
                    ItemQuantity::new(quantity).unwrap(),
                    position,
                ))
                .unwrap();
            id
        };
        let cart = place(&mut simulation, item::CART, 1);
        let wood = place(&mut simulation, item::WOOD, 7);
        let parked = place(&mut simulation, item::CART, 1);
        let stone = place(&mut simulation, item::STONE, 3);

        // One cart on a bearer, one standing on the ground, both loaded.
        simulation.item_world.move_to_container(wood, cart).unwrap();
        simulation.item_world.move_to_carried(cart, bearer).unwrap();
        simulation
            .item_world
            .equip_carried(cart, bearer, slot::TOOL)
            .unwrap();
        simulation
            .item_world
            .move_to_container(stone, parked)
            .unwrap();

        let total = |simulation: &Simulation, kind| {
            simulation
                .item_world
                .iter()
                .filter(|item| item.kind() == kind)
                .map(|item| item.quantity().get())
                .sum::<u32>()
        };
        let (wood_before, stone_before) = (
            total(&simulation, item::WOOD),
            total(&simulation, item::STONE),
        );
        let stacks_before = simulation.item_world.iter().count();

        let bytes = simulation.save_json().unwrap();
        let reloaded = Simulation::load_json(&bytes).unwrap();

        assert_eq!(reloaded.item_world.iter().count(), stacks_before);
        assert_eq!(total(&reloaded, item::WOOD), wood_before);
        assert_eq!(total(&reloaded, item::STONE), stone_before);
        assert_eq!(
            reloaded.item_world.get(wood).unwrap().container(),
            Some(cart)
        );
        assert_eq!(
            reloaded.item_world.get(stone).unwrap().container(),
            Some(parked)
        );
        assert_eq!(reloaded.item_world.holder_of(wood), Some(bearer));
        assert_eq!(
            reloaded.item_world.holder_of(stone),
            None,
            "goods in a parked cart came back belonging to somebody"
        );
        assert_eq!(reloaded.equipment(bearer), vec![(slot::TOOL, cart)]);
        assert!(reloaded.item_world.indexes_are_consistent());
    }

    /// The copper layer is the first real one, so it is also the end-to-end
    /// check that a layered resource behaves like any other: it generates, it
    /// is harvestable through the ordinary job, and it yields its own item.
    #[test]
    fn a_layered_resource_generates_and_harvests_like_any_other() {
        let simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let vein = (-160..160)
            .flat_map(|y| (-160..160).map(move |x| WorldCell::new(x, y)))
            .find(|cell| {
                simulation
                    .natural_resource_at(*cell)
                    .unwrap()
                    .is_some_and(|resource| resource.kind() == natural_resource::COPPER_VEIN)
            })
            .expect("the copper layer places veins within reach of the start");

        let resource = simulation.natural_resource_at(vein).unwrap().unwrap();
        assert_eq!(
            resource.kind().definition().yields,
            item::COPPER_ORE,
            "harvesting a vein must produce ore through the definition, not a special case"
        );
        assert!(!resource.kind().is_renewable());
        assert!((3..=6).contains(&resource.yield_quantity()));

        // A layered resource round-trips like a base one: the save stores no
        // resources at all, so it must regenerate from seed, base and layers.
        let bytes = simulation.save_json().unwrap();
        let reloaded = Simulation::load_json(&bytes).unwrap();
        assert_eq!(
            reloaded.natural_resource_at(vein).unwrap(),
            Some(resource),
            "a layered resource must survive a save round trip by regenerating"
        );
    }
    use crate::ResourceLayers;

    /// A save records the layers it was written with so that a world from a
    /// newer build is refused instead of quietly losing resources it has.
    #[test]
    fn a_save_records_its_worldgen_layers_and_rejects_one_this_build_lacks() {
        let simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let bytes = simulation.save_json().unwrap();
        let json = String::from_utf8(bytes).unwrap();
        let recorded: Vec<String> = serde_json::from_str::<serde_json::Value>(&json)
            .unwrap()
            .get("worldgen_layers")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .expect("the header records its worldgen layers");
        let expected: Vec<String> = ResourceLayers::all()
            .iter()
            .map(|layer| layer.name().to_owned())
            .collect();
        assert_eq!(recorded, expected);

        // Forge a save from a build with one more layer than this one has.
        let mut document: serde_json::Value = serde_json::from_str(&json).unwrap();
        document["worldgen_layers"]
            .as_array_mut()
            .expect("worldgen_layers is a list")
            .push(serde_json::Value::String(
                "copper_veins_from_a_newer_build".to_owned(),
            ));
        let forged = serde_json::to_string(&document).unwrap();
        let error = Simulation::load_json(forged.as_bytes()).unwrap_err();
        assert!(
            matches!(
                &error,
                SaveError::UnknownContent { kind, name }
                    if *kind == "worldgen layer" && name == "copper_veins_from_a_newer_build"
            ),
            "a save naming an unknown layer must be refused, got {error:?}"
        );
        assert!(Simulation::save_metadata(forged.as_bytes()).is_err());
    }

    /// An added worldgen layer must not grow a resource under something the
    /// player already built. Placement refuses the opposite direction, so a
    /// resource and a claim can only meet once a layer exists; this pins the
    /// guard that will keep them apart when one does.
    #[test]
    fn every_kind_of_claimed_cell_reports_no_natural_resource() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let free = |simulation: &Simulation, skip: &[WorldCell]| {
            (-8..8)
                .flat_map(|y| (-8..8).map(move |x| WorldCell::new(x, y)))
                .find(|cell| {
                    !skip.contains(cell)
                        && simulation.is_explored(*cell)
                        && simulation.is_walkable(*cell).unwrap()
                        && simulation.natural_resource_at(*cell).unwrap().is_none()
                        && !simulation.cell_is_claimed(*cell)
                        && simulation
                            .characters
                            .values()
                            .all(|character| character.position().containing_cell() != *cell)
                        && simulation.item_world.iter().all(|item| {
                            item.ground_position()
                                .is_none_or(|position| position.containing_cell() != *cell)
                        })
                })
                .expect("the starting clearing has a free cell")
        };

        let stockpile_cell = free(&simulation, &[]);
        let stockpile = simulation.create_stockpile(stockpile_cell).unwrap();
        assert!(simulation.cell_is_claimed(stockpile_cell));

        // A workbench also needs free port cells around it, so try candidates.
        let workstation_cell = (-8..8)
            .flat_map(|y| (-8..8).map(move |x| WorldCell::new(x, y)))
            .find(|cell| {
                *cell != stockpile_cell
                    && simulation.validate_workstation_cell(*cell).is_ok()
                    && simulation
                        .construction_cell_has_removable_occupant(*cell)
                        .is_ok_and(|occupied| !occupied)
                    && simulation.default_production_ports(*cell).is_ok()
            })
            .expect("the starting clearing fits one workbench");
        simulation
            .place_workstation(workstation::WORKBENCH, workstation_cell)
            .unwrap();
        assert!(simulation.cell_is_claimed(workstation_cell));

        let site_cell = free(&simulation, &[stockpile_cell, workstation_cell]);
        simulation
            .designate_construction(structure::STONE_WALL, site_cell)
            .unwrap();
        assert!(simulation.cell_is_claimed(site_cell));

        // Every claim hides a resource from both the point and the chunk query,
        // so a layer cannot make one appear under a claim through either path.
        for cell in [stockpile_cell, workstation_cell, site_cell] {
            assert!(simulation.natural_resource_at(cell).unwrap().is_none());
            let (coordinate, _) = cell.split();
            assert!(
                !simulation
                    .natural_resources_in_chunk(coordinate)
                    .unwrap()
                    .iter()
                    .any(|(resource_cell, _)| *resource_cell == cell)
            );
        }

        // Production ports around the workbench are claims of their own.
        let ports: Vec<_> = simulation
            .production_logistics()
            .flat_map(|logistics| {
                logistics
                    .cells(ProductionZoneKind::Input)
                    .chain(logistics.cells(ProductionZoneKind::Output))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(!ports.is_empty());
        for cell in ports {
            assert!(simulation.cell_is_claimed(cell));
            assert!(simulation.natural_resource_at(cell).unwrap().is_none());
        }

        // Releasing a claim releases the cell.
        simulation
            .set_stockpile_cell(stockpile, stockpile_cell, false)
            .unwrap();
        assert!(!simulation.cell_is_claimed(stockpile_cell));
    }
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
    fn active_idle_wandering_and_anchor_continue_deterministically_after_load() {
        let mut original = Simulation::new(WorldSeed::new(0)).unwrap();
        for _ in 0..96 {
            original.advance_ticks(1).unwrap();
            if original
                .characters()
                .any(|character| matches!(character.movement(), MovementState::Wandering { .. }))
            {
                break;
            }
        }
        assert!(
            original
                .characters()
                .any(|character| matches!(character.movement(), MovementState::Wandering { .. })),
            "fixture must save an active idle walk"
        );

        let encoded = original.save_json().unwrap();
        let mut restored = Simulation::load_json(&encoded).unwrap();
        assert_eq!(restored.save_json().unwrap(), encoded);

        for _ in 0..32 {
            original.advance_ticks(1).unwrap();
            restored.advance_ticks(1).unwrap();
        }
        assert_eq!(restored.save_json().unwrap(), original.save_json().unwrap());
    }

    #[test]
    fn stockpile_item_policy_round_trips_and_missing_policy_defaults_to_allow_all() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let stockpile_id = simulation.create_stockpile(WorldCell::new(-2, 0)).unwrap();
        simulation
            .set_stockpile_item_allowed(stockpile_id, item::BERRIES, false)
            .unwrap();
        let encoded = simulation.save_json().unwrap();
        let restored = Simulation::load_json(&encoded).unwrap();
        let stockpile = restored
            .stockpiles()
            .find(|stockpile| stockpile.id() == stockpile_id)
            .unwrap();
        assert!(!stockpile.accepts(item::BERRIES));
        assert!(stockpile.accepts(item::WOOD));

        let mut json: Value = serde_json::from_slice(&encoded).unwrap();
        json["stockpiles"][0]
            .as_object_mut()
            .unwrap()
            .remove("disallowed_items");
        let legacy = Simulation::load_json(&serde_json::to_vec(&json).unwrap()).unwrap();
        let legacy_stockpile = legacy
            .stockpiles()
            .find(|stockpile| stockpile.id() == stockpile_id)
            .unwrap();
        assert!(
            ItemId::all()
                .into_iter()
                .all(|kind| legacy_stockpile.accepts(kind))
        );
    }

    #[test]
    fn door_open_state_round_trips_and_missing_state_defaults_to_closed() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let id = simulation.id_allocator.allocate().unwrap();
        simulation
            .construction_world
            .insert_site(ConstructionSite::new(
                id,
                structure::DOOR,
                WorldCell::new(0, 1),
            ))
            .unwrap();
        simulation.construction_world.complete_site(id).unwrap();
        simulation
            .construction_world
            .set_door_open_until(id, Some(SimulationTick::new(7)))
            .unwrap();

        let encoded = simulation.save_json().unwrap();
        let restored = Simulation::load_json(&encoded).unwrap();
        let door = restored
            .structures()
            .find(|structure| structure.id() == id)
            .unwrap();
        assert_eq!(door.door_state(), Some(crate::DoorState::Open));
        assert_eq!(door.door_open_until(), Some(SimulationTick::new(7)));

        let mut json: Value = serde_json::from_slice(&encoded).unwrap();
        json["structures"][0]
            .as_object_mut()
            .unwrap()
            .remove("door_open_until_tick");
        let legacy = Simulation::load_json(&serde_json::to_vec(&json).unwrap()).unwrap();
        assert_eq!(
            legacy
                .structures()
                .find(|structure| structure.id() == id)
                .unwrap()
                .door_state(),
            Some(crate::DoorState::Closed)
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
            .place_workstation(workstation::WORKBENCH, WorldCell::new(0, 1))
            .unwrap();
        let stockpile_id = original.create_stockpile(WorldCell::new(-2, 0)).unwrap();
        original
            .set_stockpile_cell(stockpile_id, WorldCell::new(2, 0), true)
            .unwrap();
        original
            .add_production_order(
                workstation_id,
                recipe::PRIMITIVE_TOOL,
                ProductionTarget::Infinite,
            )
            .unwrap();
        original
            .set_terrain_override(WorldCell::new(7, 7), terrain::GRASS)
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
    fn active_berry_bush_regrowth_round_trips_and_resumes_deterministically() {
        let mut original = Simulation::new(WorldSeed::new(0)).unwrap();
        let source = WorldCell::new(-3, -3);
        assert_eq!(
            original
                .natural_resource_at(source)
                .unwrap()
                .unwrap()
                .kind(),
            natural_resource::BERRY_BUSH
        );
        let job_id = original.designate_harvest(source).unwrap();
        for _ in 0..256 {
            original.advance_ticks(1).unwrap();
            if original.job_world.get(job_id).is_none() {
                break;
            }
        }
        let ready_tick = original.renewable_resource_regrowth[&source];
        assert!(ready_tick > original.tick());
        assert_eq!(original.natural_resource_at(source).unwrap(), None);

        let encoded = original.save_json().unwrap();
        let mut restored = Simulation::load_json(&encoded).unwrap();
        assert_eq!(
            restored.renewable_resource_regrowth,
            original.renewable_resource_regrowth
        );
        assert_eq!(restored.save_json().unwrap(), encoded);

        let remaining = ready_tick.value() - original.tick().value();
        original.advance_ticks(remaining).unwrap();
        restored.advance_ticks(remaining).unwrap();
        assert_eq!(restored.save_json().unwrap(), original.save_json().unwrap());
        assert_eq!(
            restored
                .natural_resource_at(source)
                .unwrap()
                .unwrap()
                .kind(),
            natural_resource::BERRY_BUSH
        );
    }

    #[test]
    fn sparse_distant_override_and_depletion_round_trip_without_chunk_payloads() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let distant_override = WorldCell::new(50_000, -80_000);
        let base = simulation.generator.terrain_at(distant_override);
        let replacement = TerrainId::all()
            .find(|candidate| *candidate != base)
            .expect("the terrain registry defines more than one kind");
        simulation
            .set_terrain_override(distant_override, replacement)
            .unwrap();
        let depleted = (-200..=200)
            .flat_map(|y| (-200..=200).map(move |x| WorldCell::new(x, y)))
            .find(|cell| {
                simulation
                    .generator
                    .natural_resource_at(*cell)
                    .is_some_and(|resource| resource.kind() != natural_resource::BERRY_BUSH)
            })
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
