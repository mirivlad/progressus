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

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NaturalResourceKind {
    Tree,
    StoneOutcrop,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NaturalResource {
    kind: NaturalResourceKind,
    yield_quantity: u32,
}

impl NaturalResource {
    const fn new(kind: NaturalResourceKind, yield_quantity: u32) -> Self {
        Self {
            kind,
            yield_quantity,
        }
    }

    pub const fn kind(self) -> NaturalResourceKind {
        self.kind
    }

    pub const fn yield_quantity(self) -> u32 {
        self.yield_quantity
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedChunk {
    coordinate: ChunkCoord,
    cells: Vec<Terrain>,
    resources: Vec<Option<NaturalResource>>,
}

impl GeneratedChunk {
    pub const fn coordinate(&self) -> ChunkCoord {
        self.coordinate
    }

    pub fn cells(&self) -> &[Terrain] {
        &self.cells
    }

    pub fn resources(&self) -> &[Option<NaturalResource>] {
        &self.resources
    }

    pub fn terrain_at(&self, local: LocalCell) -> Option<Terrain> {
        self.cells.get(local_index(local)?).copied()
    }

    pub fn natural_resource_at(&self, local: LocalCell) -> Option<NaturalResource> {
        self.resources.get(local_index(local)?).copied().flatten()
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
        let capacity = usize::from(CHUNK_SIDE).pow(2);
        let mut cells = Vec::with_capacity(capacity);
        let mut resources = Vec::with_capacity(capacity);

        for y in 0..CHUNK_SIDE {
            for x in 0..CHUNK_SIDE {
                let local = LocalCell::new(x, y);
                let world_cell = coordinate
                    .world_cell(local)
                    .ok_or(WorldgenError::CoordinateOutOfRange(coordinate))?;
                let terrain = terrain_v1(self.seed, self.version, world_cell);
                cells.push(terrain);
                resources.push(natural_resource_v1(
                    self.seed,
                    self.version,
                    world_cell,
                    terrain,
                ));
            }
        }

        Ok(GeneratedChunk {
            coordinate,
            cells,
            resources,
        })
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

fn local_index(local: LocalCell) -> Option<usize> {
    if local.x() >= CHUNK_SIDE || local.y() >= CHUNK_SIDE {
        return None;
    }
    Some(usize::from(local.y()) * usize::from(CHUNK_SIDE) + usize::from(local.x()))
}

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

fn natural_resource_v1(
    seed: WorldSeed,
    version: WorldgenVersion,
    cell: WorldCell,
    terrain: Terrain,
) -> Option<NaturalResource> {
    if terrain != Terrain::Grass || ((-2..=2).contains(&cell.x()) && cell.y() == 0) {
        return None;
    }

    let mut sample = mix64(seed.value() ^ 0xbb67_ae85_84ca_a73b);
    sample = mix64(sample ^ u64::from(version.value()));
    sample = mix64(sample ^ cell.x() as u64);
    sample = mix64(sample ^ (cell.y() as u64).rotate_left(32));
    let kind = match sample % 100 {
        0..=17 => NaturalResourceKind::Tree,
        18..=25 => NaturalResourceKind::StoneOutcrop,
        _ => return None,
    };
    let yield_quantity = 4 + ((sample >> 32) % 5) as u32;
    Some(NaturalResource::new(kind, yield_quantity))
}

// SplitMix64's public-domain finalizer gives worldgen v1 a fully specified,
// project-owned integer mixer without relying on Rust's unspecified hashing.
fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
