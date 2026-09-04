use std::error::Error;
use std::mem::{size_of, size_of_val};
use std::time::{Duration, Instant};

use progressus_sim::{
    ChunkCoord, EntityId, GeneratedChunk, Simulation, Terrain, WorldCell, WorldPosition, WorldSeed,
};

const SEED: WorldSeed = WorldSeed::new(73);
const WORLDGEN_SAMPLES: usize = 256;
const PATH_SAMPLES: usize = 1_000;
const SAVE_LOAD_SAMPLES: usize = 100;
const TICK_SAMPLES: u64 = 100_000;

fn main() -> Result<(), Box<dyn Error>> {
    let simulation = Simulation::new(SEED)?;
    let sample_chunk = simulation.generated_chunk(ChunkCoord::new(10_000, -20_000))?;
    let resident_chunks = simulation.resident_chunk_count();
    let chunk_bytes = estimated_chunk_bytes(&sample_chunk);

    let mut worldgen_ns = Vec::with_capacity(WORLDGEN_SAMPLES);
    for index in 0..WORLDGEN_SAMPLES {
        let x = 10_000 + i64::try_from(index % 16)?;
        let y = -20_000 + i64::try_from(index / 16)?;
        let start = Instant::now();
        let chunk = simulation.generated_chunk(ChunkCoord::new(x, y))?;
        std::hint::black_box(chunk);
        worldgen_ns.push(start.elapsed().as_nanos());
    }

    let mut tick_simulation = Simulation::new(SEED)?;
    let tick_start = Instant::now();
    tick_simulation.advance_ticks(TICK_SAMPLES)?;
    let tick_elapsed = tick_start.elapsed();
    let ticks_per_second = TICK_SAMPLES as f64 / tick_elapsed.as_secs_f64();

    let mut path_simulation = Simulation::new(SEED)?;
    let cora = EntityId::new(3).expect("Cora has a stable nonzero bootstrap ID");
    let destination_cell = WorldCell::new(4, 0);
    if !path_simulation.is_explored(destination_cell)
        || path_simulation.effective_terrain_at(destination_cell)? != Terrain::Grass
    {
        return Err("benchmark path destination is not explored grass".into());
    }
    let destination = WorldPosition::from_cell_center(destination_cell)
        .expect("benchmark destination cell center is representable");
    let mut path_ns = Vec::with_capacity(PATH_SAMPLES);
    for _ in 0..PATH_SAMPLES {
        let start = Instant::now();
        path_simulation.move_to(cora, destination)?;
        path_ns.push(start.elapsed().as_nanos());
    }
    let path_waypoints = path_simulation
        .characters()
        .find(|character| character.id() == cora)
        .expect("Cora remains present")
        .navigation_waypoints()
        .count();

    let mut sparse = Simulation::new(SEED)?;
    for cell in [
        WorldCell::new(2_048, 17),
        WorldCell::new(-3_200, -19),
        WorldCell::new(25_600, 31_999),
    ] {
        let base = sparse.effective_terrain_at(cell)?;
        sparse.set_terrain_override(cell, different_terrain(base))?;
    }
    let encoded = sparse.save_json()?;
    let save_size = encoded.len();

    let mut save_ns = Vec::with_capacity(SAVE_LOAD_SAMPLES);
    for _ in 0..SAVE_LOAD_SAMPLES {
        let start = Instant::now();
        std::hint::black_box(sparse.save_json()?);
        save_ns.push(start.elapsed().as_nanos());
    }
    let mut load_ns = Vec::with_capacity(SAVE_LOAD_SAMPLES);
    for _ in 0..SAVE_LOAD_SAMPLES {
        let start = Instant::now();
        std::hint::black_box(Simulation::load_json(&encoded)?);
        load_ns.push(start.elapsed().as_nanos());
    }

    println!("prototype01 performance baseline");
    println!("seed={}", SEED.value());
    print_distribution("worldgen_chunk", &mut worldgen_ns, WORLDGEN_SAMPLES);
    println!(
        "resident_chunk_estimate bytes_per_chunk={} resident_chunks={} total_bytes={} note=excludes_allocator_and_map_overhead",
        chunk_bytes,
        resident_chunks,
        chunk_bytes * resident_chunks
    );
    println!(
        "simulation ticks={} elapsed_ms={:.3} ticks_per_second={:.0}",
        TICK_SAMPLES,
        millis(tick_elapsed),
        ticks_per_second
    );
    print_distribution("path_plan", &mut path_ns, PATH_SAMPLES);
    println!("path_plan waypoints={} destination=(4,0)", path_waypoints);
    println!("sparse_save bytes={} distant_overrides=3", save_size);
    print_distribution("save_json", &mut save_ns, SAVE_LOAD_SAMPLES);
    print_distribution("load_json", &mut load_ns, SAVE_LOAD_SAMPLES);
    Ok(())
}

fn estimated_chunk_bytes(chunk: &GeneratedChunk) -> usize {
    size_of::<GeneratedChunk>() + size_of_val(chunk.cells()) + size_of_val(chunk.resources())
}

fn different_terrain(base: Terrain) -> Terrain {
    match base {
        Terrain::Grass => Terrain::Rock,
        Terrain::Water | Terrain::Rock => Terrain::Grass,
    }
}

fn print_distribution(label: &str, samples: &mut [u128], count: usize) {
    samples.sort_unstable();
    println!(
        "{} samples={} min_us={:.3} p50_us={:.3} p95_us={:.3} max_us={:.3}",
        label,
        count,
        micros(samples[0]),
        micros(percentile(samples, 50)),
        micros(percentile(samples, 95)),
        micros(samples[samples.len() - 1])
    );
}

fn percentile(samples: &[u128], percentile: usize) -> u128 {
    let index = (samples.len() - 1) * percentile / 100;
    samples[index]
}

fn micros(nanoseconds: u128) -> f64 {
    nanoseconds as f64 / 1_000.0
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}
