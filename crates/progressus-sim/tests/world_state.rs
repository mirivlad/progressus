use progressus_sim::{ChunkCoord, LocalCell, Simulation, Terrain, WorldCell, WorldSeed};

#[test]
fn untouched_effective_chunk_matches_raw_generated_chunk() {
    let simulation = Simulation::new(WorldSeed::new(73)).unwrap();
    let coordinate = ChunkCoord::new(7, -11);
    let raw = simulation.generated_chunk(coordinate).unwrap();
    let effective = simulation.effective_chunk(coordinate).unwrap();

    assert_eq!(effective.coordinate(), raw.coordinate());
    assert_eq!(effective.cells(), raw.cells());
}

#[test]
fn point_lookup_and_materialized_chunk_use_the_same_resolution() {
    let mut simulation = Simulation::new(WorldSeed::new(73)).unwrap();
    let modified = WorldCell::new(0, 0);
    let untouched = WorldCell::new(1, 0);
    simulation
        .set_terrain_override(modified, Terrain::Rock)
        .unwrap();

    for cell in [modified, untouched] {
        let (coordinate, local) = cell.split();
        assert_eq!(
            simulation.effective_terrain_at(cell).unwrap(),
            simulation
                .effective_chunk(coordinate)
                .unwrap()
                .terrain_at(local)
                .unwrap(),
        );
    }
}

#[test]
fn one_override_changes_only_its_local_cell() {
    let mut simulation = Simulation::new(WorldSeed::new(73)).unwrap();
    let coordinate = ChunkCoord::new(0, 0);
    let target = LocalCell::new(0, 0);
    let before = simulation
        .effective_chunk(coordinate)
        .unwrap()
        .cells()
        .to_vec();

    simulation
        .set_terrain_override(WorldCell::new(0, 0), Terrain::Rock)
        .unwrap();
    let after = simulation
        .effective_chunk(coordinate)
        .unwrap()
        .cells()
        .to_vec();

    assert_eq!(
        before
            .iter()
            .zip(&after)
            .filter(|(before, after)| before != after)
            .count(),
        1,
    );
    assert_eq!(
        simulation
            .effective_chunk(coordinate)
            .unwrap()
            .terrain_at(target),
        Some(Terrain::Rock)
    );
}

fn different_from(base: Terrain) -> Terrain {
    match base {
        Terrain::Grass => Terrain::Rock,
        Terrain::Water => Terrain::Grass,
        Terrain::Rock => Terrain::Grass,
    }
}

fn effective_cells(simulation: &Simulation, coordinate: ChunkCoord) -> Vec<Terrain> {
    simulation
        .effective_chunk(coordinate)
        .unwrap()
        .cells()
        .to_vec()
}

#[test]
fn identical_mutations_and_unrelated_visitation_preserve_effective_state() {
    let mut first = Simulation::new(WorldSeed::new(73)).unwrap();
    let mut second = Simulation::new(WorldSeed::new(73)).unwrap();
    let edits = [
        WorldCell::new(2_048, 17),
        WorldCell::new(-3_200, -19),
        WorldCell::new(0, 0),
    ];

    for simulation in [&mut first, &mut second] {
        for cell in edits {
            let requested = different_from(simulation.effective_terrain_at(cell).unwrap());
            simulation.set_terrain_override(cell, requested).unwrap();
        }
    }

    let compared = [
        ChunkCoord::new(64, 0),
        ChunkCoord::new(-100, -1),
        ChunkCoord::new(0, 0),
    ];
    for coordinate in compared {
        assert_eq!(
            effective_cells(&first, coordinate),
            effective_cells(&second, coordinate)
        );
    }

    let before = effective_cells(&first, ChunkCoord::new(64, 0));
    for coordinate in [ChunkCoord::new(999, -999), ChunkCoord::new(-444, 333)] {
        first.generated_chunk(coordinate).unwrap();
        first.effective_chunk(coordinate).unwrap();
    }
    assert_eq!(effective_cells(&first, ChunkCoord::new(64, 0)), before);
}

#[test]
fn distant_queries_are_ephemeral_and_do_not_expand_residency() {
    let simulation = Simulation::new(WorldSeed::new(73)).unwrap();
    let before = simulation.resident_chunks().collect::<Vec<_>>();
    let revision = simulation.residency_revision();
    let distant = ChunkCoord::new(12_345, -23_456);

    simulation.generated_chunk(distant).unwrap();
    simulation.effective_chunk(distant).unwrap();
    simulation.natural_resources_in_chunk(distant).unwrap();

    assert_eq!(simulation.resident_chunks().collect::<Vec<_>>(), before);
    assert_eq!(simulation.residency_revision(), revision);
}
