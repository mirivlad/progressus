# Client-ready headless foundation

Status: **Proposed for implementation**

Date: 2026-08-31

## 1. Goal

Build the first executable Progressus increment as a deterministic, headless Rust foundation that is ready for a visual client to consume through an explicit application boundary.

This increment is complete when a future Bevy client can create a game, advance authoritative time, request renderable world and character read models, and submit application commands without depending on simulation internals.

The increment proves a narrow part of Prototype 01. It does not claim that Prototype 01 itself is complete.

## 2. Scope

The implementation provides:

- a Cargo workspace with separately testable world-generation, simulation, application, and headless executable crates;
- deterministic chunk generation from world seed, world-generation version, and chunk coordinate;
- signed world coordinates with correct behavior around negative chunk boundaries;
- a discrete authoritative simulation clock;
- stable Progressus-owned entity identifiers;
- a deterministic new-game scenario containing five named characters;
- an application command boundary that prevents clients from mutating simulation storage directly;
- immutable client read models containing simulation time, generated terrain, chunk coordinates, and character identity and positions;
- a headless executable that creates a game, advances it for a requested number of ticks, and prints a deterministic summary;
- automated determinism, dependency-boundary, and long-run smoke checks.

## 3. Explicit non-goals

This increment does not implement:

- Bevy or any other renderer;
- save/load;
- movement or pathfinding;
- jobs, reservations, items, hauling, crafting, or construction;
- modified chunk persistence;
- simulation LOD;
- parallel authoritative simulation;
- a final chunk-size or storage-performance decision.

Those systems remain later Prototype 01 work.

## 4. Workspace and dependency direction

The workspace has this dependency graph:

```text
progressus-headless
        |
        v
progressus-app
        |
        v
progressus-sim
        |
        v
progressus-worldgen
```

Responsibilities:

- `progressus-worldgen` owns seed/version types, coordinates, terrain definitions, and pure canonical chunk generation.
- `progressus-sim` owns authoritative time, stable entity identity, characters, and simulation state transitions.
- `progressus-app` owns new-game construction, input commands, client-facing queries, and detached read models.
- `progressus-headless` is a thin executable consumer of `progressus-app` and proves that no graphical runtime is required.

No crate in this increment depends on Bevy. The client-facing crate must not expose mutable simulation collections or storage handles.

## 5. Bootstrap coordinate decision

The architecture documentation says that the numeric coordinate representation requires an explicit decision. Before implementation, add an accepted bootstrap ADR with these narrow choices:

- authoritative terrain positions are integer world-cell coordinates using signed 64-bit axes;
- chunk coordinates use signed 64-bit axes;
- conversion uses Euclidean division and remainder so negative positions map correctly;
- local cell coordinates are non-negative values within a chunk;
- the initial chunk side is 32 cells, a bootstrap implementation parameter rather than a permanent performance conclusion;
- changing chunk geometry after persisted worlds exist requires a world-generation version change or an explicit migration.

The initial chunk side is a compile-time constant suitable for tests and diagnostic rendering. Its value does not assert that benchmarking has selected the final chunk size.

This ADR does not decide continuous/fixed-point character movement, because Prototype 01 movement is outside this increment.

## 6. Deterministic world generation

World generation is a pure operation conceptually equivalent to:

```text
generate_chunk(seed, worldgen_version, chunk_coord) -> GeneratedChunk
```

`GeneratedChunk` contains a fixed 32 by 32 collection of canonical terrain cells. The initial terrain vocabulary is grass, water, and rock. A documented, project-owned integer mixing function samples the world seed, world-generation version, and global cell coordinates; the implementation must not use Rust's unspecified default hashing as a file-format or world-generation contract. This diagnostic terrain algorithm is world-generation version 1 and may be replaced under a later version.

Generation derives every result from stable inputs. It must not consume process-global randomness, wall-clock time, neighboring cache state, or visitation order. Cross-chunk visual continuity is desirable only where it follows from stable world-space sampling; no mutable neighbor-generation protocol is introduced.

Unknown world-generation versions return an explicit error rather than silently using current behavior.

## 7. Simulation model

`Simulation` owns:

- the world seed and supported world-generation version;
- the current discrete tick;
- a monotonic stable-ID allocator;
- five initial character records;
- authoritative character positions.

A new game deterministically locates five spawn positions in or near the origin chunk on walkable terrain. IDs and names are stable for the same scenario construction. Advancing the simulation changes only authoritative tick state in this increment; no renderer frame time or wall-clock duration enters the simulation.

Tick overflow and impossible duplicate IDs return explicit errors or fail loudly in tests. The implementation does not silently wrap or repair authoritative state.

## 8. Application boundary and client read model

`progressus-app` is the only intended entry point for executables and future clients.

The initial API supports these operations conceptually:

```text
Application::new_game(options)
Application::execute(command)
Application::snapshot(query)
```

Commands cover only behavior implemented by this increment, principally deterministic tick advancement. Queries request a bounded set of chunk coordinates and return a detached snapshot.

The snapshot contains:

- current simulation tick;
- world-generation version;
- requested chunks with coordinate, dimensions, and terrain cells;
- the five characters with stable ID, name, and authoritative world position.

Returned collections have deterministic ordering. Read models own or copy their data so a client cannot mutate authoritative state or retain internal storage references. They use Progressus types, never Bevy entities or renderer handles.

This is the handoff point for a visual client: a Bevy application can translate the snapshot into disposable render entities and send commands back through `Application`.

## 9. Headless executable

The executable accepts a seed and tick count with minimal argument handling, constructs the application, advances it, queries a bounded origin-area snapshot, and prints a stable human-readable summary.

Invalid arguments and application errors produce a non-zero exit status and a concise diagnostic. A heavy CLI framework is not introduced for this small interface.

## 10. Verification

The implementation must provide focused automated checks for:

1. identical inputs generate structurally equal chunk data;
2. generating the same coordinate set in different orders produces identical per-coordinate results;
3. negative world positions map to the expected chunks and local cells;
4. unsupported world-generation versions fail explicitly;
5. a new game creates exactly five unique stable character IDs on walkable cells;
6. identical new-game options and command sequences produce identical client snapshots;
7. snapshot ordering is stable regardless of query coordinate order;
8. returned read models cannot provide mutable access to authoritative storage by API construction;
9. a large headless tick run completes without overflow, panic, or changing the bounded requested chunk set;
10. the dependency graph contains no Bevy package.

The verification gate is:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p progressus-headless -- --seed 42 --ticks 100000
```

The exact executable summary becomes part of the smoke-test evidence, not a permanent serialization format.

## 11. Client-readiness acceptance condition

The headless foundation is ready for visual-client work when all verification gates pass and a consumer outside `progressus-sim` can:

1. create an application from a seed;
2. advance authoritative time;
3. request positive and negative chunks;
4. receive deterministic renderable terrain data;
5. receive five stable character IDs, names, and positions;
6. do all of the above without Bevy, a window, GPU, audio, or access to mutable simulation internals.

At that point the next increment may add a minimal Bevy client that depends on `progressus-app` only.
