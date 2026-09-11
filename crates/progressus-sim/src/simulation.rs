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

use progressus_content::{item, terrain};

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
use crate::item_world::ItemWorld;
use crate::job::{JobWorld, JobWorldError};
use crate::pathfinding::{PathfindingError, find_closest_explored_path, find_explored_path};
use crate::production::{ProductionWorld, ProductionWorldError};
use crate::production_logistics::{ProductionLogisticsWorld, ProductionLogisticsWorldError};
use crate::residency::ChunkResidency;
use crate::stockpile::{StockpileWorld, StockpileWorldError};
use crate::workstation_world::{WorkstationWorld, WorkstationWorldError};
use crate::world_state::ModifiedWorld;
use crate::{
    CHUNK_SIDE, CURRENT_WORLDGEN_VERSION, CapabilityId, Character, ChunkCoord,
    ConstructionMaterialState, ConstructionSite, Direction, EAT_WORK_TICKS, EffectiveChunk,
    EntityId, GeneratedChunk, HAND_LOAD_UNITS, HARVEST_WORK_TICKS, InteractionRadius, ItemId,
    ItemLocation, ItemQuantity, ItemStack, Job, JobKind, JobState, LocalCell, MAX_STACK_QUANTITY,
    MovementState, NaturalResource, ProductionLogistics, ProductionOrder, ProductionTarget,
    ProductionZoneKind, RecipeId, SATIETY_DECAY_INTERVAL_TICKS, SimulationTick, SlotId, Stockpile,
    Structure, StructureId, TerrainId, Workstation, WorkstationId, WorldCell, WorldPosition,
    WorldPositionError, WorldSeed, WorldgenVersion, within_interaction_range,
};

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
            if chunk.terrain_at(local) != Some(terrain::GRASS) {
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
            (item::WOOD, 8, WorldCell::new(-2, 0), 160, 180),
            (item::STONE, 6, WorldCell::new(-1, 0), 820, 220),
            (item::WOOD, 10, WorldCell::new(1, 0), 240, 820),
            (item::STONE, 8, WorldCell::new(2, 0), 840, 760),
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
            .filter(|cell| generator.terrain_at(*cell) == terrain::GRASS)
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
                item::BERRIES,
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
            self.maintain_equip_jobs()?;
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

    /// A cell the player has claimed by building or zoning on it. Placement
    /// already refuses to build over a resource, so this only matters in the
    /// other direction: an added worldgen layer must not grow a resource under
    /// something that already stands there. See ADR-0022.
    fn cell_is_claimed(&self, cell: WorldCell) -> bool {
        self.construction_world.structure_at(cell).is_some()
            || self.construction_world.site_at(cell).is_some()
            || self.stockpile_world.stockpile_at(cell).is_some()
            || self.workstation_world.workstation_at(cell).is_some()
            || self.production_logistics_world.zone_at(cell).is_some()
    }

    pub fn natural_resource_at(
        &self,
        position: WorldCell,
    ) -> Result<Option<NaturalResource>, SimulationError> {
        if self.depleted_resources.contains(&position)
            || self.renewable_resource_regrowth.contains_key(&position)
            || self.cell_is_claimed(position)
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
                    && !self.cell_is_claimed(cell)
                {
                    resources.push((cell, resource));
                }
            }
        }
        Ok(resources)
    }

    /// What this character has equipped, by slot.
    pub fn equipment(&self, character_id: EntityId) -> Vec<(SlotId, EntityId)> {
        self.item_world
            .equipped_by(character_id)
            .map(|(slot, item)| (slot, item.id()))
            .collect()
    }

    /// Whether this character is equipped for the work. Systems ask this
    /// rather than looking in a named slot, which is what keeps a new slot
    /// free to add. See ADR-0024.
    pub fn can_perform(&self, character_id: EntityId, capability: CapabilityId) -> bool {
        self.item_world
            .equipped_by(character_id)
            .any(|(_, item)| item.kind().provides(capability))
    }

    /// Whether this character is equipped for everything the work needs.
    pub fn meets_requirements(&self, character_id: EntityId, required: &[CapabilityId]) -> bool {
        required
            .iter()
            .all(|capability| self.can_perform(character_id, *capability))
    }

    /// Moves a carried stack into its slot. The item must declare a slot, the
    /// slot must be free, and the whole stack goes: equipment is not divisible.
    pub fn equip_item(
        &mut self,
        character_id: EntityId,
        item_id: EntityId,
    ) -> Result<(), SimulationError> {
        self.ensure_item_tree_unreserved(item_id)?;
        self.equip_item_for_job(character_id, item_id)
    }

    fn equip_item_for_job(
        &mut self,
        character_id: EntityId,
        item_id: EntityId,
    ) -> Result<(), SimulationError> {
        let item = self
            .item_world
            .get(item_id)
            .ok_or(SimulationError::UnknownItem(item_id))?;
        if item.quantity().get() != 1 {
            return Err(SimulationError::EquipmentStackMustBeSingle(item_id));
        }
        let slot = item
            .kind()
            .definition()
            .equip_slot
            .ok_or(SimulationError::ItemNotEquippable(item_id))?;
        if item.carrier() != Some(character_id) {
            return Err(SimulationError::ItemNotCarriedByCharacter {
                character_id,
                item_id,
            });
        }
        self.item_world
            .equip_carried(item_id, character_id, slot)
            .map_err(|_| SimulationError::SlotAlreadyOccupied {
                character_id,
                item_id,
            })
    }

    /// Returns equipment to its bearer's hands, where carrying capacity
    /// applies to it again.
    pub fn unequip_item(
        &mut self,
        character_id: EntityId,
        item_id: EntityId,
    ) -> Result<(), SimulationError> {
        self.ensure_item_tree_unreserved(item_id)?;
        let item = self
            .item_world
            .get(item_id)
            .ok_or(SimulationError::UnknownItem(item_id))?;
        if item.bearer().map(|(bearer, _)| bearer) != Some(character_id) {
            return Err(SimulationError::ItemNotCarriedByCharacter {
                character_id,
                item_id,
            });
        }
        let (kind, quantity) = (item.kind(), item.quantity().get());
        if self.carried_load(character_id) + kind.load_cost(quantity) > HAND_LOAD_UNITS {
            return Err(SimulationError::CarryCapacityExceeded {
                character_id,
                item_id,
            });
        }
        self.item_world
            .unequip_to_carried(item_id)
            .map_err(|_| SimulationError::JobInvariantViolation)
    }

    /// What this character already holds, as a share of one pair of hands.
    pub fn carried_load(&self, character_id: EntityId) -> u64 {
        self.item_world
            .iter()
            .filter(|item| {
                matches!(
                    item.location(),
                    ItemLocation::Carried {
                        character_id: carrier
                    } if carrier == character_id
                )
            })
            .map(|item| self.item_subtree_load(item.id()))
            .sum()
    }

    fn character_has_held_payload(&self, character_id: EntityId) -> bool {
        self.item_world.iter().any(|item| match item.location() {
            ItemLocation::Carried {
                character_id: carrier,
            } => carrier == character_id,
            ItemLocation::Contained { .. } => {
                self.item_world.holder_of(item.id()) == Some(character_id)
            }
            ItemLocation::Ground { .. } | ItemLocation::Equipped { .. } => false,
        })
    }

    /// The container this character is equipped with, and how much it holds.
    /// Hands and container are counted separately rather than summed: someone
    /// pushing a cart has their hands on it. See ADR-0024.
    pub fn equipped_container(&self, character_id: EntityId) -> Option<(EntityId, u64)> {
        self.item_world
            .equipped_by(character_id)
            .find_map(|(_, item)| {
                item.kind()
                    .capacity()
                    .map(|multiplier| (item.id(), u64::from(multiplier) * HAND_LOAD_UNITS))
            })
    }

    /// What a container already holds, in the same units as a pair of hands.
    pub fn container_load(&self, container_id: EntityId) -> u64 {
        self.item_world
            .contents_of(container_id)
            .map(|item| self.item_subtree_load(item.id()))
            .sum()
    }

    fn item_subtree_load(&self, item_id: EntityId) -> u64 {
        let Some(item) = self.item_world.get(item_id) else {
            return 0;
        };
        item.kind().load_cost(item.quantity().get()) + self.container_load(item_id)
    }

    /// How much of a stack this character could still take. Zero means their
    /// hands are full. See ADR-0024.
    pub fn pickup_capacity(&self, character_id: EntityId, kind: ItemId) -> u32 {
        // Goods go into the cart when there is one, and the cart's capacity is
        // what applies to them.
        let free = match self.equipped_container(character_id) {
            Some((container_id, capacity)) => {
                capacity.saturating_sub(self.container_load(container_id))
            }
            None => HAND_LOAD_UNITS.saturating_sub(self.carried_load(character_id)),
        };
        let per_unit = kind.load_cost(1);
        if per_unit == 0 {
            return MAX_STACK_QUANTITY;
        }
        u32::try_from((free / per_unit).min(u64::from(MAX_STACK_QUANTITY)))
            .expect("the quotient is clamped to a stack")
    }

    /// Takes as much of a reserved stack as one pair of hands allows and leaves
    /// the remainder where it lay. The reserved stack keeps its identity, so the
    /// job that reserved it stays valid, and the leftover becomes ordinary
    /// ground goods that a later trip collects. See ADR-0024.
    pub(super) fn pick_up_within_capacity(
        &mut self,
        worker_id: EntityId,
        item_id: EntityId,
    ) -> Result<(), SimulationError> {
        let item = self
            .item_world
            .get(item_id)
            .ok_or(SimulationError::UnknownItem(item_id))?;
        let (kind, quantity) = (item.kind(), item.quantity().get());
        let fits = self.pickup_capacity(worker_id, kind);
        if fits == 0 {
            return Err(SimulationError::CarryCapacityExceeded {
                character_id: worker_id,
                item_id,
            });
        }
        if fits < quantity {
            let leftover_id = self.id_allocator.allocate()?;
            self.item_world
                .split_ground_stack(item_id, leftover_id, quantity - fits)
                .map_err(|_| SimulationError::JobInvariantViolation)?;
        }
        match self.equipped_container(worker_id) {
            Some((container_id, _)) => {
                self.load_into_container_for_job(worker_id, item_id, container_id)
            }
            None => self.pick_up_item_for_job(worker_id, item_id),
        }
    }

    /// Puts a reachable ground stack into a container the character has hold
    /// of, or that rests on the ground beside them.
    pub fn load_into_container(
        &mut self,
        character_id: EntityId,
        item_id: EntityId,
        container_id: EntityId,
    ) -> Result<(), SimulationError> {
        self.ensure_item_tree_unreserved(item_id)?;
        self.ensure_item_unreserved(container_id)?;
        self.load_into_container_for_job(character_id, item_id, container_id)
    }

    fn load_into_container_for_job(
        &mut self,
        character_id: EntityId,
        item_id: EntityId,
        container_id: EntityId,
    ) -> Result<(), SimulationError> {
        let character = self
            .characters
            .get(&character_id)
            .ok_or(SimulationError::UnknownCharacter(character_id))?;
        let (position, radius) = (character.position(), character.interaction_radius());
        let source_item = self
            .item_world
            .get(item_id)
            .ok_or(SimulationError::UnknownItem(item_id))?;
        let item_position = source_item
            .ground_position()
            .ok_or(SimulationError::ItemNotOnGround(item_id))?;
        let container = self
            .item_world
            .get(container_id)
            .ok_or(SimulationError::UnknownItem(container_id))?;
        let capacity = container
            .kind()
            .capacity()
            .ok_or(SimulationError::ItemNotAContainer(container_id))?;
        if container.quantity().get() != 1 {
            return Err(SimulationError::ContainerStackMustBeSingle(container_id));
        }
        if source_item.kind() == item::CART && container.kind() == item::CART {
            return Err(SimulationError::ContainerNestingNotAllowed {
                item_id,
                container_id,
            });
        }
        // Reachable means borne by this character, or standing within reach.
        let reachable = match container.ground_position() {
            Some(container_position) => within_interaction_range(
                position,
                radius,
                container_position,
                InteractionRadius::zero(),
            ),
            None => self.item_world.holder_of(container_id) == Some(character_id),
        };
        if !reachable
            || !within_interaction_range(position, radius, item_position, InteractionRadius::zero())
        {
            return Err(SimulationError::ItemOutOfReach {
                character_id,
                item_id,
            });
        }
        if self.container_load(container_id) + self.item_subtree_load(item_id)
            > u64::from(capacity) * HAND_LOAD_UNITS
        {
            return Err(SimulationError::CarryCapacityExceeded {
                character_id,
                item_id,
            });
        }
        self.item_world
            .move_to_container(item_id, container_id)
            .map_err(|_| SimulationError::JobInvariantViolation)
    }

    pub fn pick_up_item(
        &mut self,
        character_id: EntityId,
        item_id: EntityId,
    ) -> Result<(), SimulationError> {
        self.ensure_item_unreserved(item_id)?;
        self.pick_up_item_for_job(character_id, item_id)
    }

    fn ensure_item_unreserved(&self, item_id: EntityId) -> Result<(), SimulationError> {
        if self.job_world.item_job_for_item(item_id).is_some() {
            Err(SimulationError::ItemReserved(item_id))
        } else {
            Ok(())
        }
    }

    fn ensure_item_tree_unreserved(&self, item_id: EntityId) -> Result<(), SimulationError> {
        self.ensure_item_unreserved(item_id)?;
        for child in self.item_world.contents_of(item_id) {
            self.ensure_item_tree_unreserved(child.id())?;
        }
        Ok(())
    }

    pub fn item_is_reserved(&self, item_id: EntityId) -> bool {
        self.job_world.item_job_for_item(item_id).is_some()
    }

    fn pick_up_item_for_job(
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
            ItemLocation::Carried { .. }
            | ItemLocation::Equipped { .. }
            | ItemLocation::Contained { .. } => {
                return Err(SimulationError::ItemNotOnGround(item_id));
            }
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
        let (kind, quantity) = (item.kind(), item.quantity().get());
        if self.carried_load(character_id) + kind.load_cost(quantity) > HAND_LOAD_UNITS {
            return Err(SimulationError::CarryCapacityExceeded {
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
        self.ensure_item_tree_unreserved(item_id)?;
        self.drop_item_for_job(character_id, item_id, destination)
    }

    fn drop_item_for_job(
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
        // Goods a character bears in a container count as theirs to put down.
        if !matches!(
            item.location(),
            ItemLocation::Carried { .. }
                | ItemLocation::Equipped { .. }
                | ItemLocation::Contained { .. }
        ) || self.item_world.holder_of(item_id) != Some(character_id)
        {
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

    pub fn unload_from_container(
        &mut self,
        character_id: EntityId,
        item_id: EntityId,
    ) -> Result<(), SimulationError> {
        self.ensure_item_tree_unreserved(item_id)?;
        let character = self
            .characters
            .get(&character_id)
            .ok_or(SimulationError::UnknownCharacter(character_id))?;
        let position = character.position();
        let radius = character.interaction_radius();
        let container_id = match self
            .item_world
            .get(item_id)
            .ok_or(SimulationError::UnknownItem(item_id))?
            .location()
        {
            ItemLocation::Contained { container_id } => container_id,
            _ => {
                return Err(SimulationError::ItemNotCarriedByCharacter {
                    character_id,
                    item_id,
                });
            }
        };
        let container = self
            .item_world
            .get(container_id)
            .ok_or(SimulationError::UnknownItem(container_id))?;
        let reachable = match container.ground_position() {
            Some(container_position) => within_interaction_range(
                position,
                radius,
                container_position,
                InteractionRadius::zero(),
            ),
            None => self.item_world.holder_of(container_id) == Some(character_id),
        };
        if !reachable {
            return Err(SimulationError::ItemOutOfReach {
                character_id,
                item_id,
            });
        }
        self.item_world
            .move_contained_to_ground(item_id, container_id, position)
            .map_err(|_| SimulationError::JobInvariantViolation)
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

    pub(crate) fn structure_kind_at(&self, cell: WorldCell) -> Option<StructureId> {
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
        terrain: TerrainId,
    ) -> Result<(), SimulationError> {
        let (coordinate, local) = position.split();
        let base = self.base_terrain_at(position)?;
        self.modified_world
            .set_override(coordinate, local, base, terrain);
        Ok(())
    }

    pub fn effective_terrain_at(&self, position: WorldCell) -> Result<TerrainId, SimulationError> {
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

    fn base_terrain_at(&self, position: WorldCell) -> Result<TerrainId, SimulationError> {
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

    fn resolve_terrain(
        &self,
        coordinate: ChunkCoord,
        local: LocalCell,
        base: TerrainId,
    ) -> TerrainId {
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

    /// The whole point of a cart: one trip moves four times what hands do, and
    /// the goods ride in the cart rather than in the bearer's arms.
    #[test]
    fn a_cart_carries_four_times_what_hands_do() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let worker = cora();
        let position = simulation.characters[&worker].position();
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

        // Bare hands hold ten wood.
        assert_eq!(simulation.pickup_capacity(worker, item::WOOD), 10);

        let cart = place(&mut simulation, item::CART, 1);
        simulation.pick_up_item(worker, cart).unwrap();
        simulation.equip_item(worker, cart).unwrap();
        assert_eq!(
            simulation.pickup_capacity(worker, item::WOOD),
            40,
            "a cart at four should hold four pairs of hands worth of wood"
        );
        assert_eq!(simulation.pickup_capacity(worker, item::STONE), 20);

        // Loading goes into the cart, not into the arms.
        let load = place(&mut simulation, item::WOOD, 40);
        simulation.pick_up_within_capacity(worker, load).unwrap();
        assert_eq!(
            simulation.item_world.get(load).unwrap().container(),
            Some(cart)
        );
        assert_eq!(
            simulation.carried_load(worker),
            0,
            "the load ended up in the bearer's hands"
        );
        assert_eq!(simulation.item_world.holder_of(load), Some(worker));
        assert_eq!(simulation.pickup_capacity(worker, item::WOOD), 0);

        // A full cart takes what fits and leaves the rest behind.
        let extra = place(&mut simulation, item::WOOD, 5);
        assert!(matches!(
            simulation.pick_up_within_capacity(worker, extra),
            Err(SimulationError::CarryCapacityExceeded { .. })
        ));
        assert_eq!(simulation.item_world.get(extra).unwrap().container(), None);

        // Unloading puts goods on the ground straight from the cart, because
        // a cartload could never fit through a pair of hands.
        simulation.drop_item(worker, load, position).unwrap();
        assert!(
            simulation
                .item_world
                .get(load)
                .unwrap()
                .ground_position()
                .is_some()
        );
        assert_eq!(simulation.container_load(cart), 0);
        assert!(simulation.item_world.indexes_are_consistent());
    }

    /// A cart may be loaded where it stands, without anyone holding it.
    #[test]
    fn a_parked_cart_can_be_loaded_and_keeps_its_goods() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let worker = cora();
        let position = simulation.characters[&worker].position();
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
        let goods = place(&mut simulation, item::STONE, 6);

        simulation.load_into_container(worker, goods, cart).unwrap();
        assert_eq!(
            simulation.item_world.get(goods).unwrap().container(),
            Some(cart)
        );
        assert_eq!(
            simulation.item_world.holder_of(goods),
            None,
            "goods in a parked cart came to belong to somebody"
        );
        assert!(simulation.item_world.indexes_are_consistent());
    }
    use progressus_content::{capability, slot};

    /// Equipment is borne, not carried: a tool in a slot must not eat into
    /// what its bearer can still pick up, or equipping would be a punishment.
    #[test]
    fn equipment_does_not_consume_carrying_capacity() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let character = cora();
        let position = simulation.characters[&character].position();
        let tool = simulation.id_allocator.allocate().unwrap();
        simulation
            .item_world
            .insert_ground(ItemStack::new_ground(
                tool,
                item::PRIMITIVE_TOOL,
                ItemQuantity::new(1).unwrap(),
                position,
            ))
            .unwrap();

        simulation.pick_up_item(character, tool).unwrap();
        let carrying_the_tool = simulation.carried_load(character);
        assert!(carrying_the_tool > 0);

        simulation.equip_item(character, tool).unwrap();
        assert_eq!(
            simulation.carried_load(character),
            0,
            "an equipped tool still weighed on its bearer's hands"
        );
        assert_eq!(simulation.equipment(character), vec![(slot::TOOL, tool)]);
        assert!(simulation.item_world.indexes_are_consistent());
    }

    /// Requirements are answered by capability, never by looking in a named
    /// slot. That rule is what keeps a future slot free to add; see ADR-0024.
    #[test]
    fn capability_comes_from_equipment_and_not_from_holding_the_tool() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let character = cora();
        let position = simulation.characters[&character].position();
        let tool = simulation.id_allocator.allocate().unwrap();
        simulation
            .item_world
            .insert_ground(ItemStack::new_ground(
                tool,
                item::PRIMITIVE_TOOL,
                ItemQuantity::new(1).unwrap(),
                position,
            ))
            .unwrap();

        assert!(!simulation.can_perform(character, capability::MINE));
        simulation.pick_up_item(character, tool).unwrap();
        assert!(
            !simulation.can_perform(character, capability::MINE),
            "merely holding a pick is not being equipped to mine"
        );
        simulation.equip_item(character, tool).unwrap();
        assert!(simulation.can_perform(character, capability::MINE));
        assert!(simulation.meets_requirements(character, &[capability::MINE]));
        assert!(simulation.meets_requirements(character, &[]));

        simulation.unequip_item(character, tool).unwrap();
        assert!(!simulation.can_perform(character, capability::MINE));
        assert!(simulation.item_world.indexes_are_consistent());
    }

    #[test]
    fn dropping_equipment_atomically_parks_it_on_the_ground() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let character = cora();
        let position = simulation.characters[&character].position();
        let tool = simulation.id_allocator.allocate().unwrap();
        simulation
            .item_world
            .insert_ground(ItemStack::new_ground(
                tool,
                item::PRIMITIVE_TOOL,
                ItemQuantity::new(1).unwrap(),
                position,
            ))
            .unwrap();
        simulation.pick_up_item(character, tool).unwrap();
        simulation.equip_item(character, tool).unwrap();

        simulation.drop_item(character, tool, position).unwrap();
        assert_eq!(
            simulation.item_world.get(tool).unwrap().ground_position(),
            Some(position)
        );
        assert!(simulation.equipment(character).is_empty());
        assert!(simulation.item_world.indexes_are_consistent());
    }

    #[test]
    fn manual_pickup_cannot_steal_an_item_reserved_by_a_job() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let character = cora();
        let position = simulation.characters[&character].position();
        let item_id = simulation.id_allocator.allocate().unwrap();
        simulation
            .item_world
            .insert_ground(ItemStack::new_ground(
                item_id,
                item::WOOD,
                ItemQuantity::new(1).unwrap(),
                position,
            ))
            .unwrap();
        let job_id = simulation.id_allocator.allocate().unwrap();
        simulation
            .job_world
            .insert(Job::new(
                job_id,
                JobKind::Haul {
                    item_id,
                    stockpile_id: EntityId::new(100).unwrap(),
                    destination: position.containing_cell(),
                },
            ))
            .unwrap();

        assert_eq!(
            simulation.pick_up_item(character, item_id),
            Err(SimulationError::ItemReserved(item_id))
        );
        assert_eq!(
            simulation
                .item_world
                .get(item_id)
                .unwrap()
                .ground_position(),
            Some(position)
        );
        assert_eq!(
            simulation.job_world.item_job_for_item(item_id),
            Some(job_id)
        );
    }

    #[test]
    fn loading_uses_the_same_item_reach_as_pickup() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let character = cora();
        let character_position = simulation.characters[&character].position();
        let outside_reach = character_position
            .checked_translate(
                i128::from(
                    simulation.characters[&character]
                        .interaction_radius()
                        .subunits(),
                ) + 1,
                0,
            )
            .unwrap();
        let place = |simulation: &mut Simulation, kind, quantity, position| {
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
        let cart = place(&mut simulation, item::CART, 1, character_position);
        let goods = place(&mut simulation, item::WOOD, 1, outside_reach);

        assert!(matches!(
            simulation.pick_up_item(character, goods),
            Err(SimulationError::ItemOutOfReach { .. })
        ));
        assert!(matches!(
            simulation.load_into_container(character, goods, cart),
            Err(SimulationError::ItemOutOfReach { .. })
        ));
        assert_eq!(
            simulation.item_world.get(goods).unwrap().ground_position(),
            Some(outside_reach)
        );
    }

    #[test]
    fn a_character_can_unload_a_reachable_parked_cart() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let character = cora();
        let position = simulation.characters[&character].position();
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
        let goods = place(&mut simulation, item::STONE, 3);
        simulation
            .load_into_container(character, goods, cart)
            .unwrap();

        simulation.unload_from_container(character, goods).unwrap();

        assert_eq!(
            simulation.item_world.get(goods).unwrap().ground_position(),
            Some(position)
        );
        assert_eq!(simulation.container_load(cart), 0);
        assert!(simulation.item_world.indexes_are_consistent());
    }

    #[test]
    fn carts_cannot_be_stacked_or_nested_to_bypass_capacity() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let character = cora();
        let position = simulation.characters[&character].position();
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
        let outer = place(&mut simulation, item::CART, 1);
        let nested = place(&mut simulation, item::CART, 1);
        assert!(matches!(
            simulation.load_into_container(character, nested, outer),
            Err(SimulationError::ContainerNestingNotAllowed { .. })
        ));

        let stacked = place(&mut simulation, item::CART, 2);
        let goods = place(&mut simulation, item::WOOD, 1);
        assert_eq!(
            simulation.load_into_container(character, goods, stacked),
            Err(SimulationError::ContainerStackMustBeSingle(stacked))
        );
    }

    #[test]
    fn equipment_slots_hold_one_physical_item_not_a_stack() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let character = cora();
        let position = simulation.characters[&character].position();
        let tools = simulation.id_allocator.allocate().unwrap();
        simulation
            .item_world
            .insert_ground(ItemStack::new_ground(
                tools,
                item::PRIMITIVE_TOOL,
                ItemQuantity::new(2).unwrap(),
                position,
            ))
            .unwrap();
        simulation.pick_up_item(character, tools).unwrap();

        assert_eq!(
            simulation.equip_item(character, tools),
            Err(SimulationError::EquipmentStackMustBeSingle(tools))
        );
        assert!(simulation.equipment(character).is_empty());
    }

    /// A slot holds one item, and equipment survives a save without becoming a
    /// second copy or falling on the floor.
    #[test]
    fn a_slot_holds_one_item_and_equipment_round_trips_through_a_save() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let character = cora();
        let position = simulation.characters[&character].position();
        let make_tool = |simulation: &mut Simulation| {
            let id = simulation.id_allocator.allocate().unwrap();
            simulation
                .item_world
                .insert_ground(ItemStack::new_ground(
                    id,
                    item::PRIMITIVE_TOOL,
                    ItemQuantity::new(1).unwrap(),
                    position,
                ))
                .unwrap();
            id
        };
        let first = make_tool(&mut simulation);
        let second = make_tool(&mut simulation);

        simulation.pick_up_item(character, first).unwrap();
        simulation.equip_item(character, first).unwrap();
        simulation.pick_up_item(character, second).unwrap();
        assert!(
            matches!(
                simulation.equip_item(character, second),
                Err(SimulationError::SlotAlreadyOccupied { .. })
            ),
            "a second tool went into an occupied slot"
        );

        let total_before = simulation.item_world.iter().count();
        let bytes = simulation.save_json().unwrap();
        let reloaded = Simulation::load_json(&bytes).unwrap();
        assert_eq!(reloaded.equipment(character), vec![(slot::TOOL, first)]);
        assert!(reloaded.can_perform(character, capability::MINE));
        assert_eq!(
            reloaded.item_world.iter().count(),
            total_before,
            "the save changed how many stacks exist"
        );
        assert!(reloaded.item_world.indexes_are_consistent());
    }
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
            .set_terrain_override(WorldCell::new(21, 0), terrain::GRASS)
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
        assert_eq!(items[0].kind(), item::WOOD);
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
        assert_eq!(items[3].kind(), item::STONE);
        assert_eq!(items[3].quantity().get(), 8);
        assert_eq!(items[4].id(), EntityId::new(10).unwrap());
        assert_eq!(items[4].kind(), item::BERRIES);
        assert_eq!(items[4].quantity().get(), BOOTSTRAP_BERRIES);
        let berries_cell = items[4].ground_position().unwrap().containing_cell();
        assert!(berries_cell.x().abs() > 2 || berries_cell.y().abs() > 2);
        assert!(simulation.is_explored(berries_cell));
        assert_eq!(
            simulation.effective_terrain_at(berries_cell).unwrap(),
            terrain::GRASS
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
            .set_terrain_override(blocked_cell, terrain::ROCK)
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
            .set_terrain_override(position, terrain::ROCK)
            .unwrap();
        simulation.base_terrain_query_count.set(0);

        assert_eq!(
            simulation.effective_terrain_at(position).unwrap(),
            terrain::ROCK
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

        let changed = |base| {
            TerrainId::all()
                .find(|candidate| *candidate != base)
                .expect("the terrain registry defines more than one kind")
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
