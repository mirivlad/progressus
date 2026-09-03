use std::collections::BTreeMap;

use crate::{EntityId, WorldCell};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum WorkstationKind {
    Workbench,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Workstation {
    id: EntityId,
    kind: WorkstationKind,
    cell: WorldCell,
}

impl Workstation {
    pub(crate) const fn new(id: EntityId, kind: WorkstationKind, cell: WorldCell) -> Self {
        Self { id, kind, cell }
    }

    pub const fn id(&self) -> EntityId {
        self.id
    }

    pub const fn kind(&self) -> WorkstationKind {
        self.kind
    }

    pub const fn cell(&self) -> WorldCell {
        self.cell
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct WorkstationWorld {
    workstations: BTreeMap<EntityId, Workstation>,
    by_cell: BTreeMap<WorldCell, EntityId>,
    revision: u64,
}

impl WorkstationWorld {
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = &Workstation> {
        self.workstations.values()
    }

    pub(crate) fn get(&self, id: EntityId) -> Option<&Workstation> {
        self.workstations.get(&id)
    }

    pub(crate) fn workstation_at(&self, cell: WorldCell) -> Option<EntityId> {
        self.by_cell.get(&cell).copied()
    }

    pub(crate) fn insert(&mut self, workstation: Workstation) -> Result<(), WorkstationWorldError> {
        let id = workstation.id();
        let cell = workstation.cell();
        if self.workstations.contains_key(&id) {
            return Err(WorkstationWorldError::DuplicateWorkstation(id));
        }
        if let Some(existing) = self.by_cell.get(&cell) {
            return Err(WorkstationWorldError::CellAlreadyOccupied {
                cell,
                workstation_id: *existing,
            });
        }
        self.by_cell.insert(cell, id);
        self.workstations.insert(id, workstation);
        self.bump_revision()
    }

    pub(crate) fn remove(&mut self, id: EntityId) -> Result<Workstation, WorkstationWorldError> {
        let workstation = self
            .workstations
            .remove(&id)
            .ok_or(WorkstationWorldError::UnknownWorkstation(id))?;
        if self.by_cell.remove(&workstation.cell()) != Some(id) {
            return Err(WorkstationWorldError::IndexCorruption);
        }
        self.bump_revision()?;
        Ok(workstation)
    }

    fn bump_revision(&mut self) -> Result<(), WorkstationWorldError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(WorkstationWorldError::RevisionOverflow)?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn indexes_are_consistent(&self) -> bool {
        self.workstations
            .iter()
            .all(|(id, workstation)| self.by_cell.get(&workstation.cell()) == Some(id))
            && self.by_cell.iter().all(|(cell, id)| {
                self.workstations
                    .get(id)
                    .is_some_and(|workstation| workstation.cell() == *cell)
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkstationWorldError {
    DuplicateWorkstation(EntityId),
    UnknownWorkstation(EntityId),
    CellAlreadyOccupied {
        cell: WorldCell,
        workstation_id: EntityId,
    },
    IndexCorruption,
    RevisionOverflow,
}

#[cfg(test)]
mod tests {
    use crate::{EntityId, WorldCell};

    use super::{Workstation, WorkstationKind, WorkstationWorld, WorkstationWorldError};

    fn id(value: u64) -> EntityId {
        EntityId::new(value).unwrap()
    }

    #[test]
    fn workstation_cells_have_one_owner_and_removal_cleans_index() {
        let mut world = WorkstationWorld::default();
        let cell = WorldCell::new(4, -3);
        world
            .insert(Workstation::new(id(20), WorkstationKind::Workbench, cell))
            .unwrap();
        assert_eq!(world.workstation_at(cell), Some(id(20)));
        assert!(world.indexes_are_consistent());

        assert_eq!(
            world.insert(Workstation::new(id(21), WorkstationKind::Workbench, cell)),
            Err(WorkstationWorldError::CellAlreadyOccupied {
                cell,
                workstation_id: id(20),
            })
        );
        assert_eq!(world.remove(id(20)).unwrap().cell(), cell);
        assert_eq!(world.workstation_at(cell), None);
        assert!(world.indexes_are_consistent());
    }
}
