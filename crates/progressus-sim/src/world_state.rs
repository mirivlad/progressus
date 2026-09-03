#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_equal_write_removes_override_and_empty_chunk_delta() {
        let coordinate = ChunkCoord::new(11, -7);
        let local = LocalCell::new(3, 5);
        let mut world = ModifiedWorld::default();

        world.set_override(coordinate, local, Terrain::Grass, Terrain::Rock);
        assert_eq!(world.chunks.len(), 1);
        assert_eq!(world.chunks[&coordinate].overrides[&local], Terrain::Rock);

        world.set_override(coordinate, local, Terrain::Grass, Terrain::Grass);
        assert!(world.chunks.is_empty());
    }

    #[test]
    fn canonical_reversion_matches_an_untouched_world() {
        let coordinate = ChunkCoord::new(-400, 900);
        let local = LocalCell::new(31, 0);
        let mut changed = ModifiedWorld::default();

        changed.set_override(coordinate, local, Terrain::Grass, Terrain::Rock);
        changed.set_override(coordinate, local, Terrain::Grass, Terrain::Water);
        changed.set_override(coordinate, local, Terrain::Grass, Terrain::Grass);

        assert_eq!(changed, ModifiedWorld::default());
    }

    #[test]
    fn restoring_one_distant_chunk_leaves_the_other_delta() {
        let first = (ChunkCoord::new(-1_000, 2_000), LocalCell::new(1, 2));
        let second = (ChunkCoord::new(7_000, -8_000), LocalCell::new(30, 31));
        let mut world = ModifiedWorld::default();

        world.set_override(first.0, first.1, Terrain::Grass, Terrain::Rock);
        world.set_override(second.0, second.1, Terrain::Water, Terrain::Grass);
        world.set_override(first.0, first.1, Terrain::Grass, Terrain::Grass);

        assert!(!world.chunks.contains_key(&first.0));
        assert_eq!(world.override_at(second.0, second.1), Some(Terrain::Grass));
    }
}

use std::collections::BTreeMap;

use crate::{CHUNK_SIDE, ChunkCoord, LocalCell, Terrain};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ModifiedWorld {
    chunks: BTreeMap<ChunkCoord, ChunkDelta>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ChunkDelta {
    overrides: BTreeMap<LocalCell, Terrain>,
}

impl ModifiedWorld {
    pub(crate) fn overrides(&self) -> impl Iterator<Item = (ChunkCoord, LocalCell, Terrain)> + '_ {
        self.chunks.iter().flat_map(|(coordinate, delta)| {
            delta
                .overrides
                .iter()
                .map(move |(local, terrain)| (*coordinate, *local, *terrain))
        })
    }

    pub(crate) fn restore_override(
        &mut self,
        coordinate: ChunkCoord,
        local: LocalCell,
        terrain: Terrain,
    ) {
        self.chunks
            .entry(coordinate)
            .or_default()
            .overrides
            .insert(local, terrain);
    }

    pub(crate) fn override_at(&self, coordinate: ChunkCoord, local: LocalCell) -> Option<Terrain> {
        self.chunks
            .get(&coordinate)
            .and_then(|delta| delta.overrides.get(&local))
            .copied()
    }

    pub(crate) fn set_override(
        &mut self,
        coordinate: ChunkCoord,
        local: LocalCell,
        base: Terrain,
        requested: Terrain,
    ) {
        if requested != base {
            self.chunks
                .entry(coordinate)
                .or_default()
                .overrides
                .insert(local, requested);
            return;
        }

        let remove_chunk = if let Some(delta) = self.chunks.get_mut(&coordinate) {
            delta.overrides.remove(&local);
            delta.overrides.is_empty()
        } else {
            false
        };
        if remove_chunk {
            self.chunks.remove(&coordinate);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectiveChunk {
    coordinate: ChunkCoord,
    cells: Vec<Terrain>,
}

impl EffectiveChunk {
    pub(crate) fn new(coordinate: ChunkCoord, cells: Vec<Terrain>) -> Self {
        Self { coordinate, cells }
    }

    pub const fn coordinate(&self) -> ChunkCoord {
        self.coordinate
    }

    pub fn cells(&self) -> &[Terrain] {
        &self.cells
    }

    pub fn terrain_at(&self, local: LocalCell) -> Option<Terrain> {
        if local.x() >= CHUNK_SIDE || local.y() >= CHUNK_SIDE {
            return None;
        }

        let index = usize::from(local.y()) * usize::from(CHUNK_SIDE) + usize::from(local.x());
        self.cells.get(index).copied()
    }
}
