use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::{CHUNK_SIDE, ChunkCoord, LocalCell, WorldCell};

pub const CURRENT_WORLDGEN_VERSION: WorldgenVersion = WorldgenVersion::new(1);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorldSeed(u64);

impl WorldSeed {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorldgenVersion(u32);

impl WorldgenVersion {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Terrain {
    Grass,
    Water,
    Rock,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedChunk {
    coordinate: ChunkCoord,
    cells: Vec<Terrain>,
}

impl GeneratedChunk {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldGenerator {
    seed: WorldSeed,
    version: WorldgenVersion,
}

impl WorldGenerator {
    pub fn new(seed: WorldSeed, version: WorldgenVersion) -> Result<Self, WorldgenError> {
        if version != CURRENT_WORLDGEN_VERSION {
            return Err(WorldgenError::UnsupportedVersion(version));
        }

        Ok(Self { seed, version })
    }

    pub const fn seed(self) -> WorldSeed {
        self.seed
    }

    pub const fn version(self) -> WorldgenVersion {
        self.version
    }

    pub fn generate(self, coordinate: ChunkCoord) -> Result<GeneratedChunk, WorldgenError> {
        let mut cells = Vec::with_capacity(usize::from(CHUNK_SIDE).pow(2));

        for y in 0..CHUNK_SIDE {
            for x in 0..CHUNK_SIDE {
                let local = LocalCell::new(x, y);
                let world_cell = coordinate
                    .world_cell(local)
                    .ok_or(WorldgenError::CoordinateOutOfRange(coordinate))?;
                cells.push(terrain_v1(self.seed, self.version, world_cell));
            }
        }

        Ok(GeneratedChunk { coordinate, cells })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldgenError {
    UnsupportedVersion(WorldgenVersion),
    CoordinateOutOfRange(ChunkCoord),
}

impl Display for WorldgenError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(
                    formatter,
                    "unsupported world-generation version {}",
                    version.value()
                )
            }
            Self::CoordinateOutOfRange(coordinate) => write!(
                formatter,
                "chunk coordinate ({}, {}) is outside the world-cell range",
                coordinate.x(),
                coordinate.y()
            ),
        }
    }
}

impl Error for WorldgenError {}

fn terrain_v1(seed: WorldSeed, version: WorldgenVersion, cell: WorldCell) -> Terrain {
    if (-2..=2).contains(&cell.x()) && cell.y() == 0 {
        return Terrain::Grass;
    }

    let mut sample = mix64(seed.value() ^ 0x6a09_e667_f3bc_c909);
    sample = mix64(sample ^ u64::from(version.value()));
    sample = mix64(sample ^ cell.x() as u64);
    sample = mix64(sample ^ (cell.y() as u64).rotate_left(32));

    match sample % 100 {
        0..=14 => Terrain::Water,
        15..=29 => Terrain::Rock,
        _ => Terrain::Grass,
    }
}

// SplitMix64's public-domain finalizer gives worldgen v1 a fully specified,
// project-owned integer mixer without relying on Rust's unspecified hashing.
fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
