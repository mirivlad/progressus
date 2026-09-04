#[cfg(test)]
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

mod persistence;
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

    fn maintain_idle_behavior(&mut self) -> Result<(), SimulationError> {
        let tick = self.clock.tick().value();
        let seed = self.generator.seed().value();
        let character_ids = self.characters.keys().copied().collect::<Vec<_>>();

        for character_id in character_ids {
            let eligible = self.characters.get(&character_id).is_some_and(|character| {
                character.movement() == MovementState::Idle
                    && !character.is_hungry()
                    && self.job_world.job_for_worker(character_id).is_none()
                    && !self
                        .item_world
                        .iter()
                        .any(|item| item.carrier() == Some(character_id))
            });
            if !eligible {
                continue;
            }

            let phase = 16 + character_id.value().wrapping_mul(5) % 24;
            if tick % IDLE_BEHAVIOR_INTERVAL_TICKS != phase {
                continue;
            }

            let cycle = tick / IDLE_BEHAVIOR_INTERVAL_TICKS;
            let entropy = idle_entropy(seed, character_id.value(), cycle);
            let route = if entropy % IDLE_SOCIAL_CHANCE_DIVISOR == 0 {
                self.plan_idle_social_route(character_id, entropy)?
                    .or(self.plan_idle_wander_route(character_id, entropy)?)
            } else {
                self.plan_idle_wander_route(character_id, entropy)?
            };

            if let Some(route) = route {
                self.characters
                    .get_mut(&character_id)
                    .expect("idle character is still present")
                    .set_wandering_route(route);
            }
        }

        Ok(())
    }

    fn plan_idle_social_route(
        &self,
        character_id: EntityId,
        entropy: u64,
    ) -> Result<Option<NavigationRoute>, SimulationError> {
        let Some(character) = self.characters.get(&character_id) else {
            return Err(SimulationError::UnknownCharacter(character_id));
        };
        let current = character.position().containing_cell();
        let anchor = character.idle_anchor();
        let mut companions = self
            .characters
            .values()
            .filter(|other| other.id() != character_id)
            .filter(|other| other.movement() == MovementState::Idle)
            .filter(|other| !other.is_hungry())
            .filter(|other| self.job_world.job_for_worker(other.id()).is_none())
            .map(|other| {
                (
                    cell_manhattan_distance(current, other.position().containing_cell()),
                    other.id(),
                    other.position().containing_cell(),
                )
            })
            .filter(|(distance, ..)| *distance <= 6)
            .collect::<Vec<_>>();
        companions.sort_unstable();
        if companions.is_empty() {
            return Ok(None);
        }

        let start = (entropy as usize) % companions.len();
        let directions = [
            Direction::East,
            Direction::North,
            Direction::West,
            Direction::South,
        ];
        for offset in 0..companions.len() {
            let (_, _, companion_cell) = companions[(start + offset) % companions.len()];
            let direction_start =
                ((entropy >> (8 + offset.min(6) * 4)) as usize) % directions.len();
            for direction_offset in 0..directions.len() {
                let direction = directions[(direction_start + direction_offset) % directions.len()];
                let Some(destination) = direction.adjacent(companion_cell) else {
                    continue;
                };
                if !idle_cell_within_anchor(anchor, destination) {
                    continue;
                }
                if self.character_occupies_cell(character_id, destination) {
                    continue;
                }
                if let Some(route) = self.plan_idle_route_to_cell(character_id, destination)? {
                    return Ok(Some(route));
                }
            }
        }
        Ok(None)
    }

    fn plan_idle_wander_route(
        &self,
        character_id: EntityId,
        entropy: u64,
    ) -> Result<Option<NavigationRoute>, SimulationError> {
        let anchor = self
            .characters
            .get(&character_id)
            .ok_or(SimulationError::UnknownCharacter(character_id))?
            .idle_anchor();

        for attempt in 0..IDLE_DESTINATION_ATTEMPTS {
            let mixed =
                mix_idle_entropy(entropy.wrapping_add(attempt.wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            let dx = (mixed % 7) as i64 - IDLE_WANDER_RADIUS_CELLS;
            let dy = ((mixed >> 11) % 7) as i64 - IDLE_WANDER_RADIUS_CELLS;
            if dx == 0 && dy == 0 || dx.abs() + dy.abs() > IDLE_WANDER_RADIUS_CELLS {
                continue;
            }
            let Some(x) = anchor.x().checked_add(dx) else {
                continue;
            };
            let Some(y) = anchor.y().checked_add(dy) else {
                continue;
            };
            let destination = WorldCell::new(x, y);
            if self.character_occupies_cell(character_id, destination) {
                continue;
            }
            if let Some(route) = self.plan_idle_route_to_cell(character_id, destination)? {
                return Ok(Some(route));
            }
        }
        Ok(None)
    }

    fn plan_idle_route_to_cell(
        &self,
        character_id: EntityId,
        destination: WorldCell,
    ) -> Result<Option<NavigationRoute>, SimulationError> {
        let character = self
            .characters
            .get(&character_id)
            .ok_or(SimulationError::UnknownCharacter(character_id))?;
        let current_position = character.position();
        let current = current_position.containing_cell();
        let anchor = character.idle_anchor();
        if current == destination
            || !idle_cell_within_anchor(anchor, destination)
            || !self.is_explored(destination)
            || !self.is_walkable(destination)?
        {
            return Ok(None);
        }

        let cells = match find_explored_path(self, current, destination)? {
            Ok(cells) => cells,
            Err(PathfindingError::PathNotFound | PathfindingError::SearchBudgetExceeded) => {
                return Ok(None);
            }
        };
        if !cells
            .iter()
            .copied()
            .all(|cell| idle_cell_within_anchor(anchor, cell))
        {
            return Ok(None);
        }

        let destination_position = WorldPosition::from_cell_center(destination)?;
        Ok(Some(NavigationRoute {
            destination: destination_position,
            waypoints: build_waypoints(current_position, destination_position, &cells)?,
        }))
    }

    fn character_occupies_cell(&self, character_id: EntityId, cell: WorldCell) -> bool {
        self.characters.values().any(|character| {
            character.id() != character_id && character.position().containing_cell() == cell
        })
    }

    fn maintain_doors(&mut self) -> Result<(), SimulationError> {
        let occupied_cells = self
            .characters
            .values()
            .map(|character| character.position().containing_cell())
            .collect::<BTreeSet<_>>();
        self.construction_world
            .maintain_doors(self.clock.tick(), &occupied_cells)
            .map_err(SimulationError::from_construction_world)
    }

    fn decay_satiety_if_due(&mut self) {
        if self.clock.tick().value() % SATIETY_DECAY_INTERVAL_TICKS != 0 {
            return;
        }
        for character in self.characters.values_mut() {
            character.decay_satiety();
        }
    }

    fn maintain_nutrition_jobs(&mut self) -> Result<(), SimulationError> {
        let eat_jobs = self
            .job_world
            .iter()
            .filter_map(|job| matches!(job.kind(), JobKind::Eat { .. }).then_some(job.id()))
            .collect::<Vec<_>>();
        for job_id in eat_jobs {
            let Some(job) = self.job_world.get(job_id).cloned() else {
                continue;
            };
            let JobKind::Eat {
                character_id,
                item_id,
            } = job.kind()
            else {
                continue;
            };
            let valid_character = self
                .characters
                .get(&character_id)
                .is_some_and(Character::is_hungry);
            let valid_food = self.item_world.get(item_id).is_some_and(|item| {
                item.kind() == ItemKind::Berries && item.ground_position().is_some()
            });
            if !valid_character || !valid_food {
                self.cancel_job(job_id)?;
            }
        }

        let character_ids = self.characters.keys().copied().collect::<Vec<_>>();
        for character_id in character_ids {
            let Some(character) = self.characters.get(&character_id) else {
                continue;
            };
            if !character.is_hungry()
                || self.job_world.eat_job_for_character(character_id).is_some()
            {
                continue;
            }

            let current_job = self.job_world.job_for_worker(character_id);
            if current_job.is_none() && !character.is_available_for_work() {
                continue;
            }

            let character_cell = character.position().containing_cell();
            let mut candidates = self
                .item_world
                .iter()
                .filter_map(|item| {
                    let position = item.ground_position()?;
                    (item.kind() == ItemKind::Berries
                        && self.is_explored(position.containing_cell())
                        && self.job_world.item_job_for_item(item.id()).is_none())
                    .then_some((
                        cell_manhattan_distance(character_cell, position.containing_cell()),
                        item.id(),
                        position,
                        item.quantity().get(),
                    ))
                })
                .collect::<Vec<_>>();
            candidates.sort_unstable_by_key(|(distance, item_id, ..)| (*distance, *item_id));

            let mut chosen = None;
            for (_, item_id, position, quantity) in candidates {
                let route = match self.plan_navigation_route(character_id, position) {
                    Ok(route) => route,
                    Err(
                        SimulationError::MoveToDestinationBlocked(_)
                        | SimulationError::MoveToDestinationUndiscovered(_)
                        | SimulationError::MoveToPathNotFound
                        | SimulationError::MoveToSearchBudgetExceeded,
                    ) => continue,
                    Err(error) => return Err(error),
                };
                chosen = Some((item_id, position, quantity, route));
                break;
            }

            let Some((source_item_id, position, quantity, route)) = chosen else {
                let starving = self
                    .characters
                    .get(&character_id)
                    .is_some_and(Character::is_starving);
                let wandering = self.characters.get(&character_id).is_some_and(|character| {
                    matches!(character.movement(), MovementState::Wandering { .. })
                });
                if starving && (current_job.is_some() || wandering) {
                    if current_job.is_some() {
                        self.interrupt_worker_job(character_id)?;
                    }
                    self.characters
                        .get_mut(&character_id)
                        .expect("starving character is still present")
                        .set_movement(MovementState::Idle);
                }
                self.ensure_renewable_food_harvest(character_id)?;
                continue;
            };

            if current_job.is_some() {
                self.interrupt_worker_job(character_id)?;
            }

            let meal_item_id = if quantity > 1 {
                let split_id = self.id_allocator.allocate()?;
                self.item_world
                    .split_ground_stack(source_item_id, split_id, 1)
                    .map_err(|_| SimulationError::JobInvariantViolation)?;
                split_id
            } else {
                source_item_id
            };
            debug_assert_eq!(
                self.item_world
                    .get(meal_item_id)
                    .and_then(ItemStack::ground_position),
                Some(position)
            );
            let job_id = self.id_allocator.allocate()?;
            self.job_world
                .insert(Job::new(
                    job_id,
                    JobKind::Eat {
                        character_id,
                        item_id: meal_item_id,
                    },
                ))
                .map_err(SimulationError::from_job_world)?;
            self.job_world
                .reserve_worker(job_id, character_id)
                .map_err(SimulationError::from_job_world)?;
            self.apply_navigation_route(character_id, position, route);
        }
        Ok(())
    }

    fn ensure_renewable_food_harvest(
        &mut self,
        character_id: EntityId,
    ) -> Result<bool, SimulationError> {
        let character_cell = self
            .characters
            .get(&character_id)
            .ok_or(SimulationError::UnknownCharacter(character_id))?
            .position()
            .containing_cell();
        // Autonomous need satisfaction is deliberately local to the bootstrap
        // settlement. Scanning every explored cell each tick both scales with
        // explored-world size and lets a chain of newly discovered wild bushes
        // become accidental free scouting. Manual Harvest remains unrestricted.
        let mut candidates = Vec::new();
        for y in -AUTONOMOUS_FORAGE_RADIUS_CELLS..=AUTONOMOUS_FORAGE_RADIUS_CELLS {
            for x in -AUTONOMOUS_FORAGE_RADIUS_CELLS..=AUTONOMOUS_FORAGE_RADIUS_CELLS {
                if x.abs() + y.abs() > AUTONOMOUS_FORAGE_RADIUS_CELLS {
                    continue;
                }
                let cell = WorldCell::new(x, y);
                if !self.is_explored(cell) {
                    continue;
                }
                let Some(resource) = self.natural_resource_at(cell)? else {
                    continue;
                };
                if resource.kind() == NaturalResourceKind::BerryBush
                    && self.job_world.harvest_job_for_source(cell).is_none()
                {
                    candidates.push((cell_manhattan_distance(character_cell, cell), cell));
                }
            }
        }
        candidates.sort_unstable();
        for (_, source) in candidates {
            let destination = WorldPosition::from_cell_center(source)?;
            match self.plan_navigation_route(character_id, destination) {
                Ok(_) => {
                    self.designate_harvest(source)?;
                    return Ok(true);
                }
                Err(
                    SimulationError::MoveToDestinationBlocked(_)
                    | SimulationError::MoveToDestinationUndiscovered(_)
                    | SimulationError::MoveToPathNotFound
                    | SimulationError::MoveToSearchBudgetExceeded,
                ) => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(false)
    }

    pub fn set_movement_direction(
        &mut self,
        id: EntityId,
        direction: Direction,
    ) -> Result<(), SimulationError> {
        if !self.characters.contains_key(&id) {
            return Err(SimulationError::UnknownCharacter(id));
        }
        self.interrupt_worker_job(id)?;
        self.characters
            .get_mut(&id)
            .expect("character was checked above")
            .set_movement(MovementState::ManualDirectional { direction });
        Ok(())
    }

    pub fn move_to(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<(), SimulationError> {
        let route = self.plan_player_navigation_route(id, destination)?;
        self.interrupt_worker_job(id)?;
        self.apply_navigation_route(id, destination, route);
        Ok(())
    }

    pub fn stop_movement(&mut self, id: EntityId) -> Result<(), SimulationError> {
        if !self.characters.contains_key(&id) {
            return Err(SimulationError::UnknownCharacter(id));
        }
        self.interrupt_worker_job(id)?;
        self.characters
            .get_mut(&id)
            .expect("character was checked above")
            .set_movement(MovementState::Idle);
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

    pub fn designate_harvest(&mut self, source: WorldCell) -> Result<EntityId, SimulationError> {
        if !self.is_explored(source) {
            return Err(SimulationError::HarvestSourceUndiscovered(source));
        }
        if self.natural_resource_at(source)?.is_none() {
            return Err(SimulationError::NaturalResourceMissing(source));
        }
        if self.job_world.harvest_job_for_source(source).is_some() {
            return Err(SimulationError::HarvestAlreadyDesignated(source));
        }
        let id = self.id_allocator.allocate()?;
        self.job_world
            .insert(Job::new(id, JobKind::Harvest { source }))
            .map_err(SimulationError::from_job_world)?;
        Ok(id)
    }

    pub fn cancel_job(&mut self, job_id: EntityId) -> Result<(), SimulationError> {
        let job = self
            .job_world
            .get(job_id)
            .cloned()
            .ok_or(SimulationError::UnknownJob(job_id))?;
        if let JobState::Transporting { worker_id } = job.state() {
            let item_id = match job.kind() {
                JobKind::Haul { item_id, .. }
                | JobKind::SupplyProduction { item_id, .. }
                | JobKind::DeliverConstruction { item_id, .. } => Some(item_id),
                JobKind::Harvest { .. }
                | JobKind::Eat { .. }
                | JobKind::Craft { .. }
                | JobKind::Construct { .. } => None,
            };
            if let Some(item_id) = item_id {
                let position = self
                    .characters
                    .get(&worker_id)
                    .ok_or(SimulationError::UnknownCharacter(worker_id))?
                    .position();
                self.drop_item(worker_id, item_id, position)?;
                if let JobKind::DeliverConstruction { site_id, .. } = job.kind() {
                    self.construction_world
                        .mark_material_reserved(site_id, item_id)
                        .map_err(SimulationError::from_construction_world)?;
                }
            }
        }
        let worker = job.state().worker();
        self.job_world
            .remove(job_id)
            .map_err(SimulationError::from_job_world)?;
        if let Some(worker_id) = worker
            && let Some(character) = self.characters.get_mut(&worker_id)
        {
            character.set_movement(MovementState::Idle);
        }
        Ok(())
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
        kind: ItemKind,
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

    fn validate_stockpile_cell(&self, cell: WorldCell) -> Result<(), SimulationError> {
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

    fn cycle_workstation_ports(
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

    fn validate_production_zone_cell(&self, cell: WorldCell) -> Result<(), SimulationError> {
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

    pub fn workstation_at(&self, cell: WorldCell) -> Option<EntityId> {
        self.workstation_world.workstation_at(cell)
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

    fn default_workbench_ports(
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

    fn validate_workstation_cell(&self, cell: WorldCell) -> Result<(), SimulationError> {
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

    pub fn designate_construction(
        &mut self,
        kind: StructureKind,
        cell: WorldCell,
    ) -> Result<EntityId, SimulationError> {
        if kind == StructureKind::Door {
            if let Some(site_id) = self.construction_world.site_at(cell)
                && self
                    .construction_world
                    .site(site_id)
                    .is_some_and(|site| site.kind() == StructureKind::StoneWall)
            {
                self.cancel_construction(site_id)?;
            }
            if let Some(structure_id) = self.construction_world.structure_at(cell)
                && self
                    .construction_world
                    .structure(structure_id)
                    .is_some_and(|structure| structure.kind() == StructureKind::StoneWall)
            {
                self.construction_world
                    .remove_structure(structure_id)
                    .map_err(SimulationError::from_construction_world)?;
            }
        }

        self.validate_construction_cell(cell)?;
        if self.construction_world.site_at(cell).is_some()
            || self.construction_world.structure_at(cell).is_some()
        {
            return Err(SimulationError::ConstructionCellOccupied(cell));
        }
        let id = self.id_allocator.allocate()?;
        self.construction_world
            .insert_site(ConstructionSite::new(id, kind, cell))
            .map_err(SimulationError::from_construction_world)?;
        self.ensure_construction_job(id)?;
        Ok(id)
    }

    pub fn cancel_construction(&mut self, site_id: EntityId) -> Result<(), SimulationError> {
        if self.construction_world.site(site_id).is_none() {
            return Err(SimulationError::UnknownConstructionSite(site_id));
        }
        if let Some(job_id) = self.job_world.construction_delivery_job_for_site(site_id) {
            self.cancel_job(job_id)?;
        }
        if let Some(job_id) = self.job_world.construct_job_for_site(site_id) {
            self.cancel_job(job_id)?;
        }
        self.construction_world
            .release_material(site_id)
            .map_err(SimulationError::from_construction_world)?;
        self.construction_world
            .remove_site(site_id)
            .map_err(SimulationError::from_construction_world)?;
        Ok(())
    }

    fn validate_construction_cell(&self, cell: WorldCell) -> Result<(), SimulationError> {
        if !self.is_explored(cell) {
            return Err(SimulationError::ConstructionCellUndiscovered(cell));
        }
        if !self.is_walkable(cell)? {
            return Err(SimulationError::ConstructionCellBlocked(cell));
        }
        if self.natural_resource_at(cell)?.is_some()
            || self.stockpile_world.stockpile_at(cell).is_some()
            || self.workstation_world.workstation_at(cell).is_some()
            || self.production_logistics_world.zone_at(cell).is_some()
            || self
                .characters
                .values()
                .any(|character| character.position().containing_cell() == cell)
            || self.item_world.iter().any(|item| {
                item.ground_position()
                    .is_some_and(|position| position.containing_cell() == cell)
            })
        {
            return Err(SimulationError::ConstructionCellOccupied(cell));
        }
        Ok(())
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

    fn plan_player_navigation_route(
        &self,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<Option<NavigationRoute>, SimulationError> {
        let current = self
            .characters
            .get(&id)
            .ok_or(SimulationError::UnknownCharacter(id))?
            .position();
        if current == destination {
            return Ok(None);
        }

        let start = current.containing_cell();
        let goal = destination.containing_cell();
        if self.is_explored(goal) && self.is_walkable(goal)? {
            match find_explored_path(self, start, goal)? {
                Ok(cells) => {
                    return Ok(Some(NavigationRoute {
                        destination,
                        waypoints: build_waypoints(current, destination, &cells)?,
                    }));
                }
                Err(PathfindingError::SearchBudgetExceeded) => {
                    return Err(SimulationError::MoveToSearchBudgetExceeded);
                }
                Err(PathfindingError::PathNotFound) => {}
            }
        }

        let cells =
            find_closest_explored_path(self, start, goal)?.map_err(|error| match error {
                PathfindingError::PathNotFound => SimulationError::MoveToPathNotFound,
                PathfindingError::SearchBudgetExceeded => {
                    SimulationError::MoveToSearchBudgetExceeded
                }
            })?;
        let frontier = *cells.last().expect("closest explored path contains start");
        if frontier == start {
            return Ok(None);
        }
        let segment_destination = WorldPosition::from_cell_center(frontier)?;
        Ok(Some(NavigationRoute {
            destination,
            waypoints: build_waypoints(current, segment_destination, &cells)?,
        }))
    }

    fn plan_navigation_route(
        &self,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<Option<NavigationRoute>, SimulationError> {
        let current = self
            .characters
            .get(&id)
            .ok_or(SimulationError::UnknownCharacter(id))?
            .position();
        if current == destination {
            return Ok(None);
        }
        if !self.is_explored(destination.containing_cell()) {
            return Err(SimulationError::MoveToDestinationUndiscovered(
                destination.containing_cell(),
            ));
        }
        if !self.is_walkable(destination.containing_cell())? {
            return Err(SimulationError::MoveToDestinationBlocked(
                destination.containing_cell(),
            ));
        }
        let cells = find_explored_path(
            self,
            current.containing_cell(),
            destination.containing_cell(),
        )?
        .map_err(|error| match error {
            PathfindingError::PathNotFound => SimulationError::MoveToPathNotFound,
            PathfindingError::SearchBudgetExceeded => SimulationError::MoveToSearchBudgetExceeded,
        })?;
        Ok(Some(NavigationRoute {
            destination,
            waypoints: build_waypoints(current, destination, &cells)?,
        }))
    }

    fn apply_navigation_route(
        &mut self,
        id: EntityId,
        _destination: WorldPosition,
        route: Option<NavigationRoute>,
    ) {
        let character = self
            .characters
            .get_mut(&id)
            .expect("navigation routes are applied only to known characters");
        match route {
            Some(route) => character.set_navigation_route(route),
            None => {
                character.set_movement(MovementState::Idle);
            }
        }
    }

    fn interrupt_worker_job(&mut self, worker_id: EntityId) -> Result<(), SimulationError> {
        let Some(job_id) = self.job_world.job_for_worker(worker_id) else {
            return Ok(());
        };
        let job = self
            .job_world
            .get(job_id)
            .cloned()
            .ok_or(SimulationError::UnknownJob(job_id))?;
        if matches!(job.kind(), JobKind::Eat { .. }) {
            self.cancel_job(job_id)?;
            return Ok(());
        }
        if let JobState::Transporting { .. } = job.state() {
            let item_id = match job.kind() {
                JobKind::Haul { item_id, .. }
                | JobKind::SupplyProduction { item_id, .. }
                | JobKind::DeliverConstruction { item_id, .. } => Some(item_id),
                JobKind::Harvest { .. }
                | JobKind::Eat { .. }
                | JobKind::Craft { .. }
                | JobKind::Construct { .. } => None,
            };
            if let Some(item_id) = item_id {
                let position = self
                    .characters
                    .get(&worker_id)
                    .ok_or(SimulationError::UnknownCharacter(worker_id))?
                    .position();
                self.drop_item(worker_id, item_id, position)?;
                if let JobKind::DeliverConstruction { site_id, .. } = job.kind() {
                    self.construction_world
                        .mark_material_reserved(site_id, item_id)
                        .map_err(SimulationError::from_construction_world)?;
                }
            }
        }
        self.job_world
            .release_worker(job_id)
            .map_err(SimulationError::from_job_world)?;
        Ok(())
    }

    fn stockpile_destination_accepts_item(
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

    fn stockpile_merge_capacity(
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

    fn stockpile_merge_target(
        &self,
        item_id: EntityId,
        destination: WorldCell,
    ) -> Option<EntityId> {
        let item = self.item_world.get(item_id)?;
        let (target_id, capacity) = self.stockpile_merge_capacity(item_id, destination)?;
        (item.quantity().get() <= capacity).then_some(target_id)
    }

    fn stockpile_destination_for_item(
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

    fn stockpile_consolidation_destination(
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

    fn maintain_construction_jobs(&mut self) -> Result<(), SimulationError> {
        let site_ids = self
            .construction_world
            .sites()
            .map(ConstructionSite::id)
            .collect::<Vec<_>>();
        for site_id in site_ids {
            self.ensure_construction_job(site_id)?;
        }
        Ok(())
    }

    fn ensure_construction_job(&mut self, site_id: EntityId) -> Result<(), SimulationError> {
        let Some(site) = self.construction_world.site(site_id).cloned() else {
            return Ok(());
        };
        match site.material_state() {
            None => {
                let Some(item_id) = self.select_construction_material(&site) else {
                    return Ok(());
                };
                self.construction_world
                    .reserve_material(site_id, item_id)
                    .map_err(SimulationError::from_construction_world)?;
                self.create_construction_delivery_job(site_id, item_id)?;
            }
            Some(ConstructionMaterialState::Reserved) => {
                let Some(item_id) = site.material_item_id() else {
                    return Err(SimulationError::ConstructionInvariantViolation);
                };
                let valid = self.item_world.get(item_id).is_some_and(|item| {
                    item.kind() == site.kind().material_kind()
                        && item.quantity().get() >= site.kind().material_quantity()
                });
                if !valid {
                    self.construction_world
                        .release_material(site_id)
                        .map_err(SimulationError::from_construction_world)?;
                    return Ok(());
                }
                if self
                    .job_world
                    .construction_delivery_job_for_site(site_id)
                    .is_none()
                {
                    self.create_construction_delivery_job(site_id, item_id)?;
                }
            }
            Some(ConstructionMaterialState::Delivered) => {
                let Some(item_id) = site.material_item_id() else {
                    return Err(SimulationError::ConstructionInvariantViolation);
                };
                let delivered = self.item_world.get(item_id).is_some_and(|item| {
                    item.kind() == site.kind().material_kind()
                        && item.quantity().get() >= site.kind().material_quantity()
                        && item.ground_position().is_some_and(|position| {
                            cell_manhattan_distance(position.containing_cell(), site.cell()) <= 1
                        })
                });
                if !delivered {
                    self.construction_world
                        .mark_material_reserved(site_id, item_id)
                        .map_err(SimulationError::from_construction_world)?;
                    return Ok(());
                }
                if self.job_world.construct_job_for_site(site_id).is_none() {
                    let job_id = self.id_allocator.allocate()?;
                    self.job_world
                        .insert(Job::new(job_id, JobKind::Construct { site_id }))
                        .map_err(SimulationError::from_job_world)?;
                }
            }
        }
        Ok(())
    }

    fn create_construction_delivery_job(
        &mut self,
        site_id: EntityId,
        item_id: EntityId,
    ) -> Result<(), SimulationError> {
        let job_id = self.id_allocator.allocate()?;
        self.job_world
            .insert(Job::new(
                job_id,
                JobKind::DeliverConstruction { site_id, item_id },
            ))
            .map_err(SimulationError::from_job_world)?;
        Ok(())
    }

    fn select_construction_material(&self, site: &ConstructionSite) -> Option<EntityId> {
        let mut candidates = self
            .item_world
            .iter()
            .filter_map(|item| {
                let position = item.ground_position()?;
                (item.kind() == site.kind().material_kind()
                    && item.quantity().get() >= site.kind().material_quantity()
                    && self.is_explored(position.containing_cell())
                    && self.job_world.item_job_for_item(item.id()).is_none()
                    && self
                        .construction_world
                        .site_for_material(item.id())
                        .is_none())
                .then_some((
                    cell_manhattan_distance(position.containing_cell(), site.cell()),
                    item.id(),
                ))
            })
            .collect::<Vec<_>>();
        candidates.sort_unstable();
        candidates.first().map(|(_, id)| *id)
    }

    fn construction_access_cell(
        &self,
        site_cell: WorldCell,
    ) -> Result<Option<WorldCell>, SimulationError> {
        for direction in [
            Direction::East,
            Direction::North,
            Direction::South,
            Direction::West,
        ] {
            let Some(cell) = direction.adjacent(site_cell) else {
                continue;
            };
            if self.is_explored(cell)
                && self.is_walkable(cell)?
                && self.construction_world.site_at(cell).is_none()
                && self.workstation_world.workstation_at(cell).is_none()
            {
                return Ok(Some(cell));
            }
        }
        Ok(None)
    }

    fn construction_access_position(
        &self,
        site_cell: WorldCell,
        access_cell: WorldCell,
    ) -> Result<WorldPosition, SimulationError> {
        let center = WorldPosition::from_cell_center(access_cell)?;
        let dx = access_cell.x() - site_cell.x();
        let dy = access_cell.y() - site_cell.y();
        match (dx, dy) {
            (1, 0) => Ok(center.checked_translate(-256, 0)?),
            (-1, 0) => Ok(center.checked_translate(256, 0)?),
            (0, 1) => Ok(center.checked_translate(0, -256)?),
            (0, -1) => Ok(center.checked_translate(0, 256)?),
            _ => Err(SimulationError::ConstructionInvariantViolation),
        }
    }

    fn craft_local_quantity(&self, workstation_id: EntityId, kind: ItemKind) -> u32 {
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

    fn craft_incoming_quantity(&self, workstation_id: EntityId, kind: ItemKind) -> u32 {
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

    fn production_zone_destination(
        &self,
        workstation_id: EntityId,
        zone_kind: ProductionZoneKind,
        item_kind: ItemKind,
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

    fn craft_supply_source(
        &self,
        workstation_id: EntityId,
        kind: ItemKind,
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

    fn maintain_craft_supply_jobs(&mut self) -> Result<(), SimulationError> {
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
            let recipe = recipe_definition(recipe_id);
            for requirement in recipe.inputs {
                let local = self.craft_local_quantity(workstation_id, requirement.kind);
                let incoming = self.craft_incoming_quantity(workstation_id, requirement.kind);
                let mut missing = requirement
                    .quantity
                    .saturating_sub(local.saturating_add(incoming));
                while missing > 0 {
                    let Some((source_id, source_quantity)) =
                        self.craft_supply_source(workstation_id, requirement.kind)
                    else {
                        break;
                    };
                    let move_quantity = missing.min(source_quantity);
                    let Some(destination) = self.production_zone_destination(
                        workstation_id,
                        ProductionZoneKind::Input,
                        requirement.kind,
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

    fn item_is_local_input_for_waiting_craft(&self, item_id: EntityId) -> bool {
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
                && recipe_definition(recipe_id)
                    .inputs
                    .iter()
                    .any(|requirement| requirement.kind == item.kind())
        })
    }

    fn maintain_haul_jobs(&mut self) -> Result<(), SimulationError> {
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

    fn maintain_craft_jobs(&mut self) -> Result<(), SimulationError> {
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

    fn ensure_craft_job_for_workstation(
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
        if workstation.kind() != recipe_definition(order.recipe_id()).workstation {
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

    fn advance_jobs_one_tick(&mut self) -> Result<(), SimulationError> {
        let job_ids = self.job_world.iter().map(Job::id).collect::<Vec<_>>();
        for job_id in job_ids {
            let Some(job) = self.job_world.get(job_id).cloned() else {
                continue;
            };
            match job.state() {
                JobState::Available => self.try_assign_job(job_id, job.kind())?,
                JobState::Reserved { worker_id } => {
                    self.advance_reserved_job(job_id, job.kind(), worker_id)?
                }
                JobState::Transporting { worker_id } => {
                    self.advance_transporting_job(job_id, job.kind(), worker_id)?
                }
                JobState::Working {
                    worker_id,
                    remaining_ticks,
                } => self.advance_working_job(job_id, job.kind(), worker_id, remaining_ticks)?,
            }
        }
        Ok(())
    }

    fn try_assign_job(&mut self, job_id: EntityId, kind: JobKind) -> Result<(), SimulationError> {
        match kind {
            JobKind::Harvest { source } => self.try_assign_harvest(job_id, source),
            JobKind::Eat {
                character_id,
                item_id,
            } => self.try_assign_eat(job_id, character_id, item_id),
            JobKind::Haul {
                item_id,
                stockpile_id,
                destination,
            } => self.try_assign_haul(job_id, item_id, stockpile_id, destination),
            JobKind::Craft {
                workstation_id,
                order_id,
                recipe_id,
            } => self.try_assign_craft(job_id, workstation_id, order_id, recipe_id),
            JobKind::SupplyProduction {
                workstation_id,
                item_id,
                destination,
            } => self.try_assign_production_supply(job_id, workstation_id, item_id, destination),
            JobKind::DeliverConstruction { site_id, item_id } => {
                self.try_assign_construction_delivery(job_id, site_id, item_id)
            }
            JobKind::Construct { site_id } => self.try_assign_construct(job_id, site_id),
        }
    }

    fn available_workers_by_distance(&self, target: WorldCell) -> Vec<EntityId> {
        let mut candidates = self
            .characters
            .values()
            .filter(|character| {
                character.is_available_for_work()
                    && !character.is_starving()
                    && self.job_world.job_for_worker(character.id()).is_none()
                    && !self
                        .item_world
                        .iter()
                        .any(|item| item.carrier() == Some(character.id()))
            })
            .map(|character| {
                (
                    cell_manhattan_distance(character.position().containing_cell(), target),
                    character.id(),
                )
            })
            .collect::<Vec<_>>();
        candidates.sort_unstable();
        candidates.into_iter().map(|(_, id)| id).collect()
    }

    fn try_assign_eat(
        &mut self,
        job_id: EntityId,
        character_id: EntityId,
        item_id: EntityId,
    ) -> Result<(), SimulationError> {
        let Some(character) = self.characters.get(&character_id) else {
            self.cancel_job(job_id)?;
            return Ok(());
        };
        if !character.is_hungry()
            || !character.is_available_for_work()
            || self.job_world.job_for_worker(character_id).is_some()
        {
            return Ok(());
        }
        let Some(item_position) = self.item_world.get(item_id).and_then(|item| {
            (item.kind() == ItemKind::Berries)
                .then(|| item.ground_position())
                .flatten()
        }) else {
            self.cancel_job(job_id)?;
            return Ok(());
        };
        let route = match self.plan_navigation_route(character_id, item_position) {
            Ok(route) => route,
            Err(
                SimulationError::MoveToDestinationBlocked(_)
                | SimulationError::MoveToDestinationUndiscovered(_)
                | SimulationError::MoveToPathNotFound
                | SimulationError::MoveToSearchBudgetExceeded,
            ) => return Ok(()),
            Err(error) => return Err(error),
        };
        self.job_world
            .reserve_worker(job_id, character_id)
            .map_err(SimulationError::from_job_world)?;
        self.apply_navigation_route(character_id, item_position, route);
        Ok(())
    }

    fn try_assign_harvest(
        &mut self,
        job_id: EntityId,
        source: WorldCell,
    ) -> Result<(), SimulationError> {
        if self.natural_resource_at(source)?.is_none() {
            self.job_world
                .remove(job_id)
                .map_err(SimulationError::from_job_world)?;
            return Ok(());
        }
        let destination = WorldPosition::from_cell_center(source)?;
        for worker_id in self.available_workers_by_distance(source) {
            let route = match self.plan_navigation_route(worker_id, destination) {
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
            self.apply_navigation_route(worker_id, destination, route);
            return Ok(());
        }
        Ok(())
    }

    fn try_assign_haul(
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

    fn try_assign_production_supply(
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

    fn try_assign_craft(
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
        let recipe = recipe_definition(recipe_id);
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
                recipe.output_kind,
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

    fn try_assign_construction_delivery(
        &mut self,
        job_id: EntityId,
        site_id: EntityId,
        item_id: EntityId,
    ) -> Result<(), SimulationError> {
        let Some(site) = self.construction_world.site(site_id) else {
            self.job_world
                .remove(job_id)
                .map_err(SimulationError::from_job_world)?;
            return Ok(());
        };
        if site.material_item_id() != Some(item_id)
            || site.material_state() != Some(ConstructionMaterialState::Reserved)
        {
            self.job_world
                .remove(job_id)
                .map_err(SimulationError::from_job_world)?;
            return Ok(());
        }
        let Some(item) = self.item_world.get(item_id) else {
            self.construction_world
                .release_material(site_id)
                .map_err(SimulationError::from_construction_world)?;
            self.job_world
                .remove(job_id)
                .map_err(SimulationError::from_job_world)?;
            return Ok(());
        };
        let Some(item_position) = item.ground_position() else {
            return Ok(());
        };
        if item.kind() != site.kind().material_kind()
            || item.quantity().get() < site.kind().material_quantity()
        {
            self.construction_world
                .release_material(site_id)
                .map_err(SimulationError::from_construction_world)?;
            self.job_world
                .remove(job_id)
                .map_err(SimulationError::from_job_world)?;
            return Ok(());
        }
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

    fn try_assign_construct(
        &mut self,
        job_id: EntityId,
        site_id: EntityId,
    ) -> Result<(), SimulationError> {
        let Some(site) = self.construction_world.site(site_id) else {
            self.job_world
                .remove(job_id)
                .map_err(SimulationError::from_job_world)?;
            return Ok(());
        };
        if site.material_state() != Some(ConstructionMaterialState::Delivered) {
            self.job_world
                .remove(job_id)
                .map_err(SimulationError::from_job_world)?;
            return Ok(());
        }
        let Some(access_cell) = self.construction_access_cell(site.cell())? else {
            return Ok(());
        };
        let target = self.construction_access_position(site.cell(), access_cell)?;
        for worker_id in self.available_workers_by_distance(access_cell) {
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
            self.apply_navigation_route(worker_id, target, route);
            return Ok(());
        }
        Ok(())
    }

    fn select_craft_inputs(
        &self,
        workstation_id: EntityId,
        recipe_id: RecipeId,
    ) -> Option<Vec<EntityId>> {
        let logistics = self.production_logistics_world.get(workstation_id)?;
        let recipe = recipe_definition(recipe_id);
        let mut selected = BTreeSet::new();
        for requirement in recipe.inputs {
            let mut remaining = requirement.quantity;
            for item in self.item_world.iter() {
                if selected.contains(&item.id())
                    || item.kind() != requirement.kind
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

    fn craft_reserved_inputs_valid(
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
        let recipe = recipe_definition(recipe_id);
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
                    if item.kind() != requirement.kind {
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

    fn craft_consumption_plan(
        &self,
        job_id: EntityId,
        recipe_id: RecipeId,
    ) -> Option<Vec<(EntityId, u32)>> {
        let reserved = self.job_world.craft_reserved_items(job_id)?;
        let recipe = recipe_definition(recipe_id);
        let mut plan = Vec::new();
        for requirement in recipe.inputs {
            let mut remaining = requirement.quantity;
            for item_id in reserved {
                let item = self.item_world.get(*item_id)?;
                if item.kind() != requirement.kind || item.ground_position().is_none() {
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

    fn advance_reserved_job(
        &mut self,
        job_id: EntityId,
        kind: JobKind,
        worker_id: EntityId,
    ) -> Result<(), SimulationError> {
        match kind {
            JobKind::Harvest { source } => {
                if self.natural_resource_at(source)?.is_none() {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                let Some(character) = self.characters.get(&worker_id) else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                let target = WorldPosition::from_cell_center(source)?;
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    target,
                    InteractionRadius::zero(),
                ) {
                    self.characters
                        .get_mut(&worker_id)
                        .expect("worker was checked above")
                        .set_movement(MovementState::Idle);
                    self.job_world
                        .start_working(job_id, HARVEST_WORK_TICKS)
                        .map_err(SimulationError::from_job_world)?;
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::Eat {
                character_id,
                item_id,
            } => {
                if character_id != worker_id
                    || !self
                        .characters
                        .get(&character_id)
                        .is_some_and(Character::is_hungry)
                {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                let Some(item_position) = self.item_world.get(item_id).and_then(|item| {
                    (item.kind() == ItemKind::Berries)
                        .then(|| item.ground_position())
                        .flatten()
                }) else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                let character = self
                    .characters
                    .get(&worker_id)
                    .expect("eat worker was validated above");
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    item_position,
                    InteractionRadius::zero(),
                ) {
                    self.characters
                        .get_mut(&worker_id)
                        .expect("eat worker is still present")
                        .set_movement(MovementState::Idle);
                    self.job_world
                        .start_working(job_id, EAT_WORK_TICKS)
                        .map_err(SimulationError::from_job_world)?;
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::Haul {
                item_id,
                stockpile_id,
                destination,
            } => {
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
                let Some(character) = self.characters.get(&worker_id) else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    item_position,
                    InteractionRadius::zero(),
                ) {
                    let target = WorldPosition::from_cell_center(destination)?;
                    let route = match self.plan_navigation_route(worker_id, target) {
                        Ok(route) => route,
                        Err(
                            SimulationError::MoveToDestinationBlocked(_)
                            | SimulationError::MoveToDestinationUndiscovered(_)
                            | SimulationError::MoveToPathNotFound
                            | SimulationError::MoveToSearchBudgetExceeded,
                        ) => {
                            self.job_world
                                .release_worker(job_id)
                                .map_err(SimulationError::from_job_world)?;
                            return Ok(());
                        }
                        Err(error) => return Err(error),
                    };
                    self.pick_up_item(worker_id, item_id)?;
                    self.job_world
                        .start_transporting(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    self.apply_navigation_route(worker_id, target, route);
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::SupplyProduction {
                workstation_id,
                item_id,
                destination,
            } => {
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
                let Some(character) = self.characters.get(&worker_id) else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    item_position,
                    InteractionRadius::zero(),
                ) {
                    let target = WorldPosition::from_cell_center(destination)?;
                    let route = match self.plan_navigation_route(worker_id, target) {
                        Ok(route) => route,
                        Err(
                            SimulationError::MoveToDestinationBlocked(_)
                            | SimulationError::MoveToDestinationUndiscovered(_)
                            | SimulationError::MoveToPathNotFound
                            | SimulationError::MoveToSearchBudgetExceeded,
                        ) => {
                            self.job_world
                                .release_worker(job_id)
                                .map_err(SimulationError::from_job_world)?;
                            return Ok(());
                        }
                        Err(error) => return Err(error),
                    };
                    self.pick_up_item(worker_id, item_id)?;
                    self.job_world
                        .start_transporting(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    self.apply_navigation_route(worker_id, target, route);
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::Craft {
                workstation_id,
                order_id,
                recipe_id,
            } => {
                if self.production_world.get(order_id).is_none_or(|order| {
                    order.workstation_id() != workstation_id
                        || order.recipe_id() != recipe_id
                        || !order.is_pending()
                }) {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                if !self.craft_reserved_inputs_valid(job_id, workstation_id, recipe_id) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                let Some(workstation) = self.workstation_world.get(workstation_id) else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                if !self.is_walkable(workstation.cell())? {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                let Some(character) = self.characters.get(&worker_id) else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                let target = WorldPosition::from_cell_center(workstation.cell())?;
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    target,
                    InteractionRadius::zero(),
                ) {
                    self.characters
                        .get_mut(&worker_id)
                        .expect("worker was checked above")
                        .set_movement(MovementState::Idle);
                    self.job_world
                        .start_working(job_id, recipe_definition(recipe_id).work_ticks)
                        .map_err(SimulationError::from_job_world)?;
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::DeliverConstruction { site_id, item_id } => {
                let Some(site) = self.construction_world.site(site_id).cloned() else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                if site.material_item_id() != Some(item_id)
                    || site.material_state() != Some(ConstructionMaterialState::Reserved)
                {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                let Some(item_position) = self
                    .item_world
                    .get(item_id)
                    .and_then(ItemStack::ground_position)
                else {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                let Some(character) = self.characters.get(&worker_id) else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    item_position,
                    InteractionRadius::zero(),
                ) {
                    let Some(access_cell) = self.construction_access_cell(site.cell())? else {
                        self.job_world
                            .release_worker(job_id)
                            .map_err(SimulationError::from_job_world)?;
                        return Ok(());
                    };
                    let target = self.construction_access_position(site.cell(), access_cell)?;
                    let route = match self.plan_navigation_route(worker_id, target) {
                        Ok(route) => route,
                        Err(
                            SimulationError::MoveToDestinationBlocked(_)
                            | SimulationError::MoveToDestinationUndiscovered(_)
                            | SimulationError::MoveToPathNotFound
                            | SimulationError::MoveToSearchBudgetExceeded,
                        ) => {
                            self.job_world
                                .release_worker(job_id)
                                .map_err(SimulationError::from_job_world)?;
                            return Ok(());
                        }
                        Err(error) => return Err(error),
                    };
                    self.pick_up_item(worker_id, item_id)?;
                    self.job_world
                        .start_transporting(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    self.apply_navigation_route(worker_id, target, route);
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::Construct { site_id } => {
                let Some(site) = self.construction_world.site(site_id).cloned() else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                if site.material_state() != Some(ConstructionMaterialState::Delivered) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                let Some(access_cell) = self.construction_access_cell(site.cell())? else {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                let target = self.construction_access_position(site.cell(), access_cell)?;
                let Some(character) = self.characters.get(&worker_id) else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    WorldPosition::from_cell_center(site.cell())?,
                    InteractionRadius::zero(),
                ) {
                    self.characters
                        .get_mut(&worker_id)
                        .expect("worker was checked above")
                        .set_movement(MovementState::Idle);
                    self.job_world
                        .start_working(job_id, site.kind().work_ticks())
                        .map_err(SimulationError::from_job_world)?;
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    let route = self.plan_navigation_route(worker_id, target)?;
                    self.apply_navigation_route(worker_id, target, route);
                }
            }
        }
        Ok(())
    }

    fn advance_transporting_job(
        &mut self,
        job_id: EntityId,
        kind: JobKind,
        worker_id: EntityId,
    ) -> Result<(), SimulationError> {
        match kind {
            JobKind::Haul {
                item_id,
                stockpile_id,
                destination,
            } => {
                if self.stockpile_world.stockpile_at(destination) != Some(stockpile_id) {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                let Some(character) = self.characters.get(&worker_id) else {
                    return Err(SimulationError::JobInvariantViolation);
                };
                if self.item_world.get(item_id).and_then(ItemStack::carrier) != Some(worker_id) {
                    return Err(SimulationError::JobInvariantViolation);
                }
                let target = WorldPosition::from_cell_center(destination)?;
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    target,
                    InteractionRadius::zero(),
                ) {
                    let merge_target = self.stockpile_merge_target(item_id, destination);
                    self.drop_item(worker_id, item_id, target)?;
                    if let Some(target_id) = merge_target {
                        self.item_world
                            .merge_ground_stacks(target_id, item_id)
                            .expect("haul destination capacity was validated before delivery");
                    }
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    self.characters
                        .get_mut(&worker_id)
                        .expect("worker was checked above")
                        .set_movement(MovementState::Idle);
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    let position = character.position();
                    self.drop_item(worker_id, item_id, position)?;
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::SupplyProduction {
                workstation_id,
                item_id,
                destination,
            } => {
                if self.production_logistics_world.zone_at(destination)
                    != Some((workstation_id, ProductionZoneKind::Input))
                {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                let Some(character) = self.characters.get(&worker_id) else {
                    return Err(SimulationError::JobInvariantViolation);
                };
                if self.item_world.get(item_id).and_then(ItemStack::carrier) != Some(worker_id) {
                    return Err(SimulationError::JobInvariantViolation);
                }
                let target = WorldPosition::from_cell_center(destination)?;
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    target,
                    InteractionRadius::zero(),
                ) {
                    let merge_target = self.stockpile_merge_target(item_id, destination);
                    self.drop_item(worker_id, item_id, target)?;
                    if let Some(target_id) = merge_target {
                        self.item_world
                            .merge_ground_stacks(target_id, item_id)
                            .map_err(|_| SimulationError::JobInvariantViolation)?;
                    }
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    self.characters
                        .get_mut(&worker_id)
                        .expect("worker was checked above")
                        .set_movement(MovementState::Idle);
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    let position = character.position();
                    self.drop_item(worker_id, item_id, position)?;
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::DeliverConstruction { site_id, item_id } => {
                let Some(site) = self.construction_world.site(site_id).cloned() else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                if self.item_world.get(item_id).and_then(ItemStack::carrier) != Some(worker_id) {
                    return Err(SimulationError::ConstructionInvariantViolation);
                }
                let Some(character) = self.characters.get(&worker_id) else {
                    return Err(SimulationError::ConstructionInvariantViolation);
                };
                let Some(access_cell) = self.construction_access_cell(site.cell())? else {
                    let position = character.position();
                    self.drop_item(worker_id, item_id, position)?;
                    self.construction_world
                        .mark_material_reserved(site_id, item_id)
                        .map_err(SimulationError::from_construction_world)?;
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                let target = self.construction_access_position(site.cell(), access_cell)?;
                if within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    target,
                    InteractionRadius::zero(),
                ) {
                    self.drop_item(worker_id, item_id, target)?;
                    self.construction_world
                        .mark_material_delivered(site_id, item_id)
                        .map_err(SimulationError::from_construction_world)?;
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    self.characters
                        .get_mut(&worker_id)
                        .expect("worker was checked above")
                        .set_movement(MovementState::Idle);
                    self.ensure_construction_job(site_id)?;
                } else if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    let position = character.position();
                    self.drop_item(worker_id, item_id, position)?;
                    self.construction_world
                        .mark_material_reserved(site_id, item_id)
                        .map_err(SimulationError::from_construction_world)?;
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                }
            }
            JobKind::Harvest { .. }
            | JobKind::Eat { .. }
            | JobKind::Craft { .. }
            | JobKind::Construct { .. } => {
                return Err(SimulationError::JobInvariantViolation);
            }
        }
        Ok(())
    }

    fn advance_working_job(
        &mut self,
        job_id: EntityId,
        kind: JobKind,
        worker_id: EntityId,
        remaining_ticks: u32,
    ) -> Result<(), SimulationError> {
        match kind {
            JobKind::Harvest { source } => {
                let Some(resource) = self.natural_resource_at(source)? else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                if !self.characters.contains_key(&worker_id) {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                if remaining_ticks > 1 {
                    self.job_world
                        .set_remaining_work(job_id, remaining_ticks - 1)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                self.complete_harvest(job_id, worker_id, source, resource)?;
            }
            JobKind::Eat {
                character_id,
                item_id,
            } => {
                if character_id != worker_id {
                    return Err(SimulationError::JobInvariantViolation);
                }
                let Some(character) = self.characters.get(&character_id) else {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                };
                let Some(item_position) = self.item_world.get(item_id).and_then(|item| {
                    (item.kind() == ItemKind::Berries)
                        .then(|| item.ground_position())
                        .flatten()
                }) else {
                    self.cancel_job(job_id)?;
                    return Ok(());
                };
                if !within_interaction_range(
                    character.position(),
                    character.interaction_radius(),
                    item_position,
                    InteractionRadius::zero(),
                ) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                if remaining_ticks > 1 {
                    self.job_world
                        .set_remaining_work(job_id, remaining_ticks - 1)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                self.item_world
                    .consume(item_id, 1)
                    .map_err(|_| SimulationError::JobInvariantViolation)?;
                self.characters
                    .get_mut(&character_id)
                    .expect("eat worker is still present")
                    .restore_satiety(BERRIES_MEAL_SATIETY);
                self.job_world
                    .remove(job_id)
                    .map_err(SimulationError::from_job_world)?;
                self.characters
                    .get_mut(&character_id)
                    .expect("eat worker is still present")
                    .set_movement(MovementState::Idle);
            }
            JobKind::Haul { .. } | JobKind::SupplyProduction { .. } => {
                return Err(SimulationError::JobInvariantViolation);
            }
            JobKind::Craft {
                workstation_id,
                order_id,
                recipe_id,
            } => {
                if self.production_world.get(order_id).is_none_or(|order| {
                    order.workstation_id() != workstation_id
                        || order.recipe_id() != recipe_id
                        || !order.is_pending()
                }) {
                    self.cancel_job(job_id)?;
                    return Ok(());
                }
                if !self.craft_reserved_inputs_valid(job_id, workstation_id, recipe_id) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                if !self.characters.contains_key(&worker_id) {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                if remaining_ticks > 1 {
                    self.job_world
                        .set_remaining_work(job_id, remaining_ticks - 1)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                self.complete_craft(job_id, worker_id, workstation_id, order_id, recipe_id)?;
            }
            JobKind::DeliverConstruction { .. } => {
                return Err(SimulationError::ConstructionInvariantViolation);
            }
            JobKind::Construct { site_id } => {
                if !self.construction_site_ready(site_id) {
                    self.job_world
                        .release_worker(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                if !self.characters.contains_key(&worker_id) {
                    self.job_world
                        .remove(job_id)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                if remaining_ticks > 1 {
                    self.job_world
                        .set_remaining_work(job_id, remaining_ticks - 1)
                        .map_err(SimulationError::from_job_world)?;
                    return Ok(());
                }
                self.complete_construction(job_id, worker_id, site_id)?;
            }
        }
        Ok(())
    }

    fn construction_site_ready(&self, site_id: EntityId) -> bool {
        let Some(site) = self.construction_world.site(site_id) else {
            return false;
        };
        if site.material_state() != Some(ConstructionMaterialState::Delivered) {
            return false;
        }
        let Some(item_id) = site.material_item_id() else {
            return false;
        };
        self.item_world.get(item_id).is_some_and(|item| {
            item.kind() == site.kind().material_kind()
                && item.quantity().get() >= site.kind().material_quantity()
                && item.ground_position().is_some_and(|position| {
                    cell_manhattan_distance(position.containing_cell(), site.cell()) <= 1
                })
        })
    }

    fn complete_construction(
        &mut self,
        job_id: EntityId,
        worker_id: EntityId,
        site_id: EntityId,
    ) -> Result<(), SimulationError> {
        if !self.construction_site_ready(site_id) {
            return Err(SimulationError::ConstructionInvariantViolation);
        }
        let site = self
            .construction_world
            .site(site_id)
            .cloned()
            .ok_or(SimulationError::UnknownConstructionSite(site_id))?;
        let item_id = site
            .material_item_id()
            .ok_or(SimulationError::ConstructionInvariantViolation)?;
        self.item_world
            .consume(item_id, site.kind().material_quantity())
            .map_err(|_| SimulationError::ConstructionInvariantViolation)?;
        self.job_world
            .remove(job_id)
            .map_err(SimulationError::from_job_world)?;
        self.construction_world
            .complete_site(site_id)
            .map_err(SimulationError::from_construction_world)?;
        if let Some(character) = self.characters.get_mut(&worker_id) {
            character.set_movement(MovementState::Idle);
        }
        Ok(())
    }

    fn complete_craft(
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
        let recipe = recipe_definition(recipe_id);
        let plan = self
            .craft_consumption_plan(job_id, recipe_id)
            .ok_or(SimulationError::JobInvariantViolation)?;
        let output_cell = self
            .production_zone_destination(
                workstation_id,
                ProductionZoneKind::Output,
                recipe.output_kind,
                recipe.output_quantity,
            )?
            .ok_or(SimulationError::ProductionOutputBlocked(workstation_id))?;
        let merge_target = self.item_world.iter().find_map(|item| {
            (item.kind() == recipe.output_kind
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
                recipe.output_kind,
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

    fn complete_harvest(
        &mut self,
        job_id: EntityId,
        worker_id: EntityId,
        source: WorldCell,
        resource: NaturalResource,
    ) -> Result<(), SimulationError> {
        let next_resource_revision = self
            .resource_revision
            .checked_add(1)
            .ok_or(SimulationError::ResourceRevisionOverflow)?;
        let renewable_ready_tick = if resource.kind() == NaturalResourceKind::BerryBush {
            Some(SimulationTick::new(
                self.clock
                    .tick()
                    .value()
                    .checked_add(BERRY_BUSH_REGROW_TICKS)
                    .ok_or(SimulationError::TickOverflow)?,
            ))
        } else {
            None
        };
        let item_id = self.id_allocator.allocate()?;
        let kind = match resource.kind() {
            NaturalResourceKind::Tree => ItemKind::Wood,
            NaturalResourceKind::StoneOutcrop => ItemKind::Stone,
            NaturalResourceKind::BerryBush => ItemKind::Berries,
        };
        let quantity = ItemQuantity::new(resource.yield_quantity())
            .expect("worldgen natural-resource yields are positive");
        let position = WorldPosition::from_cell_center(source)?;
        self.item_world
            .insert_ground(ItemStack::new_ground(item_id, kind, quantity, position))
            .expect("allocated item IDs are unique and harvested outputs start on the ground");
        match resource.kind() {
            NaturalResourceKind::BerryBush => {
                if self
                    .renewable_resource_regrowth
                    .insert(
                        source,
                        renewable_ready_tick.expect("berry bushes precompute a regrowth tick"),
                    )
                    .is_some()
                {
                    return Err(SimulationError::JobInvariantViolation);
                }
            }
            NaturalResourceKind::Tree | NaturalResourceKind::StoneOutcrop => {
                if !self.depleted_resources.insert(source) {
                    return Err(SimulationError::JobInvariantViolation);
                }
            }
        }
        self.resource_revision = next_resource_revision;
        self.job_world
            .remove(job_id)
            .map_err(SimulationError::from_job_world)?;
        if let Some(character) = self.characters.get_mut(&worker_id) {
            character.set_movement(MovementState::Idle);
        }
        Ok(())
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

    fn advance_characters_one_tick(&mut self) -> Result<(), SimulationError> {
        let ids = self.characters.keys().copied().collect::<Vec<_>>();
        for id in &ids {
            let character = self
                .characters
                .get_mut(id)
                .expect("character ID came from the character map");
            character.set_last_tick_motion_trace(vec![character.position()]);
        }
        for id in ids {
            let (mut position, direction, mut remaining) = {
                let character = self
                    .characters
                    .get(&id)
                    .expect("character ID came from the character map");
                match character.movement() {
                    MovementState::Idle => continue,
                    MovementState::ManualDirectional { direction } => (
                        character.position(),
                        direction,
                        i128::from(character.speed().subunits_per_tick()),
                    ),
                    MovementState::Navigating { .. } | MovementState::Wandering { .. } => {
                        self.advance_navigation_one_tick(id)?;
                        continue;
                    }
                }
            };
            let trace_start = position;

            while remaining > 0 {
                let source = position.containing_cell();
                let entry_distance = entry_distance(position, source, direction)?;
                let target_is_walkable = match direction.adjacent(source) {
                    Some(target) => self.is_walkable(target)?,
                    None => false,
                };

                if !target_is_walkable {
                    if remaining < entry_distance {
                        position = translate(position, direction, remaining)?;
                        self.characters
                            .get_mut(&id)
                            .expect("character ID came from the character map")
                            .set_position(position);
                    } else {
                        position = translate(position, direction, entry_distance - 1)?;
                        let character = self
                            .characters
                            .get_mut(&id)
                            .expect("character ID came from the character map");
                        character.set_position(position);
                        character.set_movement(MovementState::Idle);
                    }
                    break;
                }

                let distance = remaining.min(entry_distance);
                position = translate(position, direction, distance)?;
                self.characters
                    .get_mut(&id)
                    .expect("character ID came from the character map")
                    .set_position(position);
                remaining -= distance;
            }
            self.characters
                .get_mut(&id)
                .expect("character ID came from the character map")
                .set_last_tick_motion_trace(vec![trace_start, position]);
        }

        let discovery_cells = self
            .characters
            .iter()
            .map(|(id, character)| (*id, character.position().containing_cell()))
            .collect::<Vec<_>>();
        for (id, cell) in discovery_cells {
            if self.last_discovery_cells.insert(id, cell) != Some(cell) {
                self.explored_world.reveal_around(cell);
            }
        }

        let partial_routes = self
            .characters
            .iter()
            .filter_map(|(id, character)| {
                if !matches!(character.movement(), MovementState::Navigating { .. }) {
                    return None;
                }
                character.navigation_route().and_then(|route| {
                    (route.waypoints.is_empty() && character.position() != route.destination)
                        .then_some((*id, route.destination))
                })
            })
            .collect::<Vec<_>>();
        for (id, destination) in partial_routes {
            let route = self.plan_player_navigation_route(id, destination)?;
            self.apply_navigation_route(id, destination, route);
        }

        Ok(())
    }

    fn advance_navigation_one_tick(&mut self, id: EntityId) -> Result<(), SimulationError> {
        let (mut position, mut remaining, mut route, wandering) = {
            let character = self
                .characters
                .get(&id)
                .expect("character ID came from map");
            (
                character.position(),
                i128::from(character.speed().subunits_per_tick()),
                character
                    .navigation_route()
                    .cloned()
                    .expect("navigating characters have a route"),
                matches!(character.movement(), MovementState::Wandering { .. }),
            )
        };
        let mut trace = vec![position];
        while remaining > 0 {
            while route.waypoints.front().copied() == Some(position) {
                route.waypoints.pop_front();
                trace.push(position);
            }
            let Some(target) = route.waypoints.front().copied() else {
                let character = self
                    .characters
                    .get_mut(&id)
                    .expect("character ID came from map");
                character.set_position(position);
                if position == route.destination {
                    character.set_movement(MovementState::Idle);
                } else if wandering {
                    character.set_wandering_route(route);
                } else {
                    character.set_navigation_route(route);
                }
                character.set_last_tick_motion_trace(trace);
                return Ok(());
            };
            let (direction, distance) = direction_and_distance(position, target)?;
            let budget = remaining.min(distance);
            let (next, consumed, blocked) = self.advance_cardinal(position, direction, budget)?;
            position = next;
            remaining -= consumed;
            if blocked {
                let character = self
                    .characters
                    .get_mut(&id)
                    .expect("character ID came from map");
                character.set_position(position);
                character.set_movement(MovementState::Idle);
                trace.push(position);
                character.set_last_tick_motion_trace(trace);
                return Ok(());
            }
        }
        while route.waypoints.front().copied() == Some(position) {
            route.waypoints.pop_front();
        }
        let character = self
            .characters
            .get_mut(&id)
            .expect("character ID came from map");
        character.set_position(position);
        if route.waypoints.is_empty() && position == route.destination {
            character.set_movement(MovementState::Idle);
        } else if wandering {
            character.set_wandering_route(route);
        } else {
            character.set_navigation_route(route);
        }
        trace.push(position);
        character.set_last_tick_motion_trace(trace);
        Ok(())
    }

    fn advance_cardinal(
        &self,
        position: WorldPosition,
        direction: Direction,
        budget: i128,
    ) -> Result<(WorldPosition, i128, bool), SimulationError> {
        let source = position.containing_cell();
        let entry = entry_distance(position, source, direction)?;
        let walkable = match direction.adjacent(source) {
            Some(cell) => self.is_walkable(cell)?,
            None => false,
        };
        if !walkable {
            return if budget < entry {
                Ok((translate(position, direction, budget)?, budget, false))
            } else {
                Ok((translate(position, direction, entry - 1)?, budget, true))
            };
        }
        let consumed = budget.min(entry);
        Ok((translate(position, direction, consumed)?, consumed, false))
    }

    fn is_walkable(&self, position: WorldCell) -> Result<bool, SimulationError> {
        if self.effective_terrain_at(position)? != Terrain::Grass {
            return Ok(false);
        }
        Ok(self
            .construction_world
            .structure_kind_at(position)
            .is_none_or(|kind| kind.navigation_cost().is_some()))
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
    fn from_job_world(error: JobWorldError) -> Self {
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
            | JobWorldError::JobNotCraft(_)
            | JobWorldError::WorkerAlreadyReserved(_)
            | JobWorldError::JobNotAvailable(_)
            | JobWorldError::JobNotReserved(_)
            | JobWorldError::JobNotWorking(_)
            | JobWorldError::IndexCorruption => Self::JobInvariantViolation,
        }
    }

    fn from_production_world(error: ProductionWorldError) -> Self {
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

    fn from_production_logistics_world(error: ProductionLogisticsWorldError) -> Self {
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

    fn from_stockpile_world(error: StockpileWorldError) -> Self {
        match error {
            StockpileWorldError::UnknownStockpile(id) => Self::UnknownStockpile(id),
            StockpileWorldError::CellAlreadyOwned { cell, stockpile_id } => {
                Self::StockpileCellAlreadyOwned { cell, stockpile_id }
            }
            StockpileWorldError::RevisionOverflow => Self::StockpileRevisionOverflow,
            StockpileWorldError::DuplicateStockpile(_) => Self::StockpileInvariantViolation,
        }
    }

    fn from_workstation_world(error: WorkstationWorldError) -> Self {
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

    fn from_construction_world(error: ConstructionWorldError) -> Self {
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
            Self::ItemNotOnGround(id) => {
                write!(formatter, "item ID {} is not on the ground", id.value())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Direction, HUNGRY_SATIETY, MAX_SATIETY, MovementSpeed, MovementState};

    fn cora() -> EntityId {
        EntityId::new(3).unwrap()
    }

    fn set_satiety(simulation: &mut Simulation, character_id: EntityId, target: u8) {
        let character = simulation.characters.get_mut(&character_id).unwrap();
        while character.satiety() > target {
            character.decay_satiety();
        }
        if character.satiety() < target {
            character.restore_satiety(target - character.satiety());
        }
    }

    fn total_berries(simulation: &Simulation) -> u32 {
        simulation
            .items()
            .filter(|item| item.kind() == ItemKind::Berries)
            .map(|item| item.quantity().get())
            .sum()
    }

    #[test]
    fn idle_characters_wander_deterministically_inside_their_local_anchor() {
        let mut first = Simulation::new(WorldSeed::new(0)).unwrap();
        let mut second = Simulation::new(WorldSeed::new(0)).unwrap();
        let initial_anchors = first
            .characters()
            .map(|character| (character.id(), character.idle_anchor()))
            .collect::<BTreeMap<_, _>>();
        let mut saw_wandering = false;

        for _ in 0..96 {
            first.advance_ticks(1).unwrap();
            second.advance_ticks(1).unwrap();
            for character in first.characters() {
                let anchor = initial_anchors[&character.id()];
                assert_eq!(character.idle_anchor(), anchor);
                assert!(idle_cell_within_anchor(
                    anchor,
                    character.position().containing_cell()
                ));
                if let MovementState::Wandering { destination } = character.movement() {
                    saw_wandering = true;
                    assert!(first.is_explored(destination.containing_cell()));
                    assert!(idle_cell_within_anchor(
                        anchor,
                        destination.containing_cell()
                    ));
                }
            }
        }

        assert!(
            saw_wandering,
            "idle settlement should visibly move without player orders"
        );
        let first_state = first
            .characters()
            .map(|character| {
                (
                    character.id(),
                    character.position(),
                    character.movement(),
                    character.idle_anchor(),
                )
            })
            .collect::<Vec<_>>();
        let second_state = second
            .characters()
            .map(|character| {
                (
                    character.id(),
                    character.position(),
                    character.movement(),
                    character.idle_anchor(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(first_state, second_state);
    }

    #[test]
    fn real_work_preempts_idle_wandering() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cora = cora();
        let route = (0..128)
            .find_map(|entropy| simulation.plan_idle_wander_route(cora, entropy).unwrap())
            .expect("seed 0 should provide a local idle route for Cora");
        simulation
            .characters
            .get_mut(&cora)
            .unwrap()
            .set_wandering_route(route);
        for id in [1_u64, 2, 4, 5].map(|id| EntityId::new(id).unwrap()) {
            simulation.characters.get_mut(&id).unwrap().set_movement(
                MovementState::ManualDirectional {
                    direction: Direction::East,
                },
            );
        }

        let (source, _) = harvest_fixture(&simulation);
        let job_id = simulation.designate_harvest(source).unwrap();
        simulation.try_assign_harvest(job_id, source).unwrap();

        assert_eq!(
            simulation.job_world.get(job_id).unwrap().state().worker(),
            Some(cora)
        );
        assert!(matches!(
            character(&simulation, cora).movement(),
            MovementState::Navigating { .. }
        ));
    }

    #[test]
    fn satiety_decays_only_on_the_global_interval() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cora = cora();
        simulation
            .advance_ticks(SATIETY_DECAY_INTERVAL_TICKS - 1)
            .unwrap();
        assert_eq!(character(&simulation, cora).satiety(), MAX_SATIETY);

        simulation.advance_ticks(1).unwrap();
        assert_eq!(character(&simulation, cora).satiety(), MAX_SATIETY - 1);
    }

    #[test]
    fn hungry_characters_split_exact_physical_meals_and_eat_concurrently() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let ada = EntityId::new(1).unwrap();
        let borin = EntityId::new(2).unwrap();
        set_satiety(&mut simulation, ada, HUNGRY_SATIETY);
        set_satiety(&mut simulation, borin, HUNGRY_SATIETY);
        assert_eq!(total_berries(&simulation), BOOTSTRAP_BERRIES);

        simulation.advance_ticks(1).unwrap();
        let eat_jobs = simulation
            .jobs()
            .filter_map(|job| match job.kind() {
                JobKind::Eat {
                    character_id,
                    item_id,
                } => Some((character_id, item_id, job.state())),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(eat_jobs.len(), 2);
        assert!(eat_jobs.iter().all(|(_, item_id, state)| {
            simulation
                .item_world
                .get(*item_id)
                .is_some_and(|item| item.kind() == ItemKind::Berries && item.quantity().get() == 1)
                && matches!(state, JobState::Reserved { .. } | JobState::Working { .. })
        }));
        assert_eq!(total_berries(&simulation), BOOTSTRAP_BERRIES);

        for _ in 0..256 {
            simulation.advance_ticks(1).unwrap();
            if simulation.job_world.eat_job_for_character(ada).is_none()
                && simulation.job_world.eat_job_for_character(borin).is_none()
            {
                break;
            }
        }
        assert!(character(&simulation, ada).satiety() > HUNGRY_SATIETY);
        assert!(character(&simulation, borin).satiety() > HUNGRY_SATIETY);
        assert!(character(&simulation, ada).satiety() <= MAX_SATIETY);
        assert!(character(&simulation, borin).satiety() <= MAX_SATIETY);
        assert_eq!(total_berries(&simulation), BOOTSTRAP_BERRIES - 2);
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(simulation.item_world.indexes_are_consistent());
    }

    #[test]
    fn one_berry_restores_exactly_fifty_satiety_at_completion() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cora = cora();
        let food_position = simulation
            .items()
            .find(|item| item.kind() == ItemKind::Berries)
            .and_then(ItemStack::ground_position)
            .unwrap();
        place_on_grass(&mut simulation, cora, food_position.containing_cell());
        set_satiety(&mut simulation, cora, HUNGRY_SATIETY);

        simulation.advance_ticks(1).unwrap();
        simulation.advance_ticks(1).unwrap();
        assert_eq!(character(&simulation, cora).satiety(), HUNGRY_SATIETY);
        simulation.advance_ticks(1).unwrap();

        assert_eq!(character(&simulation, cora).satiety(), MAX_SATIETY);
        assert_eq!(total_berries(&simulation), BOOTSTRAP_BERRIES - 1);
        assert!(simulation.job_world.eat_job_for_character(cora).is_none());
    }

    #[test]
    fn berry_bush_harvest_produces_physical_food_and_regrows() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let source = WorldCell::new(-3, -3);
        let resource = simulation.natural_resource_at(source).unwrap().unwrap();
        assert_eq!(resource.kind(), NaturalResourceKind::BerryBush);
        let berries_before = total_berries(&simulation);

        let job_id = simulation.designate_harvest(source).unwrap();
        for _ in 0..256 {
            simulation.advance_ticks(1).unwrap();
            if simulation.job_world.get(job_id).is_none() {
                break;
            }
        }
        assert!(simulation.job_world.get(job_id).is_none());
        assert_eq!(simulation.natural_resource_at(source).unwrap(), None);
        assert_eq!(
            total_berries(&simulation),
            berries_before + resource.yield_quantity()
        );
        assert!(simulation.items().any(|item| {
            item.kind() == ItemKind::Berries
                && item.ground_position() == Some(WorldPosition::from_cell_center(source).unwrap())
        }));

        let ready_tick = simulation.renewable_resource_regrowth[&source];
        assert!(ready_tick > simulation.tick());
        simulation
            .advance_ticks(ready_tick.value() - simulation.tick().value())
            .unwrap();
        let regrown = simulation.natural_resource_at(source).unwrap().unwrap();
        assert_eq!(regrown, resource);
        assert!(!simulation.renewable_resource_regrowth.contains_key(&source));
    }

    #[test]
    fn harvested_berries_use_ordinary_stockpile_logistics() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let initial_berries = simulation
            .items()
            .filter(|item| item.kind() == ItemKind::Berries)
            .map(|item| (item.id(), item.quantity().get()))
            .collect::<Vec<_>>();
        for (item_id, quantity) in initial_berries {
            simulation.item_world.consume(item_id, quantity).unwrap();
        }
        let stockpile_id = simulation.create_stockpile(WorldCell::new(0, 2)).unwrap();
        for cell in [WorldCell::new(-1, 2), WorldCell::new(1, 2)] {
            simulation
                .set_stockpile_cell(stockpile_id, cell, true)
                .unwrap();
        }
        for kind in [ItemKind::Wood, ItemKind::Stone, ItemKind::PrimitiveTool] {
            simulation
                .set_stockpile_item_allowed(stockpile_id, kind, false)
                .unwrap();
        }

        let source = WorldCell::new(-3, -3);
        simulation.designate_harvest(source).unwrap();
        let mut stockpiled = 0;
        for _ in 0..512 {
            simulation.advance_ticks(1).unwrap();
            stockpiled = simulation
                .items()
                .filter(|item| item.kind() == ItemKind::Berries)
                .filter_map(ItemStack::ground_position)
                .filter(|position| {
                    simulation.stockpile_at(position.containing_cell()) == Some(stockpile_id)
                })
                .count();
            if stockpiled != 0 {
                break;
            }
        }
        assert!(
            stockpiled > 0,
            "harvested berries never reached the stockpile"
        );
    }

    #[test]
    fn four_regrowing_bushes_sustain_five_characters_long_term() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let initial_berries = simulation
            .items()
            .filter(|item| item.kind() == ItemKind::Berries)
            .map(|item| (item.id(), item.quantity().get()))
            .collect::<Vec<_>>();
        for (item_id, quantity) in initial_berries {
            simulation.item_world.consume(item_id, quantity).unwrap();
        }

        let stockpile_id = simulation.create_stockpile(WorldCell::new(0, 2)).unwrap();
        for cell in [WorldCell::new(-1, 2), WorldCell::new(1, 2)] {
            simulation
                .set_stockpile_cell(stockpile_id, cell, true)
                .unwrap();
        }
        for kind in [ItemKind::Wood, ItemKind::Stone, ItemKind::PrimitiveTool] {
            simulation
                .set_stockpile_item_allowed(stockpile_id, kind, false)
                .unwrap();
        }

        let mut minimum_satiety = MAX_SATIETY;
        for _ in 0..625 {
            simulation.advance_ticks(16).unwrap();
            for character in simulation.characters() {
                minimum_satiety = minimum_satiety.min(character.satiety());
                assert!(
                    character.satiety() > 0,
                    "character {} starved at tick {}",
                    character.id().value(),
                    simulation.tick().value()
                );
            }
        }
        assert!(minimum_satiety <= HUNGRY_SATIETY);
        assert!(simulation.resource_revision() >= 8);
        assert!(
            simulation.explored_world.cells().all(|cell| {
                cell_manhattan_distance(WorldCell::new(0, 0), cell)
                    <= u128::try_from(AUTONOMOUS_FORAGE_RADIUS_CELLS + IDLE_WANDER_RADIUS_CELLS + 5)
                        .expect("bootstrap forage bound is positive")
            }),
            "autonomous foraging must not turn renewable food into free long-range scouting"
        );
        assert!(
            simulation
                .generator
                .natural_resource_at(WorldCell::new(-3, -3))
                .is_some_and(|resource| resource.kind() == NaturalResourceKind::BerryBush)
        );
    }

    #[test]
    fn manual_interruption_cancels_eat_and_does_not_consume_reserved_meal() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cora = cora();
        set_satiety(&mut simulation, cora, HUNGRY_SATIETY);
        simulation.advance_ticks(1).unwrap();
        let eat_job_id = simulation.job_world.eat_job_for_character(cora).unwrap();
        let JobKind::Eat { item_id, .. } = simulation.job_world.get(eat_job_id).unwrap().kind()
        else {
            unreachable!();
        };
        assert_eq!(
            simulation.item_world.get(item_id).unwrap().quantity().get(),
            1
        );
        let before = character(&simulation, cora).satiety();

        simulation
            .set_movement_direction(cora, Direction::East)
            .unwrap();

        assert!(simulation.job_world.eat_job_for_character(cora).is_none());
        assert_eq!(simulation.job_world.eat_job_for_item(item_id), None);
        assert_eq!(
            simulation.item_world.get(item_id).unwrap().quantity().get(),
            1
        );
        assert_eq!(character(&simulation, cora).satiety(), before);
        assert_eq!(
            character(&simulation, cora).movement(),
            MovementState::ManualDirectional {
                direction: Direction::East
            }
        );
    }

    #[test]
    fn starving_character_without_food_stops_work_and_is_not_reassigned() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let berries = simulation
            .items()
            .find(|item| item.kind() == ItemKind::Berries)
            .unwrap()
            .id();
        let berry_quantity = simulation.item_world.get(berries).unwrap().quantity().get();
        simulation
            .item_world
            .consume(berries, berry_quantity)
            .unwrap();
        for cell in simulation.explored_world.cells().collect::<Vec<_>>() {
            if simulation
                .generator
                .natural_resource_at(cell)
                .is_some_and(|resource| resource.kind() == NaturalResourceKind::BerryBush)
            {
                simulation
                    .renewable_resource_regrowth
                    .insert(cell, SimulationTick::new(u64::MAX));
            }
        }
        let (source, _) = harvest_fixture(&simulation);
        let job_id = simulation.designate_harvest(source).unwrap();
        for _ in 0..64 {
            simulation.advance_ticks(1).unwrap();
            if simulation
                .job_world
                .get(job_id)
                .unwrap()
                .state()
                .worker()
                .is_some()
            {
                break;
            }
        }
        let worker_id = simulation
            .job_world
            .get(job_id)
            .unwrap()
            .state()
            .worker()
            .unwrap();
        set_satiety(&mut simulation, worker_id, 0);

        simulation.advance_ticks(1).unwrap();

        assert_eq!(simulation.job_world.job_for_worker(worker_id), None);
        assert_eq!(
            character(&simulation, worker_id).movement(),
            MovementState::Idle
        );
        assert!(character(&simulation, worker_id).is_starving());
        assert!(simulation.job_world.get(job_id).is_some());
    }

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
    fn player_move_to_advances_through_newly_explored_terrain_to_the_intended_destination() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let destination = WorldCell::new(20, 0);
        place_on_grass(&mut simulation, cora, WorldCell::new(0, 0));
        for x in 0..=20 {
            simulation
                .set_terrain_override(WorldCell::new(x, 0), Terrain::Grass)
                .unwrap();
        }
        simulation.advance_ticks(1).unwrap();
        assert!(!simulation.is_explored(destination));

        let destination_position = WorldPosition::from_cell_center(destination).unwrap();
        simulation.move_to(cora, destination_position).unwrap();
        for _ in 0..160 {
            simulation.advance_ticks(1).unwrap();
            if character(&simulation, cora).position() == destination_position {
                break;
            }
        }

        assert_eq!(
            character(&simulation, cora).position(),
            destination_position
        );
        assert!(character(&simulation, cora).is_available_for_work());
        assert!(simulation.is_explored(destination));
    }

    #[test]
    fn player_move_to_stops_at_the_closest_reachable_cell_when_target_is_blocked() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let destination = WorldCell::new(20, 0);
        place_on_grass(&mut simulation, cora, WorldCell::new(0, 0));
        for x in 0..20 {
            simulation
                .set_terrain_override(WorldCell::new(x, 0), Terrain::Grass)
                .unwrap();
        }
        for cell in [
            destination,
            WorldCell::new(21, 0),
            WorldCell::new(20, 1),
            WorldCell::new(20, -1),
        ] {
            simulation
                .set_terrain_override(cell, Terrain::Rock)
                .unwrap();
        }
        simulation.advance_ticks(1).unwrap();

        simulation
            .move_to(cora, WorldPosition::from_cell_center(destination).unwrap())
            .unwrap();
        let closest = WorldPosition::from_cell_center(WorldCell::new(19, 0)).unwrap();
        for _ in 0..160 {
            simulation.advance_ticks(1).unwrap();
            if character(&simulation, cora).position() == closest {
                break;
            }
        }

        assert_eq!(character(&simulation, cora).position(), closest);
        assert!(character(&simulation, cora).is_available_for_work());
    }

    #[test]
    fn exact_destinations_remain_still_after_arrival_and_idle_ticks() {
        let destinations = [
            WorldPosition::from_subunits(512, 512).unwrap(),
            WorldPosition::from_subunits(256, 256).unwrap(),
            WorldPosition::from_subunits(900, 777).unwrap(),
            WorldPosition::from_subunits(1023, 900).unwrap(),
            WorldPosition::from_subunits(1024, 900).unwrap(),
            WorldPosition::from_subunits(-1024, 900).unwrap(),
            WorldPosition::from_subunits(1024, 1024).unwrap(),
        ];

        for destination in destinations {
            let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
            let cora = cora();
            simulation
                .set_terrain_override(destination.containing_cell(), Terrain::Grass)
                .unwrap();
            simulation.move_to(cora, destination).unwrap();
            simulation.advance_ticks(20).unwrap();

            assert_eq!(character(&simulation, cora).position(), destination);
            assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
            assert_eq!(character(&simulation, cora).navigation_destination(), None);
            simulation.advance_ticks(3).unwrap();
            assert_eq!(character(&simulation, cora).position(), destination);
            assert_eq!(
                character(&simulation, cora).last_tick_motion_trace(),
                &[destination]
            );
        }
    }

    #[test]
    fn exact_destination_near_blocked_terrain_remains_still_after_arrival() {
        for blocked in [Terrain::Water, Terrain::Rock] {
            let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
            let cora = cora();
            let (cell, direction) = find_raw_grass_with_neighbor(&simulation, blocked);
            place_on_grass(&mut simulation, cora, cell);
            simulation.advance_ticks(1).unwrap();
            let destination = near_cell_edge(cell, direction);

            simulation.move_to(cora, destination).unwrap();
            simulation.advance_ticks(20).unwrap();

            assert_eq!(character(&simulation, cora).position(), destination);
            assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
            simulation.advance_ticks(3).unwrap();
            assert_eq!(character(&simulation, cora).position(), destination);
            assert_eq!(
                character(&simulation, cora).last_tick_motion_trace(),
                &[destination]
            );
        }
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
    fn movement_crosses_positive_and_negative_chunk_boundaries_with_stable_identity() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();

        let cora = cora();
        place_on_grass(&mut simulation, cora, WorldCell::new(31, 0));
        simulation
            .set_movement_direction(cora, Direction::East)
            .unwrap();
        simulation.advance_ticks(4).unwrap();
        assert_eq!(character(&simulation, cora).id(), cora);
        assert_eq!(
            character(&simulation, cora).position(),
            position_at_cell(WorldCell::new(32, 0))
        );

        place_on_grass(&mut simulation, cora, WorldCell::new(0, 0));
        simulation
            .set_movement_direction(cora, Direction::West)
            .unwrap();
        simulation.advance_ticks(4).unwrap();
        assert_eq!(character(&simulation, cora).id(), cora);
        assert_eq!(
            character(&simulation, cora).position(),
            position_at_cell(WorldCell::new(-1, 0))
        );
    }

    #[test]
    fn replacement_direction_is_accepted_without_terrain_prevalidation() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let (position, accepted, blocked) = find_replacement_fixture(&simulation);
        place_on_grass(&mut simulation, cora, position);

        simulation.set_movement_direction(cora, accepted).unwrap();
        simulation.set_movement_direction(cora, blocked).unwrap();
        assert_eq!(
            character(&simulation, cora).movement(),
            MovementState::ManualDirectional { direction: blocked }
        );
    }

    #[test]
    fn replacement_direction_starts_from_current_post_tick_cell() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let position = find_turn_fixture(&simulation);
        place_on_grass(&mut simulation, cora, position);

        simulation
            .set_movement_direction(cora, Direction::East)
            .unwrap();
        simulation.advance_ticks(1).unwrap();
        simulation
            .set_movement_direction(cora, Direction::North)
            .unwrap();
        simulation.advance_ticks(1).unwrap();

        assert_eq!(
            character(&simulation, cora).position(),
            position_at_cell(position)
                .checked_translate(256, 256)
                .unwrap()
        );
        assert_eq!(
            character(&simulation, cora).movement(),
            MovementState::ManualDirectional {
                direction: Direction::North
            }
        );
    }

    #[test]
    fn persisted_movement_stops_on_water_at_last_valid_subunit() {
        persisted_movement_stops_on(Terrain::Water);
    }

    #[test]
    fn persisted_movement_stops_on_rock_at_last_valid_subunit() {
        persisted_movement_stops_on(Terrain::Rock);
    }

    #[test]
    fn persisted_movement_stops_on_coordinate_overflow_without_wrapping() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let controlled_character = simulation.characters.get_mut(&cora).unwrap();
        controlled_character.set_position(position_at_cell(WorldCell::new(i64::MAX, 0)));
        controlled_character.set_movement(MovementState::ManualDirectional {
            direction: Direction::East,
        });

        simulation.advance_ticks(3).unwrap();

        assert_eq!(
            character(&simulation, cora).position(),
            WorldPosition::from_subunits((i128::from(i64::MAX) + 1) * 1024 - 1, 512).unwrap()
        );
        assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
    }

    #[test]
    fn identical_direction_commands_produce_identical_authoritative_state() {
        let mut first = Simulation::new(WorldSeed::new(2)).unwrap();
        let mut second = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();

        for simulation in [&mut first, &mut second] {
            simulation
                .set_movement_direction(cora, Direction::East)
                .unwrap();
            simulation.advance_ticks(8).unwrap();
            simulation.stop_movement(cora).unwrap();
            simulation
                .set_movement_direction(cora, Direction::West)
                .unwrap();
            simulation.advance_ticks(3).unwrap();
        }

        assert_eq!(first.tick(), second.tick());
        assert_eq!(
            first.characters().cloned().collect::<Vec<_>>(),
            second.characters().cloned().collect::<Vec<_>>()
        );
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
    fn grass_overridden_to_blocked_terrain_stops_only_at_its_boundary() {
        for blocked in [Terrain::Rock, Terrain::Water] {
            let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
            let (start, direction) = find_raw_grass_with_neighbor(&simulation, Terrain::Grass);
            let target = direction.adjacent(start).unwrap();
            let cora = cora();
            place_on_grass(&mut simulation, cora, start);

            simulation.set_terrain_override(target, blocked).unwrap();
            simulation.set_movement_direction(cora, direction).unwrap();
            simulation.advance_ticks(2).unwrap();

            assert_eq!(
                character(&simulation, cora).position(),
                blocked_stop_position(start, direction)
            );
            assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
        }
    }

    #[test]
    fn blocked_terrain_overridden_to_grass_allows_continuous_step() {
        for blocked in [Terrain::Water, Terrain::Rock] {
            let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
            let (start, direction) = find_raw_grass_with_neighbor(&simulation, blocked);
            let target = direction.adjacent(start).unwrap();
            let cora = cora();
            place_on_grass(&mut simulation, cora, start);

            simulation
                .set_terrain_override(target, Terrain::Grass)
                .unwrap();
            simulation.set_movement_direction(cora, direction).unwrap();
            simulation.advance_ticks(4).unwrap();
            assert_eq!(
                character(&simulation, cora).position(),
                position_at_cell(target)
            );

            simulation.stop_movement(cora).unwrap();
            simulation.set_terrain_override(target, blocked).unwrap();
            simulation
                .characters
                .get_mut(&cora)
                .unwrap()
                .set_position(position_at_cell(start));
            simulation.set_movement_direction(cora, direction).unwrap();
            simulation.advance_ticks(2).unwrap();
            assert_eq!(
                character(&simulation, cora).position(),
                blocked_stop_position(start, direction)
            );
        }
    }

    #[test]
    fn cardinal_movement_advances_in_subcell_increments() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        place_on_grass(&mut simulation, cora, WorldCell::new(0, 0));
        simulation
            .set_movement_direction(cora, Direction::East)
            .unwrap();

        simulation.advance_ticks(1).unwrap();
        assert_eq!(character(&simulation, cora).position().x_subunits(), 768);
        assert_eq!(character(&simulation, cora).position().y_subunits(), 512);
        assert_eq!(
            character(&simulation, cora).movement(),
            MovementState::ManualDirectional {
                direction: Direction::East
            }
        );

        simulation.advance_ticks(3).unwrap();
        assert_eq!(
            character(&simulation, cora).position(),
            position_at_cell(WorldCell::new(1, 0))
        );
    }

    #[test]
    fn cross_cell_waypoints_approach_exact_destination_without_center_backtrack() {
        let cases = [
            (
                WorldCell::new(0, 0),
                WorldCell::new(1, 0),
                WorldPosition::from_subunits(1_100, 900).unwrap(),
                vec![
                    WorldPosition::from_subunits(1_100, 512).unwrap(),
                    WorldPosition::from_subunits(1_100, 900).unwrap(),
                ],
            ),
            (
                WorldCell::new(2, 0),
                WorldCell::new(1, 0),
                WorldPosition::from_subunits(1_900, 100).unwrap(),
                vec![
                    WorldPosition::from_subunits(1_900, 512).unwrap(),
                    WorldPosition::from_subunits(1_900, 100).unwrap(),
                ],
            ),
            (
                WorldCell::new(1, -1),
                WorldCell::new(1, 0),
                WorldPosition::from_subunits(1_800, 100).unwrap(),
                vec![
                    WorldPosition::from_subunits(1_536, 100).unwrap(),
                    WorldPosition::from_subunits(1_800, 100).unwrap(),
                ],
            ),
            (
                WorldCell::new(1, 1),
                WorldCell::new(1, 0),
                WorldPosition::from_subunits(1_200, 900).unwrap(),
                vec![
                    WorldPosition::from_subunits(1_536, 900).unwrap(),
                    WorldPosition::from_subunits(1_200, 900).unwrap(),
                ],
            ),
        ];

        for (start, goal, destination, expected) in cases {
            let current = WorldPosition::from_cell_center(start).unwrap();
            let route = build_waypoints(current, destination, &[start, goal]).unwrap();
            assert_eq!(route.into_iter().collect::<Vec<_>>(), expected);
        }
    }

    #[test]
    fn move_to_reaches_an_exact_same_cell_target_via_x_then_y() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(0, 0);
        place_on_grass(&mut simulation, cora, start);
        let destination = WorldPosition::from_subunits(800, 900).unwrap();
        simulation.move_to(cora, destination).unwrap();

        simulation.advance_ticks(3).unwrap();

        assert_eq!(character(&simulation, cora).position(), destination);
        assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
        assert_eq!(
            character(&simulation, cora).last_tick_motion_trace(),
            &[WorldPosition::from_subunits(800, 736).unwrap(), destination]
        );
    }

    #[test]
    fn move_to_preserves_a_turn_in_the_same_tick_motion_trace() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(0, 0);
        place_on_grass(&mut simulation, cora, start);
        character_mut(&mut simulation, cora).set_speed(MovementSpeed::new(600).unwrap());
        let destination = WorldPosition::from_subunits(800, 900).unwrap();
        simulation.move_to(cora, destination).unwrap();

        simulation.advance_ticks(1).unwrap();

        assert_eq!(
            character(&simulation, cora).last_tick_motion_trace(),
            &[
                WorldPosition::from_subunits(512, 512).unwrap(),
                WorldPosition::from_subunits(800, 512).unwrap(),
                WorldPosition::from_subunits(800, 824).unwrap(),
            ]
        );
    }

    #[test]
    fn blocked_move_to_replaces_an_existing_route_with_the_closest_approach() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(0, 0);
        place_on_grass(&mut simulation, cora, start);
        let first = WorldPosition::from_cell_center(WorldCell::new(1, 0)).unwrap();
        let blocked = WorldPosition::from_cell_center(WorldCell::new(2, 0)).unwrap();
        simulation
            .set_terrain_override(WorldCell::new(1, 0), Terrain::Grass)
            .unwrap();
        simulation
            .set_terrain_override(WorldCell::new(2, 0), Terrain::Rock)
            .unwrap();
        simulation.move_to(cora, first).unwrap();
        simulation.move_to(cora, blocked).unwrap();

        assert_eq!(
            character(&simulation, cora).movement(),
            MovementState::Navigating {
                destination: blocked
            }
        );
        assert_eq!(
            character(&simulation, cora).navigation_destination(),
            Some(blocked)
        );
        assert_eq!(
            character(&simulation, cora).navigation_waypoints().last(),
            Some(first)
        );
    }

    #[test]
    fn navigation_stops_at_a_newly_blocked_cell_boundary() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        place_on_grass(&mut simulation, cora, WorldCell::new(0, 0));
        for x in 1..=2 {
            simulation
                .set_terrain_override(WorldCell::new(x, 0), Terrain::Grass)
                .unwrap();
        }
        simulation
            .move_to(
                cora,
                WorldPosition::from_cell_center(WorldCell::new(2, 0)).unwrap(),
            )
            .unwrap();
        simulation.advance_ticks(1).unwrap();
        simulation
            .set_terrain_override(WorldCell::new(1, 0), Terrain::Rock)
            .unwrap();

        simulation.advance_ticks(1).unwrap();

        assert_eq!(character(&simulation, cora).position().x_subunits(), 1023);
        assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
        assert_eq!(character(&simulation, cora).navigation_destination(), None);
    }

    #[test]
    fn navigation_executes_a_cross_chunk_route_with_stable_identity() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(31, 0);
        let destination_cell = WorldCell::new(33, 1);
        for cell in [
            start,
            WorldCell::new(32, 0),
            WorldCell::new(33, 0),
            destination_cell,
        ] {
            simulation
                .set_terrain_override(cell, Terrain::Grass)
                .unwrap();
        }
        for cell in [
            WorldCell::new(30, 0),
            WorldCell::new(31, -1),
            WorldCell::new(31, 1),
            WorldCell::new(32, -1),
            WorldCell::new(32, 1),
            WorldCell::new(33, -1),
        ] {
            simulation
                .set_terrain_override(cell, Terrain::Rock)
                .unwrap();
        }
        place_on_grass(&mut simulation, cora, start);
        simulation.advance_ticks(1).unwrap();
        character_mut(&mut simulation, cora).set_speed(MovementSpeed::new(4_096).unwrap());
        let destination = WorldPosition::from_cell_center(destination_cell).unwrap();
        simulation.move_to(cora, destination).unwrap();

        simulation.advance_ticks(1).unwrap();

        assert_eq!(character(&simulation, cora).id(), cora);
        assert_eq!(character(&simulation, cora).position(), destination);
        assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
    }

    #[test]
    fn navigation_crosses_the_zero_to_negative_cell_boundary() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(0, 0);
        let destination_cell = WorldCell::new(-1, 0);
        for cell in [start, destination_cell] {
            simulation
                .set_terrain_override(cell, Terrain::Grass)
                .unwrap();
        }
        place_on_grass(&mut simulation, cora, start);
        character_mut(&mut simulation, cora).set_speed(MovementSpeed::new(1_024).unwrap());
        let destination = WorldPosition::from_cell_center(destination_cell).unwrap();
        simulation.move_to(cora, destination).unwrap();

        simulation.advance_ticks(1).unwrap();

        assert_eq!(character(&simulation, cora).position(), destination);
        assert_eq!(character(&simulation, cora).id(), cora);
    }

    #[test]
    fn stop_and_manual_direction_cancel_navigation_without_snapping() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(0, 0);
        place_on_grass(&mut simulation, cora, start);
        for cell in [WorldCell::new(1, 0), WorldCell::new(2, 0)] {
            simulation
                .set_terrain_override(cell, Terrain::Grass)
                .unwrap();
        }
        let destination = WorldPosition::from_cell_center(WorldCell::new(2, 0)).unwrap();
        simulation.move_to(cora, destination).unwrap();
        simulation.advance_ticks(1).unwrap();
        let mid_route_position = character(&simulation, cora).position();

        simulation.stop_movement(cora).unwrap();
        assert_eq!(character(&simulation, cora).position(), mid_route_position);
        assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
        assert_eq!(character(&simulation, cora).navigation_destination(), None);

        simulation.move_to(cora, destination).unwrap();
        simulation
            .set_movement_direction(cora, Direction::West)
            .unwrap();
        assert_eq!(
            character(&simulation, cora).movement(),
            MovementState::ManualDirectional {
                direction: Direction::West
            }
        );
        assert_eq!(character(&simulation, cora).navigation_destination(), None);
    }

    #[test]
    fn blocked_neighbour_allows_approach_then_stops_only_at_boundary() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(0, 0);
        let target = WorldCell::new(1, 0);
        place_on_grass(&mut simulation, cora, start);
        simulation
            .set_terrain_override(target, Terrain::Grass)
            .unwrap();

        simulation
            .set_movement_direction(cora, Direction::East)
            .unwrap();
        simulation.advance_ticks(1).unwrap();
        assert_eq!(character(&simulation, cora).position().x_subunits(), 768);
        assert!(matches!(
            character(&simulation, cora).movement(),
            MovementState::ManualDirectional {
                direction: Direction::East
            }
        ));

        simulation
            .set_terrain_override(target, Terrain::Rock)
            .unwrap();
        simulation.advance_ticks(1).unwrap();
        assert_eq!(character(&simulation, cora).position().x_subunits(), 1023);
        assert_eq!(
            character(&simulation, cora).position().containing_cell(),
            start
        );
        assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);

        simulation
            .set_movement_direction(cora, Direction::West)
            .unwrap();
        simulation.advance_ticks(1).unwrap();
        assert_eq!(character(&simulation, cora).position().x_subunits(), 767);
    }

    #[test]
    fn large_speed_consumes_multiple_passable_cell_transitions() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(0, 0);
        place_on_grass(&mut simulation, cora, start);
        for x in 1..=3 {
            simulation
                .set_terrain_override(WorldCell::new(x, 0), Terrain::Grass)
                .unwrap();
        }
        character_mut(&mut simulation, cora).set_speed(MovementSpeed::new(2_500).unwrap());
        simulation
            .set_movement_direction(cora, Direction::East)
            .unwrap();

        simulation.advance_ticks(1).unwrap();

        assert_eq!(character(&simulation, cora).position().x_subunits(), 3_012);
        assert_eq!(
            character(&simulation, cora).position().containing_cell(),
            WorldCell::new(2, 0)
        );
    }

    #[test]
    fn blocked_transitions_stop_at_canonical_boundaries_in_every_direction() {
        for (direction, start, target, expected) in [
            (
                Direction::East,
                WorldCell::new(0, 0),
                WorldCell::new(1, 0),
                (1023, 512),
            ),
            (
                Direction::West,
                WorldCell::new(0, 0),
                WorldCell::new(-1, 0),
                (0, 512),
            ),
            (
                Direction::North,
                WorldCell::new(0, 0),
                WorldCell::new(0, 1),
                (512, 1023),
            ),
            (
                Direction::South,
                WorldCell::new(0, 0),
                WorldCell::new(0, -1),
                (512, 0),
            ),
            (
                Direction::East,
                WorldCell::new(-2, -2),
                WorldCell::new(-1, -2),
                (-1025, -1536),
            ),
            (
                Direction::West,
                WorldCell::new(-2, -2),
                WorldCell::new(-3, -2),
                (-2048, -1536),
            ),
            (
                Direction::North,
                WorldCell::new(-2, -2),
                WorldCell::new(-2, -1),
                (-1536, -1025),
            ),
            (
                Direction::South,
                WorldCell::new(-2, -2),
                WorldCell::new(-2, -3),
                (-1536, -2048),
            ),
        ] {
            let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
            let cora = cora();
            simulation
                .set_terrain_override(start, Terrain::Grass)
                .unwrap();
            place_on_grass(&mut simulation, cora, start);
            simulation
                .set_terrain_override(target, Terrain::Water)
                .unwrap();
            character_mut(&mut simulation, cora).set_speed(MovementSpeed::new(2_000).unwrap());
            simulation.set_movement_direction(cora, direction).unwrap();

            simulation.advance_ticks(1).unwrap();

            let position = character(&simulation, cora).position();
            assert_eq!((position.x_subunits(), position.y_subunits()), expected);
            assert_eq!(position.containing_cell(), start);
            assert_eq!(
                simulation.effective_terrain_at(start).unwrap(),
                Terrain::Grass
            );
            assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
        }
    }

    #[test]
    fn world_edges_allow_interior_motion_then_stop_without_wrapping() {
        for (start, direction, expected) in [
            (
                WorldCell::new(i64::MAX, 0),
                Direction::East,
                ((i128::from(i64::MAX) + 1) * 1024 - 1, 512),
            ),
            (
                WorldCell::new(i64::MIN, 0),
                Direction::West,
                (i128::from(i64::MIN) * 1024, 512),
            ),
            (
                WorldCell::new(0, i64::MAX),
                Direction::North,
                (512, (i128::from(i64::MAX) + 1) * 1024 - 1),
            ),
            (
                WorldCell::new(0, i64::MIN),
                Direction::South,
                (512, i128::from(i64::MIN) * 1024),
            ),
        ] {
            let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
            let cora = cora();
            character_mut(&mut simulation, cora).set_position(position_at_cell(start));
            character_mut(&mut simulation, cora).set_speed(MovementSpeed::new(2_000).unwrap());
            simulation.set_movement_direction(cora, direction).unwrap();

            simulation.advance_ticks(1).unwrap();

            let position = character(&simulation, cora).position();
            assert_eq!((position.x_subunits(), position.y_subunits()), expected);
            assert_eq!(position.containing_cell(), start);
            assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
        }
    }

    #[test]
    fn harvest_job_completes_into_one_physical_stack_and_cleans_reservation() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let (source, resource) = harvest_fixture(&simulation);
        let item_revision = simulation.item_revision();
        let resource_revision = simulation.resource_revision();

        let job_id = simulation.designate_harvest(source).unwrap();
        let output_id = simulation.next_entity_id().unwrap();
        assert_eq!(simulation.jobs().count(), 1);
        assert_eq!(
            simulation.natural_resource_at(source).unwrap(),
            Some(resource)
        );

        for _ in 0..256 {
            if simulation.jobs().next().is_none() {
                break;
            }
            simulation.advance_ticks(1).unwrap();
        }

        assert_eq!(simulation.jobs().count(), 0, "harvest job did not finish");
        assert_eq!(simulation.natural_resource_at(source).unwrap(), None);
        assert_eq!(simulation.resource_revision(), resource_revision + 1);
        assert_eq!(simulation.item_revision(), item_revision + 1);
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(
            simulation
                .characters()
                .all(|c| simulation.job_for_worker(c.id()).is_none())
        );

        let output = simulation
            .items()
            .find(|item| item.id() == output_id)
            .unwrap();
        let expected_kind = match resource.kind() {
            NaturalResourceKind::Tree => ItemKind::Wood,
            NaturalResourceKind::StoneOutcrop => ItemKind::Stone,
            NaturalResourceKind::BerryBush => ItemKind::Berries,
        };
        assert_eq!(output.kind(), expected_kind);
        assert_eq!(output.quantity().get(), resource.yield_quantity());
        assert_eq!(
            output.ground_position(),
            Some(WorldPosition::from_cell_center(source).unwrap())
        );
        assert_ne!(job_id, output_id);
    }

    #[test]
    fn harvest_assignment_is_deterministic_and_exclusive() {
        let mut first = Simulation::new(WorldSeed::new(0)).unwrap();
        let mut second = first.clone();
        let (source, _) = harvest_fixture(&first);
        let first_job = first.designate_harvest(source).unwrap();
        let second_job = second.designate_harvest(source).unwrap();
        assert_eq!(first_job, second_job);

        first.advance_ticks(1).unwrap();
        second.advance_ticks(1).unwrap();

        let first_state = first.job_world.get(first_job).unwrap().state();
        let second_state = second.job_world.get(second_job).unwrap().state();
        assert_eq!(first_state, second_state);
        let worker = first_state
            .worker()
            .expect("reachable harvest should reserve a worker");
        assert_eq!(first.job_for_worker(worker), Some(first_job));
        assert!(first.job_world.indexes_are_consistent());
    }

    #[test]
    fn harvest_designation_rejects_invalid_sources_without_allocating_jobs() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let (source, _) = harvest_fixture(&simulation);
        let before_next = simulation.next_entity_id();
        let job = simulation.designate_harvest(source).unwrap();
        assert_eq!(
            simulation.designate_harvest(source),
            Err(SimulationError::HarvestAlreadyDesignated(source))
        );
        assert_eq!(simulation.jobs().count(), 1);

        simulation.cancel_job(job).unwrap();
        let empty = (0..=7)
            .flat_map(|y| (-7..=7).map(move |x| WorldCell::new(x, y)))
            .find(|cell| {
                simulation.is_explored(*cell)
                    && simulation.natural_resource_at(*cell).unwrap().is_none()
            })
            .unwrap();
        assert_eq!(
            simulation.designate_harvest(empty),
            Err(SimulationError::NaturalResourceMissing(empty))
        );
        let unknown = WorldCell::new(100, 100);
        assert_eq!(
            simulation.designate_harvest(unknown),
            Err(SimulationError::HarvestSourceUndiscovered(unknown))
        );
        assert_eq!(
            before_next.unwrap().value() + 1,
            simulation.next_entity_id().unwrap().value()
        );
    }

    #[test]
    fn cancelling_or_manually_interrupting_harvest_releases_worker_reservation() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let (source, _) = harvest_fixture(&simulation);
        let job_id = simulation.designate_harvest(source).unwrap();
        simulation.advance_ticks(1).unwrap();
        let worker = simulation
            .job_world
            .get(job_id)
            .unwrap()
            .state()
            .worker()
            .unwrap();

        simulation.stop_movement(worker).unwrap();
        assert_eq!(
            simulation.job_world.get(job_id).unwrap().state(),
            JobState::Available
        );
        assert_eq!(simulation.job_for_worker(worker), None);
        assert!(simulation.job_world.indexes_are_consistent());

        simulation.advance_ticks(1).unwrap();
        let worker = simulation
            .job_world
            .get(job_id)
            .unwrap()
            .state()
            .worker()
            .unwrap();
        simulation.cancel_job(job_id).unwrap();
        assert_eq!(simulation.jobs().count(), 0);
        assert_eq!(simulation.job_for_worker(worker), None);
        assert_eq!(
            character(&simulation, worker).movement(),
            MovementState::Idle
        );
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(simulation.natural_resource_at(source).unwrap().is_some());
    }

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
                    && simulation.effective_terrain_at(*cell).unwrap() != Terrain::Grass
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
        assert_eq!(merged.kind(), ItemKind::Wood);
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
    fn craft_consumes_exact_quantities_from_input_zone_and_outputs_to_output_zone() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let workstation_id = simulation
            .place_workstation(WorkstationKind::Workbench, WorldCell::new(0, 0))
            .unwrap();
        let (wood_id, stone_id) = seed_recipe_inputs(&mut simulation, workstation_id, 5, 3);
        let output_cell =
            production_zone_cells(&simulation, workstation_id, ProductionZoneKind::Output)[0];
        let job_id = simulation
            .designate_craft(workstation_id, RecipeId::PrimitiveTool)
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
            .filter(|item| item.kind() == ItemKind::PrimitiveTool)
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
            .place_workstation(WorkstationKind::Workbench, first_bench_cell)
            .unwrap();
        let second_workstation = simulation
            .place_workstation(WorkstationKind::Workbench, second_bench_cell)
            .unwrap();
        let first_input =
            production_zone_cells(&simulation, first_workstation, ProductionZoneKind::Input)[0];
        let second_input =
            production_zone_cells(&simulation, second_workstation, ProductionZoneKind::Input)[0];
        let first_stone = insert_ground_stack(&mut simulation, ItemKind::Stone, 1, first_input);
        let second_stone = insert_ground_stack(&mut simulation, ItemKind::Stone, 1, second_input);
        let shared_wood = insert_ground_stack(&mut simulation, ItemKind::Wood, 2, shared);

        simulation
            .add_production_order(
                first_workstation,
                RecipeId::PrimitiveTool,
                ProductionTarget::Infinite,
            )
            .unwrap();
        simulation
            .add_production_order(
                second_workstation,
                RecipeId::PrimitiveTool,
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
            if total_item_quantity(&simulation, ItemKind::PrimitiveTool) >= 1 {
                break;
            }
        }
        assert!(simulation.item_world.get(shared_wood).is_none());
        assert!(
            simulation.item_world.get(first_stone).is_none()
                ^ simulation.item_world.get(second_stone).is_none(),
            "exactly one workstation must consume its private Stone input first",
        );

        let second_wood = insert_ground_stack(&mut simulation, ItemKind::Wood, 2, shared);
        for _ in 0..256 {
            simulation.advance_ticks(1).unwrap();
            if total_item_quantity(&simulation, ItemKind::PrimitiveTool) >= 2 {
                break;
            }
        }

        assert!(simulation.item_world.get(second_wood).is_none());
        assert!(simulation.item_world.get(first_stone).is_none());
        assert!(simulation.item_world.get(second_stone).is_none());
        assert_eq!(total_item_quantity(&simulation, ItemKind::PrimitiveTool), 2);
        simulation.advance_ticks(32).unwrap();
        assert_eq!(
            total_item_quantity(&simulation, ItemKind::PrimitiveTool),
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
            .set_stockpile_item_allowed(rejected, ItemKind::Wood, false)
            .unwrap();
        let item_id = insert_ground_stack(&mut simulation, ItemKind::Wood, 4, cells[2]);

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
                ItemKind::Wood,
                ItemQuantity::new(1020).unwrap(),
                WorldPosition::from_cell_center(cells[0]).unwrap(),
            ))
            .unwrap();
        simulation
            .item_world
            .insert_ground(ItemStack::new_ground(
                source_id,
                ItemKind::Wood,
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
                .filter(|item| item.kind() == ItemKind::Wood)
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
                .filter(|item| item.kind() == ItemKind::Wood)
                .count();
            let stone = simulation
                .items()
                .filter(|item| item.kind() == ItemKind::Stone)
                .count();
            if wood == 1 && stone == 1 {
                break;
            }
        }

        let wood = simulation
            .items()
            .filter(|item| item.kind() == ItemKind::Wood)
            .collect::<Vec<_>>();
        let stone = simulation
            .items()
            .filter(|item| item.kind() == ItemKind::Stone)
            .collect::<Vec<_>>();
        assert_eq!(wood.len(), 1);
        assert_eq!(wood[0].quantity().get(), 18);
        assert_eq!(stone.len(), 1);
        assert_eq!(stone[0].quantity().get(), 14);
        assert!(simulation.item_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
    }

    #[test]
    fn production_physically_supplies_exact_recipe_amounts_from_distant_stockpile_cells() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let workstation_id = simulation
            .place_workstation(WorkstationKind::Workbench, WorldCell::new(0, 0))
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
        let wood_source = insert_ground_stack(&mut simulation, ItemKind::Wood, 20, source_cells[0]);
        let stone_source =
            insert_ground_stack(&mut simulation, ItemKind::Stone, 20, source_cells[1]);
        simulation
            .add_production_order(
                workstation_id,
                RecipeId::PrimitiveTool,
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
                            item.kind() == ItemKind::Wood && item.quantity().get() == 2;
                        saw_exact_stone_supply |=
                            item.kind() == ItemKind::Stone && item.quantity().get() == 1;
                    }
                }
            }
            saw_output |= simulation.items().any(|item| {
                item.kind() == ItemKind::PrimitiveTool
                    && item
                        .ground_position()
                        .is_some_and(|position| position.containing_cell() == output_cell)
            });
            if total_item_quantity(&simulation, ItemKind::PrimitiveTool) >= 1 && saw_output {
                break;
            }
        }

        assert!(saw_exact_wood_supply);
        assert!(saw_exact_stone_supply);
        assert!(
            saw_output,
            "crafted output must physically appear in Output zone"
        );
        assert_eq!(total_item_quantity(&simulation, ItemKind::PrimitiveTool), 1);
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
            .place_workstation(WorkstationKind::Workbench, cell)
            .unwrap();
        let job_id = simulation
            .designate_craft(workstation_id, RecipeId::PrimitiveTool)
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
                .all(|item| item.kind() != ItemKind::PrimitiveTool)
        );
    }

    #[test]
    fn production_order_repeats_craft_until_remaining_runs_reaches_zero() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let workstation_id = simulation
            .place_workstation(WorkstationKind::Workbench, WorldCell::new(0, 0))
            .unwrap();
        seed_recipe_inputs(&mut simulation, workstation_id, 6, 3);
        let order_id = simulation
            .add_production_order(
                workstation_id,
                RecipeId::PrimitiveTool,
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
        assert_eq!(total_item_quantity(&simulation, ItemKind::PrimitiveTool), 3);
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
    fn manual_interruption_releases_craft_inputs_without_consuming_them() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let workstation_id = simulation
            .place_workstation(WorkstationKind::Workbench, WorldCell::new(0, 0))
            .unwrap();
        seed_recipe_inputs(&mut simulation, workstation_id, 2, 1);
        let job_id = simulation
            .designate_craft(workstation_id, RecipeId::PrimitiveTool)
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
            .place_workstation(WorkstationKind::Workbench, WorldCell::new(0, 0))
            .unwrap();
        seed_recipe_inputs(&mut simulation, workstation_id, 2, 1);
        let job_id = simulation
            .designate_craft(workstation_id, RecipeId::PrimitiveTool)
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
    fn crafted_output_leaves_output_zone_and_reaches_stockpile() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        clear_all_items(&mut simulation);
        let workstation_id = simulation
            .place_workstation(WorkstationKind::Workbench, WorldCell::new(0, 0))
            .unwrap();
        seed_recipe_inputs(&mut simulation, workstation_id, 2, 1);
        let stock_cells = distant_stockpile_cells(&simulation, WorldCell::new(0, 0), 1);
        let stockpile_id = simulation.create_stockpile(stock_cells[0]).unwrap();
        simulation
            .add_production_order(
                workstation_id,
                RecipeId::PrimitiveTool,
                ProductionTarget::finite(1),
            )
            .unwrap();
        let mut saw_output_zone = false;
        let mut reached_stockpile = false;

        for _ in 0..512 {
            simulation.advance_ticks(1).unwrap();
            for item in simulation
                .items()
                .filter(|item| item.kind() == ItemKind::PrimitiveTool)
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
        assert_eq!(total_item_quantity(&simulation, ItemKind::PrimitiveTool), 1);
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

    #[test]
    fn harvested_output_becomes_a_physical_haul_candidate_and_reaches_stockpile() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let initial_item_ids = simulation
            .items()
            .map(ItemStack::id)
            .collect::<BTreeSet<_>>();
        let stockpile_id = simulation.create_stockpile(WorldCell::new(-2, 0)).unwrap();
        for x in -1..=2 {
            simulation
                .set_stockpile_cell(stockpile_id, WorldCell::new(x, 0), true)
                .unwrap();
        }
        let (source, resource) = harvest_fixture(&simulation);
        let output_kind = match resource.kind() {
            NaturalResourceKind::Tree => ItemKind::Wood,
            NaturalResourceKind::StoneOutcrop => ItemKind::Stone,
            NaturalResourceKind::BerryBush => ItemKind::Berries,
        };
        let initial_stockpile_quantity = simulation
            .items()
            .filter(|item| {
                item.kind() == output_kind
                    && item.ground_position().is_some_and(|position| {
                        simulation.stockpile_at(position.containing_cell()) == Some(stockpile_id)
                    })
            })
            .map(|item| item.quantity().get())
            .sum::<u32>();
        simulation.designate_harvest(source).unwrap();

        let mut saw_new_carried = false;
        for _ in 0..768 {
            simulation.advance_ticks(1).unwrap();
            saw_new_carried |= simulation
                .items()
                .any(|item| !initial_item_ids.contains(&item.id()) && item.carrier().is_some());
            let stockpile_quantity = simulation
                .items()
                .filter(|item| {
                    item.kind() == output_kind
                        && item.ground_position().is_some_and(|position| {
                            simulation.stockpile_at(position.containing_cell())
                                == Some(stockpile_id)
                        })
                })
                .map(|item| item.quantity().get())
                .sum::<u32>();
            if simulation.natural_resource_at(source).unwrap().is_none()
                && stockpile_quantity == initial_stockpile_quantity + resource.yield_quantity()
            {
                break;
            }
        }

        let final_stockpile_quantity = simulation
            .items()
            .filter(|item| {
                item.kind() == output_kind
                    && item.ground_position().is_some_and(|position| {
                        simulation.stockpile_at(position.containing_cell()) == Some(stockpile_id)
                    })
            })
            .map(|item| item.quantity().get())
            .sum::<u32>();
        assert!(
            saw_new_carried,
            "harvested output must pass through Carried during haul before merge/delivery"
        );
        assert_eq!(
            final_stockpile_quantity,
            initial_stockpile_quantity + resource.yield_quantity()
        );
        assert_eq!(simulation.natural_resource_at(source).unwrap(), None);
        assert!(simulation.item_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
    }

    #[test]
    fn cancelling_construction_during_delivery_drops_material_and_cleans_reservations() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let wall_cell = empty_stockpile_cells(&simulation, 1)[0];
        let site_id = simulation
            .designate_construction(StructureKind::StoneWall, wall_cell)
            .unwrap();

        let mut carried = None;
        for _ in 0..128 {
            simulation.advance_ticks(1).unwrap();
            if let Some(item) = simulation.items().find(|item| {
                simulation.construction_world.site_for_material(item.id()) == Some(site_id)
                    && item.carrier().is_some()
            }) {
                carried = Some(item.id());
                break;
            }
        }
        let item_id =
            carried.expect("construction material must enter Carried before cancellation");
        simulation.cancel_construction(site_id).unwrap();

        assert!(simulation.construction_world.site(site_id).is_none());
        assert_eq!(
            simulation.construction_world.site_for_material(item_id),
            None
        );
        assert!(
            simulation
                .item_world
                .get(item_id)
                .unwrap()
                .ground_position()
                .is_some()
        );
        assert!(simulation.structure_at(wall_cell).is_none());
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(simulation.construction_world.indexes_are_consistent());
        assert!(simulation.item_world.indexes_are_consistent());
    }

    #[test]
    fn stone_wall_construction_delivers_physical_material_consumes_two_and_blocks_navigation() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let wall_cell = empty_stockpile_cells(&simulation, 1)[0];
        let stone_before = simulation
            .items()
            .filter(|item| item.kind() == ItemKind::Stone)
            .map(|item| item.quantity().get())
            .sum::<u32>();

        let site_id = simulation
            .designate_construction(StructureKind::StoneWall, wall_cell)
            .unwrap();
        let mut saw_carried_material = false;
        let mut saw_delivered_material = false;
        for _ in 0..768 {
            simulation.advance_ticks(1).unwrap();
            saw_carried_material |= simulation.items().any(|item| {
                simulation.construction_world.site_for_material(item.id()) == Some(site_id)
                    && item.carrier().is_some()
            });
            saw_delivered_material |=
                simulation
                    .construction_world
                    .site(site_id)
                    .is_some_and(|site| {
                        site.material_state() == Some(ConstructionMaterialState::Delivered)
                    });
            if simulation.structure_at(wall_cell) == Some(site_id) {
                break;
            }
        }

        assert!(
            saw_carried_material,
            "construction material must pass through Carried"
        );
        assert!(
            saw_delivered_material,
            "material must be delivered before construction work"
        );
        assert_eq!(simulation.structure_at(wall_cell), Some(site_id));
        assert!(simulation.construction_site_at(wall_cell).is_none());
        assert_eq!(
            simulation
                .items()
                .filter(|item| item.kind() == ItemKind::Stone)
                .map(|item| item.quantity().get())
                .sum::<u32>(),
            stone_before - StructureKind::StoneWall.material_quantity()
        );
        assert!(!simulation.is_walkable(wall_cell).unwrap());
        let cora = EntityId::new(3).unwrap();

        for y in 0..=2 {
            for x in -1..=1 {
                simulation
                    .set_terrain_override(WorldCell::new(x, y), Terrain::Grass)
                    .unwrap();
            }
        }
        let start = WorldCell::new(0, 0);
        let goal = WorldCell::new(0, 2);
        let cora_state = simulation.characters.get_mut(&cora).unwrap();
        cora_state.set_position(WorldPosition::from_cell_center(start).unwrap());
        cora_state.set_movement(MovementState::Idle);
        simulation
            .move_to(cora, WorldPosition::from_cell_center(goal).unwrap())
            .unwrap();
        assert!(
            simulation
                .characters
                .get(&cora)
                .unwrap()
                .navigation_waypoints()
                .all(|position| position.containing_cell() != wall_cell),
            "A* route must go around the completed wall rather than through it"
        );
        assert!(simulation.construction_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(simulation.item_world.indexes_are_consistent());
    }

    #[test]
    fn door_designation_replaces_planned_stone_wall_without_leaving_old_site() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cell = empty_stockpile_cells(&simulation, 1)[0];
        let wall_id = simulation
            .designate_construction(StructureKind::StoneWall, cell)
            .unwrap();
        assert_eq!(simulation.construction_site_at(cell), Some(wall_id));

        let door_id = simulation
            .designate_construction(StructureKind::Door, cell)
            .unwrap();
        assert_ne!(door_id, wall_id);
        assert!(simulation.construction_world.site(wall_id).is_none());
        assert_eq!(simulation.construction_site_at(cell), Some(door_id));
        assert_eq!(
            simulation.construction_world.site(door_id).unwrap().kind(),
            StructureKind::Door
        );
        assert!(simulation.construction_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
        assert!(simulation.item_world.indexes_are_consistent());
    }

    #[test]
    fn door_designation_replaces_completed_stone_wall_with_new_door_site() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let cell = empty_stockpile_cells(&simulation, 1)[0];
        let wall_id = simulation.id_allocator.allocate().unwrap();
        simulation
            .construction_world
            .insert_site(ConstructionSite::new(
                wall_id,
                StructureKind::StoneWall,
                cell,
            ))
            .unwrap();
        simulation
            .construction_world
            .complete_site(wall_id)
            .unwrap();
        assert_eq!(simulation.structure_at(cell), Some(wall_id));
        assert!(!simulation.is_walkable(cell).unwrap());

        let door_id = simulation
            .designate_construction(StructureKind::Door, cell)
            .unwrap();
        assert_ne!(door_id, wall_id);
        assert!(simulation.construction_world.structure(wall_id).is_none());
        assert_eq!(simulation.structure_at(cell), None);
        assert_eq!(simulation.construction_site_at(cell), Some(door_id));
        assert_eq!(
            simulation.construction_world.site(door_id).unwrap().kind(),
            StructureKind::Door
        );
        assert!(simulation.is_walkable(cell).unwrap());
        assert!(simulation.construction_world.indexes_are_consistent());
    }

    #[test]
    fn door_is_passable_opens_for_a_character_and_closes_after_the_hold_window() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        let start = WorldCell::new(0, 1);
        let door_cell = WorldCell::new(1, 1);
        let goal = WorldCell::new(2, 1);
        for cell in [start, door_cell, goal] {
            simulation
                .set_terrain_override(cell, Terrain::Grass)
                .unwrap();
        }
        let door_id = simulation.id_allocator.allocate().unwrap();
        simulation
            .construction_world
            .insert_site(ConstructionSite::new(
                door_id,
                StructureKind::Door,
                door_cell,
            ))
            .unwrap();
        simulation
            .construction_world
            .complete_site(door_id)
            .unwrap();

        assert!(simulation.is_walkable(door_cell).unwrap());
        assert_eq!(
            simulation
                .construction_world
                .structure(door_id)
                .unwrap()
                .door_state(),
            Some(crate::DoorState::Closed)
        );

        let cora = cora();
        let cora_state = simulation.characters.get_mut(&cora).unwrap();
        cora_state.set_position(WorldPosition::from_cell_center(start).unwrap());
        cora_state.set_movement(MovementState::Idle);
        simulation
            .move_to(cora, WorldPosition::from_cell_center(goal).unwrap())
            .unwrap();

        let mut saw_open = false;
        for _ in 0..16 {
            simulation.advance_ticks(1).unwrap();
            if simulation
                .construction_world
                .structure(door_id)
                .unwrap()
                .door_state()
                == Some(crate::DoorState::Open)
            {
                saw_open = true;
            }
            if character(&simulation, cora).position()
                == WorldPosition::from_cell_center(goal).unwrap()
            {
                break;
            }
        }
        assert!(
            saw_open,
            "door must visibly open while a character passes through it"
        );
        assert_eq!(
            character(&simulation, cora).position(),
            WorldPosition::from_cell_center(goal).unwrap()
        );

        simulation
            .advance_ticks(crate::DOOR_HOLD_OPEN_TICKS + 1)
            .unwrap();
        assert_eq!(
            simulation
                .construction_world
                .structure(door_id)
                .unwrap()
                .door_state(),
            Some(crate::DoorState::Closed)
        );
    }

    #[test]
    fn pathfinding_treats_doors_as_passable_but_prefers_a_short_door_free_detour() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        for y in 0..=1 {
            for x in 0..=4 {
                simulation
                    .set_terrain_override(WorldCell::new(x, y), Terrain::Grass)
                    .unwrap();
            }
        }
        for x in 1..=3 {
            let cell = WorldCell::new(x, 0);
            let id = simulation.id_allocator.allocate().unwrap();
            simulation
                .construction_world
                .insert_site(ConstructionSite::new(id, StructureKind::Door, cell))
                .unwrap();
            simulation.construction_world.complete_site(id).unwrap();
        }

        let path =
            crate::pathfinding::find_path(&simulation, WorldCell::new(0, 0), WorldCell::new(4, 0))
                .unwrap()
                .unwrap();
        assert!(path.iter().any(|cell| cell.y() == 1));
        assert!(
            path.iter()
                .all(|cell| simulation.structure_kind_at(*cell) != Some(StructureKind::Door))
        );
    }

    fn clear_all_items(simulation: &mut Simulation) {
        let items = simulation
            .items()
            .map(|item| (item.id(), item.quantity().get()))
            .collect::<Vec<_>>();
        for (item_id, quantity) in items {
            simulation.item_world.consume(item_id, quantity).unwrap();
        }
    }

    fn production_zone_cells(
        simulation: &Simulation,
        workstation_id: EntityId,
        kind: ProductionZoneKind,
    ) -> Vec<WorldCell> {
        simulation
            .production_logistics_world
            .get(workstation_id)
            .unwrap()
            .cells(kind)
            .collect()
    }

    fn insert_ground_stack(
        simulation: &mut Simulation,
        kind: ItemKind,
        quantity: u32,
        cell: WorldCell,
    ) -> EntityId {
        let id = simulation.id_allocator.allocate().unwrap();
        simulation
            .item_world
            .insert_ground(ItemStack::new_ground(
                id,
                kind,
                ItemQuantity::new(quantity).unwrap(),
                WorldPosition::from_cell_center(cell).unwrap(),
            ))
            .unwrap();
        id
    }

    fn seed_recipe_inputs(
        simulation: &mut Simulation,
        workstation_id: EntityId,
        wood: u32,
        stone: u32,
    ) -> (EntityId, EntityId) {
        let inputs = production_zone_cells(simulation, workstation_id, ProductionZoneKind::Input);
        assert!(inputs.len() >= 2, "workbench fixture needs two Input cells");
        let wood_id = insert_ground_stack(simulation, ItemKind::Wood, wood, inputs[0]);
        let stone_id = insert_ground_stack(simulation, ItemKind::Stone, stone, inputs[1]);
        (wood_id, stone_id)
    }

    fn total_item_quantity(simulation: &Simulation, kind: ItemKind) -> u32 {
        simulation
            .items()
            .filter(|item| item.kind() == kind)
            .map(|item| item.quantity().get())
            .sum()
    }

    fn distant_stockpile_cells(
        simulation: &Simulation,
        origin: WorldCell,
        count: usize,
    ) -> Vec<WorldCell> {
        let cells = (-5..=5)
            .flat_map(|y| (-7..=7).map(move |x| WorldCell::new(x, y)))
            .filter(|cell| cell_manhattan_distance(*cell, origin) >= 4)
            .filter(|cell| simulation.validate_stockpile_cell(*cell).is_ok())
            .take(count)
            .collect::<Vec<_>>();
        assert_eq!(
            cells.len(),
            count,
            "seed 0 must expose distant stockpile cells"
        );
        cells
    }

    fn shared_workbench_fixture_cells(
        simulation: &Simulation,
    ) -> (WorldCell, WorldCell, WorldCell, WorldCell, WorldCell) {
        for y in -4..=4 {
            for x in -5..=5 {
                let shared = WorldCell::new(x, y);
                let first_bench = WorldCell::new(x - 1, y);
                let second_bench = WorldCell::new(x + 1, y);
                let first_stone = WorldCell::new(x - 1, y + 1);
                let second_stone = WorldCell::new(x + 1, y + 1);
                if simulation.validate_stockpile_cell(shared).is_ok()
                    && simulation.validate_stockpile_cell(first_stone).is_ok()
                    && simulation.validate_stockpile_cell(second_stone).is_ok()
                    && simulation.validate_workstation_cell(first_bench).is_ok()
                    && simulation.validate_workstation_cell(second_bench).is_ok()
                {
                    return (shared, first_bench, second_bench, first_stone, second_stone);
                }
            }
        }
        panic!("seed 0 must expose a shared-input two-workbench fixture");
    }

    fn empty_stockpile_cells(simulation: &Simulation, count: usize) -> Vec<WorldCell> {
        let occupied = simulation
            .items()
            .filter_map(ItemStack::ground_position)
            .map(WorldPosition::containing_cell)
            .collect::<BTreeSet<_>>();
        let cells = (-5..=5)
            .flat_map(|y| (-7..=7).map(move |x| WorldCell::new(x, y)))
            .filter(|cell| simulation.is_explored(*cell))
            .filter(|cell| simulation.effective_terrain_at(*cell).unwrap() == Terrain::Grass)
            .filter(|cell| simulation.natural_resource_at(*cell).unwrap().is_none())
            .filter(|cell| !occupied.contains(cell))
            .take(count)
            .collect::<Vec<_>>();
        assert_eq!(
            cells.len(),
            count,
            "seed 0 must expose enough empty stockpile cells"
        );
        cells
    }

    fn transporting_haul_fixture(simulation: &mut Simulation) -> (EntityId, EntityId, EntityId) {
        for _ in 0..64 {
            simulation.advance_ticks(1).unwrap();
            if let Some(job) = simulation.jobs().find(|job| {
                matches!(job.state(), JobState::Transporting { .. })
                    && matches!(job.kind(), JobKind::Haul { .. })
            }) {
                let JobKind::Haul { item_id, .. } = job.kind() else {
                    unreachable!();
                };
                return (job.id(), item_id, job.state().worker().unwrap());
            }
        }
        panic!("expected a haul job to enter Transporting state");
    }

    fn harvest_fixture(simulation: &Simulation) -> (WorldCell, NaturalResource) {
        for y in -5..=5 {
            for x in -7..=7 {
                let cell = WorldCell::new(x, y);
                if !simulation.is_explored(cell) {
                    continue;
                }
                let Some(resource) = simulation.natural_resource_at(cell).unwrap() else {
                    continue;
                };
                if resource.kind() == NaturalResourceKind::BerryBush {
                    continue;
                }
                let destination = WorldPosition::from_cell_center(cell).unwrap();
                if simulation.characters().any(|character| {
                    simulation
                        .plan_navigation_route(character.id(), destination)
                        .is_ok()
                }) {
                    return (cell, resource);
                }
            }
        }
        panic!("seed 0 must expose at least one reachable natural-resource source");
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

    fn character(simulation: &Simulation, id: EntityId) -> &Character {
        simulation.characters.get(&id).unwrap()
    }

    fn character_mut(simulation: &mut Simulation, id: EntityId) -> &mut Character {
        simulation.characters.get_mut(&id).unwrap()
    }

    fn position_at_cell(cell: WorldCell) -> WorldPosition {
        WorldPosition::from_cell_center(cell).unwrap()
    }

    fn blocked_stop_position(source: WorldCell, direction: Direction) -> WorldPosition {
        let origin = WorldPosition::from_cell_origin(source).unwrap();
        match direction {
            Direction::East => origin.checked_translate(1023, 512).unwrap(),
            Direction::West => origin.checked_translate(0, 512).unwrap(),
            Direction::North => origin.checked_translate(512, 1023).unwrap(),
            Direction::South => origin.checked_translate(512, 0).unwrap(),
        }
    }

    fn near_cell_edge(cell: WorldCell, direction: Direction) -> WorldPosition {
        let origin = WorldPosition::from_cell_origin(cell).unwrap();
        match direction {
            Direction::East => origin.checked_translate(1022, 512).unwrap(),
            Direction::West => origin.checked_translate(1, 512).unwrap(),
            Direction::North => origin.checked_translate(512, 1022).unwrap(),
            Direction::South => origin.checked_translate(512, 1).unwrap(),
        }
    }

    fn place_on_grass(simulation: &mut Simulation, id: EntityId, position: WorldCell) {
        assert_eq!(
            simulation.effective_terrain_at(position).unwrap(),
            Terrain::Grass
        );
        simulation
            .characters
            .get_mut(&id)
            .unwrap()
            .set_position(position_at_cell(position));
    }

    fn persisted_movement_stops_on(terrain: Terrain) {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let (position, direction) = find_adjacent_terrain_fixture(&simulation, terrain);
        place_on_grass(&mut simulation, cora, position);
        simulation
            .characters
            .get_mut(&cora)
            .unwrap()
            .set_movement(MovementState::ManualDirectional { direction });

        simulation.advance_ticks(3).unwrap();

        assert_eq!(
            character(&simulation, cora).position(),
            blocked_stop_position(position, direction)
        );
        assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
    }

    fn find_replacement_fixture(simulation: &Simulation) -> (WorldCell, Direction, Direction) {
        for y in -64..=64 {
            for x in -64..=64 {
                let position = WorldCell::new(x, y);
                if raw_terrain_at(simulation, position) != Terrain::Grass {
                    continue;
                }
                for accepted in [
                    Direction::East,
                    Direction::West,
                    Direction::North,
                    Direction::South,
                ] {
                    if raw_terrain_at(simulation, accepted.adjacent(position).unwrap())
                        != Terrain::Grass
                    {
                        continue;
                    }
                    for blocked in [
                        Direction::East,
                        Direction::West,
                        Direction::North,
                        Direction::South,
                    ] {
                        if raw_terrain_at(simulation, blocked.adjacent(position).unwrap())
                            != Terrain::Grass
                        {
                            return (position, accepted, blocked);
                        }
                    }
                }
            }
        }
        panic!("expected an adjacent grass and blocked terrain fixture");
    }

    fn find_turn_fixture(simulation: &Simulation) -> WorldCell {
        for y in -64..=64 {
            for x in -64..=64 {
                let position = WorldCell::new(x, y);
                let east = Direction::East.adjacent(position).unwrap();
                let north_from_east = Direction::North.adjacent(east).unwrap();
                if [position, east, north_from_east]
                    .into_iter()
                    .all(|cell| raw_terrain_at(simulation, cell) == Terrain::Grass)
                {
                    return position;
                }
            }
        }
        panic!("expected an east then north grass fixture");
    }

    fn find_adjacent_terrain_fixture(
        simulation: &Simulation,
        target_terrain: Terrain,
    ) -> (WorldCell, Direction) {
        for y in -64..=64 {
            for x in -64..=64 {
                let position = WorldCell::new(x, y);
                if raw_terrain_at(simulation, position) != Terrain::Grass {
                    continue;
                }
                for direction in [
                    Direction::East,
                    Direction::West,
                    Direction::North,
                    Direction::South,
                ] {
                    if raw_terrain_at(simulation, direction.adjacent(position).unwrap())
                        == target_terrain
                    {
                        return (position, direction);
                    }
                }
            }
        }
        panic!("expected adjacent {target_terrain:?} terrain fixture");
    }

    fn raw_terrain_at(simulation: &Simulation, position: WorldCell) -> Terrain {
        let (coordinate, local) = position.split();
        simulation
            .generated_chunk(coordinate)
            .unwrap()
            .terrain_at(local)
            .unwrap()
    }

    fn find_raw_grass_with_neighbor(
        simulation: &Simulation,
        neighbor: Terrain,
    ) -> (WorldCell, Direction) {
        for y in -64..=64 {
            for x in -64..=64 {
                let start = WorldCell::new(x, y);
                if raw_terrain_at(simulation, start) != Terrain::Grass {
                    continue;
                }
                for direction in [
                    Direction::East,
                    Direction::West,
                    Direction::North,
                    Direction::South,
                ] {
                    let target = direction.adjacent(start).unwrap();
                    if raw_terrain_at(simulation, target) == neighbor {
                        return (start, direction);
                    }
                }
            }
        }
        panic!("expected raw grass next to {neighbor:?}");
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
