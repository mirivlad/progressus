# Code audit before audio and Prototype 02 Stage B

Date: 2026-09-08

Audited revision: `127398e19f61b13b541210038178eb942b9b9ce0`.

## Repository state

`git fetch origin` succeeded. `git rev-list --left-right --count HEAD...origin/main` returned `0 0`; `git ls-remote origin refs/heads/main` independently matched HEAD. The initial worktree was clean. This report and the proposed audio specification are documentation additions; gameplay fixes and audio are not implemented by this audit.

## Findings

### P1 — Blocked craft output aborts partially advanced ticks

`crates/progressus-sim/src/simulation.rs:3730` converts absence of an output destination into `ProductionOutputBlocked`. `advance_working_job` propagates it through `advance_ticks`, after the clock, character movement and several maintenance phases already ran. `crates/progressus-client/src/runtime.rs:160` returns before refreshing the client snapshot on that error. Headless callers using `?` terminate their scenario.

Reproduced against the audited core using public APIs: create stockpiles on starting Wood/Stone, place a workbench, order one PrimitiveTool, advance to `Working { remaining_ticks: 1 }`, then make both output cells Rock with `set_terrain_override`. At tick 32 the craft is ready; ticks 33, 34 and 35 each return `Err(ProductionOutputBlocked(EntityId(15)))` while the tick counter advances. This is a controlled API obstacle injection; an equivalent native-UI sequence was not verified.

Suggested correction: blocked output should retain the completed work and reserved inputs while allowing the rest of the tick to finish. Once a destination becomes available, consume inputs and publish one output exactly once. Add a regression covering blocked ticks, save/load, unblocking and cancellation; verify physical quantities and reservation cleanup.

### P2 — Ordinary frame remainders are lost by the tick scheduler

`crates/progressus-client/src/interaction.rs:42` resets elapsed time to zero whenever a tick is emitted. This loses the fractional remainder of ordinary frames, in addition to the documented intentional discard of long-frame backlog.

Reproduced by compiling the actual `TickScheduler` source in a standalone Rust harness. Feed `Duration::from_nanos(1_000_000_000 / fps)` for `fps * 60` frames:

| Synthetic FPS | Ticks in approximately 60 seconds | Nominal target |
| --- | ---: | ---: |
| 30 | 225 | 240 |
| 60 | 225 | 240 |
| 144 | 233 | 240 |
| 200 | 240 | 240 |

These are deterministic scheduler observations, not measured native FPS. This affects presentation pacing and wall-clock game speed, not determinism for identical explicit authoritative tick sequences.

Suggested correction: retain the sub-interval remainder while still allowing at most one tick per update and discarding whole overdue intervals. Add ordinary-frame cadence tests alongside the existing pause/long-frame tests. Nanosecond truncation may legitimately leave a final tick just short of the interval; tests must account for exact supplied elapsed time.

### P2 — Current strict lint gate fails on the installed toolchain

Rust/Cargo here are `1.97.1`. The root manifest declares Rust `1.89` as a minimum, not a pinned toolchain. `./scripts/check-prototype-01.sh` fails on `clippy::manual_is_multiple_of` at `simulation.rs:302` and `:476`. An independent client Clippy invocation stops on the same core dependency errors, so client-specific lint cleanliness is not established.

Suggested correction: use equivalent integer `is_multiple_of` expressions and rerun the strict gates; keep the declared minimum compiler supported. No need to suppress warnings globally or raise MSRV for these two expressions.

### Performance candidates — global snapshot work and redundant static writes

These costs are visible in code; their current frame-time contribution has not been measured in this audit.

- `crates/progressus-app/src/lib.rs:343`: every snapshot, including spatial-only requests, collects all characters, jobs, stockpiles, workstations, orders, logistics, construction sites and structures. Carried-item extraction also scans every item. The lightweight snapshot and a spatial refresh can repeat this work in one tick.
- `crates/progressus-client/src/render.rs:176`: every dirty authority snapshot reconciles workstations and construction. `sync_workstations` and `sync_construction` recreate lookup collections and write unchanged Sprite/Transform components for all such objects, including off-screen objects. Existing workstation/construction revisions are not used to gate these passes.
- `crates/progressus-sim/src/simulation.rs:2515`: available-worker selection scans items for each eligible character, for each candidate job. Hauling and production destination/input searches also contain nested global scans. These deserve representative population/job/item measurements before introducing indexes or concurrency.
- `crates/progressus-client/src/navigation.rs:91`: interpolation allocates a vector of segment lengths per character per frame. A two-pass iterator would avoid that allocation, but this audit does not establish it as a dominant cost.

Prioritize measured snapshot/static-presentation work over broad storage rewrites. The September 5 terrain batching and unexplored-chunk filtering fixes are already present; do not reimplement them.

## Deliberate limits, not bugs to silently remove

- Autonomous food harvesting is hard-coded around the initial clearing. ADR-0018 explicitly accepts Manhattan radius 8 as a bound against accidental scouting. It will need a new accepted policy for relocation/multiple settlements, but changing it now would change gameplay scope.
- Keyboard arrows target bootstrap Cora (`EntityId(3)`), and the render origin is initialized around her. This is bootstrap coupling; ordinary mouse selection/movement is separate. Do not describe it as scalable colony control.
- The raw-chunk residency bound does not bound all explored cells, all persistent modifications or all presentation objects. Full Simulation LOD is deliberately deferred.
- `simulation.rs` has 8,291 lines, including its tests (production code ends before the test module). Extracting need/work modules when those responsibilities grow could help reviewability; file size alone is not evidence of a runtime bottleneck.

## Verification actually performed

- `cargo fmt --all -- --check`: passed as the first acceptance-script stage.
- Strict acceptance script: failed at the two Clippy diagnostics above; later script stages did not execute as part of that invocation.
- `cargo test -p progressus-worldgen -p progressus-sim -p progressus-app -p progressus-headless`: passed, 179 tests total, no failures. CLI tests include deterministic 100k ticks, travel64 and the 100k activity/save-load/residency smoke.
- `cargo check -p progressus-client --all-targets`: passed.
- `cargo clippy -p progressus-client --all-targets -- -D warnings`: blocked by the same core Clippy errors.
- `./scripts/verify-core-dependency-boundary.sh`: passed for headless and client.
- The blocked-output and tick-scheduler reproductions above ran successfully and exhibited the reported defects.

Client test code was typechecked, not linked/executed. No native GPU/frame or audio-device validation was performed. No fresh performance benchmark was run, and no speedup is claimed. Existing September 4 reference measurements were taken on a different machine and are not current measurements for this workstation.

## Plan assessment and proposed next steps

The active milestone has a coherent progression: physical food, rest/shelter, practical skills, metallurgy and knowledge prerequisites. It preserves one physical world and avoids premature civilization-wide systems.

1. Fix the reproduced tick/output defects and restore strict checks in focused commits with regressions.
2. Add the selected local acoustic ambient and bounded work effects as a client-only pass; see the audio specification.
3. Define Stage B precisely: fatigue bounds, sleep interruption/priority relative to food and player orders, one physically constructed sleeping place, its reservation lifecycle and a measurable shelter benefit. Avoid silently introducing a room/roof/weather simulation.
4. Measure active settlement and snapshot costs after each stage. Add increasing population and job/item fixtures before claiming scale readiness; the current five-person and idle reference scenarios cannot prove that.
5. Keep the integrated Prototype 02 long-run scenario growing with each stage, rather than postponing food/sleep/work interaction testing until metallurgy is finished.

Stage B and audio remain unimplemented. Stage A's current completion status was not changed.
