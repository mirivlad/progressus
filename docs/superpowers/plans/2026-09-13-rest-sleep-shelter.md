# Rest, Sleep, and Shelter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Prototype 02 Stage B exactly as accepted in ADR-0020: deterministic rest, physical beds, exclusive Sleep jobs, and bounded enclosure-based shelter.

**Architecture:** `progressus-sim` owns rest, jobs, bed occupancy, and the bounded enclosure verdict. `progressus-app` publishes detached state and commands. Bevy presents the bed and rest status without making gameplay decisions. Existing save v1 is extended additively and validated on restore.

**Tech Stack:** Rust workspace, serde save DTOs, `progressus-content` registry, Bevy client, headless simulation tests.

## Global Constraints

- Work on `main`; commit and push each independently verified task.
- ADR-0020 constants: `MAX_REST=100`, `TIRED_REST=30`, decay by `1` every `48` ticks, `SLEEP_WORK_TICKS=64`, enclosure budget `256` cells.
- Bed costs `2 Wood`, is passable at door navigation cost, and never joins the wall network.
- Sleep restoration: enclosed bed to `100`, open bed `+50`, ground `+25`; Eat preempts Sleep.
- Headless simulation remains independent of Bevy. Stable IDs and explicit reservations survive save/load.

---

### Task 1: Rest state and read model

**Files:** `crates/progressus-sim/src/entity.rs`, `crates/progressus-sim/src/simulation/needs.rs`, `crates/progressus-sim/src/simulation.rs`, `crates/progressus-sim/src/simulation/persistence.rs`, `crates/progressus-app/src/read_model.rs`, `crates/progressus-client/src/presentation.rs`.

**Interfaces:** `Character::rest() -> u8`, `Character::is_tired() -> bool`, `Character::decay_rest()`, `Character::restore_rest(u8)`, `CharacterSnapshot::rest: u8`.

- [x] Write focused save-based tests first: at ticks `47/48`, assert rest `100/99`; round-trip a non-full rest; remove `rest` from a v1 character JSON and assert default `100`. The first test failed on absent `rest` before implementation.
- [x] Add constants and the bounded field/accessors to `Character`; call `decay_rest_if_due()` from `advance_ticks()` using tick divisibility by `48`; extend `CharacterSave` with `#[serde(default = "default_rest")]` and reject values over `100`; extend detached `CharacterSnapshot` and client synthetic fixture.
- [x] Run the focused tests green, `cargo check -p progressus-client --all-targets -j 1`, `cargo test -p progressus-sim --lib`, and `cargo test -p progressus-app`; commit and push.

### Task 2: Physical bed construction

**Files:** `crates/progressus-content/src/structure.rs`, `crates/progressus-sim/src/simulation/building.rs`, `crates/progressus-sim/src/simulation/persistence.rs`.

**Interfaces:** `structure::BED: StructureId`; the existing construction command, material delivery, `Structure`, and save DTOs remain the only path to a finished bed.

- [x] Add a red registry test for bed cost/passability/non-connectivity; add a physical delivery and save/load regression for the finished bed.
- [x] Append `bed` to the registry without changing existing IDs. Existing construction and persistence paths already accept the new passable structure; no kind-specific special case was needed.
- [x] Run focused tests, all `progressus-content` tests and `cargo test -p progressus-sim --lib`; commit and push.

### Task 3: Enclosure and Sleep lifecycle

**Files:** `crates/progressus-sim/src/simulation/needs.rs`, `crates/progressus-sim/src/simulation/work.rs`, `crates/progressus-sim/src/simulation.rs`, `crates/progressus-sim/src/job.rs`, `crates/progressus-sim/src/simulation/persistence.rs`.

**Interfaces:** `JobKind::Sleep { character_id, bed_id: Option<EntityId> }`; one reservation index per character and per occupied bed; `is_enclosed(WorldCell) -> bool` is derived and read-only.

- [ ] Write red enclosure tests: closed wall ring true, one-wall gap false, open/closed door as boundary, furniture ignored, rock pocket true, `>256` cells false, no exploration/residency changes. Run `cargo test -p progressus-sim enclosure_`.
- [ ] Implement a cardinal BFS with at most `256` visited passable cells, reading effective terrain by point lookup and completed wall-network structures; never store room IDs or generate resident chunks. Run enclosure tests green.
- [ ] Write red Sleep tests: tired character takes reachable free bed; second character cannot reserve it; no bed yields ground sleep; Eat interrupts Sleep; cancellation, manual order, and route failure release reservations; completion restores the ADR's exact tiers. Run `cargo test -p progressus-sim sleep_`.
- [ ] Add indexed Sleep job creation, assignment, navigation/work completion and cleanup through the existing job lifecycle. Ground sleep has no bed reservation. Keep starvation's existing work block, but not a tiredness work block. Run focused tests green.
- [ ] Add red then green save/load tests for active bed and ground Sleep jobs, canonical continuation and invalid/orphan reservation rejection. Run `cargo test -p progressus-sim --lib`; commit and push.

### Task 4: Client bed and rest UX

**Files:** `crates/progressus-client/src/ui.rs`, `crates/progressus-client/src/i18n.rs`, `crates/progressus-client/src/runtime.rs`, `crates/progressus-client/src/low_poly/scene.rs`, `assets/procedural/low_poly/models.rs`, `docs/client.md`.

**Interfaces:** Build palette's Bed tool submits ordinary `DesignateConstruction { kind: structure::BED }`; `CharacterSnapshot` supplies rest and sleep status. Client never computes enclosure.

- [ ] Write red client tests for Bed tool mapping/localized strings, rest inspector, and deterministic bed presentation selection. Run `cargo test -p progressus-client bed_` and `cargo test -p progressus-client rest_`.
- [ ] Add Bed palette entry, localized labels, a distinct low-poly bed mesh, and rest/sleep inspector state. If completed-sleep shelter status needs a persistent signal, publish it from simulation through the application read model; do not infer it from client geometry.
- [ ] Run focused tests green and `cargo check -p progressus-client --all-targets -j 1`; perform native click/designation and visible sleep smoke; commit and push.

### Task 5: Integration and milestone evidence

**Files:** `crates/progressus-sim/src/simulation/needs.rs`, `crates/progressus-app/tests/client_boundary.rs`, `docs/milestones/prototype-02.md`, `docs/architecture/overview.md`, `README.md`.

- [ ] Add a headless five-person food-and-rest long run and a public app command/snapshot regression that constructs a bed, sleeps, saves during Sleep, reloads, and converges deterministically. This is a Stage B scenario, not a false claim that Stage C–E activity coverage is complete.
- [ ] Run red, implement only missing integration, run green. Check item conservation, stable IDs, bounded residency and save/load.
- [ ] Run `PROGRESSUS_RUN_CLIENT_TESTS=1 ./scripts/check-prototype-01.sh`; inspect full exit code and native evidence. Update docs with proved behavior and explicitly pending GUI or owner-acceptance observations.
- [ ] Run `git diff --check`, commit, push, verify `HEAD == origin/main` and a clean worktree. Report all unverified acceptance items separately.
