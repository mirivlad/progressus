# Authoritative Sparse Modified World State Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

**Goal:** Make sparse authoritative terrain overrides the current world truth while retaining immutable deterministic GeneratedChunk worldgen.

**Architecture:** Simulation owns a private ModifiedWorld keyed by chunk and local cell. It resolves raw base terrain through WorldGenerator and returns a distinct on-demand EffectiveChunk; point lookup, movement, and application snapshots share that resolution rule. The app gains only a private unit-test injection seam, not a terrain command.

**Tech Stack:** Rust 2024, Rust 1.89, standard-library BTreeMap, existing progressus-worldgen, progressus-sim, and progressus-app.

## Global Constraints

- GeneratedChunk always means untouched seed + worldgen version + ChunkCoord output; never mutate it or return modified terrain under this type.
- Add EffectiveChunk in progressus-sim, materialized by value only; no generated/effective resident cache, LRU, unload policy, serialization, or save/load.
- ModifiedWorld, ChunkDelta, and all maps stay private inside progressus-sim; use canonical BTreeMap<ChunkCoord, BTreeMap<LocalCell, Terrain>>.
- Simulation::set_terrain_override(WorldCell, Terrain) is immediate authoritative domain mutation. It is not a tick and does not appear in progressus_app::Command.
- Each write canonicalizes against raw base: differing value inserts, base-equal value removes, empty local delta removes its chunk record.
- The public primitive does not authorize app/client/headless presentation code to bypass the application command boundary. This increment adds no terrain-mutation route outside authoritative simulation systems.
- Point lookup and effective-chunk materialization use the same private override-or-base resolver.
- Movement and production Application::snapshot use effective terrain; generated_chunk is diagnostic/test-only base inspection.
- The app seam is private cfg(test), has no Cargo feature, is unavailable to client/headless, and injects only a prepared Simulation.
- Keep dependency directions unchanged: no Bevy below progressus-client; no new dependencies, terrain UI, commands, items, jobs, construction, resources, or pathfinding.
- Documentation marks P01-WORLD-05 partially advanced in-memory bootstrap only; P01-WORLD-04 and P01-SIM-04 remain incomplete.

---

## File Structure

| Path | Responsibility |
| --- | --- |
| crates/progressus-sim/src/world_state.rs | Private ModifiedWorld/ChunkDelta canonicalization and public EffectiveChunk. |
| crates/progressus-sim/src/lib.rs | Declares the private module and re-exports EffectiveChunk. |
| crates/progressus-sim/src/simulation.rs | Owns ModifiedWorld; exposes raw/effective terrain APIs; routes passability through effective lookup. |
| crates/progressus-sim/tests/world_state.rs | Public effective API, determinism, no-visitation mutation, and chunk consistency tests. |
| crates/progressus-app/src/read_model.rs | Converts only EffectiveChunk into detached ChunkSnapshot. |
| crates/progressus-app/src/lib.rs | Uses effective_chunk in production snapshots and has the private app-test injection seam. |
| README.md, docs/architecture/overview.md, docs/milestones/prototype-01.md | Record the actual bootstrap and non-goals. |

### Task 1: Add canonical world state and raw/effective terrain APIs

**Files:**
- Create: crates/progressus-sim/src/world_state.rs
- Modify: crates/progressus-sim/src/lib.rs
- Modify: crates/progressus-sim/src/simulation.rs:22-170
- Modify: crates/progressus-sim/tests/simulation.rs:1-35
- Create: crates/progressus-sim/tests/world_state.rs
- Modify: crates/progressus-app/src/lib.rs:66-78

**Interfaces:**
- Consumes: ChunkCoord: Ord, LocalCell: Ord, Terrain, GeneratedChunk, and WorldGenerator.
- Produces: EffectiveChunk::{coordinate, cells, terrain_at}; Simulation::{generated_chunk, set_terrain_override, effective_terrain_at, effective_chunk}; private ModifiedWorld canonicalization.
- Task 2 consumes the public effective APIs. This task does not yet route app snapshots or movement; it updates the app's existing raw call from generate_chunk to generated_chunk solely to preserve compilation after the explicit rename.

- [ ] **Step 1: Write failing sparse-state tests**

Create world_state.rs with these private tests before definitions:

```rust
#[test]
fn base_equal_write_removes_override_and_empty_chunk_delta() {
    let coordinate = ChunkCoord::new(11, -7);
    let local = LocalCell::new(3, 5);
    let mut world = ModifiedWorld::default();

    world.set_override(coordinate, local, Terrain::Grass, Terrain::Rock);
    assert_eq!(world.chunks.len(), 1);
    assert_eq!(world.chunks[&coordinate].overrides[&local], Terrain::Rock);

    world.set_override(coordinate, local, Terrain::Grass, Terrain::Grass);
    assert!(world.chunks.is_empty());
}

#[test]
fn canonical_reversion_matches_an_untouched_world() {
    let coordinate = ChunkCoord::new(-400, 900);
    let local = LocalCell::new(31, 0);
    let mut changed = ModifiedWorld::default();

    changed.set_override(coordinate, local, Terrain::Grass, Terrain::Rock);
    changed.set_override(coordinate, local, Terrain::Grass, Terrain::Water);
    changed.set_override(coordinate, local, Terrain::Grass, Terrain::Grass);

    assert_eq!(changed, ModifiedWorld::default());
}

#[test]
fn restoring_one_distant_chunk_leaves_the_other_delta() {
    let first = (ChunkCoord::new(-1_000, 2_000), LocalCell::new(1, 2));
    let second = (ChunkCoord::new(7_000, -8_000), LocalCell::new(30, 31));
    let mut world = ModifiedWorld::default();

    world.set_override(first.0, first.1, Terrain::Grass, Terrain::Rock);
    world.set_override(second.0, second.1, Terrain::Water, Terrain::Grass);
    world.set_override(first.0, first.1, Terrain::Grass, Terrain::Grass);

    assert!(!world.chunks.contains_key(&first.0));
    assert_eq!(world.override_at(second.0, second.1), Some(Terrain::Grass));
}
```

Create tests/world_state.rs with the external read contract:

```rust
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
    simulation.set_terrain_override(modified, Terrain::Rock).unwrap();

    for cell in [modified, untouched] {
        let (coordinate, local) = cell.split();
        assert_eq!(
            simulation.effective_terrain_at(cell).unwrap(),
            simulation.effective_chunk(coordinate).unwrap().terrain_at(local).unwrap(),
        );
    }
}

#[test]
fn one_override_changes_only_its_local_cell() {
    let mut simulation = Simulation::new(WorldSeed::new(73)).unwrap();
    let coordinate = ChunkCoord::new(0, 0);
    let target = LocalCell::new(0, 0);
    let before = simulation.effective_chunk(coordinate).unwrap().cells().to_vec();

    simulation
        .set_terrain_override(WorldCell::new(0, 0), Terrain::Rock)
        .unwrap();
    let after = simulation.effective_chunk(coordinate).unwrap().cells().to_vec();

    assert_eq!(
        before.iter().zip(&after).filter(|(before, after)| before != after).count(),
        1,
    );
    assert_eq!(simulation.effective_chunk(coordinate).unwrap().terrain_at(target), Some(Terrain::Rock));
}
```

- [ ] **Step 2: Run focused RED tests**

Run:

```bash
cargo test -p progressus-sim world_state
```

Expected: compilation fails because world_state, EffectiveChunk, generated_chunk, effective_terrain_at, and effective_chunk do not exist.

- [ ] **Step 3: Implement private ModifiedWorld and EffectiveChunk**

Create world_state.rs:

```rust
use std::collections::BTreeMap;

use crate::{CHUNK_SIDE, ChunkCoord, LocalCell, Terrain};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ModifiedWorld {
    chunks: BTreeMap<ChunkCoord, ChunkDelta>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ChunkDelta {
    overrides: BTreeMap<LocalCell, Terrain>,
}

impl ModifiedWorld {
    pub(crate) fn override_at(&self, coordinate: ChunkCoord, local: LocalCell) -> Option<Terrain> {
        self.chunks
            .get(&coordinate)
            .and_then(|delta| delta.overrides.get(&local))
            .copied()
    }

    pub(crate) fn set_override(
        &mut self,
        coordinate: ChunkCoord,
        local: LocalCell,
        base: Terrain,
        requested: Terrain,
    ) {
        if requested != base {
            self.chunks.entry(coordinate).or_default().overrides.insert(local, requested);
            return;
        }
        let remove_chunk = if let Some(delta) = self.chunks.get_mut(&coordinate) {
            delta.overrides.remove(&local);
            delta.overrides.is_empty()
        } else {
            false
        };
        if remove_chunk {
            self.chunks.remove(&coordinate);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectiveChunk {
    coordinate: ChunkCoord,
    cells: Vec<Terrain>,
}
```

Implement EffectiveChunk::new as crate-visible, coordinate as const public, cells as public slice, and terrain_at with the same local bounds and row-major index as GeneratedChunk. Declare mod world_state in lib.rs and re-export only EffectiveChunk.

- [ ] **Step 4: Implement one raw-to-effective resolution path**

Add modified_world: ModifiedWorld to Simulation and initialize it with ModifiedWorld::default(). Rename the existing raw method:

```rust
pub fn generated_chunk(
    &self,
    coordinate: ChunkCoord,
) -> Result<GeneratedChunk, SimulationError> {
    self.generator.generate(coordinate).map_err(Into::into)
}
```

Add the approved APIs and private helpers:

```rust
fn base_terrain_at(&self, position: WorldCell) -> Result<Terrain, SimulationError> {
    let (coordinate, local) = position.split();
    self.generated_chunk(coordinate)?
        .terrain_at(local)
        .ok_or(SimulationError::Worldgen(
            WorldgenError::CoordinateOutOfRange(coordinate),
        ))
}

fn resolve_terrain(&self, coordinate: ChunkCoord, local: LocalCell, base: Terrain) -> Terrain {
    self.modified_world.override_at(coordinate, local).unwrap_or(base)
}

pub fn set_terrain_override(
    &mut self,
    position: WorldCell,
    terrain: Terrain,
) -> Result<(), SimulationError> {
    let (coordinate, local) = position.split();
    let base = self.base_terrain_at(position)?;
    self.modified_world.set_override(coordinate, local, base, terrain);
    Ok(())
}

pub fn effective_terrain_at(&self, position: WorldCell) -> Result<Terrain, SimulationError> {
    let (coordinate, local) = position.split();
    Ok(self.resolve_terrain(coordinate, local, self.base_terrain_at(position)?))
}
```

Implement effective_chunk by obtaining one GeneratedChunk, iterating y then x from 0..CHUNK_SIDE, passing each raw cell through resolve_terrain, and returning EffectiveChunk::new(coordinate, cells). Do not mutate/relabel GeneratedChunk or cache cells. Update current raw test helpers from generate_chunk to generated_chunk. In progressus-app/src/lib.rs change its existing raw snapshot call from simulation.generate_chunk(coordinate) to simulation.generated_chunk(coordinate); Task 2 alone changes that source to effective_chunk.

- [ ] **Step 5: Add determinism, distant-chunk, and visitation tests**

Add helpers:

```rust
fn different_from(base: Terrain) -> Terrain {
    match base {
        Terrain::Grass => Terrain::Rock,
        Terrain::Water => Terrain::Grass,
        Terrain::Rock => Terrain::Grass,
    }
}

fn effective_cells(simulation: &Simulation, coordinate: ChunkCoord) -> Vec<Terrain> {
    simulation.effective_chunk(coordinate).unwrap().cells().to_vec()
}
```

Add this deterministic public-state test; requested values are always derived from the current base/effective value:

```rust
#[test]
fn identical_mutations_and_unrelated_visitation_preserve_effective_state() {
    let mut first = Simulation::new(WorldSeed::new(73)).unwrap();
    let mut second = Simulation::new(WorldSeed::new(73)).unwrap();
    let edits = [WorldCell::new(2_048, 17), WorldCell::new(-3_200, -19), WorldCell::new(0, 0)];

    for simulation in [&mut first, &mut second] {
        for cell in edits {
            let requested = different_from(simulation.effective_terrain_at(cell).unwrap());
            simulation.set_terrain_override(cell, requested).unwrap();
        }
    }

    let compared = [ChunkCoord::new(64, 0), ChunkCoord::new(-100, -1), ChunkCoord::new(0, 0)];
    for coordinate in compared {
        assert_eq!(effective_cells(&first, coordinate), effective_cells(&second, coordinate));
    }

    let before = effective_cells(&first, ChunkCoord::new(64, 0));
    for coordinate in [ChunkCoord::new(999, -999), ChunkCoord::new(-444, 333)] {
        first.generated_chunk(coordinate).unwrap();
        first.effective_chunk(coordinate).unwrap();
    }
    assert_eq!(effective_cells(&first, ChunkCoord::new(64, 0)), before);
}
```

The private distant-delta test from Step 1 provides the direct canonical-map assertion; this public test proves unrelated generated/effective reads do not change the authoritative result.

- [ ] **Step 6: Run focused GREEN checks**

Run:

```bash
cargo test -p progressus-sim world_state
cargo test -p progressus-sim --test simulation
cargo test -p progressus-app
cargo fmt --all -- --check
cargo clippy -p progressus-sim --all-targets -- -D warnings
```

Expected: all commands exit 0 and prove raw/effective separation, canonical sparse removal, resolver consistency, deterministic state, distant independence, and no visitation mutation.

- [ ] **Step 7: Commit Task 1**

```bash
git add crates/progressus-sim/src/lib.rs crates/progressus-sim/src/simulation.rs \
  crates/progressus-sim/src/world_state.rs crates/progressus-sim/tests/simulation.rs \
  crates/progressus-sim/tests/world_state.rs crates/progressus-app/src/lib.rs
git commit -m "feat: add sparse effective terrain state"
```

### Task 2: Route passability and app snapshots through effective terrain

**Files:**
- Modify: crates/progressus-sim/src/simulation.rs:86-170,230-490
- Modify: crates/progressus-app/src/read_model.rs:1-47
- Modify: crates/progressus-app/src/lib.rs:1-92

**Interfaces:**
- Consumes: Task 1 Simulation::effective_terrain_at, Simulation::effective_chunk, Simulation::generated_chunk, and EffectiveChunk.
- Produces: effective passability during command validation and persisted movement; production app snapshots from effective chunks; private app-test seam.
- Keeps Command, SnapshotQuery, public snapshot types, and all dependency manifests unchanged.

- [ ] **Step 1: Write failing passability and app-boundary tests**

Add raw fixture helpers in simulation.rs tests; only these helpers use generated_chunk:

```rust
fn raw_terrain_at(simulation: &Simulation, position: WorldCell) -> Terrain {
    let (coordinate, local) = position.split();
    simulation
        .generated_chunk(coordinate)
        .unwrap()
        .terrain_at(local)
        .unwrap()
}

fn find_raw_grass_with_neighbor(
    simulation: &Simulation,
    neighbor: Terrain,
) -> (WorldCell, Direction) {
    for y in -64..=64 {
        for x in -64..=64 {
            let start = WorldCell::new(x, y);
            if raw_terrain_at(simulation, start) != Terrain::Grass {
                continue;
            }
            for direction in [Direction::East, Direction::West, Direction::North, Direction::South] {
                let target = direction.adjacent(start).unwrap();
                if raw_terrain_at(simulation, target) == neighbor {
                    return (start, direction);
                }
            }
        }
    }
    panic!("expected raw grass next to {neighbor:?}");
}
```

Add:

```rust
#[test]
fn grass_overridden_to_blocked_terrain_blocks_validation_and_persisted_step() {
    for blocked in [Terrain::Rock, Terrain::Water] {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        let (start, direction) = find_raw_grass_with_neighbor(&simulation, Terrain::Grass);
        let target = direction.adjacent(start).unwrap();
        let cora = cora();
        place_on_grass(&mut simulation, cora, start);

        simulation.set_terrain_override(target, blocked).unwrap();
        assert_eq!(
            simulation.set_movement_direction(cora, direction),
            Err(SimulationError::MovementDestinationBlocked(target)),
        );

        simulation.set_terrain_override(target, Terrain::Grass).unwrap();
        simulation.set_movement_direction(cora, direction).unwrap();
        simulation.set_terrain_override(target, blocked).unwrap();
        simulation.advance_ticks(1).unwrap();

        assert_eq!(character(&simulation, cora).position(), start);
        assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
    }
}
```

Add a companion test that loops over both raw blocked values:

```rust
for blocked in [Terrain::Water, Terrain::Rock] {
    let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
    let (start, direction) = find_raw_grass_with_neighbor(&simulation, blocked);
    let target = direction.adjacent(start).unwrap();
    let cora = cora();
    place_on_grass(&mut simulation, cora, start);

    simulation.set_terrain_override(target, Terrain::Grass).unwrap();
    simulation.set_movement_direction(cora, direction).unwrap();
    simulation.advance_ticks(1).unwrap();
    assert_eq!(character(&simulation, cora).position(), target);

    simulation.stop_movement(cora).unwrap();
    simulation.set_terrain_override(target, blocked).unwrap();
    simulation.characters.get_mut(&cora).unwrap().set_position(start);
    assert_eq!(
        simulation.set_movement_direction(cora, direction),
        Err(SimulationError::MovementDestinationBlocked(target)),
    );
}
```

At the end of app lib.rs add this private unit test:

```rust
#[test]
fn snapshot_returns_effective_terrain_without_mutating_raw_worldgen() {
    let mut simulation = Simulation::new(WorldSeed::new(42)).unwrap();
    let position = WorldCell::new(0, 0);
    let (coordinate, local) = position.split();

    assert_eq!(
        simulation.generated_chunk(coordinate).unwrap().terrain_at(local),
        Some(Terrain::Grass),
    );
    simulation.set_terrain_override(position, Terrain::Rock).unwrap();
    assert_eq!(
        simulation.generated_chunk(coordinate).unwrap().terrain_at(local),
        Some(Terrain::Grass),
    );

    let application = Application::from_simulation_for_test(simulation);
    let snapshot = application.snapshot(SnapshotQuery { chunks: vec![coordinate] }).unwrap();

    assert_eq!(snapshot.chunks[0].terrain_at(local), Some(Terrain::Rock));
}
```

- [ ] **Step 2: Run focused RED tests**

Run:

```bash
cargo test -p progressus-sim simulation::tests::grass_overridden_to_blocked_terrain_blocks_validation_and_persisted_step
cargo test -p progressus-app snapshot_returns_effective_terrain_without_mutating_raw_worldgen
```

Expected: the sim test fails because is_walkable still calls raw generation; the app test fails because its private constructor and EffectiveChunk read-model conversion do not exist and snapshot still calls raw terrain.

- [ ] **Step 3: Route movement through effective point lookup**

Replace is_walkable:

```rust
fn is_walkable(&self, position: WorldCell) -> Result<bool, SimulationError> {
    Ok(self.effective_terrain_at(position)? == Terrain::Grass)
}
```

Keep fixture discovery explicitly raw and make place_on_grass use effective terrain. This covers both immediate SetMovementDirection validation and the existing per-tick persistent movement call site.

- [ ] **Step 4: Route production snapshots through EffectiveChunk**

Replace the raw conversion in read_model.rs:

```rust
use progressus_sim::{Character, EffectiveChunk, MovementState};

impl From<EffectiveChunk> for ChunkSnapshot {
    fn from(chunk: EffectiveChunk) -> Self {
        Self {
            coordinate: chunk.coordinate(),
            side: CHUNK_SIDE,
            cells: chunk.cells().to_vec(),
        }
    }
}
```

In Application::snapshot replace only the terrain source:

```rust
self.simulation
    .effective_chunk(coordinate)
    .map(ChunkSnapshot::from)
```

Add exactly this private test constructor; no production Application constructor:

```rust
#[cfg(test)]
impl Application {
    fn from_simulation_for_test(simulation: Simulation) -> Self {
        Self { simulation }
    }
}
```

Do not add Command::SetTerrain, re-exports, Cargo features, client/headless routes, or mutable snapshots.

- [ ] **Step 5: Run focused GREEN checks**

Run:

```bash
cargo test -p progressus-sim simulation::tests
cargo test -p progressus-app
cargo test -p progressus-sim
cargo fmt --all -- --check
cargo clippy -p progressus-sim --all-targets -- -D warnings
cargo clippy -p progressus-app --all-targets -- -D warnings
```

Expected: blocked and unblocked override passability both pass, raw base remains immutable, and ordinary production snapshots contain effective terrain.

- [ ] **Step 6: Commit Task 2**

```bash
git add crates/progressus-sim/src/simulation.rs \
  crates/progressus-app/src/lib.rs crates/progressus-app/src/read_model.rs
git commit -m "feat: publish effective terrain snapshots"
```

### Task 3: Document the bootstrap and run the complete gate

**Files:**
- Modify: README.md:52-108
- Modify: docs/architecture/overview.md:140-205,480-530
- Modify: docs/milestones/prototype-01.md:35-105

**Interfaces:**
- Consumes: completed Task 1/2 behavior and test evidence.
- Produces: accurate Prototype 01 status without runtime API or dependency changes.

- [ ] **Step 1: Record documentation acceptance assertions**

Use this exact checklist before editing prose:

```text
README: deterministic base worldgen remains immutable; sparse in-memory effective terrain affects movement and snapshots; no save/load, residency policy, or terrain gameplay command exists.
Architecture: GeneratedChunk is raw base; EffectiveChunk is on-demand current terrain; ModifiedWorld is private canonical BTreeMap state owned with base identity by Simulation; app snapshots use effective terrain.
Milestone: P01-WORLD-05 is partially advanced (bootstrap), P01-WORLD-04 remains incomplete, and P01-SIM-04 remains incomplete navigation.
```

- [ ] **Step 2: Make only those documentation updates**

Add concise prose matching the checklist. Do not claim persistence, resident caching/unloading, construction/mining gameplay, terrain UI, navigation/pathfinding, or a completed milestone.

- [ ] **Step 3: Verify documentation claims**

Run:

```bash
rg -n "EffectiveChunk|ModifiedWorld|P01-WORLD-05|partially advanced|save/load|residency" \
  README.md docs/architecture/overview.md docs/milestones/prototype-01.md
git diff --check
```

Expected: the required status appears and no whitespace error is reported.

- [ ] **Step 4: Run the full verification gate**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p progressus-headless -- --seed 42 --ticks 100000
cargo run -p progressus-headless -- --seed 0 --travel-chunks 64
./scripts/verify-core-dependency-boundary.sh
cargo check -p progressus-client
```

Expected: every command exits 0. The guard reports no Bevy in the headless/application chain and only Bevy plus progressus-app as client direct dependencies.

- [ ] **Step 5: Commit Task 3**

```bash
git add README.md docs/architecture/overview.md docs/milestones/prototype-01.md
git commit -m "docs: record sparse modified terrain bootstrap"
```

## Plan Self-Review

- Spec coverage: Task 1 creates canonical sparse state, EffectiveChunk, raw/effective APIs, canonical removal, resolver consistency, determinism, distant independence, and non-visitation tests. Task 2 routes both passability modes and snapshots through those APIs and proves the private seam plus raw immutability. Task 3 documents only achieved bootstrap state and runs every required gate.
- Placeholder scan: each task names exact files, interfaces, RED/GREEN commands, implementation shape, and commit.
- Type consistency: Task 1 defines EffectiveChunk, generated_chunk, effective_terrain_at, effective_chunk, and set_terrain_override. Task 2 uses exactly those names and replaces ChunkSnapshot conversion from GeneratedChunk to EffectiveChunk; no client/headless public type changes occur.
