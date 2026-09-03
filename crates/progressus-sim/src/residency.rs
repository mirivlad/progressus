use std::collections::{BTreeMap, BTreeSet};

use progressus_worldgen::{ChunkCoord, GeneratedChunk, WorldGenerator, WorldgenError};

pub const RESIDENT_CHUNK_RADIUS: i64 = 1;
pub const RESIDENT_CHUNKS_PER_CENTER: usize = 9;

#[derive(Clone, Debug, Default)]
pub(crate) struct ChunkResidency {
    chunks: BTreeMap<ChunkCoord, GeneratedChunk>,
    centers: BTreeSet<ChunkCoord>,
    revision: u64,
}

impl ChunkResidency {
    pub(crate) fn get(&self, coordinate: ChunkCoord) -> Option<&GeneratedChunk> {
        self.chunks.get(&coordinate)
    }

    pub(crate) fn coordinates(&self) -> impl ExactSizeIterator<Item = ChunkCoord> + '_ {
        self.chunks.keys().copied()
    }

    pub(crate) fn len(&self) -> usize {
        self.chunks.len()
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn reconcile(
        &mut self,
        generator: WorldGenerator,
        centers: impl IntoIterator<Item = ChunkCoord>,
    ) -> Result<(), WorldgenError> {
        let centers = centers.into_iter().collect::<BTreeSet<_>>();
        if centers == self.centers {
            return Ok(());
        }
        let desired = desired_coordinates(centers.iter().copied());
        let before = self.chunks.keys().copied().collect::<BTreeSet<_>>();
        self.chunks
            .retain(|coordinate, _| desired.contains(coordinate));

        for coordinate in desired {
            if self.chunks.contains_key(&coordinate) {
                continue;
            }
            match generator.generate(coordinate) {
                Ok(chunk) => {
                    self.chunks.insert(coordinate, chunk);
                }
                Err(WorldgenError::CoordinateOutOfRange(_)) => {
                    // Extreme world-edge chunks can be only partially representable.
                    // Residency is a derived optimization, so skipping such a cache
                    // entry must never make otherwise-valid point movement fail.
                }
                Err(error) => return Err(error),
            }
        }

        let after = self.chunks.keys().copied().collect::<BTreeSet<_>>();
        if after != before {
            self.revision = self.revision.saturating_add(1);
        }
        self.centers = centers;
        Ok(())
    }
}

fn desired_coordinates(centers: impl IntoIterator<Item = ChunkCoord>) -> BTreeSet<ChunkCoord> {
    let mut desired = BTreeSet::new();
    for center in centers {
        for dy in -RESIDENT_CHUNK_RADIUS..=RESIDENT_CHUNK_RADIUS {
            for dx in -RESIDENT_CHUNK_RADIUS..=RESIDENT_CHUNK_RADIUS {
                let (Some(x), Some(y)) = (center.x().checked_add(dx), center.y().checked_add(dy))
                else {
                    continue;
                };
                desired.insert(ChunkCoord::new(x, y));
            }
        }
    }
    desired
}

#[cfg(test)]
mod tests {
    use super::*;
    use progressus_worldgen::{CURRENT_WORLDGEN_VERSION, WorldSeed};

    #[test]
    fn one_center_requests_a_three_by_three_neighbourhood() {
        let generator = WorldGenerator::new(WorldSeed::new(1), CURRENT_WORLDGEN_VERSION).unwrap();
        let mut residency = ChunkResidency::default();
        residency
            .reconcile(generator, [ChunkCoord::new(10, -20)])
            .unwrap();

        assert_eq!(residency.len(), RESIDENT_CHUNKS_PER_CENTER);
        let expected = (-21..=-19)
            .flat_map(|y| (9..=11).map(move |x| ChunkCoord::new(x, y)))
            .collect::<BTreeSet<_>>();
        assert_eq!(residency.coordinates().collect::<BTreeSet<_>>(), expected);
    }
}
