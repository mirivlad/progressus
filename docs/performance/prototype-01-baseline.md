# Prototype 01 performance baseline

This document records evidence, not release targets. Re-run it after changes that materially affect world generation, residency, pathfinding, simulation scheduling, or persistence.

## Reproduce

```bash
./scripts/benchmark-prototype-01.sh
```

The script runs `progressus-sim`'s `prototype01_benchmark` example with the Cargo `release` profile and `CARGO_BUILD_JOBS=1` by default. The benchmark does not link Bevy.

## Reference machine

Measured 2026-09-04 on:

- Ubuntu 24.04.4 LTS, Linux 6.8.0-107-generic;
- Intel Core i3-2120 @ 3.30 GHz, 2 cores / 4 threads;
- 7.7 GiB RAM;
- rustc 1.89.0 / cargo 1.89.0;
- world seed 73.

The table below is the second warm run after the release binary was already built.

| Measurement | Prototype 01 baseline |
| --- | ---: |
| Raw worldgen, 256 distant chunks | p50 887.890 us; p95 1,356.525 us |
| Raw worldgen range | 732.781-1,723.936 us/chunk |
| Estimated raw resident chunk payload | 9,280 bytes/chunk |
| Initial resident set | 12 chunks, ~111,360 bytes raw payload |
| Idle simulation, 100,000 ticks | 88.977 ms; ~1,123,885 ticks/s |
| Local cardinal A* plan, 4-cell route, 1,000 samples | p50 8.681 us; p95 8.897 us |
| Sparse save with three distant terrain overrides | 8,341 bytes |
| `save_json`, 100 samples | p50 22.815 us; p95 23.578 us |
| `load_json`, 100 samples | p50 10.414 ms; p95 10.668 ms |

## Interpretation and limits

The resident-memory number is deliberately an estimate of raw generated-chunk payload: `GeneratedChunk` plus its terrain/resource arrays. It excludes allocator bookkeeping, `BTreeMap` nodes, authoritative sparse modifications, entities, jobs, and client presentation state. It is useful for comparing chunk geometry changes, not as a process RSS estimate.

The tick number is an idle authoritative simulation baseline. The separate `--activity-smoke` acceptance scenario is used for correctness under harvesting, hauling, production, construction, persistence, and residency; it is not currently a microbenchmark.

The pathfinding sample repeatedly plans from Cora's unchanged start position to `(4, 0)`, producing four cardinal waypoints. It measures the current local A* bootstrap, not future hierarchical or long-distance navigation.

`save_json` measures a sparse world with three widely separated terrain overrides. `load_json` is much more expensive because loading validates the save and reconstructs derived state, including regenerating the current 12 raw resident chunks. Resident chunks themselves are not serialized.

These values are intentionally not pass/fail thresholds. A future benchmark should add activity-heavy ticks/s, larger local A* fixtures, more entities, and Simulation LOD once those systems exist.
