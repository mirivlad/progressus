use std::collections::BTreeMap;

use progressus_worldgen::{
    CURRENT_WORLDGEN_VERSION, ChunkCoord, Terrain, WorldCell, WorldGenerator, WorldSeed,
    WorldgenError, WorldgenVersion,
};

#[test]
fn identical_inputs_generate_identical_chunks() {
    let first = WorldGenerator::new(WorldSeed::new(42), CURRENT_WORLDGEN_VERSION).unwrap();
    let second = WorldGenerator::new(WorldSeed::new(42), CURRENT_WORLDGEN_VERSION).unwrap();
    let coordinate = ChunkCoord::new(7, -11);

    assert_eq!(
        first.generate(coordinate).unwrap(),
        second.generate(coordinate).unwrap()
    );
}

#[test]
fn generation_order_does_not_change_chunk_data() {
    let coordinates = [
        ChunkCoord::new(-1, 0),
        ChunkCoord::new(0, 0),
        ChunkCoord::new(1, 0),
        ChunkCoord::new(0, -1),
    ];
    let generator = WorldGenerator::new(WorldSeed::new(73), CURRENT_WORLDGEN_VERSION).unwrap();

    let forward = coordinates
        .iter()
        .copied()
        .map(|coordinate| (coordinate, generator.generate(coordinate).unwrap()))
        .collect::<BTreeMap<_, _>>();
    let reverse = coordinates
        .iter()
        .rev()
        .copied()
        .map(|coordinate| (coordinate, generator.generate(coordinate).unwrap()))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(forward, reverse);
}

#[test]
fn bootstrap_spawn_cells_are_grass_for_every_seed() {
    for seed in [0, 1, 42, u64::MAX] {
        let generator =
            WorldGenerator::new(WorldSeed::new(seed), CURRENT_WORLDGEN_VERSION).unwrap();

        for x in -2..=2 {
            let cell = WorldCell::new(x, 0);
            let (chunk_coordinate, local) = cell.split();
            let chunk = generator.generate(chunk_coordinate).unwrap();
            assert_eq!(chunk.terrain_at(local), Some(Terrain::Grass));
        }
    }
}

#[test]
fn unsupported_worldgen_versions_fail_explicitly() {
    assert_eq!(
        WorldGenerator::new(WorldSeed::new(42), WorldgenVersion::new(999)),
        Err(WorldgenError::UnsupportedVersion(WorldgenVersion::new(999)))
    );
}

#[test]
fn coordinates_outside_world_cell_range_fail_explicitly() {
    let generator = WorldGenerator::new(WorldSeed::new(42), CURRENT_WORLDGEN_VERSION).unwrap();
    let coordinate = ChunkCoord::new(i64::MAX, 0);

    assert_eq!(
        generator.generate(coordinate),
        Err(WorldgenError::CoordinateOutOfRange(coordinate))
    );
}
