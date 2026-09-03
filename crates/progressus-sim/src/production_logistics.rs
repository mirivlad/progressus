use std::collections::{BTreeMap, BTreeSet};

use crate::{EntityId, WorldCell};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProductionZoneKind {
    Input,
    Output,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductionLogistics {
    workstation_id: EntityId,
    input_cells: BTreeSet<WorldCell>,
    output_cells: BTreeSet<WorldCell>,
}

impl ProductionLogistics {
    pub(crate) const fn new(workstation_id: EntityId) -> Self {
        Self {
            workstation_id,
            input_cells: BTreeSet::new(),
            output_cells: BTreeSet::new(),
        }
    }

    pub const fn workstation_id(&self) -> EntityId {
        self.workstation_id
    }

    pub fn cells(&self, kind: ProductionZoneKind) -> impl ExactSizeIterator<Item = WorldCell> + '_ {
        match kind {
            ProductionZoneKind::Input => self.input_cells.iter().copied(),
            ProductionZoneKind::Output => self.output_cells.iter().copied(),
        }
    }

    pub fn contains(&self, kind: ProductionZoneKind, cell: WorldCell) -> bool {
        match kind {
            ProductionZoneKind::Input => self.input_cells.contains(&cell),
            ProductionZoneKind::Output => self.output_cells.contains(&cell),
        }
    }

    fn cells_mut(&mut self, kind: ProductionZoneKind) -> &mut BTreeSet<WorldCell> {
        match kind {
            ProductionZoneKind::Input => &mut self.input_cells,
            ProductionZoneKind::Output => &mut self.output_cells,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ProductionLogisticsWorld {
    by_workstation: BTreeMap<EntityId, ProductionLogistics>,
    owner_by_cell: BTreeMap<WorldCell, (EntityId, ProductionZoneKind)>,
    revision: u64,
}

impl ProductionLogisticsWorld {
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = &ProductionLogistics> {
        self.by_workstation.values()
    }

    pub(crate) fn get(&self, workstation_id: EntityId) -> Option<&ProductionLogistics> {
        self.by_workstation.get(&workstation_id)
    }

    pub(crate) fn zone_at(&self, cell: WorldCell) -> Option<(EntityId, ProductionZoneKind)> {
        self.owner_by_cell.get(&cell).copied()
    }

    pub(crate) fn insert_workstation(
        &mut self,
        workstation_id: EntityId,
    ) -> Result<(), ProductionLogisticsWorldError> {
        if self.by_workstation.contains_key(&workstation_id) {
            return Err(ProductionLogisticsWorldError::DuplicateWorkstation(
                workstation_id,
            ));
        }
        self.by_workstation
            .insert(workstation_id, ProductionLogistics::new(workstation_id));
        self.bump_revision()
    }

    pub(crate) fn remove_workstation(
        &mut self,
        workstation_id: EntityId,
    ) -> Result<ProductionLogistics, ProductionLogisticsWorldError> {
        let logistics = self.by_workstation.remove(&workstation_id).ok_or(
            ProductionLogisticsWorldError::UnknownWorkstation(workstation_id),
        )?;
        for cell in logistics
            .cells(ProductionZoneKind::Input)
            .chain(logistics.cells(ProductionZoneKind::Output))
        {
            let removed = self.owner_by_cell.remove(&cell);
            if removed.is_none() {
                return Err(ProductionLogisticsWorldError::IndexCorruption);
            }
        }
        self.bump_revision()?;
        Ok(logistics)
    }

    pub(crate) fn set_cell(
        &mut self,
        workstation_id: EntityId,
        kind: ProductionZoneKind,
        cell: WorldCell,
        enabled: bool,
    ) -> Result<(), ProductionLogisticsWorldError> {
        if enabled {
            if let Some((owner, owner_kind)) = self.owner_by_cell.get(&cell).copied() {
                if owner == workstation_id && owner_kind == kind {
                    return Ok(());
                }
                return Err(ProductionLogisticsWorldError::CellAlreadyOwned {
                    cell,
                    workstation_id: owner,
                    kind: owner_kind,
                });
            }
            let logistics = self.by_workstation.get_mut(&workstation_id).ok_or(
                ProductionLogisticsWorldError::UnknownWorkstation(workstation_id),
            )?;
            logistics.cells_mut(kind).insert(cell);
            self.owner_by_cell.insert(cell, (workstation_id, kind));
            return self.bump_revision();
        }

        let logistics = self.by_workstation.get_mut(&workstation_id).ok_or(
            ProductionLogisticsWorldError::UnknownWorkstation(workstation_id),
        )?;
        if !logistics.cells_mut(kind).remove(&cell) {
            return Ok(());
        }
        if self.owner_by_cell.remove(&cell) != Some((workstation_id, kind)) {
            return Err(ProductionLogisticsWorldError::IndexCorruption);
        }
        self.bump_revision()
    }

    fn bump_revision(&mut self) -> Result<(), ProductionLogisticsWorldError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(ProductionLogisticsWorldError::RevisionOverflow)?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn indexes_are_consistent(&self) -> bool {
        self.by_workstation
            .iter()
            .all(|(workstation_id, logistics)| {
                logistics.cells(ProductionZoneKind::Input).all(|cell| {
                    self.owner_by_cell.get(&cell)
                        == Some(&(*workstation_id, ProductionZoneKind::Input))
                }) && logistics.cells(ProductionZoneKind::Output).all(|cell| {
                    self.owner_by_cell.get(&cell)
                        == Some(&(*workstation_id, ProductionZoneKind::Output))
                })
            })
            && self
                .owner_by_cell
                .iter()
                .all(|(cell, (workstation_id, kind))| {
                    self.by_workstation
                        .get(workstation_id)
                        .is_some_and(|logistics| logistics.contains(*kind, *cell))
                })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProductionLogisticsWorldError {
    DuplicateWorkstation(EntityId),
    UnknownWorkstation(EntityId),
    CellAlreadyOwned {
        cell: WorldCell,
        workstation_id: EntityId,
        kind: ProductionZoneKind,
    },
    IndexCorruption,
    RevisionOverflow,
}

#[cfg(test)]
mod tests {
    use crate::{EntityId, WorldCell};

    use super::{ProductionLogisticsWorld, ProductionLogisticsWorldError, ProductionZoneKind};

    fn id(value: u64) -> EntityId {
        EntityId::new(value).unwrap()
    }

    #[test]
    fn production_zone_cells_have_one_owner_and_workstation_removal_cleans_them() {
        let mut world = ProductionLogisticsWorld::default();
        world.insert_workstation(id(20)).unwrap();
        world.insert_workstation(id(21)).unwrap();
        let input = WorldCell::new(3, 4);
        let output = WorldCell::new(4, 4);
        world
            .set_cell(id(20), ProductionZoneKind::Input, input, true)
            .unwrap();
        world
            .set_cell(id(20), ProductionZoneKind::Output, output, true)
            .unwrap();
        assert_eq!(
            world.set_cell(id(21), ProductionZoneKind::Input, input, true),
            Err(ProductionLogisticsWorldError::CellAlreadyOwned {
                cell: input,
                workstation_id: id(20),
                kind: ProductionZoneKind::Input,
            })
        );
        assert!(world.indexes_are_consistent());
        world.remove_workstation(id(20)).unwrap();
        assert_eq!(world.zone_at(input), None);
        assert_eq!(world.zone_at(output), None);
        assert!(world.indexes_are_consistent());
    }
}
