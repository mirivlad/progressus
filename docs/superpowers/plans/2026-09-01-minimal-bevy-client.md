# Minimal Bevy Client Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a small native Bevy 2D client that visualizes and controls the existing authoritative application without giving Bevy ownership of game state.

**Architecture:** `progressus-client` owns an `Application`, sends existing commands, and derives all Bevy entities from detached `ClientSnapshot` values. Pure policy modules calculate visible chunks, character reconciliation, input edges, and the bounded tick scheduler; Bevy systems only execute that policy and create disposable presentation entities. Terrain is requested only on initial synchronization and an authoritative Cora chunk change.

**Tech Stack:** Rust 2024, MSRV Rust 1.89, Bevy 0.18.1 with explicit 2D native features, `progressus-app`.

## Global Constraints

- Workspace `rust-version` is exactly `1.89`; Bevy is exactly `0.18.1`, never a prerelease.
- `progressus-client` has only direct dependencies on `progressus-app` and Bevy; it does not name `progressus-sim` or `progressus-worldgen` in its manifest.
- `progressus-sim`, `progressus-worldgen`, `progressus-app`, and `progressus-headless` remain Bevy-free.
- All authoritative interaction uses `Application::execute` and `Application::snapshot`; client code never mutates a simulation character or storage.
- Cora is selected by `EntityId::new(3)`, never by a Bevy handle or client position.
- Terrain query radius is one chunk (nine chunks normally). Request it only on initial presentation sync or central-chunk change; no terrain snapshot happens at render-frame frequency.
- Input precedes an optional tick, then a lightweight snapshot, then presentation synchronization.
- Nominal cadence is one simulation tick per 250 ms; one frame performs at most one `AdvanceTicks { count: 1 }`, and elapsed excess is discarded.
- The client wall clock and frame rate are not authoritative. The deterministic contract remains identical initial state plus ordered commands plus tick count.
- Automated tests must not create a window, require X11/Wayland, or initialize a GPU.
- No pathfinding, jobs, persistence, residency, mutable chunks, animation, audio, 3D, egui, or lower-layer Bevy dependency is added.

---

## File Structure

| Path | Responsibility |
| --- | --- |
| `Cargo.toml` | Adds client workspace member and raises the explicit workspace MSRV. |
| `crates/progressus-client/Cargo.toml` | Pins the minimal direct client dependencies and binary target. |
| `crates/progressus-client/src/lib.rs` | Declares the client modules and exposes `run()`. |
| `crates/progressus-client/src/main.rs` | Reports a startup failure and calls `progressus_client::run()`. |
| `crates/progressus-client/src/presentation.rs` | Pure visible-window and snapshot reconciliation policy, with no window startup. |
| `crates/progressus-client/src/interaction.rs` | Pure 250 ms scheduler and edge-triggered movement-command selection. |
| `crates/progressus-client/src/runtime.rs` | Owns `Application` and applies the required input/tick/snapshot ordering. |
| `crates/progressus-client/src/render.rs` | Owns disposable Bevy entities, terrain cache rebuilds, character visuals, and camera controls. |
| `scripts/verify-core-dependency-boundary.sh` | Checks the headless chain and direct client dependency boundary. |
| `README.md`, `docs/architecture/overview.md`, `docs/milestones/prototype-01.md` | Record the actual minimal-client capability and remaining Prototype 01 work. |

## Task 1: Add the client crate and pure presentation policy

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/progressus-client/Cargo.toml`
- Create: `crates/progressus-client/src/lib.rs`
- Create: `crates/progressus-client/src/presentation.rs`

**Interfaces:**
- Consumes: `progressus_app::{CharacterSnapshot, ChunkCoord, ClientSnapshot, EntityId}`.
- Produces: `controlled_character`, `VisibleChunkWindow::around`, `terrain_refresh_needed`, and `character_sync_actions` for the Bevy runtime.

- [ ] **Step 1: Write failing presentation-policy tests**

Create `crates/progressus-client/src/presentation.rs` with these tests before implementation:

```rust
#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use progressus_app::{CharacterSnapshot, ChunkCoord, MovementState, WorldCell};

    use super::{
        character_sync_actions, terrain_refresh_needed, CharacterSyncAction,
        VisibleChunkWindow,
    };

    #[test]
    fn radius_one_window_is_row_major_three_by_three() {
        let window = VisibleChunkWindow::around(ChunkCoord::new(4, -2)).unwrap();
        assert_eq!(window.coordinates(), &[
            ChunkCoord::new(3, -3), ChunkCoord::new(4, -3), ChunkCoord::new(5, -3),
            ChunkCoord::new(3, -2), ChunkCoord::new(4, -2), ChunkCoord::new(5, -2),
            ChunkCoord::new(3, -1), ChunkCoord::new(4, -1), ChunkCoord::new(5, -1),
        ]);
    }

    #[test]
    fn terrain_rebuild_happens_only_for_initial_or_changed_center() {
        let center = ChunkCoord::new(0, 0);
        assert!(terrain_refresh_needed(None, center));
        assert!(!terrain_refresh_needed(Some(center), center));
        assert!(terrain_refresh_needed(Some(center), ChunkCoord::new(1, 0)));
    }

    #[test]
    fn reconciliation_uses_stable_ids_and_removes_only_missing_ids() {
        let rendered = BTreeSet::from([
            progressus_app::EntityId::new(3).unwrap(),
            progressus_app::EntityId::new(8).unwrap(),
        ]);
        let snapshots = vec![CharacterSnapshot {
            id: progressus_app::EntityId::new(3).unwrap(),
            name: "Cora".to_owned(),
            position: WorldCell::new(32, 0),
            movement: MovementState::Idle,
        }];

        assert_eq!(
            character_sync_actions(&rendered, &snapshots),
            vec![
                CharacterSyncAction::Update(snapshots[0].clone()),
                CharacterSyncAction::Despawn(progressus_app::EntityId::new(8).unwrap()),
            ],
        );
    }
}
```

- [ ] **Step 2: Run the focused test to verify it fails**

Run: `cargo test -p progressus-client presentation`

Expected: Cargo reports that package `progressus-client` does not exist.

- [ ] **Step 3: Add the manifest, workspace member, and minimal policy implementation**

Update the workspace members and package floor:

```toml
members = [
    "crates/progressus-app",
    "crates/progressus-client",
    "crates/progressus-headless",
    "crates/progressus-sim",
    "crates/progressus-worldgen",
]

[workspace.package]
edition = "2024"
rust-version = "1.89"
```

Create `crates/progressus-client/Cargo.toml`:

```toml
[package]
name = "progressus-client"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
bevy = { version = "0.18.1", default-features = false, features = [
    "2d_bevy_render",
    "bevy_winit",
    "default_app",
    "multi_threaded",
    "std",
    "wayland",
    "x11",
] }
progressus-app = { path = "../progressus-app" }

[[bin]]
name = "progressus-client"
path = "src/main.rs"

[lints]
workspace = true
```

Create the crate roots:

```rust
// crates/progressus-client/src/lib.rs
pub mod presentation;
```

Implement the complete pure policy API in `presentation.rs`:

```rust
use std::collections::{BTreeMap, BTreeSet};

use progressus_app::{CharacterSnapshot, ChunkCoord, EntityId};

pub const VISIBLE_CHUNK_RADIUS: i64 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresentationError {
    VisibleWindowOutOfRange { center: ChunkCoord },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisibleChunkWindow {
    center: ChunkCoord,
    coordinates: Vec<ChunkCoord>,
}

impl VisibleChunkWindow {
    pub fn around(center: ChunkCoord) -> Result<Self, PresentationError> {
        let mut coordinates = Vec::with_capacity(9);
        for y_offset in -VISIBLE_CHUNK_RADIUS..=VISIBLE_CHUNK_RADIUS {
            let y = center.y().checked_add(y_offset).ok_or(
                PresentationError::VisibleWindowOutOfRange { center },
            )?;
            for x_offset in -VISIBLE_CHUNK_RADIUS..=VISIBLE_CHUNK_RADIUS {
                let x = center.x().checked_add(x_offset).ok_or(
                    PresentationError::VisibleWindowOutOfRange { center },
                )?;
                coordinates.push(ChunkCoord::new(x, y));
            }
        }
        Ok(Self { center, coordinates })
    }

    pub const fn center(&self) -> ChunkCoord { self.center }
    pub fn coordinates(&self) -> &[ChunkCoord] { &self.coordinates }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CharacterSyncAction {
    Spawn(CharacterSnapshot),
    Update(CharacterSnapshot),
    Despawn(EntityId),
}

pub fn controlled_character(characters: &[CharacterSnapshot]) -> Option<&CharacterSnapshot> {
    let cora = EntityId::new(3)?;
    characters.iter().find(|character| character.id == cora)
}

pub fn terrain_refresh_needed(rendered: Option<ChunkCoord>, current: ChunkCoord) -> bool {
    rendered != Some(current)
}

pub fn character_sync_actions(
    rendered: &BTreeSet<EntityId>,
    characters: &[CharacterSnapshot],
) -> Vec<CharacterSyncAction> {
    let authoritative = characters.iter().cloned().map(|character| (character.id, character))
        .collect::<BTreeMap<_, _>>();
    let mut actions = Vec::new();
    for (id, character) in &authoritative {
        actions.push(if rendered.contains(id) {
            CharacterSyncAction::Update(character.clone())
        } else {
            CharacterSyncAction::Spawn(character.clone())
        });
    }
    for id in rendered {
        if !authoritative.contains_key(id) {
            actions.push(CharacterSyncAction::Despawn(*id));
        }
    }
    actions
}
```

`VisibleChunkWindow::around` reports coordinate overflow instead of wrapping.
The runtime will log this presentation failure and retain its previous cache.

- [ ] **Step 4: Run focused tests and check the client crate**

Run:

```bash
cargo test -p progressus-client presentation
cargo check -p progressus-client
```

Expected: the three presentation tests pass; the crate compiles without opening a window.

- [ ] **Step 5: Commit the policy and dependency setup**

```bash
git add Cargo.toml Cargo.lock crates/progressus-client
git commit -m "feat: add Bevy client presentation policy"
```

## Task 2: Add edge-triggered input and the bounded client tick scheduler

**Files:**
- Create: `crates/progressus-client/src/interaction.rs`

**Interfaces:**
- Consumes: Bevy `ButtonInput<KeyCode>`, `std::time::Duration`, and public `progressus_app::{Command, Direction, EntityId}`.
- Produces: `TickScheduler::advance` and `movement_command` used before snapshot refresh in `runtime::advance_authority`.

- [ ] **Step 1: Write the failing interaction tests**

Add these tests to `interaction.rs`:

```rust
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::prelude::{ButtonInput, KeyCode};
    use progressus_app::{Command, Direction, EntityId};

    use super::{movement_command, TickScheduler};

    #[test]
    fn scheduler_emits_one_tick_and_discards_long_frame_backlog() {
        let mut scheduler = TickScheduler::default();
        assert!(!scheduler.advance(Duration::from_millis(249)));
        assert!(scheduler.advance(Duration::from_millis(1)));
        assert!(!scheduler.advance(Duration::ZERO));
        assert!(scheduler.advance(Duration::from_secs(3)));
        assert!(!scheduler.advance(Duration::ZERO));
    }

    #[test]
    fn direction_is_sent_on_press_edge_not_on_held_frame() {
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::ArrowRight);
        assert_eq!(
            movement_command(&keys, EntityId::new(3).unwrap()),
            Some(Command::SetMovementDirection {
                character_id: EntityId::new(3).unwrap(),
                direction: Direction::East,
            }),
        );
        keys.clear_just_pressed(KeyCode::ArrowRight);
        assert_eq!(movement_command(&keys, EntityId::new(3).unwrap()), None);
    }

    #[test]
    fn stop_event_has_priority_over_direction_event() {
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::Space);
        keys.press(KeyCode::ArrowUp);
        assert_eq!(
            movement_command(&keys, EntityId::new(3).unwrap()),
            Some(Command::StopMovement { character_id: EntityId::new(3).unwrap() }),
        );
    }
}
```

- [ ] **Step 2: Run the focused interaction test to verify it fails**

Run: `cargo test -p progressus-client interaction`

Expected: FAIL because `interaction` and its exported types do not yet exist.

- [ ] **Step 3: Implement the bounded scheduler and fixed-priority input mapping**

Implement `interaction.rs` as follows:

```rust
use std::time::Duration;

use bevy::prelude::{ButtonInput, KeyCode};
use progressus_app::{Command, Direction, EntityId};

pub const TICK_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Default)]
pub struct TickScheduler {
    elapsed: Duration,
}

impl TickScheduler {
    pub fn advance(&mut self, frame_delta: Duration) -> bool {
        self.elapsed = self.elapsed.saturating_add(frame_delta);
        if self.elapsed < TICK_INTERVAL {
            return false;
        }
        self.elapsed = Duration::ZERO;
        true
    }
}

pub fn movement_command(keys: &ButtonInput<KeyCode>, character_id: EntityId) -> Option<Command> {
    if keys.just_pressed(KeyCode::Space) {
        return Some(Command::StopMovement { character_id });
    }
    let direction = if keys.just_pressed(KeyCode::ArrowRight) {
        Some(Direction::East)
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        Some(Direction::North)
    } else if keys.just_pressed(KeyCode::ArrowDown) {
        Some(Direction::South)
    } else if keys.just_pressed(KeyCode::ArrowLeft) {
        Some(Direction::West)
    } else {
        None
    }?;
    Some(Command::SetMovementDirection { character_id, direction })
}
```

The `Duration::ZERO` reset is the explicit no-catch-up rule. `just_pressed`
implements persistent-direction input without repeated per-frame commands.

- [ ] **Step 4: Run interaction tests and lint the new code**

Run:

```bash
cargo test -p progressus-client interaction
cargo clippy -p progressus-client --all-targets -- -D warnings
```

Expected: all three tests pass and Clippy emits no warning.

- [ ] **Step 5: Commit scheduler/input semantics**

```bash
git add crates/progressus-client/src/interaction.rs
git commit -m "feat: add bounded client tick scheduler"
```

## Task 3: Run the Bevy client through snapshots and disposable rendering entities

**Files:**
- Modify: `crates/progressus-client/src/lib.rs`
- Create: `crates/progressus-client/src/main.rs`
- Create: `crates/progressus-client/src/runtime.rs`
- Create: `crates/progressus-client/src/render.rs`

**Interfaces:**
- Consumes: Task 1 `VisibleChunkWindow`, `terrain_refresh_needed`, `character_sync_actions`, and `controlled_character`; Task 2 `TickScheduler` and `movement_command`.
- Produces: `progressus_client::run() -> Result<(), ClientError>`, with a native window, colored terrain/characters, camera controls, and command/tick/snapshot system ordering.

- [ ] **Step 1: Write a failing no-window runtime contract test**

Add to `runtime.rs` a constructor test that does not call `App::run`:

```rust
#[cfg(test)]
mod tests {
    use progressus_app::EntityId;

    use super::AuthoritativeClient;

    #[test]
    fn new_game_exposes_cora_in_lightweight_snapshot() {
        let client = AuthoritativeClient::new().unwrap();
        assert!(client
            .snapshot()
            .characters
            .iter()
            .any(|character| character.id == EntityId::new(3).unwrap()));
        assert!(client.snapshot().chunks.is_empty());
    }
}
```

- [ ] **Step 2: Run the focused runtime test to verify it fails**

Run: `cargo test -p progressus-client runtime::tests::new_game_exposes_cora_in_lightweight_snapshot`

Expected: FAIL because `AuthoritativeClient` is not defined.

- [ ] **Step 3: Implement authoritative ordering and presentation synchronization**

Implement `AuthoritativeClient` in `runtime.rs` with exactly these fields and methods:

```rust
#[derive(Resource)]
pub(crate) struct AuthoritativeClient {
    application: Application,
    snapshot: ClientSnapshot,
    snapshot_dirty: bool,
}

impl AuthoritativeClient {
    pub(crate) fn new() -> Result<Self, ClientError> {
        let application = Application::new_game(NewGameOptions { seed: WorldSeed::new(0) })?;
        let snapshot = application.snapshot(SnapshotQuery::default())?;
        Ok(Self { application, snapshot, snapshot_dirty: true })
    }

    pub(crate) fn snapshot(&self) -> &ClientSnapshot { &self.snapshot }

    pub(crate) fn terrain_snapshot(
        &self,
        chunks: Vec<ChunkCoord>,
    ) -> Result<ClientSnapshot, ClientError> {
        Ok(self.application.snapshot(SnapshotQuery { chunks })?)
    }

    pub(crate) fn take_snapshot_dirty(&mut self) -> bool {
        std::mem::take(&mut self.snapshot_dirty)
    }

    fn refresh_lightweight_snapshot(&mut self) -> Result<(), ClientError> {
        self.snapshot = self.application.snapshot(SnapshotQuery::default())?;
        self.snapshot_dirty = true;
        Ok(())
    }
}
```

Define the selected identity once in the same module, without a Bevy handle:

```rust
pub(crate) fn cora_id() -> EntityId {
    EntityId::new(3).expect("3 is a valid nonzero Progressus entity ID")
}
```

Add an `advance_authority` `Update` system that executes in this exact order:

```rust
let mut command_attempted = false;
if let Some(command) = movement_command(&keys, cora_id()) {
    command_attempted = true;
    if let Err(error) = authoritative.application.execute(command) {
        warn!("movement command rejected: {error}");
    }
}
let tick_due = scheduler.advance(time.delta());
if tick_due {
    if let Err(error) = authoritative.application.execute(Command::AdvanceTicks { count: 1 }) {
        error!("authoritative tick failed: {error}");
        return;
    }
}
if command_attempted || tick_due {
    if let Err(error) = authoritative.refresh_lightweight_snapshot() {
        error!("authoritative snapshot failed: {error}");
    }
}
```

Rejected movement is logged but does not alter presentation cache directly;
the lightweight snapshot refresh is still requested because a command was
attempted.

In `render.rs`, define:

```rust
#[derive(Component)]
pub(crate) struct TerrainRoot;

#[derive(Component)]
pub(crate) struct CharacterVisual {
    pub(crate) id: EntityId,
}

#[derive(Resource, Default)]
pub(crate) struct PresentationCache {
    pub(crate) central_chunk: Option<ChunkCoord>,
    pub(crate) terrain_root: Option<Entity>,
    pub(crate) characters: BTreeMap<EntityId, Entity>,
}
```

Make `sync_presentation` return immediately when `snapshot_dirty` is false.
When it is true, reconcile `CharacterVisual` entities from
`character_sync_actions`, derive Cora's `position.split().0`, and compare it
with `PresentationCache::central_chunk` using `terrain_refresh_needed`. Only
on `true`, build `VisibleChunkWindow::around(center)`, call
`Application::snapshot(SnapshotQuery { chunks: window.coordinates().to_vec() })`,
despawn the former `TerrainRoot`, and spawn a new root with 32x32 colored
`Sprite::from_color` cell children for each returned chunk. Store the new root
and center only after the terrain snapshot succeeds.

Use `Sprite::from_color(color, Vec2::splat(CELL_SIZE))`; terrain colors are a
fixed `match` on `Terrain::{Grass, Water, Rock}`. Use a brighter fixed color
and a larger Z value for characters. Convert world coordinates to Bevy floats
relative to the lower-left `WorldCell` of the central chunk, calculated from
public `ChunkCoord::world_cell(LocalCell::new(0, 0))`. This keeps the local
visual window usable without treating Bevy's finite-precision transforms as
authoritative world coordinates.

Create `setup_camera` with `commands.spawn(Camera2d)`. Add a separate
presentation-only camera system: `W/A/S/D` pans by a constant times
`Time::delta_secs()`, and mouse-wheel events change orthographic scale within
`0.25..=8.0`. It does not call `Application`.

Wire the app in `run()`:

```rust
pub fn run() -> Result<(), ClientError> {
    App::new()
        .insert_resource(AuthoritativeClient::new()?)
        .insert_resource(TickScheduler::default())
        .insert_resource(PresentationCache::default())
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Progressus — Prototype 01".to_owned(),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup_camera)
        .add_systems(Update, (advance_authority, sync_presentation, camera_controls).chain())
        .run();
    Ok(())
}
```

`ClientError` wraps `ApplicationError` and `PresentationError` with `Display`,
`Error`, and `From` implementations. `sync_presentation` logs a presentation
error and retains the previous terrain cache if a coordinate edge cannot form
a full window.

Replace `lib.rs` with the complete runtime surface and create the binary entry
point only in this task:

```rust
// crates/progressus-client/src/lib.rs
mod interaction;
pub mod presentation;
mod render;
mod runtime;

pub use runtime::{run, ClientError};
```

```rust
// crates/progressus-client/src/main.rs
fn main() {
    if let Err(error) = progressus_client::run() {
        eprintln!("progressus-client: {error}");
        std::process::exit(1);
    }
}
```

- [ ] **Step 4: Run no-window tests and compile the graphical target**

Run:

```bash
cargo test -p progressus-client
cargo check -p progressus-client
```

Expected: policy, scheduler, and constructor tests pass without opening a window; the Bevy binary type-checks.

- [ ] **Step 5: Commit the minimal client runtime**

```bash
git add crates/progressus-client/src/lib.rs crates/progressus-client/src/main.rs \
  crates/progressus-client/src/runtime.rs crates/progressus-client/src/render.rs
git commit -m "feat: render Progressus snapshots in Bevy"
```

## Task 4: Guard the dependency direction and document only achieved client behavior

**Files:**
- Modify: `scripts/verify-core-dependency-boundary.sh`
- Modify: `README.md`
- Modify: `docs/architecture/overview.md`
- Modify: `docs/milestones/prototype-01.md`

**Interfaces:**
- Consumes: the client manifest introduced in Task 1 and observed client behavior from Task 3.
- Produces: a repeatable guard that checks both sides of the boundary and documentation that describes a bootstrap client without overstating Prototype 01 completion.

- [ ] **Step 1: Write the failing direct-dependency guard check**

Amend `scripts/verify-core-dependency-boundary.sh` before its success message:

```bash
client_direct_tree="$(cargo tree -p progressus-client --depth 1 --edges normal,build,dev --prefix none)"

if ! grep -Eq '^bevy v' <<<"${client_direct_tree}"; then
    echo "error: progressus-client must depend on Bevy" >&2
    exit 1
fi

if grep -Eq '^progressus-(sim|worldgen) v' <<<"${client_direct_tree}"; then
    echo "error: progressus-client directly depends on a lower authoritative crate" >&2
    echo "${client_direct_tree}" >&2
    exit 1
fi
```

Run: `./scripts/verify-core-dependency-boundary.sh`

Expected before Task 1 is implemented: the command cannot resolve `progressus-client`; after Task 1 it must be used as the regression guard.

- [ ] **Step 2: Finish the guard and run it successfully**

Keep the existing complete `progressus-headless` tree scan for any Bevy package. Print both success facts:

```bash
echo "headless dependency boundary: no Bevy packages in the application chain"
echo "client dependency boundary: direct dependencies are Bevy and progressus-app only"
```

Run: `./scripts/verify-core-dependency-boundary.sh`

Expected: exit 0 and both boundary messages.

- [ ] **Step 3: Update documentation with bounded claims**

Make these exact documentation claims:

- In `README.md`, list the native command `cargo run -p progressus-client`, state that it opens a seed-0 2D visual bootstrap with a 3x3 Cora-centered terrain window, five snapshot-driven characters, arrow/Stop movement input, and presentation-only pan/zoom. State that the manual graphical smoke requires a local display/GPU.
- In `docs/architecture/overview.md`, replace the obsolete statement that the bootstrap lacks a visual client. Record the exact five-crate dependency graph, the no-direct-lower-crate rule, lightweight-versus-terrain snapshot flow, and the non-authoritative 4 Hz no-catch-up scheduler.
- In `docs/milestones/prototype-01.md`, add a **Partially advanced (bootstrap)** note under UI requirements: this client proves rendering/input and normal chunk-window refresh, but does not satisfy the remaining interface, navigation, jobs, persistence, residency, resource, construction, or save/load criteria. Do not mark P01-SIM-04, TEST-P01-03, or the full Prototype 01 checklist complete.

- [ ] **Step 4: Check documentation links and run the dependency guard**

Run:

```bash
git diff --check
./scripts/verify-core-dependency-boundary.sh
```

Expected: no whitespace errors; both dependency assertions pass.

- [ ] **Step 5: Commit the guard and documentation**

```bash
git add scripts/verify-core-dependency-boundary.sh README.md \
  docs/architecture/overview.md docs/milestones/prototype-01.md
git commit -m "docs: record minimal Bevy client bootstrap"
```

## Task 5: Run the full verification gate and manual graphical smoke

**Files:**
- Verify only; do not change files unless a concrete gate failure requires its own new TDD task.

**Interfaces:**
- Consumes: the completed workspace, headless scenario, dependency guard, and client binary.
- Produces: evidence for the final report; no claim that GUI smoke was automated.

- [ ] **Step 1: Format and run static checks**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: both commands exit 0.

- [ ] **Step 2: Run all non-graphical automated tests**

Run: `cargo test --workspace`

Expected: all existing worldgen/application/simulation/headless tests and new client policy tests pass without a window.

- [ ] **Step 3: Re-run the authoritative headless scenarios and dependency guard**

Run:

```bash
cargo run -p progressus-headless -- --seed 42 --ticks 100000
cargo run -p progressus-headless -- --seed 0 --travel-chunks 64
./scripts/verify-core-dependency-boundary.sh
cargo check -p progressus-client
```

Expected: both headless scenarios retain their current deterministic behavior; the travel scenario crosses 64 positive boundaries; the guard reports no Bevy below the client; the client checks successfully.

- [ ] **Step 4: Perform a separate manual graphical smoke check**

Run: `cargo run -p progressus-client`

Verify visually on a machine with a display and GPU:

1. a window opens with colored grass, water, and rock cells;
2. all five characters appear;
3. arrow input commands Cora through `progressus-app` and she advances only on ticks;
4. Cora's authoritative crossing into a neighboring chunk causes a new 3x3 terrain window;
5. camera pan/zoom changes only the presentation;
6. a blocked movement command logs without crashing or manually moving a character.

Expected: the visual checks pass. If the environment has no display/GPU, record this as unperformed manual verification rather than altering automated gates.

- [ ] **Step 5: Inspect final state before integration**

Run:

```bash
git status --short --branch
git log --oneline origin/main..HEAD
```

Expected: only intentional commits are ahead; no untracked or unstaged files remain.

## Plan Self-Review

- **Spec coverage:** Task 1 covers Bevy 0.18.1/MSRV/direct dependency and snapshot-only presentation policy. Task 2 covers edge-triggered persistent-direction commands and the one-tick, no-catch-up scheduler. Task 3 covers seed 0, Cora ID 3, the required ordering, 3x3 terrain cache behavior, simple colors, character mapping, and presentation-only camera. Task 4 covers the normative guard and bounded documentation. Task 5 covers every requested automated gate and separate manual smoke.
- **No scope expansion:** The plan adds neither an authoritative API nor simulation behavior; it uses existing commands, snapshots, terrain data, and coordinate types. It explicitly excludes the requested future systems.
- **Type consistency:** `VisibleChunkWindow`, `CharacterSyncAction`, `TickScheduler`, and `movement_command` are produced before `runtime`/`render` consume them. All simulation-facing types are imported from `progressus-app`.
- **Placeholder scan:** No task relies on an unspecified interface or a deferred implementation; each code task includes concrete types, commands, expected failures, and expected passes.
