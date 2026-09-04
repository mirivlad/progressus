use std::collections::{BTreeMap, BTreeSet};

use crate::{EntityId, RecipeId, WorldCell};

pub const HARVEST_WORK_TICKS: u32 = 4;
pub const EAT_WORK_TICKS: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum JobKind {
    Harvest {
        source: WorldCell,
    },
    Eat {
        character_id: EntityId,
        item_id: EntityId,
    },
    Haul {
        item_id: EntityId,
        stockpile_id: EntityId,
        destination: WorldCell,
    },
    Craft {
        workstation_id: EntityId,
        order_id: EntityId,
        recipe_id: RecipeId,
    },
    SupplyProduction {
        workstation_id: EntityId,
        item_id: EntityId,
        destination: WorldCell,
    },
    DeliverConstruction {
        site_id: EntityId,
        item_id: EntityId,
    },
    Construct {
        site_id: EntityId,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum JobState {
    Available,
    Reserved {
        worker_id: EntityId,
    },
    Transporting {
        worker_id: EntityId,
    },
    Working {
        worker_id: EntityId,
        remaining_ticks: u32,
    },
}

impl JobState {
    pub const fn worker(self) -> Option<EntityId> {
        match self {
            Self::Available => None,
            Self::Reserved { worker_id }
            | Self::Transporting { worker_id }
            | Self::Working { worker_id, .. } => Some(worker_id),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Job {
    id: EntityId,
    kind: JobKind,
    state: JobState,
}

impl Job {
    pub(crate) const fn new(id: EntityId, kind: JobKind) -> Self {
        Self {
            id,
            kind,
            state: JobState::Available,
        }
    }

    pub const fn id(&self) -> EntityId {
        self.id
    }

    pub const fn kind(&self) -> JobKind {
        self.kind
    }

    pub const fn state(&self) -> JobState {
        self.state
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct JobWorld {
    jobs: BTreeMap<EntityId, Job>,
    harvest_by_source: BTreeMap<WorldCell, EntityId>,
    eat_by_character: BTreeMap<EntityId, EntityId>,
    eat_by_item: BTreeMap<EntityId, EntityId>,
    haul_by_item: BTreeMap<EntityId, EntityId>,
    haul_by_destination: BTreeMap<WorldCell, EntityId>,
    production_supply_by_item: BTreeMap<EntityId, EntityId>,
    production_supply_by_destination: BTreeMap<WorldCell, EntityId>,
    craft_by_workstation: BTreeMap<EntityId, EntityId>,
    craft_by_order: BTreeMap<EntityId, EntityId>,
    craft_item_reservations: BTreeMap<EntityId, EntityId>,
    craft_items_by_job: BTreeMap<EntityId, BTreeSet<EntityId>>,
    construction_delivery_by_site: BTreeMap<EntityId, EntityId>,
    construct_by_site: BTreeMap<EntityId, EntityId>,
    worker_jobs: BTreeMap<EntityId, EntityId>,
    revision: u64,
}

impl JobWorld {
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = &Job> {
        self.jobs.values()
    }

    pub(crate) fn get(&self, id: EntityId) -> Option<&Job> {
        self.jobs.get(&id)
    }

    pub(crate) fn job_for_worker(&self, worker_id: EntityId) -> Option<EntityId> {
        self.worker_jobs.get(&worker_id).copied()
    }

    pub(crate) fn harvest_job_for_source(&self, source: WorldCell) -> Option<EntityId> {
        self.harvest_by_source.get(&source).copied()
    }

    pub(crate) fn eat_job_for_character(&self, character_id: EntityId) -> Option<EntityId> {
        self.eat_by_character.get(&character_id).copied()
    }

    pub(crate) fn eat_job_for_item(&self, item_id: EntityId) -> Option<EntityId> {
        self.eat_by_item.get(&item_id).copied()
    }

    pub(crate) fn haul_job_for_item(&self, item_id: EntityId) -> Option<EntityId> {
        self.haul_by_item.get(&item_id).copied()
    }

    pub(crate) fn haul_job_for_destination(&self, destination: WorldCell) -> Option<EntityId> {
        self.haul_by_destination.get(&destination).copied()
    }

    pub(crate) fn production_supply_job_for_item(&self, item_id: EntityId) -> Option<EntityId> {
        self.production_supply_by_item.get(&item_id).copied()
    }

    pub(crate) fn production_supply_job_for_destination(
        &self,
        destination: WorldCell,
    ) -> Option<EntityId> {
        self.production_supply_by_destination
            .get(&destination)
            .copied()
    }

    pub(crate) fn logistics_job_for_item(&self, item_id: EntityId) -> Option<EntityId> {
        self.haul_job_for_item(item_id)
            .or_else(|| self.production_supply_job_for_item(item_id))
    }

    pub(crate) fn item_job_for_item(&self, item_id: EntityId) -> Option<EntityId> {
        self.logistics_job_for_item(item_id)
            .or_else(|| self.eat_job_for_item(item_id))
            .or_else(|| self.craft_job_for_item(item_id))
    }

    pub(crate) fn logistics_job_for_destination(&self, destination: WorldCell) -> Option<EntityId> {
        self.haul_job_for_destination(destination)
            .or_else(|| self.production_supply_job_for_destination(destination))
    }

    pub(crate) fn craft_job_for_workstation(&self, workstation_id: EntityId) -> Option<EntityId> {
        self.craft_by_workstation.get(&workstation_id).copied()
    }

    pub(crate) fn craft_job_for_order(&self, order_id: EntityId) -> Option<EntityId> {
        self.craft_by_order.get(&order_id).copied()
    }

    pub(crate) fn craft_job_for_item(&self, item_id: EntityId) -> Option<EntityId> {
        self.craft_item_reservations.get(&item_id).copied()
    }

    pub(crate) fn craft_reserved_items(&self, job_id: EntityId) -> Option<&BTreeSet<EntityId>> {
        self.craft_items_by_job.get(&job_id)
    }

    pub(crate) fn construction_delivery_job_for_site(&self, site_id: EntityId) -> Option<EntityId> {
        self.construction_delivery_by_site.get(&site_id).copied()
    }

    pub(crate) fn construct_job_for_site(&self, site_id: EntityId) -> Option<EntityId> {
        self.construct_by_site.get(&site_id).copied()
    }

    pub(crate) fn insert(&mut self, job: Job) -> Result<(), JobWorldError> {
        let id = job.id();
        if self.jobs.contains_key(&id) {
            return Err(JobWorldError::DuplicateJob(id));
        }
        match job.kind() {
            JobKind::Harvest { source } => {
                if self.harvest_by_source.contains_key(&source) {
                    return Err(JobWorldError::HarvestSourceAlreadyDesignated(source));
                }
                self.harvest_by_source.insert(source, id);
            }
            JobKind::Eat {
                character_id,
                item_id,
            } => {
                if self.eat_by_character.contains_key(&character_id) {
                    return Err(JobWorldError::EatCharacterAlreadyDesignated(character_id));
                }
                if self.item_job_for_item(item_id).is_some() {
                    return Err(JobWorldError::EatItemAlreadyReserved(item_id));
                }
                self.eat_by_character.insert(character_id, id);
                self.eat_by_item.insert(item_id, id);
            }
            JobKind::Haul {
                item_id,
                destination,
                ..
            } => {
                if self.item_job_for_item(item_id).is_some() {
                    return Err(JobWorldError::HaulItemAlreadyReserved(item_id));
                }
                if self.logistics_job_for_destination(destination).is_some() {
                    return Err(JobWorldError::HaulDestinationAlreadyReserved(destination));
                }
                self.haul_by_item.insert(item_id, id);
                self.haul_by_destination.insert(destination, id);
            }
            JobKind::SupplyProduction {
                item_id,
                destination,
                ..
            } => {
                if self.item_job_for_item(item_id).is_some() {
                    return Err(JobWorldError::ProductionSupplyItemAlreadyReserved(item_id));
                }
                if self.logistics_job_for_destination(destination).is_some() {
                    return Err(JobWorldError::ProductionSupplyDestinationAlreadyReserved(
                        destination,
                    ));
                }
                self.production_supply_by_item.insert(item_id, id);
                self.production_supply_by_destination
                    .insert(destination, id);
            }
            JobKind::Craft {
                workstation_id,
                order_id,
                ..
            } => {
                if self.craft_by_workstation.contains_key(&workstation_id) {
                    return Err(JobWorldError::CraftWorkstationAlreadyDesignated(
                        workstation_id,
                    ));
                }
                if self.craft_by_order.contains_key(&order_id) {
                    return Err(JobWorldError::CraftOrderAlreadyDesignated(order_id));
                }
                self.craft_by_workstation.insert(workstation_id, id);
                self.craft_by_order.insert(order_id, id);
            }
            JobKind::DeliverConstruction { site_id, .. } => {
                if self.construction_delivery_by_site.contains_key(&site_id) {
                    return Err(JobWorldError::ConstructionDeliveryAlreadyDesignated(
                        site_id,
                    ));
                }
                self.construction_delivery_by_site.insert(site_id, id);
            }
            JobKind::Construct { site_id } => {
                if self.construct_by_site.contains_key(&site_id) {
                    return Err(JobWorldError::ConstructionAlreadyDesignated(site_id));
                }
                self.construct_by_site.insert(site_id, id);
            }
        }
        self.jobs.insert(id, job);
        self.bump_revision()
    }

    pub(crate) fn reserve_craft_items(
        &mut self,
        job_id: EntityId,
        item_ids: &[EntityId],
    ) -> Result<(), JobWorldError> {
        let job = self
            .jobs
            .get(&job_id)
            .ok_or(JobWorldError::UnknownJob(job_id))?;
        if !matches!(job.kind(), JobKind::Craft { .. }) {
            return Err(JobWorldError::JobNotCraft(job_id));
        }
        if self.craft_items_by_job.contains_key(&job_id) {
            return Err(JobWorldError::CraftInputsAlreadyReserved(job_id));
        }
        let unique = item_ids.iter().copied().collect::<BTreeSet<_>>();
        if unique.len() != item_ids.len() {
            return Err(JobWorldError::IndexCorruption);
        }
        for item_id in &unique {
            if self.item_job_for_item(*item_id).is_some() {
                return Err(JobWorldError::CraftItemAlreadyReserved(*item_id));
            }
        }
        for item_id in &unique {
            self.craft_item_reservations.insert(*item_id, job_id);
        }
        self.craft_items_by_job.insert(job_id, unique);
        self.bump_revision()
    }

    pub(crate) fn reserve_worker(
        &mut self,
        job_id: EntityId,
        worker_id: EntityId,
    ) -> Result<(), JobWorldError> {
        if self.worker_jobs.contains_key(&worker_id) {
            return Err(JobWorldError::WorkerAlreadyReserved(worker_id));
        }
        let job = self
            .jobs
            .get_mut(&job_id)
            .ok_or(JobWorldError::UnknownJob(job_id))?;
        if job.state != JobState::Available {
            return Err(JobWorldError::JobNotAvailable(job_id));
        }
        job.state = JobState::Reserved { worker_id };
        self.worker_jobs.insert(worker_id, job_id);
        self.bump_revision()
    }

    pub(crate) fn start_transporting(&mut self, job_id: EntityId) -> Result<(), JobWorldError> {
        let job = self
            .jobs
            .get_mut(&job_id)
            .ok_or(JobWorldError::UnknownJob(job_id))?;
        let JobState::Reserved { worker_id } = job.state else {
            return Err(JobWorldError::JobNotReserved(job_id));
        };
        job.state = JobState::Transporting { worker_id };
        self.bump_revision()
    }

    pub(crate) fn start_working(
        &mut self,
        job_id: EntityId,
        remaining_ticks: u32,
    ) -> Result<(), JobWorldError> {
        let job = self
            .jobs
            .get_mut(&job_id)
            .ok_or(JobWorldError::UnknownJob(job_id))?;
        let JobState::Reserved { worker_id } = job.state else {
            return Err(JobWorldError::JobNotReserved(job_id));
        };
        job.state = JobState::Working {
            worker_id,
            remaining_ticks,
        };
        self.bump_revision()
    }

    pub(crate) fn set_remaining_work(
        &mut self,
        job_id: EntityId,
        remaining_ticks: u32,
    ) -> Result<(), JobWorldError> {
        let job = self
            .jobs
            .get_mut(&job_id)
            .ok_or(JobWorldError::UnknownJob(job_id))?;
        let JobState::Working { worker_id, .. } = job.state else {
            return Err(JobWorldError::JobNotWorking(job_id));
        };
        job.state = JobState::Working {
            worker_id,
            remaining_ticks,
        };
        self.bump_revision()
    }

    pub(crate) fn release_worker(&mut self, job_id: EntityId) -> Result<(), JobWorldError> {
        let job = self
            .jobs
            .get_mut(&job_id)
            .ok_or(JobWorldError::UnknownJob(job_id))?;
        let worker_id = job
            .state
            .worker()
            .ok_or(JobWorldError::JobNotReserved(job_id))?;
        if self.worker_jobs.remove(&worker_id) != Some(job_id) {
            return Err(JobWorldError::IndexCorruption);
        }
        job.state = JobState::Available;
        self.release_craft_items_inner(job_id)?;
        self.bump_revision()
    }

    pub(crate) fn remove(&mut self, job_id: EntityId) -> Result<Job, JobWorldError> {
        let job = self
            .jobs
            .remove(&job_id)
            .ok_or(JobWorldError::UnknownJob(job_id))?;
        if let Some(worker_id) = job.state().worker()
            && self.worker_jobs.remove(&worker_id) != Some(job_id)
        {
            return Err(JobWorldError::IndexCorruption);
        }
        match job.kind() {
            JobKind::Harvest { source } => {
                if self.harvest_by_source.remove(&source) != Some(job_id) {
                    return Err(JobWorldError::IndexCorruption);
                }
            }
            JobKind::Eat {
                character_id,
                item_id,
            } => {
                if self.eat_by_character.remove(&character_id) != Some(job_id)
                    || self.eat_by_item.remove(&item_id) != Some(job_id)
                {
                    return Err(JobWorldError::IndexCorruption);
                }
            }
            JobKind::Haul {
                item_id,
                destination,
                ..
            } => {
                if self.haul_by_item.remove(&item_id) != Some(job_id)
                    || self.haul_by_destination.remove(&destination) != Some(job_id)
                {
                    return Err(JobWorldError::IndexCorruption);
                }
            }
            JobKind::SupplyProduction {
                item_id,
                destination,
                ..
            } => {
                if self.production_supply_by_item.remove(&item_id) != Some(job_id)
                    || self.production_supply_by_destination.remove(&destination) != Some(job_id)
                {
                    return Err(JobWorldError::IndexCorruption);
                }
            }
            JobKind::Craft {
                workstation_id,
                order_id,
                ..
            } => {
                if self.craft_by_workstation.remove(&workstation_id) != Some(job_id)
                    || self.craft_by_order.remove(&order_id) != Some(job_id)
                {
                    return Err(JobWorldError::IndexCorruption);
                }
                self.release_craft_items_inner(job_id)?;
            }
            JobKind::DeliverConstruction { site_id, .. } => {
                if self.construction_delivery_by_site.remove(&site_id) != Some(job_id) {
                    return Err(JobWorldError::IndexCorruption);
                }
            }
            JobKind::Construct { site_id } => {
                if self.construct_by_site.remove(&site_id) != Some(job_id) {
                    return Err(JobWorldError::IndexCorruption);
                }
            }
        }
        self.bump_revision()?;
        Ok(job)
    }

    fn release_craft_items_inner(&mut self, job_id: EntityId) -> Result<bool, JobWorldError> {
        let Some(items) = self.craft_items_by_job.remove(&job_id) else {
            return Ok(false);
        };
        for item_id in items {
            if self.craft_item_reservations.remove(&item_id) != Some(job_id) {
                return Err(JobWorldError::IndexCorruption);
            }
        }
        Ok(true)
    }

    fn bump_revision(&mut self) -> Result<(), JobWorldError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(JobWorldError::RevisionOverflow)?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn indexes_are_consistent(&self) -> bool {
        for (id, job) in &self.jobs {
            match job.kind() {
                JobKind::Harvest { source } => {
                    if self.harvest_by_source.get(&source) != Some(id) {
                        return false;
                    }
                }
                JobKind::Eat {
                    character_id,
                    item_id,
                } => {
                    if self.eat_by_character.get(&character_id) != Some(id)
                        || self.eat_by_item.get(&item_id) != Some(id)
                    {
                        return false;
                    }
                }
                JobKind::Haul {
                    item_id,
                    destination,
                    ..
                } => {
                    if self.haul_by_item.get(&item_id) != Some(id)
                        || self.haul_by_destination.get(&destination) != Some(id)
                    {
                        return false;
                    }
                }
                JobKind::SupplyProduction {
                    item_id,
                    destination,
                    ..
                } => {
                    if self.production_supply_by_item.get(&item_id) != Some(id)
                        || self.production_supply_by_destination.get(&destination) != Some(id)
                    {
                        return false;
                    }
                }
                JobKind::Craft {
                    workstation_id,
                    order_id,
                    ..
                } => {
                    if self.craft_by_workstation.get(&workstation_id) != Some(id)
                        || self.craft_by_order.get(&order_id) != Some(id)
                    {
                        return false;
                    }
                    if let Some(items) = self.craft_items_by_job.get(id)
                        && items
                            .iter()
                            .any(|item_id| self.craft_item_reservations.get(item_id) != Some(id))
                    {
                        return false;
                    }
                }
                JobKind::DeliverConstruction { site_id, .. } => {
                    if self.construction_delivery_by_site.get(&site_id) != Some(id) {
                        return false;
                    }
                }
                JobKind::Construct { site_id } => {
                    if self.construct_by_site.get(&site_id) != Some(id) {
                        return false;
                    }
                }
            }
            if let Some(worker_id) = job.state().worker()
                && self.worker_jobs.get(&worker_id) != Some(id)
            {
                return false;
            }
        }
        self.worker_jobs.iter().all(|(worker_id, job_id)| {
            self.jobs
                .get(job_id)
                .is_some_and(|job| job.state().worker() == Some(*worker_id))
        }) && self.harvest_by_source.len()
            == self
                .jobs
                .values()
                .filter(|job| matches!(job.kind(), JobKind::Harvest { .. }))
                .count()
            && self.eat_by_character.len()
                == self
                    .jobs
                    .values()
                    .filter(|job| matches!(job.kind(), JobKind::Eat { .. }))
                    .count()
            && self.eat_by_character.len() == self.eat_by_item.len()
            && self.haul_by_item.len()
                == self
                    .jobs
                    .values()
                    .filter(|job| matches!(job.kind(), JobKind::Haul { .. }))
                    .count()
            && self.haul_by_item.len() == self.haul_by_destination.len()
            && self.production_supply_by_item.len()
                == self
                    .jobs
                    .values()
                    .filter(|job| matches!(job.kind(), JobKind::SupplyProduction { .. }))
                    .count()
            && self.production_supply_by_item.len() == self.production_supply_by_destination.len()
            && self.craft_by_workstation.len()
                == self
                    .jobs
                    .values()
                    .filter(|job| matches!(job.kind(), JobKind::Craft { .. }))
                    .count()
            && self
                .craft_item_reservations
                .iter()
                .all(|(item_id, job_id)| {
                    self.craft_items_by_job
                        .get(job_id)
                        .is_some_and(|items| items.contains(item_id))
                })
            && self.craft_items_by_job.iter().all(|(job_id, items)| {
                self.jobs
                    .get(job_id)
                    .is_some_and(|job| matches!(job.kind(), JobKind::Craft { .. }))
                    && items
                        .iter()
                        .all(|item_id| self.craft_item_reservations.get(item_id) == Some(job_id))
            })
            && self.construction_delivery_by_site.len()
                == self
                    .jobs
                    .values()
                    .filter(|job| matches!(job.kind(), JobKind::DeliverConstruction { .. }))
                    .count()
            && self.construct_by_site.len()
                == self
                    .jobs
                    .values()
                    .filter(|job| matches!(job.kind(), JobKind::Construct { .. }))
                    .count()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JobWorldError {
    DuplicateJob(EntityId),
    UnknownJob(EntityId),
    HarvestSourceAlreadyDesignated(WorldCell),
    EatCharacterAlreadyDesignated(EntityId),
    EatItemAlreadyReserved(EntityId),
    HaulItemAlreadyReserved(EntityId),
    HaulDestinationAlreadyReserved(WorldCell),
    ProductionSupplyItemAlreadyReserved(EntityId),
    ProductionSupplyDestinationAlreadyReserved(WorldCell),
    CraftWorkstationAlreadyDesignated(EntityId),
    CraftOrderAlreadyDesignated(EntityId),
    CraftItemAlreadyReserved(EntityId),
    CraftInputsAlreadyReserved(EntityId),
    ConstructionDeliveryAlreadyDesignated(EntityId),
    ConstructionAlreadyDesignated(EntityId),
    JobNotCraft(EntityId),
    WorkerAlreadyReserved(EntityId),
    JobNotAvailable(EntityId),
    JobNotReserved(EntityId),
    JobNotWorking(EntityId),
    IndexCorruption,
    RevisionOverflow,
}

#[cfg(test)]
mod tests {
    use crate::{EntityId, RecipeId, WorldCell};

    use super::{Job, JobKind, JobState, JobWorld};

    fn id(value: u64) -> EntityId {
        EntityId::new(value).unwrap()
    }

    #[test]
    fn reservation_and_release_keep_worker_and_source_indexes_consistent() {
        let mut jobs = JobWorld::default();
        let source = WorldCell::new(4, -2);
        jobs.insert(Job::new(id(10), JobKind::Harvest { source }))
            .unwrap();
        jobs.reserve_worker(id(10), id(3)).unwrap();
        assert_eq!(jobs.job_for_worker(id(3)), Some(id(10)));
        assert_eq!(jobs.harvest_job_for_source(source), Some(id(10)));
        assert_eq!(
            jobs.get(id(10)).unwrap().state(),
            JobState::Reserved { worker_id: id(3) }
        );
        assert!(jobs.indexes_are_consistent());

        jobs.release_worker(id(10)).unwrap();
        assert_eq!(jobs.job_for_worker(id(3)), None);
        assert_eq!(jobs.get(id(10)).unwrap().state(), JobState::Available);
        assert!(jobs.indexes_are_consistent());

        jobs.remove(id(10)).unwrap();
        assert_eq!(jobs.harvest_job_for_source(source), None);
        assert!(jobs.indexes_are_consistent());
    }

    #[test]
    fn eat_reserves_one_character_and_physical_item_until_removed() {
        let mut jobs = JobWorld::default();
        jobs.insert(Job::new(
            id(20),
            JobKind::Eat {
                character_id: id(3),
                item_id: id(6),
            },
        ))
        .unwrap();
        assert_eq!(jobs.eat_job_for_character(id(3)), Some(id(20)));
        assert_eq!(jobs.eat_job_for_item(id(6)), Some(id(20)));
        assert!(matches!(
            jobs.insert(Job::new(
                id(21),
                JobKind::Eat {
                    character_id: id(3),
                    item_id: id(7),
                },
            )),
            Err(super::JobWorldError::EatCharacterAlreadyDesignated(_))
        ));
        assert!(matches!(
            jobs.insert(Job::new(
                id(22),
                JobKind::Haul {
                    item_id: id(6),
                    stockpile_id: id(30),
                    destination: WorldCell::new(1, 1),
                },
            )),
            Err(super::JobWorldError::HaulItemAlreadyReserved(_))
        ));
        assert!(jobs.indexes_are_consistent());

        jobs.remove(id(20)).unwrap();
        assert_eq!(jobs.eat_job_for_character(id(3)), None);
        assert_eq!(jobs.eat_job_for_item(id(6)), None);
        assert!(jobs.indexes_are_consistent());
    }

    #[test]
    fn craft_reservations_exclude_workstation_and_input_reuse_and_cleanup() {
        let mut jobs = JobWorld::default();
        jobs.insert(Job::new(
            id(30),
            JobKind::Craft {
                workstation_id: id(20),
                order_id: id(25),
                recipe_id: RecipeId::PrimitiveTool,
            },
        ))
        .unwrap();
        assert_eq!(jobs.craft_job_for_workstation(id(20)), Some(id(30)));
        jobs.reserve_worker(id(30), id(3)).unwrap();
        jobs.reserve_craft_items(id(30), &[id(6), id(7)]).unwrap();
        assert_eq!(jobs.craft_job_for_item(id(6)), Some(id(30)));
        assert_eq!(jobs.craft_reserved_items(id(30)).unwrap().len(), 2);
        assert!(jobs.indexes_are_consistent());

        jobs.release_worker(id(30)).unwrap();
        assert!(jobs.craft_reserved_items(id(30)).is_none());
        assert_eq!(jobs.craft_job_for_item(id(6)), None);
        assert!(jobs.indexes_are_consistent());

        jobs.reserve_worker(id(30), id(4)).unwrap();
        jobs.reserve_craft_items(id(30), &[id(8), id(9)]).unwrap();
        jobs.remove(id(30)).unwrap();
        assert_eq!(jobs.craft_job_for_workstation(id(20)), None);
        assert_eq!(jobs.craft_job_for_item(id(8)), None);
        assert!(jobs.indexes_are_consistent());
    }

    #[test]
    fn construction_jobs_reserve_each_site_once_and_cleanup_indexes() {
        let mut jobs = JobWorld::default();
        jobs.insert(Job::new(
            id(40),
            JobKind::DeliverConstruction {
                site_id: id(30),
                item_id: id(6),
            },
        ))
        .unwrap();
        assert_eq!(
            jobs.construction_delivery_job_for_site(id(30)),
            Some(id(40))
        );
        assert!(jobs.indexes_are_consistent());
        jobs.remove(id(40)).unwrap();
        assert_eq!(jobs.construction_delivery_job_for_site(id(30)), None);

        jobs.insert(Job::new(id(41), JobKind::Construct { site_id: id(30) }))
            .unwrap();
        assert_eq!(jobs.construct_job_for_site(id(30)), Some(id(41)));
        assert!(jobs.indexes_are_consistent());
        jobs.remove(id(41)).unwrap();
        assert_eq!(jobs.construct_job_for_site(id(30)), None);
        assert!(jobs.indexes_are_consistent());
    }

    #[test]
    fn haul_indexes_reserve_one_item_destination_and_worker() {
        let mut jobs = JobWorld::default();
        let destination = WorldCell::new(2, 3);
        jobs.insert(Job::new(
            id(20),
            JobKind::Haul {
                item_id: id(6),
                stockpile_id: id(19),
                destination,
            },
        ))
        .unwrap();
        assert_eq!(jobs.haul_job_for_item(id(6)), Some(id(20)));
        assert_eq!(jobs.haul_job_for_destination(destination), Some(id(20)));

        jobs.reserve_worker(id(20), id(4)).unwrap();
        jobs.start_transporting(id(20)).unwrap();
        assert_eq!(
            jobs.get(id(20)).unwrap().state(),
            JobState::Transporting { worker_id: id(4) }
        );
        assert!(jobs.indexes_are_consistent());

        jobs.release_worker(id(20)).unwrap();
        assert_eq!(jobs.get(id(20)).unwrap().state(), JobState::Available);
        jobs.remove(id(20)).unwrap();
        assert_eq!(jobs.haul_job_for_item(id(6)), None);
        assert_eq!(jobs.haul_job_for_destination(destination), None);
        assert!(jobs.indexes_are_consistent());
    }
}
