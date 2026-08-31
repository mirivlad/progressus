# Headless Character Movement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add deterministic, direction-driven character movement that crosses generated chunk boundaries through the public headless application boundary.

**Architecture:** `progressus-sim` owns a single persistent direction per character and applies at most one checked, terrain-validated step per tick. `progressus-app` validates and accepts commands before mutating the state and publishes detached movement snapshots. `progressus-headless` remains an external controller: its bounded deterministic walker observes snapshots, picks one adjacent grass cell, and issues ordinary commands without giving simulation a route or a pathfinding algorithm.

**Tech Stack:** Rust 2024 workspace, Rust standard library only, `BTreeMap` authoritative character storage and walker-local visit counts, integration tests, Bash dependency-boundary guard.

## Global Constraints

- `progressus-sim` and every crate below the application boundary must not depend on Bevy.
- Authoritative movement is one cardinal world cell at most per checked `u64` simulation tick; it never depends on wall-clock time.
- `MovementState` is exactly `Idle` or `Moving { direction: Direction }`; there is no command queue and no path state.
- `Direction` is exactly `East`, `West`, `North`, or `South` and computes an adjacent `WorldCell` using checked `i64` arithmetic.
- `SetMovementDirection` validates the target before replacing existing movement; a rejected command preserves the existing state.
- Every tick rechecks coordinate overflow and terrain. Water, rock, and overflow leave the position unchanged and set movement to `Idle`.
- Passability is `Terrain::Grass` only. Chunk transitions are ordinary `WorldCell` neighbors and use existing Euclidean `WorldCell::split` conversion.
- The application boundary is the only mutation path used by headless code; snapshots are owned, detached values.
- No Bevy, A*, full pathfinding, jobs, persistence, chunk residency/cache, mutable world state, collision, speed model, or UI is added.
- Every behavioral change follows a focused red-green test cycle.

## File map

- Modify `crates/progressus-sim/src/entity.rs`: define public movement types and hold state on `Character`.
- Modify `crates/progressus-sim/src/lib.rs`: export `Direction` and `MovementState`.
- Modify `crates/progressus-sim/src/simulation.rs`: expose command validation, carry out per-tick movement, and return explicit errors.
- Modify `crates/progressus-sim/src/simulation.rs`: add private-module crossing, replacement, blocking, overflow, and deterministic-state tests beside the authoritative state they set up.
- Modify `crates/progressus-sim/tests/simulation.rs`: retain the public bootstrap and public-state determinism contract tests.
- Modify `crates/progressus-app/src/lib.rs`: add movement commands and forward them without exposing simulation storage.
- Modify `crates/progressus-app/src/read_model.rs`: copy movement into `CharacterSnapshot`.
- Modify `crates/progressus-app/tests/client_boundary.rs`: test the command/read-model contract through `Application`.
- Modify `crates/progressus-headless/src/main.rs`: parse `--travel-chunks N` and implement the bounded least-visited external walker.
- Modify `crates/progressus-headless/tests/cli.rs`: test the deterministic long-travel CLI scenario and argument validation.
- Modify `README.md`, `docs/architecture/overview.md`, and `docs/milestones/prototype-01.md`: document the exact bootstrap status without claiming full navigation or residency.

---

### Task 1: Authoritative movement state and tick transitions

**Files:**
- Modify: `crates/progressus-sim/src/entity.rs`
- Modify: `crates/progressus-sim/src/lib.rs`
- Modify: `crates/progressus-sim/src/simulation.rs`
- Modify: `crates/progressus-sim/tests/simulation.rs`

**Interfaces:**
- Produces: `Direction::{East, West, North, South}`, `Direction::adjacent(WorldCell) -> Option<WorldCell>`, `MovementState::{Idle, Moving { direction: Direction }}`, `Character::movement() -> MovementState`, `Simulation::set_movement_direction(EntityId, Direction) -> Result<(), SimulationError>`, and `Simulation::stop_movement(EntityId) -> Result<(), SimulationError>`.
- Consumes: `WorldCell`, `WorldGenerator`, `Terrain`, BTreeMap stable ordering, and the existing checked simulation clock.

- [ ] **Step 1: Write the failing simulation tests**

Add a `#[cfg(test)] mod tests` at the end of `simulation.rs`. Its private `place_on_grass` helper may access the private character map, but it must generate/check grass before assigning the test position. This keeps controlled coordinate setup out of every public API and out of the application/headless boundary. Add these tests:

```rust
#[test]
fn movement_crosses_positive_and_negative_chunk_boundaries() {
    let mut simulation = Simulation::new(WorldSeed::new(42)).unwrap();
    let cora = EntityId::new(3).unwrap();

    place_on_grass(&mut simulation, cora, WorldCell::new(31, 0));
    simulation.set_movement_direction(cora, Direction::East).unwrap();
    simulation.advance_ticks(1).unwrap();
    assert_eq!(character(&simulation, cora).position(), WorldCell::new(32, 0));

    place_on_grass(&mut simulation, cora, WorldCell::new(0, 0));
    simulation.set_movement_direction(cora, Direction::West).unwrap();
    simulation.advance_ticks(1).unwrap();
    assert_eq!(character(&simulation, cora).position(), WorldCell::new(-1, 0));
}

#[test]
fn rejected_replacement_preserves_existing_direction() {
    let mut simulation = Simulation::new(WorldSeed::new(42)).unwrap();
    let cora = EntityId::new(3).unwrap();
    let blocked = find_adjacent_non_grass(&simulation, WorldCell::new(0, 0)).unwrap();
    let direction = direction_to(WorldCell::new(0, 0), blocked).unwrap();

    simulation.set_movement_direction(cora, Direction::East).unwrap();
    assert!(matches!(simulation.set_movement_direction(cora, direction), Err(SimulationError::MovementDestinationBlocked(_))));
    assert_eq!(character(&simulation, cora).movement(), MovementState::Moving { direction: Direction::East });
}

#[test]
fn replacement_starts_from_current_tick_position_and_discards_old_direction() {
    let mut simulation = Simulation::new(WorldSeed::new(42)).unwrap();
    let cora = EntityId::new(3).unwrap();
    simulation.set_movement_direction(cora, Direction::East).unwrap();
    simulation.advance_ticks(1).unwrap();
    simulation.set_movement_direction(cora, Direction::North).unwrap();
    simulation.advance_ticks(1).unwrap();

    assert_eq!(character(&simulation, cora).position(), WorldCell::new(1, 1));
    assert_eq!(character(&simulation, cora).movement(), MovementState::Moving { direction: Direction::North });
}
```

Add two separate tests that find a grass cell adjacent to `Terrain::Water` and `Terrain::Rock`, start the character toward that obstacle, then assert the following tick retains its position and makes it idle. Add an overflow test that privately places Cora on `WorldCell::new(i64::MAX, 0)`, sets its movement directly to east inside the source-module test, advances one tick, and proves it becomes idle without wrapping. Add a paired identical-command test that compares cloned public characters after the same command/tick sequence. Add helpers that derive all obstacle coordinates from `generate_chunk`, not hard-coded terrain samples.

- [ ] **Step 2: Run the simulation suite and observe the red state**

Run: `cargo test -p progressus-sim --test simulation`

Expected: compilation fails because movement types and command methods do not exist.

- [ ] **Step 3: Add movement value types and character state**

In `entity.rs`, add copyable, comparable public types and make direction arithmetic checked:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction { East, West, North, South }

impl Direction {
    pub fn adjacent(self, cell: WorldCell) -> Option<WorldCell> {
        match self {
            Self::East => cell.x().checked_add(1).map(|x| WorldCell::new(x, cell.y())),
            Self::West => cell.x().checked_sub(1).map(|x| WorldCell::new(x, cell.y())),
            Self::North => cell.y().checked_add(1).map(|y| WorldCell::new(cell.x(), y)),
            Self::South => cell.y().checked_sub(1).map(|y| WorldCell::new(cell.x(), y)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementState { Idle, Moving { direction: Direction } }
```

Initialize every `Character` as idle, add an immutable `movement` accessor, and restrict position/movement setters to `pub(crate)`. Re-export both movement types from `lib.rs`.

- [ ] **Step 4: Implement validation and one-step-per-tick movement**

Add these explicit errors to `SimulationError`: `UnknownCharacter(EntityId)`, `MovementCoordinateOverflow(WorldCell)`, and `MovementDestinationBlocked(WorldCell)`. `set_movement_direction` must first load the character immutably, call `direction.adjacent`, generate the target’s chunk with `target.split`, and accept only grass. Only after all checks pass may it fetch the mutable character and set `MovementState::Moving { direction }`. `stop_movement` must return `UnknownCharacter` for an absent ID and otherwise set idle.

Preserve the existing all-or-nothing tick-overflow behavior by checking the end tick before beginning movement, then use a one-tick loop so every successful clock advance applies movement exactly once:

```rust
self.clock.tick().value().checked_add(count).ok_or(SimulationError::TickOverflow)?;
for _ in 0..count {
    self.clock.advance(1)?;
    self.advance_characters_one_tick()?;
}
```

Before moving each ordered `EntityId`, copy its current position/direction from the map. On `None` from `adjacent`, set its state to idle. On non-grass terrain, set idle. On grass, set the position to the target and retain the moving state. Never return a fatal error merely for blocked/overflow continuation; propagate only genuine worldgen failures. Keep the loop stable by collecting/copying IDs from the `BTreeMap` before mutation. Keep all controlled-position setup private to the source-module tests; production code exposes no spawn, teleport, or direct mutable character API.

- [ ] **Step 5: Verify the focused simulation contract**

Run: `cargo fmt --all -- --check && cargo clippy -p progressus-sim --all-targets -- -D warnings && cargo test -p progressus-sim`

Expected: the added crossing, safe-stop, replacement, stable-ID, and determinism tests pass; existing clock and bootstrap tests remain green.

- [ ] **Step 6: Commit the authoritative movement change**

```bash
git add crates/progressus-sim/src/entity.rs crates/progressus-sim/src/lib.rs crates/progressus-sim/src/simulation.rs crates/progressus-sim/tests/simulation.rs
git commit -m "feat: add deterministic character movement"
```

---

### Task 2: Application commands and detached movement snapshots

**Files:**
- Modify: `crates/progressus-app/src/lib.rs`
- Modify: `crates/progressus-app/src/read_model.rs`
- Modify: `crates/progressus-app/tests/client_boundary.rs`

**Interfaces:**
- Consumes: `progressus_sim::{Direction, MovementState, Simulation}` and `EntityId`.
- Produces: `Command::SetMovementDirection { character_id: EntityId, direction: Direction }`, `Command::StopMovement { character_id: EntityId }`, and `CharacterSnapshot { id, name, position, movement }`.

- [ ] **Step 1: Write failing application-boundary tests**

Extend all existing expected `CharacterSnapshot` literals with `movement: MovementState::Idle`. Add a command-only test:

```rust
#[test]
fn movement_commands_are_applied_and_published_through_snapshots() {
    let mut application = Application::new_game(NewGameOptions { seed: WorldSeed::new(42) }).unwrap();
    let cora = EntityId::new(3).unwrap();
    application.execute(Command::SetMovementDirection { character_id: cora, direction: Direction::East }).unwrap();
    application.execute(Command::AdvanceTicks { count: 1 }).unwrap();
    let snapshot = application.snapshot(SnapshotQuery::default()).unwrap();
    let cora = snapshot.characters.iter().find(|character| character.id == cora).unwrap();
    assert_eq!(cora.position, WorldCell::new(1, 0));
    assert_eq!(cora.movement, MovementState::Moving { direction: Direction::East });

    application.execute(Command::StopMovement { character_id: cora.id }).unwrap();
    assert_eq!(application.snapshot(SnapshotQuery::default()).unwrap().characters[2].movement, MovementState::Idle);
}
```

Add a test that issues a rejected `SetMovementDirection` after a valid direction, asserts `ApplicationError::Simulation(SimulationError::MovementDestinationBlocked(_))`, advances one tick, and proves the previously accepted direction still moved the character. It must never inspect `Simulation` directly.

- [ ] **Step 2: Run the application tests and observe the red state**

Run: `cargo test -p progressus-app --test client_boundary`

Expected: compilation fails because the command variants and snapshot field are missing.

- [ ] **Step 3: Forward commands and copy movement state**

Re-export `Direction` and `MovementState` from `progressus-app`. Add the two `Command` variants and dispatch them in `Application::execute` directly to `Simulation::set_movement_direction` and `Simulation::stop_movement`. Extend `CharacterSnapshot` and its `From<&Character>` conversion:

```rust
pub struct CharacterSnapshot {
    pub id: EntityId,
    pub name: String,
    pub position: WorldCell,
    pub movement: MovementState,
}
```

Do not add an application-side queue, cache, mutation hook, or a simulation accessor.

- [ ] **Step 4: Verify the application seam**

Run: `cargo fmt --all -- --check && cargo clippy -p progressus-app --all-targets -- -D warnings && cargo test -p progressus-app`

Expected: all snapshot tests pass and snapshots remain detached after mutation of returned `String`/`Vec` values.

- [ ] **Step 5: Commit the application boundary change**

```bash
git add crates/progressus-app/src/lib.rs crates/progressus-app/src/read_model.rs crates/progressus-app/tests/client_boundary.rs
git commit -m "feat: expose movement through application boundary"
```

---

### Task 3: Bounded external headless walker

**Files:**
- Modify: `crates/progressus-headless/src/main.rs`
- Modify: `crates/progressus-headless/tests/cli.rs`

**Interfaces:**
- Consumes: only public `progressus_app::{Application, Command, Direction, SnapshotQuery, Terrain, WorldCell}` types.
- Produces: optional `--travel-chunks N` CLI behavior and a summary containing `travel character_id=... start_chunk_x=... max_chunk_x=... crossed_boundaries=... steps=...`.

- [ ] **Step 1: Write failing CLI tests**

Keep the existing `--seed --ticks` smoke test. Add:

```rust
#[test]
fn travel_chunks_crosses_many_boundaries_deterministically() {
    let first = headless_command().args(["--seed", "0", "--travel-chunks", "64"]).output().unwrap();
    let second = headless_command().args(["--seed", "0", "--travel-chunks", "64"]).output().unwrap();
    assert!(first.status.success());
    assert_eq!(first.stdout, second.stdout);
    let stdout = String::from_utf8(first.stdout).unwrap();
    assert!(stdout.contains("travel character_id=3"));
    assert!(stdout.contains("crossed_boundaries=64"));
}

#[test]
fn travel_chunks_requires_a_positive_count() {
    let output = headless_command().args(["--seed", "42", "--travel-chunks", "0"]).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr).unwrap().contains("travel chunk count must be positive"));
}
```

Use seed 0 as the fixed long-traversal fixture. It crosses 64 boundaries in 5,213 steps under the 32,768-step bound. Do not add seed search to the binary or simulation.

- [ ] **Step 2: Run the CLI tests and observe the red state**

Run: `cargo test -p progressus-headless --test cli`

Expected: the travel scenario is rejected as an unknown argument.

- [ ] **Step 3: Implement parsing and terrain lookup through snapshots**

Extend `Config` with `travel_chunks: Option<u64>`; preserve `--ticks` as required only when travel mode is absent. In travel mode, reject zero and checked-multiplication overflow with explicit `CliError`s. Derive the target character once from the initial detached snapshot by stable ID 3.

From the first authoritative snapshot, initialize walker-local `BTreeMap<WorldCell, u64>` with the starting position at count 1. For each step, obtain the four candidate neighbor cells with `Direction::adjacent`; query precisely the unique `ChunkCoord`s returned by `WorldCell::split`; locate terrain by matching the queried chunk coordinate and calling `ChunkSnapshot::terrain_at`. Among grass candidates choose the smallest count returned by `visit_counts.get(&cell).copied().unwrap_or(0)`; resolve equal counts by the fixed `[East, North, South, West]` iteration order. Then send `SetMovementDirection`, advance exactly one tick, get a fresh snapshot, require the position to equal the selected cell, and increment only the count for that actual authoritative position. Do not count merely selected candidates.

Compute start and current chunk x with `WorldCell::split`. Increment a local `crossed_boundaries` only when the current chunk x becomes greater than the maximum previously seen value; stop only at `crossed_boundaries >= requested`. The max steps is `max(1_024, requested.checked_mul(512).ok_or_else(|| CliError("travel chunk count is too large".to_owned()))?)`. On no candidate, rejected command, position mismatch, or exhaustion, return an error naming seed, ID, current position, maximum chunk x, crossed boundaries, and steps. Do not retain a route, destination frontier, open/closed set, destination search, BFS, Dijkstra, A*, path-cost heuristic, or direct character reference.

- [ ] **Step 4: Verify the external traversal proof**

Run: `cargo fmt --all -- --check && cargo clippy -p progressus-headless --all-targets -- -D warnings && cargo test -p progressus-headless && cargo run -p progressus-headless -- --seed 0 --travel-chunks 64`

Expected: the CLI test passes with byte-identical output on repeated executions and the manual scenario reports `crossed_boundaries=64`, `steps=5213`, and character ID 3.

- [ ] **Step 5: Commit the headless scenario**

```bash
git add crates/progressus-headless/src/main.rs crates/progressus-headless/tests/cli.rs
git commit -m "feat: add headless chunk traversal scenario"
```

---

### Task 4: Document partial milestone progress and close the verification gate

**Files:**
- Modify: `README.md`
- Modify: `docs/architecture/overview.md`
- Modify: `docs/milestones/prototype-01.md`

**Interfaces:**
- Consumes: the tested public movement/app/headless behavior from Tasks 1–3.
- Produces: documentation that accurately labels P01-SIM-04 as direction-driven bootstrap movement and explicitly leaves complete navigation/pathfinding and the excluded systems incomplete.

- [ ] **Step 1: Write documentation assertions as review checklist**

Before editing, list the claims to make: one cell per tick; grass-only passability; checked stops for obstacles/overflow; app commands/snapshots; headless external bounded walker; positive and negative chunk-boundary proof; no Bevy. List claims that must not appear: full navigation, A*, chunk residency, persistence, speed/animation, or a completed P01-SIM-04.

- [ ] **Step 2: Update only achieved requirements**

In `docs/milestones/prototype-01.md`, label P01-SIM-04 and TEST-P01-03 as **partially advanced**, not complete. State that the implementation currently accepts persistent cardinal directions, rechecks generated terrain per tick, and proves cross-chunk traversal headlessly; retain navigation around obstacles, jobs/AI interruption, speed/collision, residency, and persistence as future work. In `README.md` and `docs/architecture/overview.md`, document the app command/read-model seam and `--travel-chunks N` invocation with the same limitation language.

- [ ] **Step 3: Run the complete required verification gate**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p progressus-headless -- --seed 42 --ticks 100000
cargo run -p progressus-headless -- --seed 42 --travel-chunks 64
./scripts/verify-core-dependency-boundary.sh
```

Expected: every command exits zero; workspace tests include new movement and walker coverage; the dependency script reports no Bevy dependency below the client boundary.

- [ ] **Step 4: Commit documentation and verification-aligned changes**

```bash
git add README.md docs/architecture/overview.md docs/milestones/prototype-01.md
git commit -m "docs: record bootstrap movement status"
```
