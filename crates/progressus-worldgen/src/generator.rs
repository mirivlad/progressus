use std::error::Error;
use std::fmt::{self, Display, Formatter};

use progressus_content::{NaturalResourceId, TerrainId, natural_resource, terrain};

use crate::{CHUNK_SIDE, ChunkCoord, LocalCell, WorldCell};

pub const CURRENT_WORLDGEN_VERSION: WorldgenVersion = WorldgenVersion::new(3);

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

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NaturalResource {
    kind: NaturalResourceId,
    yield_quantity: u32,
}

impl NaturalResource {
    const fn new(kind: NaturalResourceId, yield_quantity: u32) -> Self {
        Self {
            kind,
            yield_quantity,
        }
    }

    pub const fn kind(self) -> NaturalResourceId {
        self.kind
    }

    pub const fn yield_quantity(self) -> u32 {
        self.yield_quantity
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedChunk {
    coordinate: ChunkCoord,
    cells: Vec<TerrainId>,
    resources: Vec<Option<NaturalResource>>,
}

impl GeneratedChunk {
    pub const fn coordinate(&self) -> ChunkCoord {
        self.coordinate
    }

    pub fn cells(&self) -> &[TerrainId] {
        &self.cells
    }

    pub fn resources(&self) -> &[Option<NaturalResource>] {
        &self.resources
    }

    pub fn terrain_at(&self, local: LocalCell) -> Option<TerrainId> {
        self.cells.get(local_index(local)?).copied()
    }

    pub fn natural_resource_at(&self, local: LocalCell) -> Option<NaturalResource> {
        self.resources.get(local_index(local)?).copied().flatten()
    }
}

/// A resource layer places resources only where the base generation and every
/// earlier layer left the cell empty. Adding one therefore never removes or
/// moves anything a world already has, which is what lets an update reach a
/// world that was created before it. See ADR-0022.
#[derive(Clone, Copy)]
pub struct ResourceLayerDefinition {
    /// Stable identity used by saves.
    pub name: &'static str,
    pub place: fn(WorldSeed, WorldCell, TerrainId) -> Option<NaturalResource>,
}

impl fmt::Debug for ResourceLayerDefinition {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResourceLayerDefinition")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl PartialEq for ResourceLayerDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for ResourceLayerDefinition {}

/// Append-only, and never reordered: a layer's position decides which of two
/// layers claims a contested cell, so moving one rewrites existing worlds.
///
/// Empty until the first resource that did not exist at the base generation.
/// The base versions below are not layers: each is one world's whole original
/// generation, chosen once and pinned for that world's life.
pub static RESOURCE_LAYERS: &[ResourceLayerDefinition] = &[
    #[cfg(test)]
    ResourceLayerDefinition {
        name: "test_dense_grass",
        place: test_layers::dense_grass,
    },
    #[cfg(test)]
    ResourceLayerDefinition {
        name: "test_sparse_grass",
        place: test_layers::sparse_grass,
    },
];

/// The most layers a [`ResourceLayers`] bitmask can hold.
pub const MAX_RESOURCE_LAYERS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceLayerId(u16);

impl ResourceLayerId {
    pub const fn definition(self) -> &'static ResourceLayerDefinition {
        &RESOURCE_LAYERS[self.0 as usize]
    }

    pub const fn name(self) -> &'static str {
        self.definition().name
    }

    /// Resolves a persisted name. `None` means this build predates the layer,
    /// which callers must surface rather than quietly drop.
    pub fn from_name(name: &str) -> Option<Self> {
        RESOURCE_LAYERS
            .iter()
            .position(|layer| layer.name == name)
            .map(|index| Self(index as u16))
    }

    pub fn all() -> impl ExactSizeIterator<Item = Self> + Clone {
        (0..RESOURCE_LAYERS.len()).map(|index| Self(index as u16))
    }
}

/// The layers active in one world, as a bitmask over [`RESOURCE_LAYERS`].
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceLayers(u64);

impl ResourceLayers {
    /// No layers: the world is exactly its base generation.
    pub const fn none() -> Self {
        Self(0)
    }

    /// Every layer this build knows. New worlds and loaded worlds both use
    /// this, which is how an update reaches an existing world.
    pub fn all() -> Self {
        ResourceLayerId::all().fold(Self::none(), Self::with)
    }

    pub const fn with(self, layer: ResourceLayerId) -> Self {
        Self(self.0 | (1 << layer.0))
    }

    pub const fn contains(self, layer: ResourceLayerId) -> bool {
        self.0 & (1 << layer.0) != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Active layers in registry order, which is the order they are consulted.
    pub fn iter(self) -> impl Iterator<Item = ResourceLayerId> {
        ResourceLayerId::all().filter(move |layer| self.contains(*layer))
    }
}

/// Layers used only to prove the mechanism. The shipped registry is empty until
/// a resource exists that the base generations did not have; see ADR-0022.
#[cfg(test)]
mod test_layers {
    use super::{
        NaturalResource, TerrainId, WorldCell, WorldSeed, cell_hash, natural_resource, terrain,
    };

    /// Claims most free grass, so it contests cells the sparse layer also wants.
    pub(super) fn dense_grass(
        seed: WorldSeed,
        cell: WorldCell,
        cell_terrain: TerrainId,
    ) -> Option<NaturalResource> {
        if cell_terrain != terrain::GRASS {
            return None;
        }
        let sample = cell_hash(seed, 0x7465_7374_6465_6e73, cell);
        (sample % 100 < 60).then(|| NaturalResource::new(natural_resource::TREE, 1))
    }

    /// Claims a subset of the same cells, to show the earlier layer wins.
    pub(super) fn sparse_grass(
        seed: WorldSeed,
        cell: WorldCell,
        cell_terrain: TerrainId,
    ) -> Option<NaturalResource> {
        if cell_terrain != terrain::GRASS {
            return None;
        }
        let sample = cell_hash(seed, 0x7465_7374_7370_7273, cell);
        (sample % 100 < 80).then(|| NaturalResource::new(natural_resource::STONE_OUTCROP, 2))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldGenerator {
    seed: WorldSeed,
    /// The world's base generation: its terrain and the resources that existed
    /// when it was created. Pinned for the world's life.
    version: WorldgenVersion,
    /// Additive resource layers applied over that base.
    layers: ResourceLayers,
}

impl WorldGenerator {
    /// A generator with every layer this build knows, which is what both a new
    /// world and a loaded world want.
    pub fn new(seed: WorldSeed, version: WorldgenVersion) -> Result<Self, WorldgenError> {
        Self::with_layers(seed, version, ResourceLayers::all())
    }

    pub fn with_layers(
        seed: WorldSeed,
        version: WorldgenVersion,
        layers: ResourceLayers,
    ) -> Result<Self, WorldgenError> {
        if !matches!(version.value(), 1..=3) {
            return Err(WorldgenError::UnsupportedVersion(version));
        }

        Ok(Self {
            seed,
            version,
            layers,
        })
    }

    pub const fn seed(self) -> WorldSeed {
        self.seed
    }

    pub const fn version(self) -> WorldgenVersion {
        self.version
    }

    pub const fn layers(self) -> ResourceLayers {
        self.layers
    }

    pub fn terrain_at(self, cell: WorldCell) -> TerrainId {
        match self.version.value() {
            1 => terrain_v1(self.seed, self.version, cell),
            2 | 3 => terrain_v2(self.seed, cell),
            _ => unreachable!("supported worldgen versions are checked at construction"),
        }
    }

    pub fn natural_resource_at(self, cell: WorldCell) -> Option<NaturalResource> {
        let terrain = self.terrain_at(cell);
        if let Some(resource) = self.base_resource_at(cell, terrain) {
            return Some(resource);
        }
        // Layers only fill what the base left empty, in registry order, so an
        // added layer cannot displace anything an existing world already has.
        self.layers
            .iter()
            .find_map(|layer| (layer.definition().place)(self.seed, cell, terrain))
    }

    fn base_resource_at(self, cell: WorldCell, terrain: TerrainId) -> Option<NaturalResource> {
        match self.version.value() {
            1 => natural_resource_v1(self.seed, self.version, cell, terrain),
            2 => natural_resource_v2(self.seed, cell, terrain),
            3 => natural_resource_v3(self.seed, cell, terrain),
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

fn terrain_v1(seed: WorldSeed, version: WorldgenVersion, cell: WorldCell) -> TerrainId {
    if (-2..=2).contains(&cell.x()) && cell.y() == 0 {
        return terrain::GRASS;
    }

    let mut sample = mix64(seed.value() ^ 0x6a09_e667_f3bc_c909);
    sample = mix64(sample ^ u64::from(version.value()));
    sample = mix64(sample ^ cell.x() as u64);
    sample = mix64(sample ^ (cell.y() as u64).rotate_left(32));

    match sample % 100 {
        0..=14 => terrain::WATER,
        15..=29 => terrain::ROCK,
        _ => terrain::GRASS,
    }
}

fn natural_resource_v1(
    seed: WorldSeed,
    version: WorldgenVersion,
    cell: WorldCell,
    terrain: TerrainId,
) -> Option<NaturalResource> {
    if terrain != terrain::GRASS || ((-2..=2).contains(&cell.x()) && cell.y() == 0) {
        return None;
    }

    let mut sample = mix64(seed.value() ^ 0xbb67_ae85_84ca_a73b);
    sample = mix64(sample ^ u64::from(version.value()));
    sample = mix64(sample ^ cell.x() as u64);
    sample = mix64(sample ^ (cell.y() as u64).rotate_left(32));
    let kind = match sample % 100 {
        0..=17 => natural_resource::TREE,
        18..=25 => natural_resource::STONE_OUTCROP,
        _ => return None,
    };
    let yield_quantity = 4 + ((sample >> 32) % 5) as u32;
    Some(NaturalResource::new(kind, yield_quantity))
}

const TERRAIN_FEATURE_REGION: i64 = 10;
const FOREST_FEATURE_REGION: i64 = 12;

fn terrain_v2(seed: WorldSeed, cell: WorldCell) -> TerrainId {
    if spawn_clearing(cell, 4) {
        return terrain::GRASS;
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
                return terrain::WATER;
            }
            rock = true;
        }
    }
    if rock { terrain::ROCK } else { terrain::GRASS }
}

fn natural_resource_v2(
    seed: WorldSeed,
    cell: WorldCell,
    terrain: TerrainId,
) -> Option<NaturalResource> {
    if terrain != terrain::GRASS || spawn_clearing(cell, 2) {
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
    .any(|neighbor| terrain_v2(seed, neighbor) == terrain::ROCK);
    if (near_rock && sample % 100 < 34) || sample % 1000 < 18 {
        return Some(NaturalResource::new(
            natural_resource::STONE_OUTCROP,
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
            natural_resource::TREE,
            4 + ((sample >> 40) % 5) as u32,
        ));
    }
    None
}

fn natural_resource_v3(
    seed: WorldSeed,
    cell: WorldCell,
    terrain: TerrainId,
) -> Option<NaturalResource> {
    if terrain != terrain::GRASS || spawn_clearing(cell, 2) {
        return None;
    }

    if matches!((cell.x(), cell.y()), (-3, -3) | (3, -3) | (-3, 3) | (3, 3)) {
        let sample = cell_hash(seed, 0x6265_7272_795f_7633, cell);
        return Some(NaturalResource::new(
            natural_resource::BERRY_BUSH,
            3 + ((sample >> 32) % 3) as u32,
        ));
    }

    if let Some(resource) = natural_resource_v2(seed, cell, terrain) {
        return Some(resource);
    }

    let sample = cell_hash(seed, 0x6265_7272_795f_7764, cell);
    if sample % 1000 < 18 {
        return Some(NaturalResource::new(
            natural_resource::BERRY_BUSH,
            3 + ((sample >> 40) % 3) as u32,
        ));
    }
    None
}

fn starter_resource_v2(seed: WorldSeed, cell: WorldCell) -> Option<NaturalResourceId> {
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
        Some(natural_resource::TREE)
    } else if (cell.x(), cell.y()) == stone {
        Some(natural_resource::STONE_OUTCROP)
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

#[cfg(test)]
mod layer_tests {
    use super::*;

    fn layer(name: &str) -> ResourceLayerId {
        ResourceLayerId::from_name(name).expect("test layer is registered")
    }

    fn sample_cells() -> impl Iterator<Item = WorldCell> {
        (-40..40).flat_map(|y| (-40..40).map(move |x| WorldCell::new(x, y)))
    }

    #[test]
    fn a_world_without_layers_generates_its_base_exactly() {
        let seed = WorldSeed::new(7);
        for version in 1..=3 {
            let version = WorldgenVersion::new(version);
            let plain = WorldGenerator::with_layers(seed, version, ResourceLayers::none()).unwrap();
            for cell in sample_cells() {
                assert_eq!(
                    plain.natural_resource_at(cell),
                    plain.base_resource_at(cell, plain.terrain_at(cell)),
                    "cell {cell:?} at base version {version:?}"
                );
            }
        }
    }

    /// The point of the whole mechanism: an update may add resources to a world
    /// that already exists, but may never change what that world already had.
    #[test]
    fn adding_a_layer_only_fills_cells_the_base_left_empty() {
        let seed = WorldSeed::new(11);
        let version = CURRENT_WORLDGEN_VERSION;
        let before = WorldGenerator::with_layers(seed, version, ResourceLayers::none()).unwrap();
        let after = WorldGenerator::with_layers(
            seed,
            version,
            ResourceLayers::none().with(layer("test_dense_grass")),
        )
        .unwrap();

        let mut added = 0_usize;
        for cell in sample_cells() {
            assert_eq!(
                before.terrain_at(cell),
                after.terrain_at(cell),
                "layers must never touch terrain at {cell:?}"
            );
            match before.natural_resource_at(cell) {
                Some(existing) => assert_eq!(
                    after.natural_resource_at(cell),
                    Some(existing),
                    "layer displaced an existing resource at {cell:?}"
                ),
                None => {
                    if after.natural_resource_at(cell).is_some() {
                        added += 1;
                    }
                }
            }
        }
        assert!(
            added > 0,
            "the layer never placed anything to prove the case"
        );
    }

    #[test]
    fn an_earlier_layer_wins_a_contested_cell() {
        let seed = WorldSeed::new(13);
        let version = CURRENT_WORLDGEN_VERSION;
        let (dense, sparse) = (layer("test_dense_grass"), layer("test_sparse_grass"));
        let base = WorldGenerator::with_layers(seed, version, ResourceLayers::none()).unwrap();
        let dense_only =
            WorldGenerator::with_layers(seed, version, ResourceLayers::none().with(dense)).unwrap();
        let both = WorldGenerator::with_layers(seed, version, ResourceLayers::all()).unwrap();

        let mut contested = 0_usize;
        let mut filled_by_later = 0_usize;
        for cell in sample_cells() {
            if base.natural_resource_at(cell).is_some() {
                continue;
            }
            match dense_only.natural_resource_at(cell) {
                Some(claimed) => {
                    contested += 1;
                    assert_eq!(
                        both.natural_resource_at(cell),
                        Some(claimed),
                        "a later layer overrode the earlier one at {cell:?}"
                    );
                }
                None => {
                    if both.natural_resource_at(cell).is_some() {
                        filled_by_later += 1;
                    }
                }
            }
        }
        assert!(
            contested > 0 && filled_by_later > 0,
            "the fixture proved nothing"
        );
        assert!(ResourceLayers::all().contains(sparse));
    }

    #[test]
    fn layers_are_addressed_by_stable_name_and_survive_a_round_trip() {
        for id in ResourceLayerId::all() {
            assert_eq!(ResourceLayerId::from_name(id.name()), Some(id));
        }
        assert_eq!(ResourceLayerId::from_name("no_such_layer"), None);
    }

    #[test]
    fn the_layer_mask_addresses_every_registered_layer() {
        assert!(
            RESOURCE_LAYERS.len() <= MAX_RESOURCE_LAYERS,
            "the layer bitmask cannot address every registered layer"
        );
        let all = ResourceLayers::all();
        for id in ResourceLayerId::all() {
            assert!(all.contains(id));
        }
        assert_eq!(
            all.iter().collect::<Vec<_>>(),
            ResourceLayerId::all().collect::<Vec<_>>()
        );
        assert!(ResourceLayers::none().is_empty());
    }

    #[test]
    fn layer_names_are_unique() {
        let mut names: Vec<_> = ResourceLayerId::all().map(ResourceLayerId::name).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before, "resource layer names collide");
    }
}
