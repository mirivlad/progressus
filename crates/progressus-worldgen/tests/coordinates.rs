use progressus_worldgen::{CHUNK_SIDE, ChunkCoord, LocalCell, WorldCell};

#[test]
fn negative_world_cells_use_euclidean_chunk_mapping() {
    assert_eq!(
        WorldCell::new(-1, -33).split(),
        (ChunkCoord::new(-1, -2), LocalCell::new(31, 31))
    );
    assert_eq!(
        WorldCell::new(-32, -32).split(),
        (ChunkCoord::new(-1, -1), LocalCell::new(0, 0))
    );
}

#[test]
fn chunk_local_round_trip_preserves_world_cells() {
    for cell in [
        WorldCell::new(i64::from(CHUNK_SIDE), 0),
        WorldCell::new(-1, 17),
        WorldCell::new(-10_000, 10_000),
    ] {
        let (chunk, local) = cell.split();
        assert_eq!(chunk.world_cell(local), Some(cell));
    }
}
