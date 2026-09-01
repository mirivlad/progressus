# Interactive Pathfinding and Click-to-Move Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add deterministic `MoveTo`, exact fixed-point routes, client click selection/input, and a visual movement probe.

**Architecture:** Simulation owns a bounded grid A*, private routes, live terrain checks, and transient traces. Application owns command/read translation. Client owns selection, quantization, overlay, and interpolation, depending only on Bevy plus app.

**Tech Stack:** Rust 1.89, pure Rust `progressus-sim`, Bevy 0.18.1 in client only.

## Global Constraints

- `WorldCell` remains terrain/chunk/A* topology; `WorldPosition` remains signed-i128 fixed point with 1024 subunits/cell.
- No authoritative Bevy/f32/f64, renderer time, global finite map, persistent search cache, auto-repath, diagonal A*, collision, jobs/AI, or persistence.
- A* is effective-terrain-only and cardinal in East, North, South, West order, with ordered maps/heap keys and `PATHFINDING_NODE_BUDGET = 50_000`.
- Search-local ordered `EffectiveChunk` cache is ephemeral and not residency.
- Preserve exact half-open blocked-boundary/overflow semantics; blocked current origin can exit but cannot be re-entered by a route.
- Default snapshots omit route/trace; only `SnapshotQuery::navigation_for` requests detached selected data.

---

### Task 1: Deterministic grid A*

**Files:** Create `crates/progressus-sim/src/pathfinding.rs`; modify `crates/progressus-sim/src/lib.rs`; test in the new module.

**Produces:** private `find_path(&Simulation, WorldCell, WorldCell) -> Result<Vec<WorldCell>, PathfindingError>` and node-budget constant.

- [ ] Write RED tests for a golden equal-cost obstacle route, blocked-start escape/no-reentry, exhausted-frontier versus node-budget result, effective override block/open, and cross-chunk search.
- [ ] Run `cargo test -p progressus-sim pathfinding`; expect missing module/API.
- [ ] Implement A* with `BinaryHeap<Reverse<OpenNode>>` ordered by `(f, h, insertion, cell)`, ordered cost/predecessor sets, checked Manhattan arithmetic, and function-local `BTreeMap<ChunkCoord, EffectiveChunk>`.
- [ ] Run `cargo test -p progressus-sim pathfinding && cargo clippy -p progressus-sim --all-targets -- -D warnings`; review that Simulation gains no cache.
- [ ] Commit/push: `feat: add deterministic grid pathfinding`.

### Task 2: Exact routes and trace

**Files:** Modify `crates/progressus-sim/src/entity.rs` and `crates/progressus-sim/src/simulation.rs`; test in `simulation.rs`.

**Produces:** `Simulation::move_to`, private `NavigationRoute { destination, waypoints: VecDeque<WorldPosition> }`, `MovementState::{Idle, ManualDirectional, Navigating}`, trace accessors.

- [ ] Write RED tests for exact same-cell X/Y route, straight/multi-turn/cross-chunk/negative routes, exact arrival, speed remainder over waypoint, trace corner, stranded exit, route replacement, failed-command preservation, stop/manual cancellation, and live terrain route invalidation at exact boundary.
- [ ] Run focused same-cell RED test; expect `move_to`/state absence.
- [ ] Implement geometry: same cell is current -> `(destination.x,current.y)` -> destination; cross-cell is current-to-start-center X/Y, A* centers, then center-to-destination X/Y; delete zero legs. Reuse cardinal transition checks and spend remainder across waypoints. Traces are `[position]` without movement or `[start, waypoint..., final]` per executed final tick.
- [ ] Run `cargo test -p progressus-sim && cargo clippy -p progressus-sim --all-targets -- -D warnings`; review no auto-repath and exact blockage behavior.
- [ ] Commit/push: `feat: execute exact navigation routes`.

### Task 3: Application command and selected navigation snapshot

**Files:** Modify `crates/progressus-app/src/lib.rs`, `crates/progressus-app/src/read_model.rs`, and `crates/progressus-app/tests/client_boundary.rs`.

**Produces:** `Command::MoveTo`, `SnapshotQuery::navigation_for`, and `NavigationSnapshot { character_id, destination: Option<WorldPosition>, remaining_waypoints, last_tick_motion_trace }`.

- [ ] Write RED boundary tests proving a default query omits route data, selected data is detached, and rejected MoveTo leaves published old route intact.
- [ ] Run focused app RED test; expect command/query/read-model absence.
- [ ] Delegate only to Simulation move/trace accessors; copy route/trace only for the requested ID and do not expose mutable state.
- [ ] Run `cargo test -p progressus-app && cargo clippy -p progressus-app --all-targets -- -D warnings`; review no Bevy or terrain mutation command crosses app.
- [ ] Commit/push: `feat: publish selected navigation snapshots`.

### Task 4: Client interaction and visual probe

**Files:** Create `crates/progressus-client/src/navigation.rs`; modify `runtime.rs`, `render.rs`, `lib.rs`; test new module and runtime no-window systems.

**Produces:** client-only selection, left/right click conversion, trace-polyline interpolator, F3 selected/destination/route/authority markers.

- [ ] Write RED tests for EntityId selection tie, center/arbitrary/negative/boundary click quantization, panned/zoomed local conversion, corner-following interpolation, selected disappearance, rebase projection, overlay-from-detached data, rejected command without optimistic authority edits.
- [ ] Run focused interpolation RED test; expect absent module.
- [ ] Convert viewport local coordinates by scaling/rounding to subunits then checked-adding an exact cell-center origin before `WorldPosition::from_subunits`. Request selected navigation only. Interpolate by trace polyline length, clamp to authority endpoint, and never write simulation.
- [ ] Run `cargo test -p progressus-client && cargo clippy -p progressus-client --all-targets -- -D warnings && ./scripts/verify-core-dependency-boundary.sh`; review direct client dependencies.
- [ ] Commit/push: `feat: add interactive route presentation`.

### Task 5: Consumer regression, documentation, and integration

**Files:** Modify headless consumer only if enum matching requires it; modify `README.md`, `docs/architecture/overview.md`, `docs/milestones/prototype-01.md`.

- [ ] Keep public headless behavior green with `cargo test -p progressus-headless`, 100k ticks, and travel64.
- [ ] Document achieved A*/click-to-move bootstrap only; retain partial P01-SIM-04/TEST-P01-03 status for no auto-repath, collision, jobs/AI, persistence, residency, or hierarchical navigation.
- [ ] Run `cargo fmt --all -- --check`, workspace Clippy with `-D warnings`, workspace tests, both headless scenarios, boundary guard, client check, and `git diff --check`.
- [ ] Perform final review for raw-terrain shortcut, default route copy, auto-repath, global-float input, chord-cutting, Bevy leakage, and scope creep. Attempt graphical smoke if display/GPU exists.
- [ ] Commit/push docs, fast-forward into main, run workspace test on merged main, and push origin/main.
