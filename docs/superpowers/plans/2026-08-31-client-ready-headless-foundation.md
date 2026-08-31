# Client-ready Headless Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (\`- [ ]\`) syntax for tracking.

**Goal:** Build and verify a deterministic Rust application boundary that a future Bevy client can consume without accessing authoritative simulation internals.

**Architecture:** A four-crate Cargo workspace enforces the dependency chain \`progressus-headless -> progressus-app -> progressus-sim -> progressus-worldgen\`. World generation and simulation remain pure Rust; \`progressus-app\` converts commands and bounded snapshot queries into detached, deterministically ordered read models.

**Tech Stack:** Rust 1.97.1 verification toolchain, Rust 1.85 minimum version for edition 2024, Cargo workspace, Rust standard library only, integration tests, Bash for the final dependency check.

## Global Constraints

- \`progressus-sim\` and every crate below the application boundary must not depend on Bevy.
- Authoritative time is a checked \`u64\` tick and never consumes wall-clock time.
- World and chunk axes use \`i64\`; negative coordinate conversion uses Euclidean division and remainder.
- World-generation version 1 uses 32 by 32 chunks and project-owned stable integer mixing.
- Entity IDs are Progressus-owned non-zero \`u64\` values and never renderer handles.
- No save/load, movement, jobs, items, persistence, Simulation LOD, or authoritative parallelism is added.
- Every behavioral implementation follows a focused red-green test cycle.

## File map

- Create \`.gitignore\`: ignore Cargo build output only.
- Create and extend \`Cargo.toml\`: declare the workspace, add crates as they become buildable, and set the resolver, edition, minimum Rust version, and common lint policy.
- Create \`docs/adr/0003-bootstrap-world-coordinates.md\`: accept the narrow bootstrap coordinate and chunk contract.
- Create \`crates/progressus-worldgen/src/coordinate.rs\`: coordinate types and checked conversions.
- Create \`crates/progressus-worldgen/src/generator.rs\`: versioned deterministic terrain generation.
- Create \`crates/progressus-worldgen/src/lib.rs\`: the worldgen public surface.
- Create \`crates/progressus-worldgen/tests/coordinates.rs\`: negative-boundary regression tests.
- Create \`crates/progressus-worldgen/tests/generation.rs\`: repeatability, order, and version tests.
- Create \`crates/progressus-sim/src/clock.rs\`: checked deterministic simulation time.
- Create \`crates/progressus-sim/src/entity.rs\`: stable IDs and character records.
- Create \`crates/progressus-sim/src/simulation.rs\`: new-game state and authoritative stepping.
- Create \`crates/progressus-sim/src/lib.rs\`: the simulation public surface and worldgen re-exports.
- Create \`crates/progressus-sim/tests/simulation.rs\`: five-character and deterministic stepping tests.
- Create \`crates/progressus-app/src/read_model.rs\`: detached client snapshots.
- Create \`crates/progressus-app/src/lib.rs\`: commands, queries, and application façade.
- Create \`crates/progressus-app/tests/client_boundary.rs\`: external-consumer contract tests.
- Create \`crates/progressus-headless/src/main.rs\`: minimal CLI and deterministic summary.
- Create \`crates/progressus-headless/tests/cli.rs\`: successful and invalid CLI tests.
- Modify \`README.md\`: document implemented status and developer commands.
- Modify \`docs/architecture/overview.md\`: record the concrete bootstrap crate layout without changing accepted boundaries.

---

### Task 1: Workspace and bootstrap coordinate contract

**Files:**
- Create: \`.gitignore\`
- Create: \`Cargo.toml\`
- Create: \`docs/adr/0003-bootstrap-world-coordinates.md\`
- Create: \`crates/progressus-worldgen/Cargo.toml\`
- Create: \`crates/progressus-worldgen/src/lib.rs\`
- Create: \`crates/progressus-worldgen/src/coordinate.rs\`
- Create: \`crates/progressus-worldgen/tests/coordinates.rs\`

**Interfaces:**
- Produces: \`CHUNK_SIDE: u16\`, \`WorldCell::new(i64, i64)\`, \`WorldCell::split()\`, \`ChunkCoord::new(i64, i64)\`, \`ChunkCoord::world_cell(LocalCell) -> Option<WorldCell>\`.

- [x] **Step 1: Add the workspace shell and failing coordinate tests**

Create the root workspace with only \`crates/progressus-worldgen\` as the initial member, \`resolver = "3"\`, edition 2024, rust-version 1.85, and workspace lints forbidding unsafe code. Add each later crate to the member list in the task that creates its manifest. Add \`/target/\` to \`.gitignore\`.

Create \`crates/progressus-worldgen/src/lib.rs\`:

\`\`\`rust
mod coordinate;

pub use coordinate::{CHUNK_SIDE, ChunkCoord, LocalCell, WorldCell};
\`\`\`

Create \`crates/progressus-worldgen/tests/coordinates.rs\`:

\`\`\`rust
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
\`\`\`

- [x] **Step 2: Run the coordinate test and observe the red state**

Run: \`cargo test -p progressus-worldgen --test coordinates\`

Expected: compilation fails because \`coordinate.rs\` and the exported types do not exist.

- [x] **Step 3: Implement checked coordinate conversion**

Create immutable, copyable, ordered structs with private fields and public \`x()\` and \`y()\` accessors. Set \`CHUNK_SIDE\` to 32. Implement \`WorldCell::split\` with \`div_euclid\` and \`rem_euclid\`. Implement \`ChunkCoord::world_cell\` with \`checked_mul\` and \`checked_add\`; reject local values greater than or equal to \`CHUNK_SIDE\`.

The core conversion is:

\`\`\`rust
pub fn split(self) -> (ChunkCoord, LocalCell) {
    let side = i64::from(CHUNK_SIDE);
    (
        ChunkCoord::new(self.x.div_euclid(side), self.y.div_euclid(side)),
        LocalCell::new(
            self.x.rem_euclid(side) as u16,
            self.y.rem_euclid(side) as u16,
        ),
    )
}

pub fn world_cell(self, local: LocalCell) -> Option<WorldCell> {
    if local.x() >= CHUNK_SIDE || local.y() >= CHUNK_SIDE {
        return None;
    }
    let side = i64::from(CHUNK_SIDE);
    Some(WorldCell::new(
        self.x.checked_mul(side)?.checked_add(i64::from(local.x()))?,
        self.y.checked_mul(side)?.checked_add(i64::from(local.y()))?,
    ))
}
\`\`\`

Write ADR-0003 with status Accepted, the six coordinate choices from the design, explicit provisional status of 32 cells, and the rule that persisted geometry changes require a worldgen version bump or migration.

- [x] **Step 4: Verify and commit the coordinate contract**

Run: \`cargo fmt --all -- --check && cargo test -p progressus-worldgen --test coordinates\`

Expected: 2 tests pass.

Commit:

\`\`\`bash
git add .gitignore Cargo.toml Cargo.lock docs/adr/0003-bootstrap-world-coordinates.md crates/progressus-worldgen
git commit -m "feat: establish world coordinate contract"
\`\`\`

---

### Task 2: Versioned deterministic terrain generation

**Files:**
- Modify: \`crates/progressus-worldgen/src/lib.rs\`
- Create: \`crates/progressus-worldgen/src/generator.rs\`
- Create: \`crates/progressus-worldgen/tests/generation.rs\`

**Interfaces:**
- Consumes: \`ChunkCoord\`, \`LocalCell\`, \`WorldCell\`, \`CHUNK_SIDE\`.
- Produces: \`WorldSeed(u64)\`, \`WorldgenVersion(u32)\`, \`CURRENT_WORLDGEN_VERSION\`, \`Terrain::{Grass, Water, Rock}\`, \`GeneratedChunk\`, \`WorldGenerator::new\`, \`WorldGenerator::generate\`.

- [x] **Step 1: Write repeatability, order-independence, spawn-clearing, and unsupported-version tests**

The tests construct two generators from the same seed/version, compare generated chunks directly, generate coordinates in forward and reverse order into \`BTreeMap\`, assert the maps are equal, assert world cells \`(-2, 0)..=(2, 0)\` are grass, and assert version 999 returns:

\`\`\`rust
Err(WorldgenError::UnsupportedVersion(WorldgenVersion::new(999)))
\`\`\`

- [x] **Step 2: Run the worldgen tests and observe the red state**

Run: \`cargo test -p progressus-worldgen --test generation\`

Expected: compilation fails with unresolved imports for generator types.

- [x] **Step 3: Implement world-generation version 1**

Add public derives \`Clone, Debug, Eq, PartialEq\` to chunks and terrain; add \`Copy, Hash, Ord, PartialOrd\` where values are scalar. \`GeneratedChunk\` exposes its coordinate and cell slice, plus:

\`\`\`rust
pub fn terrain_at(&self, local: LocalCell) -> Option<Terrain> {
    if local.x() >= CHUNK_SIDE || local.y() >= CHUNK_SIDE {
        return None;
    }
    let index = usize::from(local.y()) * usize::from(CHUNK_SIDE)
        + usize::from(local.x());
    self.cells.get(index).copied()
}
\`\`\`

\`WorldGenerator::new\` rejects every version except 1. \`generate\` iterates local cells row-major, converts them with \`ChunkCoord::world_cell\`, and returns \`CoordinateOutOfRange\` on checked-conversion failure.

Use a private, documented SplitMix64 finalizer:

\`\`\`rust
fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
\`\`\`

Mix the seed, version, world x, and world y in separate rounds. Force the five cells from x -2 through 2 at y 0 to grass. For other cells map \`sample % 100\` values 0..=14 to water, 15..=29 to rock, and 30..=99 to grass. Do not use \`DefaultHasher\` or external RNG state.

- [x] **Step 4: Verify and commit deterministic worldgen**

Run: \`cargo fmt --all -- --check && cargo clippy -p progressus-worldgen --all-targets -- -D warnings && cargo test -p progressus-worldgen\`

Expected: coordinate and generation suites pass with no warnings.

Commit:

\`\`\`bash
git add crates/progressus-worldgen
git commit -m "feat: add deterministic chunk generation"
\`\`\`

---

### Task 3: Deterministic simulation clock and five-character scenario

**Files:**
- Modify: \`Cargo.toml\`
- Create: \`crates/progressus-sim/Cargo.toml\`
- Create: \`crates/progressus-sim/src/lib.rs\`
- Create: \`crates/progressus-sim/src/clock.rs\`
- Create: \`crates/progressus-sim/src/entity.rs\`
- Create: \`crates/progressus-sim/src/simulation.rs\`
- Create: \`crates/progressus-sim/tests/simulation.rs\`

**Interfaces:**
- Consumes: versioned \`WorldGenerator\` and coordinate/terrain types.
- Produces: \`Simulation::new(WorldSeed)\`, \`Simulation::advance_ticks(u64)\`, \`Simulation::characters()\`, \`Simulation::generate_chunk(ChunkCoord)\`, \`SimulationTick\`, \`EntityId\`, and \`Character\`.

- [ ] **Step 1: Write failing simulation contract tests**

Add \`crates/progressus-sim\` to the root workspace and create its manifest with a path dependency on \`progressus-worldgen\`. Test that \`Simulation::new(WorldSeed::new(42))\` has tick 0; contains exactly the ordered IDs 1 through 5; contains names Ada, Borin, Cora, Dain, and Elin at world cells x -2 through 2 and y 0; and all positions are grass in the generated origin chunk. Create two simulations, advance both by 100,000 ticks, and assert their public character/tick state is equal.

Add a unit test in \`clock.rs\` that constructs a clock at \`u64::MAX - 1\`, advances once successfully, then receives \`SimulationError::TickOverflow\` when advancing once more.

- [ ] **Step 2: Run the simulation tests and observe the red state**

Run: \`cargo test -p progressus-sim\`

Expected: compilation fails because simulation modules and types are absent.

- [ ] **Step 3: Implement the minimal authoritative state**

\`SimulationTick\` and \`EntityId\` are private-field newtypes with \`new\`/value accessors, ordering, hashing, and equality. \`EntityId::new(0)\` returns \`None\`. The allocator starts at 1 and uses checked addition.

\`Character\` owns \`EntityId\`, \`String\`, and \`WorldCell\` and exposes immutable accessors.

\`Simulation\` owns a \`WorldGenerator\`, \`SimulationClock\`, and \`BTreeMap<EntityId, Character>\`. New-game construction inserts exactly:

\`\`\`rust
const INITIAL_CHARACTERS: [(&str, i64); 5] = [
    ("Ada", -2),
    ("Borin", -1),
    ("Cora", 0),
    ("Dain", 1),
    ("Elin", 2),
];
\`\`\`

Before insertion, generate the corresponding terrain and fail with \`SpawnNotWalkable(WorldCell)\` unless it is grass. Duplicate IDs, allocator exhaustion, tick overflow, and worldgen errors are explicit \`SimulationError\` variants with \`Display\` and \`Error\` implementations.

- [ ] **Step 4: Verify and commit the simulation**

Run: \`cargo fmt --all -- --check && cargo clippy -p progressus-sim --all-targets -- -D warnings && cargo test -p progressus-sim\`

Expected: all clock and integration tests pass without Bevy initialization.

Commit:

\`\`\`bash
git add Cargo.toml crates/progressus-sim
git commit -m "feat: add deterministic headless simulation"
\`\`\`

---

### Task 4: Application commands and detached client snapshots

**Files:**
- Modify: \`Cargo.toml\`
- Create: \`crates/progressus-app/Cargo.toml\`
- Create: \`crates/progressus-app/src/lib.rs\`
- Create: \`crates/progressus-app/src/read_model.rs\`
- Create: \`crates/progressus-app/tests/client_boundary.rs\`

**Interfaces:**
- Consumes: the public \`progressus-sim\` API only.
- Produces: \`Application::new_game(NewGameOptions)\`, \`Application::execute(Command)\`, \`Application::snapshot(SnapshotQuery)\`, \`ClientSnapshot\`, \`ChunkSnapshot\`, and \`CharacterSnapshot\`.

- [ ] **Step 1: Write the external-consumer contract tests**

Add \`crates/progressus-app\` to the root workspace and create its manifest with a path dependency on \`progressus-sim\`. From the integration test, import only \`progressus_app\`. Create an application with seed 42, execute \`Command::AdvanceTicks { count: 100_000 }\`, and query chunks in the deliberately unsorted sequence \`(1, 0), (-1, 0), (0, 0), (1, 0)\`.

Assert that:

- tick equals 100,000;
- worldgen version equals 1;
- chunks are deduplicated and ordered \`(-1, 0), (0, 0), (1, 0)\`;
- every chunk reports side 32 and 1,024 cells;
- characters contain the expected five stable IDs/names/positions;
- repeating the same options and commands creates an equal \`ClientSnapshot\`;
- mutating an owned clone of a snapshot does not change a later snapshot from the application.

- [ ] **Step 2: Run the application tests and observe the red state**

Run: \`cargo test -p progressus-app --test client_boundary\`

Expected: Cargo reports that the package or imported application API does not exist.

- [ ] **Step 3: Implement the façade and read models**

Use these public command/query shapes:

\`\`\`rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NewGameOptions {
    pub seed: WorldSeed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    AdvanceTicks { count: u64 },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SnapshotQuery {
    pub chunks: Vec<ChunkCoord>,
}
\`\`\`

\`Application::execute\` delegates checked advancement. \`snapshot\` sorts and deduplicates the requested coordinates before generation. It copies terrain vectors and character strings into read models that derive \`Clone, Debug, Eq, PartialEq\`. No read model contains references, Bevy types, entity storage indexes, or mutation callbacks.

Map simulation errors into an \`ApplicationError::Simulation\` variant with source chaining.

- [ ] **Step 4: Verify and commit the client boundary**

Run: \`cargo fmt --all -- --check && cargo clippy -p progressus-app --all-targets -- -D warnings && cargo test -p progressus-app\`

Expected: the integration test passes while depending only on \`progressus-app\`.

Commit:

\`\`\`bash
git add Cargo.toml crates/progressus-app
git commit -m "feat: expose client application boundary"
\`\`\`

---

### Task 5: Headless consumer and long-run smoke test

**Files:**
- Modify: \`Cargo.toml\`
- Create: \`crates/progressus-headless/Cargo.toml\`
- Create: \`crates/progressus-headless/src/main.rs\`
- Create: \`crates/progressus-headless/tests/cli.rs\`

**Interfaces:**
- Consumes: \`progressus-app\` only.
- Produces: \`progressus-headless --seed <u64> --ticks <u64>\`.

- [ ] **Step 1: Write failing CLI integration tests**

Add \`crates/progressus-headless\` to the root workspace and create its manifest with a path dependency on \`progressus-app\`. Use \`env!("CARGO_BIN_EXE_progressus-headless")\` and \`std::process::Command\`. For \`--seed 42 --ticks 100000\`, assert success and stdout lines containing:

\`\`\`text
seed=42 tick=100000 worldgen_version=1 chunks=3 characters=5
character id=1 name=Ada position=(-2, 0)
character id=5 name=Elin position=(2, 0)
\`\`\`

For missing \`--ticks\` and for \`--seed invalid --ticks 1\`, assert non-zero status and a concise stderr message containing usage or invalid seed respectively.

- [ ] **Step 2: Run the CLI tests and observe the red state**

Run: \`cargo test -p progressus-headless --test cli\`

Expected: Cargo reports that the binary package does not exist.

- [ ] **Step 3: Implement minimal argument parsing and summary output**

Parse exactly one \`--seed\` value and one \`--ticks\` value using the standard library; reject unknown, duplicate, missing, and unparsable arguments. The executable creates \`Application\`, advances ticks once, and queries chunks \`(-1, 0), (0, 0), (1, 0)\`.

Print the fixed summary first, followed by ordered character records and terrain counts for grass, water, and rock. Route all errors through \`run() -> Result<(), Box<dyn Error>>\`; \`main\` prints \`error: {message}\` and exits with code 2.

- [ ] **Step 4: Verify and commit the headless consumer**

Run: \`cargo fmt --all -- --check && cargo clippy -p progressus-headless --all-targets -- -D warnings && cargo test -p progressus-headless && cargo run -p progressus-headless -- --seed 42 --ticks 100000\`

Expected: CLI tests pass; the manual run reports tick 100000, three chunks, and five characters.

Commit:

\`\`\`bash
git add Cargo.toml crates/progressus-headless
git commit -m "feat: add headless application runner"
\`\`\`

---

### Task 6: Documentation, dependency audit, and completion gate

**Files:**
- Modify: \`README.md\`
- Modify: \`docs/architecture/overview.md\`

**Interfaces:**
- Consumes: all implemented crates and their verified commands.
- Produces: documented development entry points and evidence that visual-client work can begin.

- [ ] **Step 1: Update documentation to match reality**

Change README status from pre-production-only to Prototype 01 foundation in progress. List:

\`\`\`text
cargo test --workspace
cargo run -p progressus-headless -- --seed 42 --ticks 100000
\`\`\`

Document that the current executable foundation exposes deterministic terrain and five characters through \`progressus-app\`, while movement, jobs, items, persistence, and Bevy remain unimplemented.

Append a Prototype 01 bootstrap implementation section to the architecture overview with the exact four-crate dependency chain and the rule that future Bevy code depends on \`progressus-app\`, not \`progressus-sim\`.

- [ ] **Step 2: Run the complete fresh verification gate**

Run:

\`\`\`bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p progressus-headless -- --seed 42 --ticks 100000
cargo tree --workspace --edges normal,build,dev
\`\`\`

Expected:

- formatting exits 0;
- clippy exits 0 with no warnings;
- all workspace tests pass;
- headless output reports tick 100000, three chunks, and five characters;
- the full dependency tree contains only the four Progressus crates and no occurrence of \`bevy\`.

- [ ] **Step 3: Audit every client-readiness requirement**

Check the design requirements against evidence:

- application construction: external \`progressus-app\` integration test;
- authoritative stepping: tick and overflow tests;
- positive/negative chunks: coordinate and snapshot tests;
- renderable terrain: 32 by 32 typed cell vectors in snapshots;
- stable characters: exact ID/name/position tests;
- detached state: clone-mutation regression test;
- no graphics dependency: complete Cargo tree;
- headless runtime: successful 100,000-tick process.

Do not mark the goal complete if any evidence is missing or indirect.

- [ ] **Step 4: Commit the documentation and verified handoff**

\`\`\`bash
git add README.md docs/architecture/overview.md Cargo.lock
git commit -m "docs: record client-ready headless foundation"
\`\`\`

Run after the commit: \`git status --short --branch\`

Expected: clean worktree, with local \`main\` ahead of \`origin/main\` only by the intentional design, plan, and implementation commits.
