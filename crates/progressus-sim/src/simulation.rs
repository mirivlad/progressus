//! The authoritative simulation and its owned world state.
//!
//! This module keeps `Simulation`, its construction, the tick loop and the
//! read accessors. Each responsibility lives in a child module that adds its
//! own `impl Simulation` block: `movement`, `needs`, `idle`, `work`,
//! `logistics`, `crafting`, `workstations`, `building`, `error` and
//! `persistence`. Children pull the shared imports through `use super::*`, so
//! a type used by several of them is imported once here. Methods a child
//! shares with the rest of the tree are `pub(super)`; the simulation surface
//! outside this module is unchanged.

#[cfg(test)]
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

mod building;
mod crafting;
mod error;
mod idle;
mod logistics;
mod movement;
mod needs;
mod persistence;
mod work;
mod workstations;

#[cfg(test)]
mod test_support;

pub use error::SimulationError;
pub use persistence::{SAVE_FORMAT_VERSION, SaveError, SaveMetadata};

use progressus_worldgen::{WorldGenerator, WorldgenError};

use crate::clock::SimulationClock;
use crate::construction::{ConstructionWorld, ConstructionWorldError};
use crate::entity::{EntityIdAllocator, NavigationRoute};
use crate::exploration::ExploredWorld;
use crate::item::ItemWorld;
use crate::job::{JobWorld, JobWorldError};
use crate::pathfinding::{PathfindingError, find_closest_explored_path, find_explored_path};
use crate::production::{ProductionWorld, ProductionWorldError};
use crate::production_logistics::{ProductionLogisticsWorld, ProductionLogisticsWorldError};
use crate::residency::ChunkResidency;
use crate::stockpile::{StockpileWorld, StockpileWorldError};
use crate::workstation::{WorkstationWorld, WorkstationWorldError};
use crate::world_state::ModifiedWorld;
use crate::{
    BERRIES_MEAL_SATIETY, CHUNK_SIDE, CURRENT_WORLDGEN_VERSION, Character, ChunkCoord,
    ConstructionMaterialState, ConstructionSite, Direction, EAT_WORK_TICKS, EffectiveChunk,
    EntityId, GeneratedChunk, HARVEST_WORK_TICKS, InteractionRadius, ItemKind, ItemLocation,
    ItemQuantity, ItemStack, Job, JobKind, JobState, LocalCell, MAX_STACK_QUANTITY, MovementState,
    NaturalResource, NaturalResourceKind, ProductionLogistics, ProductionOrder, ProductionTarget,
    ProductionZoneKind, RecipeId, SATIETY_DECAY_INTERVAL_TICKS, SimulationTick, Stockpile,
    Structure, StructureKind, Terrain, Workstation, WorkstationKind, WorldCell, WorldPosition,
    WorldPositionError, WorldSeed, WorldgenVersion, recipe_definition, within_interaction_range,
};

pub const BERRY_BUSH_REGROW_TICKS: u64 = 512;
const BOOTSTRAP_BERRIES: u32 = 10;
const AUTONOMOUS_FORAGE_RADIUS_CELLS: i64 = 8;

const IDLE_BEHAVIOR_INTERVAL_TICKS: u64 = 48;
const IDLE_WANDER_RADIUS_CELLS: i64 = 3;
const IDLE_SOCIAL_CHANCE_DIVISOR: u64 = 5;
const IDLE_DESTINATION_ATTEMPTS: u64 = 16;

const INITIAL_CHARACTERS: [(&str, i64); 5] = [
    ("Ada", -2),
    ("Borin", -1),
    ("Cora", 0),
    ("Dain", 1),
    ("Elin", 2),
];

#[derive(Clone, Debug)]
pub struct Simulation {
    generator: WorldGenerator,
    clock: SimulationClock,
    id_allocator: EntityIdAllocator,
    characters: BTreeMap<EntityId, Character>,
    modified_world: ModifiedWorld,
    item_world: ItemWorld,
    job_world: JobWorld,
    production_world: ProductionWorld,
    production_logistics_world: ProductionLogisticsWorld,
    stockpile_world: StockpileWorld,
    workstation_world: WorkstationWorld,
    construction_world: ConstructionWorld,
    chunk_residency: ChunkResidency,
    depleted_resources: BTreeSet<WorldCell>,
    renewable_resource_regrowth: BTreeMap<WorldCell, SimulationTick>,
    resource_revision: u64,
    explored_world: ExploredWorld,
    last_discovery_cells: BTreeMap<EntityId, WorldCell>,
    #[cfg(test)]
    base_terrain_query_count: Cell<u64>,
}

impl Simulation {
    pub fn new(seed: WorldSeed) -> Result<Self, SimulationError> {
        let generator = WorldGenerator::new(seed, CURRENT_WORLDGEN_VERSION)?;
        let mut id_allocator = EntityIdAllocator::new();
        let mut characters = BTreeMap::new();

        for (name, x) in INITIAL_CHARACTERS {
            let position = WorldCell::new(x, 0);
            let (chunk_coordinate, local) = position.split();
            let chunk = generator.generate(chunk_coordinate)?;
            if chunk.terrain_at(local) != Some(Terrain::Grass) {
                return Err(SimulationError::SpawnNotWalkable(position));
            }

            let id = id_allocator.allocate()?;
            let character = Character::new(id, name, WorldPosition::from_cell_center(position)?);
            if characters.insert(id, character).is_some() {
                return Err(SimulationError::DuplicateEntityId(id));
            }
        }

        let mut item_world = ItemWorld::default();
        for (kind, quantity, cell, offset_x, offset_y) in [
            (ItemKind::Wood, 8, WorldCell::new(-2, 0), 160, 180),
            (ItemKind::Stone, 6, WorldCell::new(-1, 0), 820, 220),
            (ItemKind::Wood, 10, WorldCell::new(1, 0), 240, 820),
            (ItemKind::Stone, 8, WorldCell::new(2, 0), 840, 760),
        ] {
            let id = id_allocator.allocate()?;
            let position =
                WorldPosition::from_cell_origin(cell)?.checked_translate(offset_x, offset_y)?;
            item_world
                .insert_ground(ItemStack::new_ground(
                    id,
                    kind,
                    ItemQuantity::new(quantity).expect("bootstrap stack quantities are positive"),
                    position,
                ))
                .expect("bootstrap item IDs are unique and stacks start on the ground");
        }

        let mut explored_world = ExploredWorld::default();
        for character in characters.values() {
            explored_world.reveal_around(character.position().containing_cell());
        }
        let occupied_cells = characters
            .values()
            .map(|character| character.position().containing_cell())
            .chain(
                item_world
                    .iter()
                    .filter_map(ItemStack::ground_position)
                    .map(WorldPosition::containing_cell),
            )
            .collect::<BTreeSet<_>>();
        let berries_cell = explored_world
            .cells()
            .filter(|cell| !occupied_cells.contains(cell))
            .filter(|cell| i128::from(cell.x()).abs() > 2 || i128::from(cell.y()).abs() > 2)
            .filter(|cell| generator.terrain_at(*cell) == Terrain::Grass)
            .filter(|cell| generator.natural_resource_at(*cell).is_none())
            .min_by_key(|cell| {
                (
                    i128::from(cell.x()).abs() + i128::from(cell.y()).abs(),
                    cell.x(),
                    cell.y(),
                )
            })
            .ok_or(SimulationError::NoBootstrapFoodCell)?;
        let berries_id = id_allocator.allocate()?;
        item_world
            .insert_ground(ItemStack::new_ground(
                berries_id,
                ItemKind::Berries,
                ItemQuantity::new(BOOTSTRAP_BERRIES)
                    .expect("bootstrap food quantity is within stack capacity"),
                WorldPosition::from_cell_center(berries_cell)?,
            ))
            .expect("bootstrap food ID is unique and starts on the ground");

        let last_discovery_cells = characters
            .iter()
            .map(|(id, character)| (*id, character.position().containing_cell()))
            .collect();
        let mut chunk_residency = ChunkResidency::default();
        chunk_residency.reconcile(
            generator,
            characters
                .values()
                .map(|character| character.position().containing_cell().split().0),
        )?;

        Ok(Self {
            generator,
            clock: SimulationClock::new(0),
            id_allocator,
            characters,
            modified_world: ModifiedWorld::default(),
            item_world,
            job_world: JobWorld::default(),
            production_world: ProductionWorld::default(),
            production_logistics_world: ProductionLogisticsWorld::default(),
            stockpile_world: StockpileWorld::default(),
            workstation_world: WorkstationWorld::default(),
            construction_world: ConstructionWorld::default(),
            chunk_residency,
            depleted_resources: BTreeSet::new(),
            renewable_resource_regrowth: BTreeMap::new(),
            resource_revision: 0,
            explored_world,
            last_discovery_cells,
            #[cfg(test)]
            base_terrain_query_count: Cell::new(0),
        })
    }

    pub const fn tick(&self) -> SimulationTick {
        self.clock.tick()
    }

    pub const fn worldgen_version(&self) -> WorldgenVersion {
        self.generator.version()
    }

    pub fn next_entity_id(&self) -> Option<EntityId> {
        self.id_allocator.peek()
    }

    pub fn resident_chunks(&self) -> impl ExactSizeIterator<Item = ChunkCoord> + '_ {
        self.chunk_residency.coordinates()
    }

    pub fn resident_chunk_count(&self) -> usize {
        self.chunk_residency.len()
    }

    pub const fn residency_revision(&self) -> u64 {
        self.chunk_residency.revision()
    }

    pub fn is_explored(&self, position: WorldCell) -> bool {
        self.explored_world.contains(position)
    }

    pub fn has_explored_cells_in_chunk(&self, coordinate: ChunkCoord) -> bool {
        self.explored_world.contains_chunk(coordinate)
    }

    pub const fn exploration_revision(&self) -> u64 {
        self.explored_world.revision()
    }

    pub fn advance_ticks(&mut self, count: u64) -> Result<(), SimulationError> {
        self.clock
            .tick()
            .value()
            .checked_add(count)
            .ok_or(SimulationError::TickOverflow)?;

        for _ in 0..count {
            self.clock.advance(1)?;
            self.maintain_renewable_resources()?;
            self.advance_characters_one_tick()?;
            self.decay_satiety_if_due();
            self.maintain_nutrition_jobs()?;
            self.maintain_construction_jobs()?;
            self.maintain_craft_jobs()?;
            self.maintain_craft_supply_jobs()?;
            self.maintain_haul_jobs()?;
            self.advance_jobs_one_tick()?;
            self.maintain_idle_behavior()?;
            self.maintain_doors()?;
            self.reconcile_chunk_residency()?;
        }

        Ok(())
    }

    fn maintain_renewable_resources(&mut self) -> Result<(), SimulationError> {
        let tick = self.clock.tick();
        let ready = self
            .renewable_resource_regrowth
            .iter()
            .filter_map(|(cell, ready_tick)| (*ready_tick <= tick).then_some(*cell))
            .collect::<Vec<_>>();
        if ready.is_empty() {
            return Ok(());
        }
        let next_resource_revision = self
            .resource_revision
            .checked_add(1)
            .ok_or(SimulationError::ResourceRevisionOverflow)?;
        for cell in ready {
            self.renewable_resource_regrowth.remove(&cell);
        }
        self.resource_revision = next_resource_revision;
        Ok(())
    }

    pub fn characters(&self) -> impl ExactSizeIterator<Item = &Character> {
        self.characters.values()
    }

    pub fn items(&self) -> impl ExactSizeIterator<Item = &ItemStack> {
        self.item_world.iter()
    }

    pub fn ground_items_in_chunk(
        &self,
        coordinate: ChunkCoord,
    ) -> impl Iterator<Item = &ItemStack> {
        self.item_world.ground_items_in_chunk(coordinate)
    }

    pub const fn item_revision(&self) -> u64 {
        self.item_world.revision()
    }

    pub const fn resource_revision(&self) -> u64 {
        self.resource_revision
    }

    pub fn natural_resource_at(
        &self,
        position: WorldCell,
    ) -> Result<Option<NaturalResource>, SimulationError> {
        if self.depleted_resources.contains(&position)
            || self.renewable_resource_regrowth.contains_key(&position)
        {
            return Ok(None);
        }
        let (coordinate, local) = position.split();
        if let Some(chunk) = self.chunk_residency.get(coordinate) {
            return Ok(chunk.natural_resource_at(local));
        }
        Ok(self.generator.natural_resource_at(position))
    }

    pub fn natural_resources_in_chunk(
        &self,
        coordinate: ChunkCoord,
    ) -> Result<Vec<(WorldCell, NaturalResource)>, SimulationError> {
        let generated = self.generated_chunk(coordinate)?;
        let mut resources = Vec::new();
        for y in 0..CHUNK_SIDE {
            for x in 0..CHUNK_SIDE {
                let local = LocalCell::new(x, y);
                let Some(resource) = generated.natural_resource_at(local) else {
                    continue;
                };
                let cell = coordinate
                    .world_cell(local)
                    .ok_or(SimulationError::Worldgen(
                        WorldgenError::CoordinateOutOfRange(coordinate),
                    ))?;
                if !self.depleted_resources.contains(&cell)
                    && !self.renewable_resource_regrowth.contains_key(&cell)
                {
                    resources.push((cell, resource));
                }
            }
        }
        Ok(resources)
    }

    pub fn pick_up_item(
        &mut self,
        character_id: EntityId,
        item_id: EntityId,
    ) -> Result<(), SimulationError> {
        let character = self
            .characters
            .get(&character_id)
            .ok_or(SimulationError::UnknownCharacter(character_id))?;
        let character_position = character.position();
        let interaction_radius = character.interaction_radius();
        let item = self
            .item_world
            .get(item_id)
            .ok_or(SimulationError::UnknownItem(item_id))?;
        let item_position = match item.location() {
            ItemLocation::Ground { position } => position,
            ItemLocation::Carried { .. } => return Err(SimulationError::ItemNotOnGround(item_id)),
        };
        if !within_interaction_range(
            character_position,
            interaction_radius,
            item_position,
            InteractionRadius::zero(),
        ) {
            return Err(SimulationError::ItemOutOfReach {
                character_id,
                item_id,
            });
        }

        self.item_world
            .move_to_carried(item_id, character_id)
            .expect("pickup preconditions were validated against the canonical item world");
        Ok(())
    }

    pub fn drop_item(
        &mut self,
        character_id: EntityId,
        item_id: EntityId,
        destination: WorldPosition,
    ) -> Result<(), SimulationError> {
        let character = self
            .characters
            .get(&character_id)
            .ok_or(SimulationError::UnknownCharacter(character_id))?;
        let item = self
            .item_world
            .get(item_id)
            .ok_or(SimulationError::UnknownItem(item_id))?;
        if item.carrier() != Some(character_id) {
            return Err(SimulationError::ItemNotCarriedByCharacter {
                character_id,
                item_id,
            });
        }
        if !within_interaction_range(
            character.position(),
            character.interaction_radius(),
            destination,
            InteractionRadius::zero(),
        ) {
            return Err(SimulationError::ItemOutOfReach {
                character_id,
                item_id,
            });
        }
        if !self.is_walkable(destination.containing_cell())? {
            return Err(SimulationError::ItemDropBlocked(
                destination.containing_cell(),
            ));
        }

        self.item_world
            .move_to_ground(item_id, character_id, destination)
            .expect("drop preconditions were validated against the canonical item world");
        Ok(())
    }

    pub fn jobs(&self) -> impl ExactSizeIterator<Item = &Job> {
        self.job_world.iter()
    }

    pub const fn job_revision(&self) -> u64 {
        self.job_world.revision()
    }

    pub fn job_for_worker(&self, worker_id: EntityId) -> Option<EntityId> {
        self.job_world.job_for_worker(worker_id)
    }

    pub fn stockpiles(&self) -> impl ExactSizeIterator<Item = &Stockpile> {
        self.stockpile_world.iter()
    }

    pub const fn stockpile_revision(&self) -> u64 {
        self.stockpile_world.revision()
    }

    pub fn stockpile_at(&self, cell: WorldCell) -> Option<EntityId> {
        self.stockpile_world.stockpile_at(cell)
    }

    pub fn workstations(&self) -> impl ExactSizeIterator<Item = &Workstation> {
        self.workstation_world.iter()
    }

    pub const fn workstation_revision(&self) -> u64 {
        self.workstation_world.revision()
    }

    pub fn production_orders(&self) -> impl ExactSizeIterator<Item = &ProductionOrder> {
        self.production_world.iter()
    }

    pub const fn production_revision(&self) -> u64 {
        self.production_world.revision()
    }

    pub fn production_logistics(&self) -> impl ExactSizeIterator<Item = &ProductionLogistics> {
        self.production_logistics_world.iter()
    }

    pub const fn production_logistics_revision(&self) -> u64 {
        self.production_logistics_world.revision()
    }

    pub fn workstation_at(&self, cell: WorldCell) -> Option<EntityId> {
        self.workstation_world.workstation_at(cell)
    }

    pub fn construction_sites(&self) -> impl ExactSizeIterator<Item = &ConstructionSite> {
        self.construction_world.sites()
    }

    pub fn structures(&self) -> impl ExactSizeIterator<Item = &Structure> {
        self.construction_world.structures()
    }

    pub const fn construction_revision(&self) -> u64 {
        self.construction_world.revision()
    }

    pub fn construction_site_at(&self, cell: WorldCell) -> Option<EntityId> {
        self.construction_world.site_at(cell)
    }

    pub fn structure_at(&self, cell: WorldCell) -> Option<EntityId> {
        self.construction_world.structure_at(cell)
    }

    pub(crate) fn structure_kind_at(&self, cell: WorldCell) -> Option<StructureKind> {
        self.construction_world.structure_kind_at(cell)
    }

    pub fn generated_chunk(
        &self,
        coordinate: ChunkCoord,
    ) -> Result<GeneratedChunk, SimulationError> {
        if let Some(chunk) = self.chunk_residency.get(coordinate) {
            return Ok(chunk.clone());
        }
        self.generator.generate(coordinate).map_err(Into::into)
    }

    pub fn set_terrain_override(
        &mut self,
        position: WorldCell,
        terrain: Terrain,
    ) -> Result<(), SimulationError> {
        let (coordinate, local) = position.split();
        let base = self.base_terrain_at(position)?;
        self.modified_world
            .set_override(coordinate, local, base, terrain);
        Ok(())
    }

    pub fn effective_terrain_at(&self, position: WorldCell) -> Result<Terrain, SimulationError> {
        let (coordinate, local) = position.split();
        if let Some(override_terrain) = self.modified_world.override_at(coordinate, local) {
            return Ok(override_terrain);
        }

        self.base_terrain_at(position)
    }

    pub fn effective_chunk(
        &self,
        coordinate: ChunkCoord,
    ) -> Result<EffectiveChunk, SimulationError> {
        let generated = self.generated_chunk(coordinate)?;
        let mut cells = Vec::with_capacity(usize::from(CHUNK_SIDE).pow(2));

        for y in 0..CHUNK_SIDE {
            for x in 0..CHUNK_SIDE {
                let local = LocalCell::new(x, y);
                let base = generated
                    .terrain_at(local)
                    .expect("generated chunks contain every valid local cell");
                cells.push(self.resolve_terrain(coordinate, local, base));
            }
        }

        Ok(EffectiveChunk::new(coordinate, cells))
    }

    fn base_terrain_at(&self, position: WorldCell) -> Result<Terrain, SimulationError> {
        #[cfg(test)]
        self.base_terrain_query_count
            .set(self.base_terrain_query_count.get() + 1);

        let (coordinate, local) = position.split();
        if let Some(chunk) = self.chunk_residency.get(coordinate) {
            return chunk.terrain_at(local).ok_or(SimulationError::Worldgen(
                WorldgenError::CoordinateOutOfRange(coordinate),
            ));
        }
        Ok(self.generator.terrain_at(position))
    }

    fn reconcile_chunk_residency(&mut self) -> Result<(), SimulationError> {
        self.chunk_residency.reconcile(
            self.generator,
            self.characters
                .values()
                .map(|character| character.position().containing_cell().split().0),
        )?;
        Ok(())
    }

    fn resolve_terrain(&self, coordinate: ChunkCoord, local: LocalCell, base: Terrain) -> Terrain {
        self.modified_world
            .override_at(coordinate, local)
            .unwrap_or(base)
    }
}

fn entry_distance(
    position: WorldPosition,
    source: WorldCell,
    direction: Direction,
) -> Result<i128, SimulationError> {
    let lower_x = i128::from(source.x())
        .checked_mul(crate::SUBUNITS_PER_CELL)
        .ok_or(SimulationError::Position(
            WorldPositionError::OutsideWorldCellRange,
        ))?;
    let lower_y = i128::from(source.y())
        .checked_mul(crate::SUBUNITS_PER_CELL)
        .ok_or(SimulationError::Position(
            WorldPositionError::OutsideWorldCellRange,
        ))?;
    let upper_x =
        lower_x
            .checked_add(crate::SUBUNITS_PER_CELL)
            .ok_or(SimulationError::Position(
                WorldPositionError::OutsideWorldCellRange,
            ))?;
    let upper_y =
        lower_y
            .checked_add(crate::SUBUNITS_PER_CELL)
            .ok_or(SimulationError::Position(
                WorldPositionError::OutsideWorldCellRange,
            ))?;

    match direction {
        Direction::East => {
            upper_x
                .checked_sub(position.x_subunits())
                .ok_or(SimulationError::Position(
                    WorldPositionError::OutsideWorldCellRange,
                ))
        }
        Direction::West => position
            .x_subunits()
            .checked_sub(lower_x)
            .and_then(|distance| distance.checked_add(1))
            .ok_or(SimulationError::Position(
                WorldPositionError::OutsideWorldCellRange,
            )),
        Direction::North => {
            upper_y
                .checked_sub(position.y_subunits())
                .ok_or(SimulationError::Position(
                    WorldPositionError::OutsideWorldCellRange,
                ))
        }
        Direction::South => position
            .y_subunits()
            .checked_sub(lower_y)
            .and_then(|distance| distance.checked_add(1))
            .ok_or(SimulationError::Position(
                WorldPositionError::OutsideWorldCellRange,
            )),
    }
}

fn production_input_layouts(center: WorldCell) -> Vec<[WorldCell; 2]> {
    let north = center
        .y()
        .checked_add(1)
        .map(|y| WorldCell::new(center.x(), y));
    let east = center
        .x()
        .checked_add(1)
        .map(|x| WorldCell::new(x, center.y()));
    let south = center
        .y()
        .checked_sub(1)
        .map(|y| WorldCell::new(center.x(), y));
    let west = center
        .x()
        .checked_sub(1)
        .map(|x| WorldCell::new(x, center.y()));
    [
        (north, south),
        (west, east),
        (north, east),
        (east, south),
        (south, west),
        (west, north),
    ]
    .into_iter()
    .filter_map(|(first, second)| Some([first?, second?]))
    .collect()
}

fn production_output_layouts(center: WorldCell) -> Vec<[WorldCell; 2]> {
    let north_west = center
        .x()
        .checked_sub(1)
        .zip(center.y().checked_add(1))
        .map(|(x, y)| WorldCell::new(x, y));
    let north_east = center
        .x()
        .checked_add(1)
        .zip(center.y().checked_add(1))
        .map(|(x, y)| WorldCell::new(x, y));
    let south_east = center
        .x()
        .checked_add(1)
        .zip(center.y().checked_sub(1))
        .map(|(x, y)| WorldCell::new(x, y));
    let south_west = center
        .x()
        .checked_sub(1)
        .zip(center.y().checked_sub(1))
        .map(|(x, y)| WorldCell::new(x, y));
    [
        (north_west, south_east),
        (north_east, south_west),
        (north_west, north_east),
        (north_east, south_east),
        (south_east, south_west),
        (south_west, north_west),
    ]
    .into_iter()
    .filter_map(|(first, second)| Some([first?, second?]))
    .collect()
}

fn is_production_zone_neighbour(center: WorldCell, cell: WorldCell) -> bool {
    let dx = i128::from(cell.x()) - i128::from(center.x());
    let dy = i128::from(cell.y()) - i128::from(center.y());
    dx.abs() <= 1 && dy.abs() <= 1 && (dx != 0 || dy != 0)
}

fn idle_cell_within_anchor(anchor: WorldCell, cell: WorldCell) -> bool {
    let dx = (i128::from(anchor.x()) - i128::from(cell.x())).unsigned_abs();
    let dy = (i128::from(anchor.y()) - i128::from(cell.y())).unsigned_abs();
    dx + dy <= IDLE_WANDER_RADIUS_CELLS as u128
}

fn idle_entropy(seed: u64, character_id: u64, cycle: u64) -> u64 {
    mix_idle_entropy(
        seed ^ character_id.wrapping_mul(0xD6E8_FEB8_6659_FD93)
            ^ cycle.wrapping_mul(0xA076_1D64_78BD_642F),
    )
}

fn mix_idle_entropy(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn cell_manhattan_distance(first: WorldCell, second: WorldCell) -> u128 {
    let dx = (i128::from(first.x()) - i128::from(second.x())).unsigned_abs();
    let dy = (i128::from(first.y()) - i128::from(second.y())).unsigned_abs();
    dx + dy
}

fn build_waypoints(
    current: WorldPosition,
    destination: WorldPosition,
    cells: &[WorldCell],
) -> Result<VecDeque<WorldPosition>, SimulationError> {
    let mut points = Vec::new();
    let same_cell = current.containing_cell() == destination.containing_cell();
    if same_cell {
        points.push(WorldPosition::from_subunits(
            destination.x_subunits(),
            current.y_subunits(),
        )?);
    } else {
        let start_center = WorldPosition::from_cell_center(current.containing_cell())?;
        points.push(WorldPosition::from_subunits(
            start_center.x_subunits(),
            current.y_subunits(),
        )?);
        points.push(start_center);
        points.extend(
            cells
                .iter()
                .skip(1)
                .take(cells.len().saturating_sub(2))
                .map(|cell| WorldPosition::from_cell_center(*cell))
                .collect::<Result<Vec<_>, _>>()?,
        );
        let approach_cell = *cells
            .get(cells.len().saturating_sub(2))
            .expect("cross-cell path contains an approach cell");
        let destination_cell = destination.containing_cell();
        debug_assert_eq!(cell_manhattan_distance(approach_cell, destination_cell), 1);
        let approach_center = WorldPosition::from_cell_center(approach_cell)?;
        let final_turn = if approach_cell.x() != destination_cell.x() {
            WorldPosition::from_subunits(destination.x_subunits(), approach_center.y_subunits())?
        } else {
            WorldPosition::from_subunits(approach_center.x_subunits(), destination.y_subunits())?
        };
        points.push(final_turn);
    }
    points.push(destination);
    Ok(points.into_iter().filter(|point| *point != current).fold(
        VecDeque::new(),
        |mut waypoints, point| {
            if waypoints.back().copied() != Some(point) {
                waypoints.push_back(point);
            }
            waypoints
        },
    ))
}

fn direction_and_distance(
    position: WorldPosition,
    target: WorldPosition,
) -> Result<(Direction, i128), SimulationError> {
    let delta_x = target.x_subunits() - position.x_subunits();
    let delta_y = target.y_subunits() - position.y_subunits();
    if delta_x > 0 && delta_y == 0 {
        Ok((Direction::East, delta_x))
    } else if delta_x < 0 && delta_y == 0 {
        Ok((Direction::West, -delta_x))
    } else if delta_y > 0 && delta_x == 0 {
        Ok((Direction::North, delta_y))
    } else if delta_y < 0 && delta_x == 0 {
        Ok((Direction::South, -delta_y))
    } else {
        Err(SimulationError::Position(
            WorldPositionError::OutsideWorldCellRange,
        ))
    }
}

fn translate(
    position: WorldPosition,
    direction: Direction,
    distance: i128,
) -> Result<WorldPosition, SimulationError> {
    let (delta_x, delta_y) = match direction {
        Direction::East => (distance, 0),
        Direction::West => (-distance, 0),
        Direction::North => (0, distance),
        Direction::South => (0, -distance),
    };
    position
        .checked_translate(delta_x, delta_y)
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::test_support::*;

    #[test]
    fn initial_characters_reveal_the_union_of_radius_five_disks() {
        let simulation = Simulation::new(WorldSeed::new(2)).unwrap();

        assert!(simulation.is_explored(WorldCell::new(-7, 0)));
        assert!(simulation.is_explored(WorldCell::new(7, 0)));
        assert!(simulation.is_explored(WorldCell::new(4, 4)));
        assert!(!simulation.is_explored(WorldCell::new(-8, 0)));
        assert!(!simulation.is_explored(WorldCell::new(0, 6)));
    }

    #[test]
    fn discovery_updates_only_after_a_character_enters_a_new_cell() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        place_on_grass(&mut simulation, cora, WorldCell::new(20, 0));
        simulation
            .set_terrain_override(WorldCell::new(21, 0), Terrain::Grass)
            .unwrap();
        simulation.advance_ticks(1).unwrap();
        let revision = simulation.exploration_revision();

        simulation
            .set_movement_direction(cora, Direction::East)
            .unwrap();
        simulation.advance_ticks(1).unwrap();
        assert_eq!(simulation.exploration_revision(), revision);
        assert!(!simulation.is_explored(WorldCell::new(26, 0)));

        simulation.advance_ticks(1).unwrap();
        assert!(simulation.exploration_revision() > revision);
        assert!(simulation.is_explored(WorldCell::new(26, 0)));
    }

    #[test]
    fn any_character_can_reveal_negative_and_distant_cells() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let ada = EntityId::new(1).unwrap();
        let distant = WorldCell::new(-100, -20);
        place_on_grass(&mut simulation, ada, distant);
        assert!(!simulation.is_explored(WorldCell::new(-105, -20)));

        simulation.advance_ticks(1).unwrap();

        assert!(simulation.is_explored(WorldCell::new(-105, -20)));
        assert!(simulation.is_explored(WorldCell::new(-97, -16)));
    }

    #[test]
    fn starting_supplies_have_stable_ids_quantities_and_exact_subcell_positions() {
        let simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let items = simulation.items().collect::<Vec<_>>();

        assert_eq!(items.len(), 5);
        assert_eq!(items[0].id(), EntityId::new(6).unwrap());
        assert_eq!(items[0].kind(), ItemKind::Wood);
        assert_eq!(items[0].quantity().get(), 8);
        assert_eq!(
            items[0].ground_position(),
            Some(
                WorldPosition::from_cell_origin(WorldCell::new(-2, 0))
                    .unwrap()
                    .checked_translate(160, 180)
                    .unwrap()
            )
        );
        assert_eq!(items[3].id(), EntityId::new(9).unwrap());
        assert_eq!(items[3].kind(), ItemKind::Stone);
        assert_eq!(items[3].quantity().get(), 8);
        assert_eq!(items[4].id(), EntityId::new(10).unwrap());
        assert_eq!(items[4].kind(), ItemKind::Berries);
        assert_eq!(items[4].quantity().get(), BOOTSTRAP_BERRIES);
        let berries_cell = items[4].ground_position().unwrap().containing_cell();
        assert!(berries_cell.x().abs() > 2 || berries_cell.y().abs() > 2);
        assert!(simulation.is_explored(berries_cell));
        assert_eq!(
            simulation.effective_terrain_at(berries_cell).unwrap(),
            Terrain::Grass
        );
        assert!(
            simulation
                .natural_resource_at(berries_cell)
                .unwrap()
                .is_none()
        );
        assert!(
            items
                .iter()
                .all(|item| matches!(item.location(), ItemLocation::Ground { .. }))
        );
    }

    #[test]
    fn pickup_and_drop_preserve_item_identity_quantity_and_exact_location() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let ada = EntityId::new(1).unwrap();
        let item_id = EntityId::new(6).unwrap();
        let before = simulation.item_revision();
        let original = simulation.item_world.get(item_id).unwrap().clone();

        simulation.pick_up_item(ada, item_id).unwrap();
        let carried = simulation.item_world.get(item_id).unwrap();
        assert_eq!(carried.id(), original.id());
        assert_eq!(carried.kind(), original.kind());
        assert_eq!(carried.quantity(), original.quantity());
        assert_eq!(
            carried.location(),
            ItemLocation::Carried { character_id: ada }
        );
        assert_eq!(simulation.item_revision(), before + 1);
        assert_eq!(
            simulation
                .ground_items_in_chunk(WorldCell::new(-2, 0).split().0)
                .filter(|item| item.id() == item_id)
                .count(),
            0
        );

        let destination = character(&simulation, ada)
            .position()
            .checked_translate(200, 100)
            .unwrap();
        simulation.drop_item(ada, item_id, destination).unwrap();
        let dropped = simulation.item_world.get(item_id).unwrap();
        assert_eq!(dropped.id(), original.id());
        assert_eq!(dropped.kind(), original.kind());
        assert_eq!(dropped.quantity(), original.quantity());
        assert_eq!(
            dropped.location(),
            ItemLocation::Ground {
                position: destination
            }
        );
        assert_eq!(simulation.item_revision(), before + 2);
        assert!(simulation.item_world.indexes_are_consistent());
    }

    #[test]
    fn failed_item_transfers_are_atomic_for_reach_carrier_and_blocked_drop() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let ada = EntityId::new(1).unwrap();
        let cora = EntityId::new(3).unwrap();
        let item_id = EntityId::new(6).unwrap();
        let initial_item = simulation.item_world.get(item_id).unwrap().clone();
        let initial_revision = simulation.item_revision();

        assert_eq!(
            simulation.pick_up_item(cora, item_id),
            Err(SimulationError::ItemOutOfReach {
                character_id: cora,
                item_id,
            })
        );
        assert_eq!(simulation.item_world.get(item_id), Some(&initial_item));
        assert_eq!(simulation.item_revision(), initial_revision);

        simulation.pick_up_item(ada, item_id).unwrap();
        let carried_revision = simulation.item_revision();
        let carried = simulation.item_world.get(item_id).unwrap().clone();
        assert_eq!(
            simulation.drop_item(cora, item_id, character(&simulation, cora).position()),
            Err(SimulationError::ItemNotCarriedByCharacter {
                character_id: cora,
                item_id,
            })
        );
        assert_eq!(simulation.item_world.get(item_id), Some(&carried));
        assert_eq!(simulation.item_revision(), carried_revision);

        let blocked_cell = character(&simulation, ada).position().containing_cell();
        simulation
            .set_terrain_override(blocked_cell, Terrain::Rock)
            .unwrap();
        let destination = character(&simulation, ada).position();
        assert_eq!(
            simulation.drop_item(ada, item_id, destination),
            Err(SimulationError::ItemDropBlocked(blocked_cell))
        );
        assert_eq!(simulation.item_world.get(item_id), Some(&carried));
        assert_eq!(simulation.item_revision(), carried_revision);
        assert!(simulation.item_world.indexes_are_consistent());
    }

    #[test]
    fn effective_terrain_point_lookup_uses_override_without_base_query() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let position = WorldCell::new(0, 0);

        simulation
            .set_terrain_override(position, Terrain::Rock)
            .unwrap();
        simulation.base_terrain_query_count.set(0);

        assert_eq!(
            simulation.effective_terrain_at(position).unwrap(),
            Terrain::Rock
        );
        assert_eq!(simulation.base_terrain_query_count.get(), 0);
    }

    #[test]
    fn distant_sparse_modifications_survive_resident_unload_reload_cycles() {
        let mut simulation = Simulation::new(WorldSeed::new(73)).unwrap();
        let first_chunk = ChunkCoord::new(800, -900);
        let second_chunk = ChunkCoord::new(-1_200, 1_400);
        let first_cell = first_chunk.world_cell(LocalCell::new(7, 11)).unwrap();
        let second_cell = second_chunk.world_cell(LocalCell::new(19, 23)).unwrap();

        let changed = |terrain| match terrain {
            Terrain::Grass => Terrain::Rock,
            Terrain::Water | Terrain::Rock => Terrain::Grass,
        };
        let first_changed = changed(simulation.effective_terrain_at(first_cell).unwrap());
        let second_changed = changed(simulation.effective_terrain_at(second_cell).unwrap());
        simulation
            .set_terrain_override(first_cell, first_changed)
            .unwrap();
        simulation
            .set_terrain_override(second_cell, second_changed)
            .unwrap();

        let relocate = |simulation: &mut Simulation, cell: WorldCell| {
            let position = WorldPosition::from_cell_center(cell).unwrap();
            for character in simulation.characters.values_mut() {
                character.set_position(position);
                character.set_movement(MovementState::Idle);
            }
            simulation.reconcile_chunk_residency().unwrap();
        };

        relocate(&mut simulation, first_cell);
        assert_eq!(
            simulation.resident_chunk_count(),
            crate::RESIDENT_CHUNKS_PER_CENTER
        );
        assert!(
            simulation
                .resident_chunks()
                .any(|chunk| chunk == first_chunk)
        );
        assert!(
            !simulation
                .resident_chunks()
                .any(|chunk| chunk == second_chunk)
        );
        assert_eq!(
            simulation.effective_terrain_at(first_cell).unwrap(),
            first_changed
        );

        relocate(&mut simulation, second_cell);
        assert!(
            !simulation
                .resident_chunks()
                .any(|chunk| chunk == first_chunk)
        );
        assert!(
            simulation
                .resident_chunks()
                .any(|chunk| chunk == second_chunk)
        );
        assert_eq!(
            simulation.effective_terrain_at(first_cell).unwrap(),
            first_changed
        );
        assert_eq!(
            simulation.effective_terrain_at(second_cell).unwrap(),
            second_changed
        );

        relocate(&mut simulation, first_cell);
        assert!(
            simulation
                .resident_chunks()
                .any(|chunk| chunk == first_chunk)
        );
        assert!(
            !simulation
                .resident_chunks()
                .any(|chunk| chunk == second_chunk)
        );
        assert_eq!(
            simulation.effective_terrain_at(first_cell).unwrap(),
            first_changed
        );
    }
}
