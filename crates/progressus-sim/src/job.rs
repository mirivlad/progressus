use std::collections::BTreeMap;

use crate::{EntityId, WorldCell};

pub const HARVEST_WORK_TICKS: u32 = 4;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum JobKind {
    Harvest { source: WorldCell },
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum JobState {
    Available,
    Reserved {
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
            Self::Reserved { worker_id } | Self::Working { worker_id, .. } => Some(worker_id),
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
        }
        self.jobs.insert(id, job);
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
        }
        self.bump_revision()?;
        Ok(job)
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
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JobWorldError {
    DuplicateJob(EntityId),
    UnknownJob(EntityId),
    HarvestSourceAlreadyDesignated(WorldCell),
    WorkerAlreadyReserved(EntityId),
    JobNotAvailable(EntityId),
    JobNotReserved(EntityId),
    JobNotWorking(EntityId),
    IndexCorruption,
    RevisionOverflow,
}

#[cfg(test)]
mod tests {
    use crate::{EntityId, WorldCell};

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
}
