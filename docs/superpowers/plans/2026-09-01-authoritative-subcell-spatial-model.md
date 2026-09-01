# Authoritative Sub-cell Spatial Model Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace bootstrap `WorldCell` character positions and one-cell-per-tick movement with deterministic fixed-point points and continuous cardinal movement over effective terrain.

**Architecture:** `progressus-sim` owns canonical `WorldPosition` (`i128`, 1024 subunits/cell), speed, interaction reach, and movement. `progressus-app` publishes detached exact positions and derived coarse cells. Headless and Bevy client consume only that boundary; Bevy floats are local presentation conversions only.

**Tech Stack:** Rust 1.89; pure Rust `progressus-sim`; `progressus-app`; Bevy 0.18.1 in `progressus-client` only.

## Global Constraints

- `WorldCell` remains worldgen, terrain, chunk, topology, and first-pathfinding-graph coordinate; no smaller global terrain grid.
- `WorldPosition` has private signed `i128` coordinates and `SUBUNITS_PER_CELL = 1024`; authoritative `f32`/`f64` are forbidden.
- Every valid position maps by Euclidean division to one representable `WorldCell(i64)`.
- `DEFAULT_CHARACTER_SPEED = 256 subunits/tick` is bootstrap gameplay data, not coordinate-format data.
- Keep only `SetMovementDirection` and `StopMovement`; do not add `MoveTo`, A*, jobs, collision, physics, navigation footprints, or persistence.
- Movement uses only `effective_terrain_at`. A blocked target forbids only the actual transition: first consume any budget that remains inside source; once the tick could enter it, stop at `B - 1` positive or `B` negative, become idle, and drop that tick's remainder.
- Direct terrain mutation may strand an existing pawn in newly blocked terrain; do not eject/snap it. The invariant forbids movement from entering a blocked cell.
- `InteractionRadius` is reach only; zero represents a point item. It is not collision or a navigation footprint.
- Preserve `progressus-client -> progressus-app -> progressus-sim -> progressus-worldgen`; Bevy remains absent below client.
- Every task is TDD, independently reviewed, committed, and pushed to `origin` before the next task.

---

## Task 1: Canonical fixed-point coordinates and interaction reach

**Files:**
- Create: `crates/progressus-sim/src/position.rs`
- Modify: `crates/progressus-sim/src/lib.rs`
- Test: `crates/progressus-sim/tests/position.rs`

**Produces:**

```rust
pub const SUBUNITS_PER_CELL: i128 = 1024;
pub struct WorldPosition { /* private x/y i128 */ }
pub enum WorldPositionError { OutsideWorldCellRange }
pub struct InteractionRadius(/* private u32 */);

impl WorldPosition {
    pub fn from_subunits(x: i128, y: i128) -> Result<Self, WorldPositionError>;
    pub fn from_cell_origin(cell: WorldCell) -> Result<Self, WorldPositionError>;
    pub fn from_cell_center(cell: WorldCell) -> Result<Self, WorldPositionError>;
    pub const fn x_subunits(self) -> i128;
    pub const fn y_subunits(self) -> i128;
    pub fn containing_cell(self) -> WorldCell;
    pub fn checked_translate(self, dx: i128, dy: i128) -> Result<Self, WorldPositionError>;
}
impl InteractionRadius {
    pub const fn zero() -> Self;
    pub const fn new(subunits: u32) -> Self;
    pub const fn subunits(self) -> u32;
}
pub fn within_interaction_range(
    first: WorldPosition, first_radius: InteractionRadius,
    second: WorldPosition, second_radius: InteractionRadius,
) -> bool;
```

- [ ] **Step 1: Write failing public tests**

Create `crates/progressus-sim/tests/position.rs` with these assertions and a far-apart no-panic case:

```rust
assert_eq!(SUBUNITS_PER_CELL, 1024);
assert_eq!(WorldPosition::from_cell_center(WorldCell::new(-1, 0)).unwrap().x_subunits(), -512);
assert_eq!(WorldPosition::from_subunits(-1, 0).unwrap().containing_cell(), WorldCell::new(-1, 0));
assert_eq!(WorldPosition::from_subunits(0, 0).unwrap().containing_cell(), WorldCell::new(0, 0));
for cell in [WorldCell::new(-32, -1), WorldCell::new(0, 0), WorldCell::new(32, 7)] {
    assert_eq!(WorldPosition::from_cell_center(cell).unwrap().containing_cell(), cell);
}
let maximum = WorldPosition::from_cell_origin(WorldCell::new(i64::MAX, 0)).unwrap();
assert!(maximum.checked_translate(SUBUNITS_PER_CELL, 0).is_err());
let actor = WorldPosition::from_subunits(100, 100).unwrap();
assert!(within_interaction_range(actor, InteractionRadius::new(5), WorldPosition::from_subunits(103, 104).unwrap(), InteractionRadius::zero()));
assert!(!within_interaction_range(actor, InteractionRadius::new(5), WorldPosition::from_subunits(104, 104).unwrap(), InteractionRadius::zero()));
```

- [ ] **Step 2: Verify RED**

Run `cargo test -p progressus-sim --test position`.

Expected: compilation fails because position/reach types do not exist.

- [ ] **Step 3: Implement the smallest canonical module**

Validate each `from_subunits` axis by `div_euclid(SUBUNITS_PER_CELL)` and `i64::try_from`. Construct origins/centers through checked `i128` multiplication/addition. Implement `checked_translate` with `checked_add` then re-validation and export this module from `lib.rs`.

Implement exact bounded reach as:

```rust
let reach = i128::from(u64::from(first_radius.subunits()) + u64::from(second_radius.subunits()));
let dx = first.x_subunits() - second.x_subunits();
let dy = first.y_subunits() - second.y_subunits();
let abs_dx = dx.abs();
let abs_dy = dy.abs();
if abs_dx > reach || abs_dy > reach { return false; }
abs_dx * abs_dx + abs_dy * abs_dy <= reach * reach
```

The valid position range makes subtraction/absolute value safe; two `u32` radii make all squares fit `i128`. Do not add items, collision bodies, or terrain APIs.

- [ ] **Step 4: Verify GREEN**

```bash
cargo fmt --all -- --check
cargo test -p progressus-sim --test position
cargo clippy -p progressus-sim --all-targets -- -D warnings
```

Expected: every command exits 0.

- [ ] **Step 5: Review, commit, push**

Confirm fields are private and `InteractionRadius` documentation says reach only. Run `git diff --check`, then:

```bash
git add crates/progressus-sim/src/lib.rs crates/progressus-sim/src/position.rs crates/progressus-sim/tests/position.rs
git commit -m "feat: add fixed-point world positions"
git push origin HEAD
```

## Task 2: Exact characters and detached snapshots

**Files:**
- Modify: `crates/progressus-sim/src/entity.rs`
- Modify: `crates/progressus-sim/src/simulation.rs`
- Modify: `crates/progressus-sim/src/lib.rs`
- Modify: `crates/progressus-app/src/lib.rs`
- Modify: `crates/progressus-app/src/read_model.rs`
- Test: `crates/progressus-sim/tests/simulation.rs`
- Test: `crates/progressus-app/tests/client_boundary.rs`

**Consumes:** Task 1 `WorldPosition`.

**Produces:**

```rust
pub const DEFAULT_CHARACTER_SPEED: MovementSpeed = MovementSpeed::new(256).unwrap();
pub struct MovementSpeed(/* private nonzero u32 */);
impl Character {
    pub const fn position(&self) -> WorldPosition;
    pub const fn speed(&self) -> MovementSpeed;
}
pub struct CharacterSnapshot {
    pub id: EntityId,
    pub name: String,
    pub position: WorldPosition,
    pub containing_cell: WorldCell,
    pub movement: MovementState,
}
```

- [ ] **Step 1: Write failing migration tests**

Change spawn assertions to:

```rust
assert_eq!(character.position(), WorldPosition::from_cell_center(WorldCell::new(x, 0)).unwrap());
```

In `client_boundary.rs`, add:

```rust
assert_eq!(cora.position, WorldPosition::from_cell_center(WorldCell::new(0, 0)).unwrap());
assert_eq!(cora.containing_cell, WorldCell::new(0, 0));
assert_eq!(cora.position.containing_cell(), cora.containing_cell);
```

Keep the detached-snapshot proof by mutating an owned snapshot then reading a fresh one.

- [ ] **Step 2: Verify RED**

```bash
cargo test -p progressus-sim --test simulation
cargo test -p progressus-app --test client_boundary
```

Expected: compilation fails because character/snapshot position is still `WorldCell`.

- [ ] **Step 3: Implement migration**

Store `WorldPosition` in `Character`; spawn at walkable-cell centers. Define `MovementSpeed(u32)` with an `Option`-returning zero-rejecting constructor and `subunits_per_tick()` getter. Assign every character `DEFAULT_CHARACTER_SPEED`; expose no production speed setter or app command. A `#[cfg(test)]` simulation helper may set speed.

Map `WorldPositionError` into explicit `SimulationError`. Build `CharacterSnapshot.containing_cell` from copied canonical position. Re-export position/speed values through sim then app, without direct client dependency on sim.

- [ ] **Step 4: Verify GREEN**

```bash
cargo fmt --all -- --check
cargo test -p progressus-sim --test simulation
cargo test -p progressus-app --test client_boundary
cargo clippy -p progressus-app --all-targets -- -D warnings
```

- [ ] **Step 5: Review, commit, push**

Verify no authoritative character `WorldCell` remains and no app-level speed mutation exists. Then:

```bash
git add crates/progressus-sim/src/entity.rs crates/progressus-sim/src/lib.rs crates/progressus-sim/src/simulation.rs crates/progressus-sim/tests/simulation.rs crates/progressus-app/src/lib.rs crates/progressus-app/src/read_model.rs crates/progressus-app/tests/client_boundary.rs
git commit -m "feat: publish exact character positions"
git push origin HEAD
```

## Task 3: Effective-terrain continuous cardinal movement

**Files:**
- Modify: `crates/progressus-sim/src/simulation.rs`
- Test: `crates/progressus-sim/src/simulation.rs`

**Consumes:** Task 2 character positions and speeds.

**Produces:** Existing direction/stop commands move up to speed subunits per tick, with multiple sequential cell transitions allowed.

- [ ] **Step 1: Write failing simulation tests**

Use test-only position/speed helpers and real `set_terrain_override` calls. Add independent tests for: 128-subunit partial motion; 256-speed exact next center after four ticks; two distinct speeds; multiple grass-cell crossings in one tick; positive/negative chunk crossings; deterministic equality; coordinate overflow; Stop at a noncenter point; and valid noncenter direction replacement.

For every direction, set movement while its neighbour is grass, move part way, then override that neighbour to water/rock. With a small speed and a distant boundary, assert several ticks consume their complete budget inside source and retain `MovementState::Moving`; only the tick that could enter stops exactly. East from cell 0 stops at x=1023; west from cell 0 stops at x=0; north/south have equivalent y values. Repeat the four cases near negative cells. Assert `effective_terrain_at(position.containing_cell()) == Grass`, `MovementState::Idle`, and that a large speed's unused remainder did not cross another cell. Finally issue reverse movement and assert it starts at the retained point, not a center.

Add table-driven positive/negative x/y world-limit cases. A pawn starts at the extreme cell center and may move inside that cell; only an outward transition stops at the final valid subunit without wrapping. Also replace old tests that expect `MovementDestinationBlocked` from `SetMovementDirection`: a blocked neighbour command is accepted and deterministically stops only at its boundary.

- [ ] **Step 2: Verify RED**

Run `cargo test -p progressus-sim simulation::tests`.

Expected: partial-position and boundary assertions fail because current code assigns an adjacent `WorldCell` once per tick.

- [ ] **Step 3: Implement checked boundary consumption**

`set_movement_direction` replaces persistent direction from the exact current position without a terrain lookup. It preserves prior state only for true command errors such as an unknown entity; blocked terrain is handled when movement reaches a transition.

For each moving character in stable ID order, initialize `remaining` from speed and repeat:

```text
source = position.containing_cell()
target = direction.adjacent(source), or idle normally on coordinate limit
entry_distance = subunits until the first point belonging to target
if target is blocked or does not exist beyond a world-domain edge:
    if remaining < entry_distance:
        translate remaining in direction
        keep Moving and finish tick
    else:
        translate entry_distance - 1 in direction
        set Idle and discard remaining
else:
    translate min(remaining, entry_distance) in direction
    subtract translated distance and continue while remaining > 0
```

For east/north, `entry_distance = upper_boundary - coordinate`; for west/south, `entry_distance = coordinate - lower_boundary + 1`. The same formulas apply to an outward representable-world edge, where the external target is treated as impassable. Compute boundaries with checked `i128` arithmetic. Apply all deltas through `WorldPosition::checked_translate`. Any arithmetic failure is a normal idle stop at the last exact position. Do not validate source terrain: direct mutation may strand a pawn there, and this increment must not eject/snap it.

- [ ] **Step 4: Verify GREEN**

```bash
cargo fmt --all -- --check
cargo test -p progressus-sim simulation::tests
cargo test -p progressus-sim --test simulation
cargo test -p progressus-sim --test world_state
cargo clippy -p progressus-sim --all-targets -- -D warnings
```

- [ ] **Step 5: Review, commit, push**

Check the four boundary formulas, effective lookup, no-wrap handling, and stable iteration order. Then:

```bash
git add crates/progressus-sim/src/simulation.rs
git commit -m "feat: advance characters at subcell resolution"
git push origin HEAD
```

## Task 4: Headless and Bevy snapshot consumers

**Files:**
- Modify: `crates/progressus-headless/src/main.rs`
- Modify: `crates/progressus-client/src/render.rs`
- Modify: `crates/progressus-client/src/presentation.rs`
- Modify: `crates/progressus-client/src/runtime.rs`
- Test: `crates/progressus-headless/tests/cli.rs`
- Test: `crates/progressus-client/src/render.rs`
- Test: `crates/progressus-client/src/presentation.rs`
- Test: `crates/progressus-client/src/runtime.rs`

**Consumes:** Task 2 snapshots and Task 3 four-tick center-to-center default movement.

- [ ] **Step 1: Write failing consumer tests**

Migrate snapshot fixtures to `WorldPosition::from_cell_center(...)` plus matching `containing_cell`. Add a render assertion that `WorldPosition::from_cell_center(WorldCell::new(0, 0))` is exactly the center of terrain tile `(0, 0)`, then assert `WorldPosition::from_subunits(768, 512)` relative to that cell **center** has x translation `3.0` pixels with `CELL_SIZE = 12`.

Change headless travel tests so the walker sends one direction command and advances exactly one ordinary tick at a time until snapshot `position` equals the selected target cell center. It must not choose its next neighbour only because `containing_cell` changed at a boundary. Bound this inner wait and include seed, entity ID, exact subunits, containing cell, target, and wait ticks in failure output.

- [ ] **Step 2: Verify RED**

```bash
cargo test -p progressus-headless --test cli
cargo test -p progressus-client
```

Expected: fixture type and one-tick walker assertions fail.

- [ ] **Step 3: Implement boundary-only presentation conversion**

Derive terrain central chunk from `CharacterSnapshot.containing_cell`. In `render.rs`, make the terrain origin `WorldPosition::from_cell_center(origin_cell)` and convert only after local subtraction:

```rust
let x = (position.x_subunits() - origin.x_subunits()) as f32
    / SUBUNITS_PER_CELL as f32
    * CELL_SIZE;
```

Apply the same y rule. Never pass floats through app/sim. In the external walker retain only the existing `BTreeMap<WorldCell, u64>`; use snapshots/commands only through `progressus-app`, wait for exact target centers, and scale its checked tick limit for four ticks per cell. Update client boundary-crossing tests to advance enough ticks to reach expected centers while keeping terrain refresh tied only to changed authoritative central chunks.

- [ ] **Step 4: Verify GREEN**

```bash
cargo fmt --all -- --check
cargo test -p progressus-headless --test cli
cargo test -p progressus-client
cargo check -p progressus-client
cargo clippy -p progressus-client --all-targets -- -D warnings
```

- [ ] **Step 5: Review, commit, push**

Verify headless has no direct sim access, client has no direct sim/worldgen dependency, no terrain FPS query was introduced, and floats are presentation-only. Then:

```bash
git add crates/progressus-headless/src/main.rs crates/progressus-headless/tests/cli.rs crates/progressus-client/src/render.rs crates/progressus-client/src/presentation.rs crates/progressus-client/src/runtime.rs
git commit -m "feat: present fixed-point character positions"
git push origin HEAD
```

## Task 5: Documentation, final gate, and merge readiness

**Files:**
- Modify: `README.md`
- Modify: `docs/architecture/overview.md`
- Modify: `docs/milestones/prototype-01.md`

- [ ] **Step 1: Write the documentation checklist**

Only claim fixed-point authoritative character positions, effective-terrain continuous cardinal movement, detached exact snapshots, and headless/client consumption. Keep P01-SIM-04 and TEST-P01-03 partial. Do not claim A*/click-to-move, jobs/AI, collision, physical footprints, items/resources, persistence, or residency.

- [ ] **Step 2: Update documentation**

Replace claims that character position is only `WorldCell` or every tick is one cell. State that topology stays cellular, living positions are 1024-subunit fixed-point, 256 is bootstrap speed, and Bevy receives exact snapshots but owns only presentation floats. Retain ADR-0004 and interactive pathfinding as future work.

- [ ] **Step 3: Run the full gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p progressus-headless -- --seed 42 --ticks 100000
cargo run -p progressus-headless -- --seed 0 --travel-chunks 64
./scripts/verify-core-dependency-boundary.sh
cargo check -p progressus-client
git diff --check
```

Expected: all commands exit 0. Record new deterministic 64-boundary travel ticks, exact final position, and boundary count; remove obsolete one-cell-tick numbers.

- [ ] **Step 4: Independent final review**

Review against the approved specification: private no-float position state; all four half-open blocked-boundary rules; effective-terrain-only passability; remainder/overflow/determinism; snapshot/client boundary; and no pathfinding, `MoveTo`, collision, jobs, physics, persistence, or selection scope creep. Fix any finding by a RED/GREEN commit and repeat the full gate.

- [ ] **Step 5: Commit, push, and merge handoff**

```bash
git add README.md docs/architecture/overview.md docs/milestones/prototype-01.md
git commit -m "docs: record subcell movement bootstrap"
git push origin HEAD
```

After green review and gates, merge the isolated feature branch into `main`, run `cargo test --workspace` on merged state, and push `origin/main`.
