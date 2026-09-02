use std::collections::{BTreeMap, BTreeSet};

use crate::{EntityId, WorldCell};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stockpile {
    id: EntityId,
    cells: BTreeSet<WorldCell>,
}

impl Stockpile {
    pub(crate) fn new(id: EntityId, first_cell: WorldCell) -> Self {
        Self {
            id,
            cells: BTreeSet::from([first_cell]),
        }
    }

    pub const fn id(&self) -> EntityId {
        self.id
    }

    pub fn cells(&self) -> impl ExactSizeIterator<Item = WorldCell> + '_ {
        self.cells.iter().copied()
    }

    pub fn contains(&self, cell: WorldCell) -> bool {
        self.cells.contains(&cell)
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct StockpileWorld {
    stockpiles: BTreeMap<EntityId, Stockpile>,
    owner_by_cell: BTreeMap<WorldCell, EntityId>,
    revision: u64,
}

impl StockpileWorld {
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = &Stockpile> {
        self.stockpiles.values()
    }

    pub(crate) fn get(&self, id: EntityId) -> Option<&Stockpile> {
        self.stockpiles.get(&id)
    }

    pub(crate) fn stockpile_at(&self, cell: WorldCell) -> Option<EntityId> {
        self.owner_by_cell.get(&cell).copied()
    }

    pub(crate) fn insert(&mut self, stockpile: Stockpile) -> Result<(), StockpileWorldError> {
        let id = stockpile.id();
        if self.stockpiles.contains_key(&id) {
            return Err(StockpileWorldError::DuplicateStockpile(id));
        }
        for cell in stockpile.cells() {
            if let Some(existing) = self.owner_by_cell.get(&cell) {
                return Err(StockpileWorldError::CellAlreadyOwned {
                    cell,
                    stockpile_id: *existing,
                });
            }
        }
        for cell in stockpile.cells() {
            self.owner_by_cell.insert(cell, id);
        }
        self.stockpiles.insert(id, stockpile);
        self.bump_revision()?;
        Ok(())
    }

    /// Returns true when removing the cell also removes the now-empty stockpile.
    pub(crate) fn set_cell(
        &mut self,
        stockpile_id: EntityId,
        cell: WorldCell,
        enabled: bool,
    ) -> Result<bool, StockpileWorldError> {
        if enabled {
            if let Some(existing) = self.owner_by_cell.get(&cell) {
                if *existing == stockpile_id {
                    return Ok(false);
                }
                return Err(StockpileWorldError::CellAlreadyOwned {
                    cell,
                    stockpile_id: *existing,
                });
            }
            self.stockpiles
                .get_mut(&stockpile_id)
                .ok_or(StockpileWorldError::UnknownStockpile(stockpile_id))?
                .cells
                .insert(cell);
            self.owner_by_cell.insert(cell, stockpile_id);
            self.bump_revision()?;
            return Ok(false);
        }

        let stockpile = self
            .stockpiles
            .get_mut(&stockpile_id)
            .ok_or(StockpileWorldError::UnknownStockpile(stockpile_id))?;
        if !stockpile.cells.remove(&cell) {
            return Ok(false);
        }
        debug_assert_eq!(self.owner_by_cell.remove(&cell), Some(stockpile_id));
        let remove_stockpile = stockpile.cells.is_empty();
        if remove_stockpile {
            self.stockpiles.remove(&stockpile_id);
        }
        self.bump_revision()?;
        Ok(remove_stockpile)
    }

    fn bump_revision(&mut self) -> Result<(), StockpileWorldError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(StockpileWorldError::RevisionOverflow)?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn indexes_are_consistent(&self) -> bool {
        self.stockpiles.iter().all(|(id, stockpile)| {
            !stockpile.cells.is_empty()
                && stockpile
                    .cells()
                    .all(|cell| self.owner_by_cell.get(&cell) == Some(id))
        }) && self.owner_by_cell.iter().all(|(cell, id)| {
            self.stockpiles
                .get(id)
                .is_some_and(|stockpile| stockpile.contains(*cell))
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StockpileWorldError {
    DuplicateStockpile(EntityId),
    UnknownStockpile(EntityId),
    CellAlreadyOwned {
        cell: WorldCell,
        stockpile_id: EntityId,
    },
    RevisionOverflow,
}

#[cfg(test)]
mod tests {
    use crate::{EntityId, WorldCell};

    use super::{Stockpile, StockpileWorld, StockpileWorldError};

    fn id(value: u64) -> EntityId {
        EntityId::new(value).unwrap()
    }

    #[test]
    fn cells_have_one_stockpile_owner_and_empty_stockpiles_disappear() {
        let mut world = StockpileWorld::default();
        let a = WorldCell::new(1, 2);
        let b = WorldCell::new(2, 2);
        world.insert(Stockpile::new(id(10), a)).unwrap();
        world.set_cell(id(10), b, true).unwrap();
        assert_eq!(world.stockpile_at(a), Some(id(10)));
        assert_eq!(world.stockpile_at(b), Some(id(10)));
        assert!(world.indexes_are_consistent());

        assert_eq!(
            world.insert(Stockpile::new(id(11), b)),
            Err(StockpileWorldError::CellAlreadyOwned {
                cell: b,
                stockpile_id: id(10),
            })
        );
        assert!(!world.set_cell(id(10), a, false).unwrap());
        assert!(world.set_cell(id(10), b, false).unwrap());
        assert!(world.get(id(10)).is_none());
        assert!(world.indexes_are_consistent());
    }
}
