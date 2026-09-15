# Prototype 02 Rock Excavation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the player physically remove an explored Rock cell, leaving passable Grass and one haulable Stone while preserving skill, reservation, save and client-refresh contracts.

**Architecture:** The pure-Rust simulation owns the terrain override, exclusive excavation job, work lifecycle, tool prerequisite, physical item and Mining practice. Save v1 persists the new kind/revision; `progressus-app` publishes detached state and the Bevy client only designates and redraws it.

**Tech Stack:** Rust workspace, serde save v1, deterministic JobWorld/ItemWorld, detached application snapshots, Bevy low-poly client and headless tests.

## Global Constraints

- Follow [the owner-confirmed D1 spec](../specs/2026-09-15-rock-excavation-design.md) and accepted ADR-0001/0002/0003/0004/0022.
- One completed `Rock -> Grass` cell creates exactly physical `Stone x1`; no pits, soil excavation, underground levels or smelting in D1.
- Rock work requires equipped `capability::MINE`; base work is four ticks, Mining mastery uses three, and only completion awards one practice point.
- A designation may wait for a route/tool, but no unavailable/abandoned job may hold a worker or create an item. Exact `Working.remaining_ticks` and stable IDs survive save/load.
- Worldgen base/layers never change; mutation is a sparse persisted terrain override. Bevy animation/rendering never determines completion.
- Work directly on `main`; after each independently green task, commit and push to `origin/main`. Native click/remesh behavior remains separately reported until observed.

## File map

- `crates/progressus-sim/src/simulation.rs` owns terrain revision and cell mutation; `simulation/error.rs` names failures; `simulation/persistence.rs` saves the revision and excavation kind.
- `crates/progressus-sim/src/job.rs` owns `ExcavateRock { cell }` and an exclusive source index; `simulation/work.rs` owns designation, tool fetch, assignment, work and completion.
- `crates/progressus-app/src/lib.rs` exposes the command/revision; `read_model.rs` publishes detached terrain revision and job kind.
- `crates/progressus-client/src/low_poly/scene.rs` invalidates visible terrain meshes; `runtime.rs`, `ui.rs`, `i18n.rs`, `render.rs` accept player designation and show its state. `low_poly/character.rs` maps the physical Working job to the existing work pose. `assets/procedural/low_poly/terrain.rs` and `low_poly/mod.rs` expose the rock mesh's maximum height to keep drag/marker geometry visibly above it.
- `docs/milestones/prototype-02.md`, `docs/architecture/overview.md`, `docs/client.md`, `README.md` describe verified D1, not unfinished soil/metallurgy.

---

### Task 1: Authoritative terrain revision and visible remesh

**Files:** Modify `crates/progressus-sim/src/simulation.rs`, `simulation/error.rs`, `simulation/persistence.rs`, `crates/progressus-app/src/read_model.rs`, `crates/progressus-app/src/lib.rs`, `crates/progressus-client/src/low_poly/scene.rs`; tests beside existing terrain/save/snapshot/scene tests.

**Interfaces:** Produce `Simulation::terrain_revision() -> u64`, additive `SaveV1.terrain_revision: u64`, `ClientSnapshot.terrain_revision: u64`, and presentation refresh on that revision. No excavation kind exists yet.

- [ ] **Step 1: Write failing tests.** In simulation tests, start from known Grass cell `(0, 0)`, assert revision 0, set Rock and assert 1, set Rock again and assert 1, restore Grass and assert 2. In persistence tests, save after that mutation, load and assert both terrain and revision round-trip; remove `terrain_revision` from JSON and assert old-save load defaults to 0. In the application boundary, assert a snapshot's revision equals simulation revision after an override. In `low_poly/scene.rs`, test that `terrain_refresh_required(false, Some([2, 4, 7, 9]), [2, 5, 7, 9])` is true while an item-only change is false.

```rust
#[test]
fn terrain_revision_changes_only_with_effective_terrain() {
    let mut sim = Simulation::new(WorldSeed::new(0)).unwrap();
    let cell = WorldCell::new(0, 0);
    assert_eq!(sim.terrain_revision(), 0);
    sim.set_terrain_override(cell, terrain::ROCK).unwrap();
    assert_eq!(sim.terrain_revision(), 1);
    sim.set_terrain_override(cell, terrain::ROCK).unwrap();
    assert_eq!(sim.terrain_revision(), 1);
    sim.set_terrain_override(cell, terrain::GRASS).unwrap();
    assert_eq!(sim.terrain_revision(), 2);
}
```

- [ ] **Step 2: Run red.** `cargo test -p progressus-sim --lib terrain_revision_changes_only_with_effective_terrain` must fail on the missing method; then run each new persistence/app/client focused test red before adding its code.
- [ ] **Step 3: Implement only revision/persistence/refresh.** Add `terrain_revision: u64` to `Simulation` (new 0, restore saved value), `SaveV1` (`#[serde(default)]`), and `ClientSnapshot`. `set_terrain_override` first compares `effective_terrain_at(cell)` with requested terrain; only an actual change preflights `checked_add(1)` (new `SimulationError::TerrainRevisionOverflow`), writes the sparse override and advances revision. In the scene cache, change revisions to `[exploration, terrain, item, resource]`; extract the terrain predicate for testing and keep item/resource predicates keyed by their own indexes.

```rust
fn terrain_refresh_required(viewport_changed: bool, old: Option<[u64; 4]>, now: [u64; 4]) -> bool {
    viewport_changed || old.is_none_or(|value| value[0] != now[0] || value[1] != now[1])
}
```

- [ ] **Step 4: Run green.** Run all four focused tests; `cargo test -p progressus-sim --lib`, `cargo test -p progressus-app`, `cargo check -p progressus-client --all-targets -j 1`, `cargo fmt --all -- --check`, `cargo clippy -p progressus-sim --all-targets -- -D warnings`, and `git diff --check`. Verify untouched worldgen golden tests remain unchanged.
- [ ] **Step 5: Commit/push.** Commit the files above as `feat: refresh mutated terrain through a saved revision`, push `origin main`, and require `HEAD == origin/main`.

### Task 2: Exclusive physical rock job, work and persistence

**Files:** Modify `crates/progressus-sim/src/job.rs`, `simulation/error.rs`, `simulation/work.rs`, `simulation/persistence.rs`, `crates/progressus-app/src/lib.rs`, app boundary tests; update exhaustive `JobKind` matches in `crates/progressus-client/src/{i18n,render,runtime}.rs` and `low_poly/character.rs` so the workspace still compiles. Add focused tests beside job/work/save tests.

**Interfaces:** Produce `JobKind::ExcavateRock { cell: WorldCell }`, `JobWorld::excavation_job_at(cell) -> Option<EntityId>`, `Simulation::designate_rock_excavation(cell) -> Result<EntityId, SimulationError>`, and `Command::DesignateRockExcavation { cell }`.

- [ ] **Step 1: Write failing tests.** Create a seed-0 explored Rock fixture with one adjacent Grass approach, stage Cora there and advance one discovery tick. Assert hidden/non-Rock/duplicate/claimed designations reject without ID allocation, and a valid job stays `Available` with no tool. Add a JobWorld test that two jobs for one cell cannot coexist and removal clears its index. Add a real-completion test: place `PrimitiveTool x1` at the worker's reachable ground position, order fetch/equip, designate Rock, advance until removed, then assert effective Grass, one added Stone unit at that cell, Mining+1 only for that worker, consistent item/job indexes and no second output after 32 ticks. Clone the fixture at mastery 5 and compare initial `Working.remaining_ticks` 4 versus 3; mastery without equipped tool does not start. Add cancellation/manual/path/target/tool-loss tests with zero output and practice.

```rust
#[test]
fn excavation_requires_a_physical_tool_and_changes_one_cell_once() {
    let (mut sim, rock, approach, worker) = explored_rock_fixture();
    let job = sim.designate_rock_excavation(rock).unwrap();
    sim.advance_ticks(8).unwrap();
    assert_eq!(sim.job_world.get(job).unwrap().state(), JobState::Available);
    assert_eq!(sim.effective_terrain_at(rock).unwrap(), terrain::ROCK);
    let tool = insert_ground_stack(&mut sim, item::PRIMITIVE_TOOL, 1, approach);
    sim.designate_equipment_fetch(worker, tool).unwrap();
    for _ in 0..512 { if sim.job_world.get(job).is_none() { break; } sim.advance_ticks(1).unwrap(); }
    assert!(sim.job_world.get(job).is_none());
    assert_eq!(sim.effective_terrain_at(rock).unwrap(), terrain::GRASS);
    assert_eq!(sim.characters[&worker].skill_practice(skill::MINING), 1);
}
```

The fixture helper belongs in the test module, not production. Exact Stone quantity/position, tool ownership and post-completion repeats are separate assertions in the same test. Add a nearby accepting stockpile and advance ordinary Haul until that exact Stone unit is carried and deposited there. Add save tests for `Available`, `Reserved`, mastered `Working` and completed terrain/item, asserting deterministic `save_json()` after identical future ticks and rejecting invalid hidden/non-Rock loaded jobs. Explicitly remove the assigned worker during `Reserved` and `Working` in tests, then assert the job returns `Available` with clean worker index and no output/credit.

- [ ] **Step 2: Run red.** `cargo test -p progressus-sim --lib excavation_requires_a_physical_tool_and_changes_one_cell_once` must fail on the missing designation API; JobWorld/persistence/app tests must likewise fail on missing kinds/commands before implementation.
- [ ] **Step 3: Add the job/index and minimal designation.** Add `ExcavateRock { cell }` and `excavate_by_cell: BTreeMap<WorldCell, EntityId>` to JobWorld, with collision error, `insert/restore/remove/indexes_are_consistent` arms and `excavation_job_at`. Reject unexplored, effective non-Rock, existing job, claim or natural resource before ID allocation. Serialize the kind as serde `"excavate_rock"` with `CellSave` and validate its cell/state on load. Keep `JobState` and exact remaining work persistence unchanged.

```rust
pub fn designate_rock_excavation(&mut self, cell: WorldCell) -> Result<EntityId, SimulationError> {
    if !self.is_explored(cell) { return Err(SimulationError::RockExcavationUndiscovered(cell)); }
    if self.effective_terrain_at(cell)? != terrain::ROCK { return Err(SimulationError::RockExcavationNotRock(cell)); }
    if self.cell_is_claimed(cell) || self.natural_resource_at(cell)?.is_some() { return Err(SimulationError::RockExcavationClaimed(cell)); }
    if self.job_world.excavation_job_at(cell).is_some() { return Err(SimulationError::RockExcavationAlreadyDesignated(cell)); }
    let id = self.id_allocator.allocate()?;
    self.job_world.insert(Job::new(id, JobKind::ExcavateRock { cell })).map_err(SimulationError::from_job_world)?;
    Ok(id)
}
```

- [ ] **Step 4: Wire the tool prerequisite and route.** An available excavation adds \`capability::MINE\` to the existing exclusive tool-fetch policy. Implement \`try_assign_rock_excavation\` by trying available workers in distance/ID order, requiring equipped \`MINE\`, then testing explored/walkable N/E/S/W neighboring centers in that order. Use the real \`plan_navigation_route\`; reserve only after it succeeds. If none succeeds, return with the job still \`Available\`. Run the tool/path tests green before the next step.

\`\`\`rust
for worker_id in self.available_workers_by_distance(cell) {
    if !self.can_perform(worker_id, capability::MINE) { continue; }
    for direction in [Direction::North, Direction::East, Direction::South, Direction::West] {
        let Some(neighbor) = direction.adjacent(cell) else { continue; };
        if !self.is_explored(neighbor) || !self.is_walkable(neighbor)? { continue; }
        let target = WorldPosition::from_cell_center(neighbor)?;
        let route = match self.plan_navigation_route(worker_id, target) {
            Ok(route) => route,
            Err(SimulationError::MoveToPathNotFound
                | SimulationError::MoveToDestinationBlocked(_)
                | SimulationError::MoveToDestinationUndiscovered(_)
                | SimulationError::MoveToSearchBudgetExceeded) => continue,
            Err(error) => return Err(error),
        };
        self.job_world.reserve_worker(job_id, worker_id).map_err(SimulationError::from_job_world)?;
        self.apply_navigation_route(worker_id, target, route);
        return Ok(());
    }
}
Ok(())
\`\`\`

- [ ] **Step 5: Wire Reserved/Working lifecycle.** A non-Rock target cancels; absent worker or missing equipped \`MINE\` releases the worker. At the Rock face, use target reach \`InteractionRadius::new(256)\`, set movement Idle and start \`skilled_work_ticks(worker_id, skill::MINING, HARVEST_WORK_TICKS)\`. Working rechecks those facts and either decrements \`remaining_ticks\` or invokes completion at one tick. Run the 4-versus-3 timing, manual interruption and tool-loss tests green.

\`\`\`rust
let reach = InteractionRadius::new(256);
let target = WorldPosition::from_cell_center(cell)?;
if within_interaction_range(character.position(), character.interaction_radius(), target, reach) {
    let ticks = self.skilled_work_ticks(worker_id, skill::MINING, HARVEST_WORK_TICKS);
    self.job_world.start_working(job_id, ticks).map_err(SimulationError::from_job_world)?;
}
\`\`\`

- [ ] **Step 6: Complete physical mutation once.** Preflight terrain-revision overflow and stable item allocation before changing state. Set the \`Grass\` override, insert one \`ItemStack::new_ground\` of \`item::STONE\` quantity one at cell center, remove the job/index and only then award \`skill::MINING\` to its worker. Match existing ItemWorld insertion error policy rather than silently discarding output. Run the exact-output, haul, save and post-completion-repeat tests green.

\`\`\`rust
let _next_revision = self.terrain_revision.checked_add(1)
    .ok_or(SimulationError::TerrainRevisionOverflow)?;
let item_id = self.id_allocator.allocate()?;
let position = WorldPosition::from_cell_center(cell)?;
self.set_terrain_override(cell, terrain::GRASS)?;
self.item_world.insert_ground(ItemStack::new_ground(
    item_id, item::STONE, ItemQuantity::new(1).unwrap(), position,
)).expect("allocated item id is unique and the excavated cell is now traversable");
self.job_world.remove(job_id).map_err(SimulationError::from_job_world)?;
self.characters.get_mut(&worker_id).unwrap().record_skill_practice(skill::MINING);
\`\`\`

- [ ] **Step 7: Keep client/build matches exhaustive.** Add the four cell-bearing Rock errors to \`SimulationError\` and concrete \`Display\` text. In client \`i18n\`, name the job \`выемка породы\` / \`rock excavation\`; \`low_poly/character\` treats Working excavation as \`PoseKind::Work\`. \`render::draw_job_designations\` returns \`None\` until Task 3 adds the elevated marker, and \`runtime::apply_tool_area\` ignores excavation in Harvest-source enumeration and Cancel Jobs until Task 3. Run \`cargo check -p progressus-client --all-targets -j 1\`.

- [ ] **Step 8: Run green/commit.** Run focused rock, JobWorld, save and app tests; \`cargo test -p progressus-sim --lib\`, \`cargo test -p progressus-app\`, \`cargo check -p progressus-client --all-targets -j 1\`, formatting, strict sim Clippy and \`git diff --check\`. Commit/push as \`sim: excavate rock into physical stone\` after these gates. Do not claim native UX from this task.
- [ ] **Step 5: Run green/commit.** Run focused rock, JobWorld, save and app tests; `cargo test -p progressus-sim --lib`, `cargo test -p progressus-app`, `cargo check -p progressus-client --all-targets -j 1`, formatting, strict sim Clippy and `git diff --check`. Commit/push as `sim: excavate rock into physical stone` after these gates. Do not claim native UX from this task.

### Task 3: Player designation, visible status and Stage D1 evidence

**Files:** Modify `crates/progressus-client/src/runtime.rs`, `ui.rs`, `i18n.rs`, `render.rs`, `low_poly/scene.rs`, `low_poly/mod.rs`, `assets/procedural/low_poly/terrain.rs`; update `README.md`, `docs/client.md`, `docs/milestones/prototype-02.md`, `docs/architecture/overview.md`, and the D1 spec status.

**Interfaces:** Consume Task 2's `Command::DesignateRockExcavation`, `JobKind::ExcavateRock`, and Task 1's detached terrain revision. Produce localized `ToolMode::ExcavateRock` in Orders area tools and a pending/working marker.

- [ ] **Step 1: Write failing client tests.** Test `cell_selection_allowed(ToolMode::ExcavateRock)` and a pure known-cell filter over an area snapshot: only `KnownTerrain::Known(terrain::ROCK)` cells are submitted, an unknown/Grass cell is not. Test localized Russian/English HUD help includes the mining tool, `Rock -> Grass` and physical Stone. Test a detached job cell maps `JobKind::ExcavateRock { cell }` to a visible designation marker and that Cancel Jobs rectangle finds/removes that job. Test that both Rock drag/marker geometry is above the maximum rock center height (3.1 presentation units), not at the ground plane. Test scene terrain refresh from Task 1 with a same-viewport changed terrain revision.

```rust
#[test]
fn excavation_area_selects_only_known_rock() {
    assert!(cell_selection_allowed(ToolMode::ExcavateRock));
    let selected = BTreeSet::from([WorldCell::new(1, 0), WorldCell::new(2, 0)]);
    let known = [(WorldCell::new(1, 0), KnownTerrain::Known(terrain::ROCK)),
                 (WorldCell::new(2, 0), KnownTerrain::Unknown)];
    assert_eq!(excavatable_cells(&selected, &known), vec![WorldCell::new(1, 0)]);
}
```

- [ ] **Step 2: Run red.** `cargo test -p progressus-client --lib excavation_area_selects_only_known_rock` must fail on the missing mode/helper; localized/marker/cancel tests fail before their wiring.
- [ ] **Step 3: Wire the smallest UI workflow.** Add the Orders icon and localized mode/help; `apply_tool_area` reads requested known terrain chunks, filters selected known Rock cells, skips already indexed excavation jobs, and issues the app command without probing hidden cells. `draw_job_designations` uses the job's cell and work-state color. Export a presentation-only maximum Rock mesh height from the procedural terrain recipe and place the Rock drag preview/marker just above it; ordinary ground markers remain at their current height. `CancelJobs` area includes `ExcavateRock` by its cell. The existing terrain revision remeshes the completed cell and item revision publishes Stone. Client code never changes terrain/skill/item ownership.
- [ ] **Step 4: Verify/document.** Run client focused/full tests and `cargo check -p progressus-client --all-targets -j 1`, then `PROGRESSUS_RUN_CLIENT_TESTS=1 ./scripts/check-prototype-01.sh`. Inspect native click/worker/remesh/haul if a display is available; otherwise state exactly that visual operation is unverified. Update docs only after corresponding headless/app/client gates prove D1. Soil excavation, furnace, metallurgy and research remain open. Run `git diff --check` and review the final diff against the approved spec.
- [ ] **Step 5: Commit/push.** Commit/push `client: designate and display rock excavation`; verify clean `git status --short` and identical `HEAD`/`origin/main`. Do not mark all Stage D or Prototype 02 complete.
