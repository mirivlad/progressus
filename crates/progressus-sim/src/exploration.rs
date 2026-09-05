use std::collections::BTreeSet;

use crate::{ChunkCoord, WorldCell};

pub const CHARACTER_VISION_RADIUS_CELLS: i64 = 5;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ExploredWorld {
    cells: BTreeSet<WorldCell>,
    chunks: BTreeSet<ChunkCoord>,
    revision: u64,
}

impl ExploredWorld {
    pub(crate) fn contains(&self, cell: WorldCell) -> bool {
        self.cells.contains(&cell)
    }

    pub(crate) fn cells(&self) -> impl ExactSizeIterator<Item = WorldCell> + '_ {
        self.cells.iter().copied()
    }

    pub(crate) fn contains_chunk(&self, chunk: ChunkCoord) -> bool {
        self.chunks.contains(&chunk)
    }

    pub(crate) fn restore_cells(cells: impl IntoIterator<Item = WorldCell>) -> Self {
        let cells = cells.into_iter().collect::<BTreeSet<_>>();
        let chunks = cells.iter().map(|cell| cell.split().0).collect();
        Self {
            cells,
            chunks,
            revision: 0,
        }
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn reveal_around(&mut self, center: WorldCell) -> bool {
        let mut changed = false;
        for y_offset in -CHARACTER_VISION_RADIUS_CELLS..=CHARACTER_VISION_RADIUS_CELLS {
            for x_offset in -CHARACTER_VISION_RADIUS_CELLS..=CHARACTER_VISION_RADIUS_CELLS {
                if x_offset * x_offset + y_offset * y_offset
                    > CHARACTER_VISION_RADIUS_CELLS * CHARACTER_VISION_RADIUS_CELLS
                {
                    continue;
                }
                let Some(x) = center.x().checked_add(x_offset) else {
                    continue;
                };
                let Some(y) = center.y().checked_add(y_offset) else {
                    continue;
                };
                let cell = WorldCell::new(x, y);
                changed |= self.cells.insert(cell);
                self.chunks.insert(cell.split().0);
            }
        }
        if changed {
            self.revision = self
                .revision
                .checked_add(1)
                .expect("exploration revision overflow");
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use crate::WorldCell;

    use super::{CHARACTER_VISION_RADIUS_CELLS, ExploredWorld};

    #[test]
    fn radius_five_is_an_euclidean_disk_and_discovery_is_monotonic() {
        let mut explored = ExploredWorld::default();
        assert!(explored.reveal_around(WorldCell::new(0, 0)));
        assert!(explored.contains(WorldCell::new(5, 0)));
        assert!(explored.contains(WorldCell::new(3, 4)));
        assert!(!explored.contains(WorldCell::new(4, 4)));
        assert_eq!(CHARACTER_VISION_RADIUS_CELLS, 5);
        let revision = explored.revision();

        assert!(!explored.reveal_around(WorldCell::new(0, 0)));
        assert_eq!(explored.revision(), revision);
        assert!(explored.reveal_around(WorldCell::new(-5, 0)));
        assert!(explored.contains(WorldCell::new(-10, 0)));
        assert!(explored.contains(WorldCell::new(5, 0)));
        assert!(explored.contains_chunk(WorldCell::new(-10, 0).split().0));
        assert!(!explored.contains_chunk(WorldCell::new(10_000, 10_000).split().0));

        let restored = ExploredWorld::restore_cells(explored.cells());
        assert!(restored.contains_chunk(WorldCell::new(-10, 0).split().0));
        assert!(!restored.contains_chunk(WorldCell::new(10_000, 10_000).split().0));
    }
}
