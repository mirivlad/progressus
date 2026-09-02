#[cfg(test)]
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use progressus_worldgen::{WorldGenerator, WorldgenError};

use crate::clock::SimulationClock;
use crate::entity::{EntityIdAllocator, NavigationRoute};
use crate::exploration::ExploredWorld;
use crate::item::ItemWorld;
use crate::job::{JobWorld, JobWorldError};
use crate::pathfinding::{PathfindingError, find_explored_path};
use crate::stockpile::{StockpileWorld, StockpileWorldError};
use crate::world_state::ModifiedWorld;
use crate::{
    CHUNK_SIDE, CURRENT_WORLDGEN_VERSION, Character, ChunkCoord, Direction, EffectiveChunk,
    EntityId, GeneratedChunk, HARVEST_WORK_TICKS, InteractionRadius, ItemKind, ItemLocation,
    ItemQuantity, ItemStack, Job, JobKind, JobState, LocalCell, MovementState, NaturalResource,
    NaturalResourceKind, SimulationTick, Stockpile, Terrain, WorldCell, WorldPosition,
    WorldPositionError, WorldSeed, WorldgenVersion, within_interaction_range,
};

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
    stockpile_world: StockpileWorld,
    depleted_resources: BTreeSet<WorldCell>,
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
        let last_discovery_cells = characters
            .iter()
            .map(|(id, character)| (*id, character.position().containing_cell()))
            .collect();

        Ok(Self {
            generator,
            clock: SimulationClock::new(0),
            id_allocator,
            characters,
            modified_world: ModifiedWorld::default(),
            item_world,
            job_world: JobWorld::default(),
            stockpile_world: StockpileWorld::default(),
            depleted_resources: BTreeSet::new(),
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
            self.advance_characters_one_tick()?;
            self.maintain_haul_jobs()?;
            self.advance_jobs_one_tick()?;
        }

        Ok(())
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
        let route = self.plan_navigation_route(id, destination)?;
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
        if self.depleted_resources.contains(&position) {
            return Ok(None);
        }
        let (coordinate, local) = position.split();
        Ok(self.generated_chunk(coordinate)?.natural_resource_at(local))
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
                if !self.depleted_resources.contains(&cell) {
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
        if let (JobKind::Haul { item_id, .. }, JobState::Transporting { worker_id }) =
            (job.kind(), job.state())
        {
            let position = self
                .characters
                .get(&worker_id)
                .ok_or(SimulationError::UnknownCharacter(worker_id))?
                .position();
            self.drop_item(worker_id, item_id, position)?;
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
        Ok(())
    }

    pub fn generated_chunk(
        &self,
        coordinate: ChunkCoord,
    ) -> Result<GeneratedChunk, SimulationError> {
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
        destination: WorldPosition,
        route: Option<NavigationRoute>,
    ) {
        let character = self
            .characters
            .get_mut(&id)
            .expect("navigation routes are applied only to known characters");
        match route {
            Some(route) => character.set_navigation_route(route),
            None => {
                debug_assert_eq!(character.position(), destination);
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
        if let (JobKind::Haul { item_id, .. }, JobState::Transporting { .. }) =
            (job.kind(), job.state())
        {
            let position = self
                .characters
                .get(&worker_id)
                .ok_or(SimulationError::UnknownCharacter(worker_id))?
                .position();
            self.drop_item(worker_id, item_id, position)?;
        }
        self.job_world
            .release_worker(job_id)
            .map_err(SimulationError::from_job_world)?;
        Ok(())
    }

    fn maintain_haul_jobs(&mut self) -> Result<(), SimulationError> {
        let existing_haul_jobs = self
            .job_world
            .iter()
            .filter_map(|job| match job.kind() {
                JobKind::Haul { .. } => Some(job.id()),
                JobKind::Harvest { .. } => None,
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
                && self.is_walkable(destination)?;
            let item_valid = match job.state() {
                JobState::Transporting { worker_id } => self
                    .item_world
                    .get(item_id)
                    .is_some_and(|item| item.carrier() == Some(worker_id)),
                _ => self.item_world.get(item_id).is_some_and(|item| {
                    item.ground_position().is_some_and(|position| {
                        self.stockpile_world
                            .stockpile_at(position.containing_cell())
                            .is_none()
                    })
                }),
            };
            let destination_occupied = self.item_world.iter().any(|item| {
                item.id() != item_id
                    && item
                        .ground_position()
                        .is_some_and(|position| position.containing_cell() == destination)
            });
            if !destination_valid || !item_valid || destination_occupied {
                self.cancel_job(job_id)?;
            }
        }

        let mut destinations = Vec::new();
        for stockpile in self.stockpile_world.iter() {
            for cell in stockpile.cells() {
                if self.job_world.haul_job_for_destination(cell).is_some()
                    || !self.is_walkable(cell)?
                    || self.item_world.iter().any(|item| {
                        item.ground_position()
                            .is_some_and(|position| position.containing_cell() == cell)
                    })
                {
                    continue;
                }
                destinations.push((stockpile.id(), cell));
            }
        }
        let mut destinations = destinations.into_iter();
        let candidate_items = self
            .item_world
            .iter()
            .filter_map(|item| {
                let position = item.ground_position()?;
                let cell = position.containing_cell();
                (self.is_explored(cell)
                    && self.stockpile_world.stockpile_at(cell).is_none()
                    && self.job_world.haul_job_for_item(item.id()).is_none())
                .then_some(item.id())
            })
            .collect::<Vec<_>>();
        for item_id in candidate_items {
            let Some((stockpile_id, destination)) = destinations.next() else {
                break;
            };
            let job_id = self.id_allocator.allocate()?;
            self.job_world
                .insert(Job::new(
                    job_id,
                    JobKind::Haul {
                        item_id,
                        stockpile_id,
                        destination,
                    },
                ))
                .map_err(SimulationError::from_job_world)?;
        }
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
            JobKind::Haul {
                item_id,
                stockpile_id,
                destination,
            } => self.try_assign_haul(job_id, item_id, stockpile_id, destination),
        }
    }

    fn available_workers_by_distance(&self, target: WorldCell) -> Vec<EntityId> {
        let mut candidates = self
            .characters
            .values()
            .filter(|character| {
                character.movement() == MovementState::Idle
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
        if self
            .stockpile_world
            .stockpile_at(item_position.containing_cell())
            .is_some()
        {
            self.cancel_job(job_id)?;
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
        }
        Ok(())
    }

    fn advance_transporting_job(
        &mut self,
        job_id: EntityId,
        kind: JobKind,
        worker_id: EntityId,
    ) -> Result<(), SimulationError> {
        let JobKind::Haul {
            item_id,
            stockpile_id,
            destination,
        } = kind
        else {
            return Err(SimulationError::JobInvariantViolation);
        };
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
            self.drop_item(worker_id, item_id, target)?;
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
            JobKind::Haul { .. } => return Err(SimulationError::JobInvariantViolation),
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
        let item_id = self.id_allocator.allocate()?;
        let kind = match resource.kind() {
            NaturalResourceKind::Tree => ItemKind::Wood,
            NaturalResourceKind::StoneOutcrop => ItemKind::Stone,
        };
        let quantity = ItemQuantity::new(resource.yield_quantity())
            .expect("worldgen natural-resource yields are positive");
        let position = WorldPosition::from_cell_center(source)?;
        self.item_world
            .insert_ground(ItemStack::new_ground(item_id, kind, quantity, position))
            .expect("allocated item IDs are unique and harvested outputs start on the ground");
        if !self.depleted_resources.insert(source) {
            return Err(SimulationError::JobInvariantViolation);
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
        self.generated_chunk(coordinate)?
            .terrain_at(local)
            .ok_or(SimulationError::Worldgen(
                WorldgenError::CoordinateOutOfRange(coordinate),
            ))
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
                    MovementState::Navigating { .. } => {
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

        Ok(())
    }

    fn advance_navigation_one_tick(&mut self, id: EntityId) -> Result<(), SimulationError> {
        let (mut position, mut remaining, mut route) = {
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
                character.set_movement(MovementState::Idle);
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
        let character = self
            .characters
            .get_mut(&id)
            .expect("character ID came from map");
        character.set_position(position);
        character.set_navigation_route(route);
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
        Ok(self.effective_terrain_at(position)? == Terrain::Grass)
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
                .map(|cell| WorldPosition::from_cell_center(*cell))
                .collect::<Result<Vec<_>, _>>()?,
        );
        let destination_center = WorldPosition::from_cell_center(destination.containing_cell())?;
        points.push(WorldPosition::from_subunits(
            destination.x_subunits(),
            destination_center.y_subunits(),
        )?);
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
    UnknownCharacter(EntityId),
    UnknownItem(EntityId),
    UnknownJob(EntityId),
    UnknownStockpile(EntityId),
    StockpileCellUndiscovered(WorldCell),
    StockpileCellBlocked(WorldCell),
    StockpileCellOccupiedByResource(WorldCell),
    StockpileCellAlreadyOwned {
        cell: WorldCell,
        stockpile_id: EntityId,
    },
    StockpileRevisionOverflow,
    StockpileInvariantViolation,
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
            JobWorldError::RevisionOverflow => Self::JobRevisionOverflow,
            JobWorldError::DuplicateJob(_)
            | JobWorldError::HaulItemAlreadyReserved(_)
            | JobWorldError::HaulDestinationAlreadyReserved(_)
            | JobWorldError::WorkerAlreadyReserved(_)
            | JobWorldError::JobNotAvailable(_)
            | JobWorldError::JobNotReserved(_)
            | JobWorldError::JobNotWorking(_)
            | JobWorldError::IndexCorruption => Self::JobInvariantViolation,
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
    use crate::{Direction, MovementSpeed, MovementState};

    fn cora() -> EntityId {
        EntityId::new(3).unwrap()
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
    fn player_move_to_rejects_an_undiscovered_destination_and_path_gap() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let destination = WorldCell::new(20, 0);
        for x in 0..=20 {
            simulation
                .set_terrain_override(WorldCell::new(x, 0), Terrain::Grass)
                .unwrap();
        }
        assert_eq!(
            simulation.move_to(cora, WorldPosition::from_cell_center(destination).unwrap()),
            Err(SimulationError::MoveToDestinationUndiscovered(destination))
        );

        place_on_grass(&mut simulation, EntityId::new(1).unwrap(), destination);
        simulation.advance_ticks(1).unwrap();
        assert!(simulation.is_explored(destination));
        assert!(!simulation.is_explored(WorldCell::new(12, 0)));
        assert_eq!(
            simulation.move_to(cora, WorldPosition::from_cell_center(destination).unwrap()),
            Err(SimulationError::MoveToPathNotFound)
        );
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

        assert_eq!(items.len(), 4);
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
    fn rejected_move_to_preserves_an_existing_navigation_route() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let cora = cora();
        let start = WorldCell::new(0, 0);
        place_on_grass(&mut simulation, cora, start);
        let accepted = WorldPosition::from_cell_center(WorldCell::new(1, 0)).unwrap();
        let rejected = WorldPosition::from_cell_center(WorldCell::new(2, 0)).unwrap();
        simulation
            .set_terrain_override(WorldCell::new(1, 0), Terrain::Grass)
            .unwrap();
        simulation
            .set_terrain_override(WorldCell::new(2, 0), Terrain::Rock)
            .unwrap();
        simulation.move_to(cora, accepted).unwrap();

        assert_eq!(
            simulation.move_to(cora, rejected),
            Err(SimulationError::MoveToDestinationBlocked(WorldCell::new(
                2, 0
            )))
        );
        assert_eq!(
            character(&simulation, cora).movement(),
            MovementState::Navigating {
                destination: accepted
            }
        );
        assert_eq!(
            character(&simulation, cora).navigation_destination(),
            Some(accepted)
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
        assert!(simulation.jobs().next().is_none());
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
                JobKind::Harvest { .. } => None,
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
        simulation.designate_harvest(source).unwrap();

        let mut harvested_id = None;
        let mut saw_carried = false;
        for _ in 0..768 {
            simulation.advance_ticks(1).unwrap();
            if harvested_id.is_none() {
                harvested_id = simulation
                    .items()
                    .find(|item| !initial_item_ids.contains(&item.id()))
                    .map(ItemStack::id);
            }
            if let Some(item_id) = harvested_id {
                let item = simulation.item_world.get(item_id).unwrap();
                saw_carried |= item.carrier().is_some();
                if item.ground_position().is_some_and(|position| {
                    simulation.stockpile_at(position.containing_cell()) == Some(stockpile_id)
                }) {
                    break;
                }
            }
        }

        let harvested_id = harvested_id.expect("harvest must create a physical item stack");
        let harvested = simulation.item_world.get(harvested_id).unwrap();
        assert!(
            saw_carried,
            "harvested output must pass through Carried during haul"
        );
        assert_eq!(
            harvested.kind(),
            match resource.kind() {
                NaturalResourceKind::Tree => ItemKind::Wood,
                NaturalResourceKind::StoneOutcrop => ItemKind::Stone,
            }
        );
        assert_eq!(harvested.quantity().get(), resource.yield_quantity());
        assert!(harvested.ground_position().is_some_and(|position| {
            simulation.stockpile_at(position.containing_cell()) == Some(stockpile_id)
        }));
        assert_eq!(simulation.natural_resource_at(source).unwrap(), None);
        assert!(simulation.item_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
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
}
