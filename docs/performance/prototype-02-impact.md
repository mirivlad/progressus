# Prototype 02 performance impact

Status: measured on 2026-09-26; descriptive comparison, not a capacity target.

## Method

The same `./scripts/benchmark-prototype-01.sh` release harness ran at historical Prototype 01 commit `f93a6e1` in an isolated worktree and at current Prototype 02 commit `01c7e7f` on the same host. Both commits used Rust 1.97.1 and the same shared Cargo target directory. The host reports Intel Xeon E5-2680 v4, 28 logical CPUs and 31 GiB RAM. Each build was run twice; both outputs are retained below rather than choosing one favorable result. The older published i3-2120 table in `prototype-01-baseline.md` is not used for the ratio.

| Measurement | P01 run 1 | P01 run 2 | P02 run 1 | P02 run 2 |
| --- | ---: | ---: | ---: | ---: |
| Idle simulation, 100,000 ticks (ms) | 69.183 | 70.808 | 1,685.207 | 1,656.737 |
| Idle ticks/s | 1,445,433 | 1,412,264 | 59,340 | 60,360 |
| Worldgen p50 (us/chunk) | 840.618 | 724.761 | 1,184.106 | 1,056.579 |
| Worldgen p95 (us/chunk) | 987.133 | 820.698 | 1,803.767 | 1,167.659 |
| Local path p50 (us) | 6.571 | 6.609 | 8.695 | 8.107 |
| Sparse save size (bytes) | 8,341 | 8,341 | 9,259 | 9,259 |
| Save p50 (us) | 16.955 | 16.565 | 20.472 | 21.766 |
| Load p50 (ms) | 8.599 | 8.649 | 12.662 | 12.430 |
| Raw resident payload estimate (bytes/chunk) | 9,280 | 9,280 | 14,400 | 14,400 |

On the second run, idle ticks/s fell by about 23.4 times. The current five-person simulation still advanced 100,000 idle ticks in 1.66 seconds on this host. The change covers all work since Prototype 01, including needs, jobs, resource layers, and client-independent gameplay; this benchmark does not attribute the cost to one subsystem. The 10,000-tick Prototype 02 settlement acceptance test separately exercises food, bed sleep, study, smelting, save/load and physical ownership, and reached at most 16 resident chunks.

The harness reports distributions but does not persist its 256 worldgen, 1,000 path, or 100 save/load individual samples; therefore these percentile comparisons cannot be independently re-aggregated. It also measures an idle five-person world, not population scaling or a busy settlement. Profile the tick loop with representative activity and larger populations before setting a performance target or optimizing a suspected subsystem.
