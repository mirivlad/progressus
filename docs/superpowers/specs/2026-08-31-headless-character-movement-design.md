# Headless character movement across chunk boundaries

Status: **Approved for implementation**

Date: 2026-08-31

## Goal

Prove that authoritative characters can move deterministically, one world cell per simulation tick, across positive and negative chunk boundaries without a finite global grid, preloaded route, or client-owned simulation state.

This is a bootstrap implementation of P01-SIM-04. It deliberately does not claim complete navigation or pathfinding.

## Scope

The increment adds:

- a persistent `MovementState` to authoritative characters;
- four cardinal `Direction` values;
- deterministic one-cell movement through ordinary simulation ticks;
- terrain-based passability checks using generated terrain;
- movement commands and movement read models through `progressus-app`;
- a headless deterministic walker that drives those commands through the public app API and proves long-distance chunk traversal;
- focused tests and documentation of the partial P01-SIM-04 status.

It does not add Bevy, A*, general pathfinding, jobs, queues of movement commands, chunk residency/cache in the simulation, save/load, mutable chunks, collision, speed, UI, or a world-wide grid.

## Authoritative model

`Character` owns one of:

```text
MovementState::Idle
MovementState::Moving { direction: Direction }
```

`Direction` is `East`, `West`, `North`, or `South`. It computes only one cardinally adjacent `WorldCell` using checked signed-coordinate arithmetic. It neither has nor infers a path.

At most one `MovementState` exists per character. There is no movement command queue.

## Command semantics

`progressus-app` exposes:

```text
Command::SetMovementDirection { character_id, direction }
Command::StopMovement { character_id }
```

Commands are atomically applied to the authoritative application state before a subsequent tick is advanced. `SetMovementDirection` validates, without altering existing state:

1. that the character exists;
2. that the first adjacent cell is representable without coordinate overflow;
3. that the first adjacent cell is `Terrain::Grass`.

Only a successful validation replaces the old `MovementState`. An invalid command returns an explicit application/simulation error and leaves the character's current direction unchanged. This covers replacement of an in-progress direction without rolling back a step completed by an earlier tick.

`StopMovement` makes an existing character idle. Stopping an already idle character is successful and idempotent.

## Tick semantics

Each authoritative tick processes characters in their stable ID order. An idle character does nothing. A moving character:

1. computes its adjacent cell from its current authoritative position and saved direction;
2. stops at its current position if the coordinate would overflow;
3. regenerates the target cell's chunk from worldgen and checks passability;
4. moves exactly one cell if the target is grass;
5. otherwise stops at its current position.

Water and rock never accept a moving character. A non-passable next cell and coordinate overflow are normal deterministic stops, not a fatal simulation-tick error. A chunk boundary receives no special handling: the existing `WorldCell::split` model determines the needed `ChunkCoord` for terrain lookup.

One tick advances a character by at most one cell. The model has no speed or sub-cell timing semantics yet.

## Application read model

`CharacterSnapshot` gains an owned/copyable movement field. A consumer can observe the current direction or idle state through `progressus-app` without accessing `progressus-sim` storage.

All movement intent enters through `Application::execute`; snapshots remain detached copies. Bevy remains absent from the application, simulation, worldgen, and headless dependency chain.

## Deterministic headless walker

`progressus-headless` gains an optional `--travel-chunks <positive count>` scenario. It uses only `progressus-app` commands and snapshots:

1. read the active character and terrain of its adjacent cells through a bounded snapshot query;
2. consider only grass neighbors and choose the one with the lowest walker-local `visit_count`;
3. break equal visit counts using fixed priority `East`, `North`, `South`, `West`;
4. send `SetMovementDirection`, advance exactly one tick, take a new snapshot, and increment `visit_count` only for the authoritative cell actually reached;
5. stop successfully only when the character has crossed at least the requested number of positive chunk-x boundaries from its starting chunk.

The walker initializes the starting authoritative cell with `visit_count = 1`, then stores only a local `BTreeMap<WorldCell, visit_count>` and progress counters for the active scenario; it does not precompute or pass a full route to simulation. It has no frontier, open/closed set, destination search, BFS, Dijkstra, A*, path-cost heuristic, or authority over character state. Its visit count is only a deterministic local tie-break rule for the next adjacent step. It is a bounded external test driver, not a pathfinding system.

The scenario has a fixed step limit of `max(1_024, requested_chunk_count * 512)`, with checked multiplication. Exhausting it, finding no unvisited grass neighbor, or observing a failed authoritative step returns a diagnostic error containing the seed, character ID, position, maximum reached chunk x-coordinate, and step count. For a fixed seed and initial snapshot, candidate order and command sequence are deterministic.

## Tests

The test suite must prove:

1. eastward movement crosses world cell x=31 to x=32 in one tick;
2. westward movement crosses x=0 to x=-1 in one tick;
3. a movement command over water or rock is rejected before it replaces an existing valid direction;
4. persisted movement rechecks terrain each tick and stops safely when its next cell is impassable or would overflow;
5. a replacement direction starts from the current post-tick cell and discards the old direction;
6. equal seed, command sequence, and tick count give equal authoritative movement state;
7. a headless external walker crosses many positive chunk boundaries within its deterministic bound while keeping the same `EntityId`;
8. the existing worldgen, application, headless, and dependency-boundary tests remain green.

The long-traversal fixture is seed 0: the least-visited walker crosses 64 positive chunk boundaries in 5,050 steps under the 32,768-step bound. The production simulation must not contain seed search or walker logic.

## Documentation status

Update Prototype 01 documentation to state that P01-SIM-04 has a bootstrap direction-driven movement implementation. Explicitly retain the following as incomplete: navigation/pathfinding around obstacles, job/AI movement policy, speed, collision, chunk residency, and long-distance routing.

## Verification gate

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p progressus-headless -- --seed 42 --ticks 100000
./scripts/verify-core-dependency-boundary.sh
```
