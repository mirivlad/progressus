use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::{CHUNK_SIDE, ChunkCoord, LocalCell, WorldCell};

pub const CURRENT_WORLDGEN_VERSION: WorldgenVersion = WorldgenVersion::new(2);

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
        if !matches!(version.value(), 1 | 2) {
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

    pub fn terrain_at(self, cell: WorldCell) -> Terrain {
        match self.version.value() {
            1 => terrain_v1(self.seed, self.version, cell),
            2 => terrain_v2(self.seed, cell),
            _ => unreachable!("supported worldgen versions are checked at construction"),
        }
    }

    pub fn natural_resource_at(self, cell: WorldCell) -> Option<NaturalResource> {
        let terrain = self.terrain_at(cell);
        match self.version.value() {
            1 => natural_resource_v1(self.seed, self.version, cell, terrain),
            2 => natural_resource_v2(self.seed, cell, terrain),
            _ => unreachable!("supported worldgen versions are checked at construction"),
        }
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
                let terrain = self.terrain_at(world_cell);
                let resource = self.natural_resource_at(world_cell);
                cells.push(terrain);
                resources.push(resource);
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

const TERRAIN_FEATURE_REGION: i64 = 10;
const FOREST_FEATURE_REGION: i64 = 12;

fn terrain_v2(seed: WorldSeed, cell: WorldCell) -> Terrain {
    if spawn_clearing(cell, 4) {
        return Terrain::Grass;
    }

    let region_x = cell.x().div_euclid(TERRAIN_FEATURE_REGION);
    let region_y = cell.y().div_euclid(TERRAIN_FEATURE_REGION);
    let mut rock = false;
    for feature_y in (region_y - 1)..=(region_y + 1) {
        for feature_x in (region_x - 1)..=(region_x + 1) {
            let sample = feature_hash(seed, 0x7465_7272_6169_6e32, feature_x, feature_y);
            let roll = sample % 100;
            if roll >= 42 {
                continue;
            }
            let radius_x = 2 + ((sample >> 8) % 3) as i64;
            let radius_y = 2 + ((sample >> 12) % 3) as i64;
            if !feature_contains(
                cell,
                feature_x,
                feature_y,
                TERRAIN_FEATURE_REGION,
                radius_x,
                radius_y,
                sample,
            ) {
                continue;
            }
            if roll < 20 {
                return Terrain::Water;
            }
            rock = true;
        }
    }
    if rock { Terrain::Rock } else { Terrain::Grass }
}

fn natural_resource_v2(
    seed: WorldSeed,
    cell: WorldCell,
    terrain: Terrain,
) -> Option<NaturalResource> {
    if terrain != Terrain::Grass || spawn_clearing(cell, 2) {
        return None;
    }
    if let Some(kind) = starter_resource_v2(seed, cell) {
        let sample = cell_hash(seed, 0x7374_6172_7465_725f, cell);
        return Some(NaturalResource::new(kind, 4 + ((sample >> 32) % 5) as u32));
    }

    let sample = cell_hash(seed, 0x7265_736f_7572_6365, cell);
    let near_rock = [
        WorldCell::new(cell.x().saturating_add(1), cell.y()),
        WorldCell::new(cell.x().saturating_sub(1), cell.y()),
        WorldCell::new(cell.x(), cell.y().saturating_add(1)),
        WorldCell::new(cell.x(), cell.y().saturating_sub(1)),
    ]
    .into_iter()
    .any(|neighbor| terrain_v2(seed, neighbor) == Terrain::Rock);
    if (near_rock && sample % 100 < 34) || sample % 1000 < 18 {
        return Some(NaturalResource::new(
            NaturalResourceKind::StoneOutcrop,
            4 + ((sample >> 32) % 5) as u32,
        ));
    }

    let region_x = cell.x().div_euclid(FOREST_FEATURE_REGION);
    let region_y = cell.y().div_euclid(FOREST_FEATURE_REGION);
    let mut inside_forest = false;
    for feature_y in (region_y - 1)..=(region_y + 1) {
        for feature_x in (region_x - 1)..=(region_x + 1) {
            let forest = feature_hash(seed, 0x666f_7265_7374_7632, feature_x, feature_y);
            if forest % 100 >= 58 {
                continue;
            }
            let radius_x = 3 + ((forest >> 8) % 4) as i64;
            let radius_y = 3 + ((forest >> 12) % 4) as i64;
            if feature_contains(
                cell,
                feature_x,
                feature_y,
                FOREST_FEATURE_REGION,
                radius_x,
                radius_y,
                forest,
            ) {
                inside_forest = true;
                break;
            }
        }
        if inside_forest {
            break;
        }
    }

    let tree_roll = (sample >> 16) % 100;
    if (inside_forest && tree_roll < 62) || (!inside_forest && sample % 1000 < 22) {
        return Some(NaturalResource::new(
            NaturalResourceKind::Tree,
            4 + ((sample >> 40) % 5) as u32,
        ));
    }
    None
}

fn starter_resource_v2(seed: WorldSeed, cell: WorldCell) -> Option<NaturalResourceKind> {
    const RING: [(i64, i64); 12] = [
        (-4, -2),
        (-4, 2),
        (-3, -4),
        (0, -4),
        (3, -4),
        (4, -2),
        (4, 2),
        (3, 4),
        (0, 4),
        (-3, 4),
        (-4, 3),
        (4, -3),
    ];
    let tree_index = (mix64(seed.value() ^ 0x7374_6172_745f_7472) % RING.len() as u64) as usize;
    let stone_index = (tree_index + RING.len() / 2) % RING.len();
    let tree = RING[tree_index];
    let stone = RING[stone_index];
    if (cell.x(), cell.y()) == tree {
        Some(NaturalResourceKind::Tree)
    } else if (cell.x(), cell.y()) == stone {
        Some(NaturalResourceKind::StoneOutcrop)
    } else {
        None
    }
}

fn spawn_clearing(cell: WorldCell, radius: i64) -> bool {
    cell.x().unsigned_abs() <= radius as u64 && cell.y().unsigned_abs() <= radius as u64
}

fn feature_hash(seed: WorldSeed, salt: u64, region_x: i64, region_y: i64) -> u64 {
    let mut sample = mix64(seed.value() ^ salt);
    sample = mix64(sample ^ region_x as u64);
    mix64(sample ^ (region_y as u64).rotate_left(32))
}

fn cell_hash(seed: WorldSeed, salt: u64, cell: WorldCell) -> u64 {
    let mut sample = mix64(seed.value() ^ salt);
    sample = mix64(sample ^ cell.x() as u64);
    mix64(sample ^ (cell.y() as u64).rotate_left(32))
}

fn feature_contains(
    cell: WorldCell,
    region_x: i64,
    region_y: i64,
    region_side: i64,
    radius_x: i64,
    radius_y: i64,
    sample: u64,
) -> bool {
    let side = i128::from(region_side);
    let base_x = i128::from(region_x) * side;
    let base_y = i128::from(region_y) * side;
    let margin = 2_i128;
    let span = (region_side - 4).max(1) as u64;
    let center_x = base_x + margin + i128::from((sample >> 20) % span);
    let center_y = base_y + margin + i128::from((sample >> 28) % span);
    let dx = i128::from(cell.x()) - center_x;
    let dy = i128::from(cell.y()) - center_y;
    let rx = i128::from(radius_x);
    let ry = i128::from(radius_y);
    if dx.abs() > rx + 1 || dy.abs() > ry + 1 {
        return false;
    }
    let lhs = dx * dx * ry * ry + dy * dy * rx * rx;
    let limit = rx * rx * ry * ry;
    let irregularity =
        i128::from(86 + (cell_hash(WorldSeed::new(sample), 0x626c_6f62_6564_6765, cell) % 31));
    lhs * 100 <= limit * irregularity
}

// SplitMix64's public-domain finalizer gives worldgen v1 a fully specified,
// project-owned integer mixer without relying on Rust's unspecified hashing.
fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
