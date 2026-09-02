use std::collections::BTreeMap;

use progressus_worldgen::{
    CURRENT_WORLDGEN_VERSION, ChunkCoord, GeneratedChunk, NaturalResourceKind, Terrain, WorldCell,
    WorldGenerator, WorldSeed, WorldgenError, WorldgenVersion,
};

fn terrain_digest(chunk: &GeneratedChunk) -> u64 {
    chunk
        .cells()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |digest, terrain| {
            let value = match terrain {
                Terrain::Grass => 0_u64,
                Terrain::Water => 1,
                Terrain::Rock => 2,
            };
            (digest ^ value).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

fn resource_digest(chunk: &GeneratedChunk) -> u64 {
    chunk
        .resources()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |digest, resource| {
            let value = match resource {
                None => 0_u64,
                Some(resource) => {
                    let kind = match resource.kind() {
                        NaturalResourceKind::Tree => 1_u64,
                        NaturalResourceKind::StoneOutcrop => 2_u64,
                    };
                    kind | (u64::from(resource.yield_quantity()) << 8)
                }
            };
            (digest ^ value).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

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

#[test]
fn worldgen_v1_golden_fixtures_do_not_drift() {
    let coordinates = [
        ChunkCoord::new(-2, -1),
        ChunkCoord::new(0, 0),
        ChunkCoord::new(3, 2),
    ];
    let actual = [42, 73]
        .into_iter()
        .flat_map(|seed| {
            let generator =
                WorldGenerator::new(WorldSeed::new(seed), CURRENT_WORLDGEN_VERSION).unwrap();
            coordinates
                .into_iter()
                .map(move |coordinate| terrain_digest(&generator.generate(coordinate).unwrap()))
        })
        .collect::<Vec<_>>();

    assert_eq!(
        actual,
        vec![
            9_371_191_092_319_503_193,
            4_450_540_460_892_477_893,
            12_191_232_430_594_058_174,
            4_960_922_673_262_440_894,
            13_870_068_215_867_835_910,
            8_887_388_049_380_320_079,
        ]
    );
}

#[test]
fn natural_resources_exist_only_on_grass_and_leave_spawn_corridor_clear() {
    for seed in [0, 1, 42, 73, u64::MAX] {
        let generator =
            WorldGenerator::new(WorldSeed::new(seed), CURRENT_WORLDGEN_VERSION).unwrap();
        for coordinate in [
            ChunkCoord::new(-1, 0),
            ChunkCoord::new(0, 0),
            ChunkCoord::new(1, 0),
        ] {
            let chunk = generator.generate(coordinate).unwrap();
            for y in 0..progressus_worldgen::CHUNK_SIDE {
                for x in 0..progressus_worldgen::CHUNK_SIDE {
                    let local = progressus_worldgen::LocalCell::new(x, y);
                    if chunk.natural_resource_at(local).is_some() {
                        assert_eq!(chunk.terrain_at(local), Some(Terrain::Grass));
                    }
                }
            }
        }
        for x in -2..=2 {
            let cell = WorldCell::new(x, 0);
            let (coordinate, local) = cell.split();
            assert_eq!(
                generator
                    .generate(coordinate)
                    .unwrap()
                    .natural_resource_at(local),
                None
            );
        }
    }
}

#[test]
fn worldgen_v1_natural_resource_golden_fixtures_do_not_drift() {
    let coordinates = [
        ChunkCoord::new(-2, -1),
        ChunkCoord::new(0, 0),
        ChunkCoord::new(3, 2),
    ];
    let actual = [42, 73]
        .into_iter()
        .flat_map(|seed| {
            let generator =
                WorldGenerator::new(WorldSeed::new(seed), CURRENT_WORLDGEN_VERSION).unwrap();
            coordinates
                .into_iter()
                .map(move |coordinate| resource_digest(&generator.generate(coordinate).unwrap()))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        vec![
            2_835_845_614_304_509_097,
            13_108_369_062_182_181_737,
            2_361_086_587_400_993_908,
            12_973_908_920_169_108_313,
            2_924_805_587_358_519_751,
            5_345_185_340_990_405_334,
        ]
    );
}
