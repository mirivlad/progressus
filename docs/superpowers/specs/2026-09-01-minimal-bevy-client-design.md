# Minimal Bevy Client Design

**Status:** Approved for planning

## 1. Goal

Add the first graphical Progressus client as a deliberately small Bevy 2D
consumer of the existing `progressus-app` boundary. It proves that a new game
can be visualized and controlled without making Bevy state authoritative or
giving the client access to simulation internals.

The client opens a window for seed `0`, renders a local 3x3 chunk terrain
window and the five authoritative characters, and lets the player control Cora
(`EntityId` 3) with the existing movement commands.

## 2. Architecture and dependency boundary

The normative dependency direction after this increment is:

```text
progressus-client -> progressus-app -> progressus-sim -> progressus-worldgen
progressus-headless -> progressus-app
```

- `progressus-client` has exactly two direct dependencies: `progressus-app`
  and Bevy.
- `progressus-client` does not directly depend on `progressus-sim` or
  `progressus-worldgen`; all shared public coordinate, terrain, identity, and
  movement types come from `progressus-app` re-exports.
- No lower crate gains a Bevy dependency. In particular, the headless chain
  remains executable without Bevy, a display server, GPU, window, or audio.
- `Application` remains the only client route for authoritative commands and
  snapshots. The client never mutates a `Character` or simulation storage.

This preserves ADR-0001 INV-005 through INV-007 and ADR-0002: Rust decides
authoritative state; Bevy only presents it.

## 3. Toolchain and Bevy selection

Use stable Bevy `0.18.1`. Its declared Rust version is `1.89.0`, so the
workspace MSRV is raised explicitly from Rust `1.85` to Rust `1.89`.

The client uses `bevy = "0.18.1"` with `default-features = false` and an
explicit native 2D feature set:

- `default_app`, `std`, `bevy_winit`, `x11`, `wayland`, and `multi_threaded`
  for the desktop application/window/input platform;
- `2d_bevy_render` for camera, sprites, and the 2D render path.

It does not enable Bevy's `2d` profile because that profile also enables UI,
scenes, audio, and picking. This increment needs none of those systems. It
also does not use a Bevy prerelease or retain the old MSRV by selecting an
older Bevy release.

## 4. Authoritative interaction and client scheduler

`progressus-client` owns one `Application` initialized with `NewGameOptions`
and seed `0`. Its interaction loop is strictly ordered within a Bevy frame:

1. read input;
2. submit `SetMovementDirection` or `StopMovement` with
   `Application::execute`;
3. if the client cadence is due, submit exactly one
   `AdvanceTicks { count: 1 }`;
4. request a fresh, lightweight character `ClientSnapshot` with no chunks;
5. synchronize characters and derive Cora's authoritative central chunk;
6. only when that central chunk changed, request the radius-one terrain
   snapshot and rebuild the terrain presentation cache.

The presentation-side scheduler has a nominal cadence of four simulation
ticks per second (one tick after approximately 250 ms of client wall-clock
time). It performs at most one authoritative tick in a Bevy frame. After a
long frame, it discards the elapsed backlog rather than catching up with
multiple ticks now or on following frames. Frame delta is never supplied to
the simulation.

This makes an input sampled immediately before a due tick affect that tick.
The scheduler is not deterministic with respect to graphical frame rate or
wall-clock input timing. Determinism remains the existing property of the
authoritative simulation for the same initial state, ordered commands, and
tick count.

## 5. Snapshot-driven presentation

Bevy entities are disposable presentation objects. A client resource maps
stable `EntityId` values to current Bevy `Entity` handles only for rendering.
It is rebuilt from snapshots and is never serialized or sent back to the
application.

Character synchronization consumes `CharacterSnapshot` values:

- create a colored character primitive when an authoritative ID first appears;
- update its transform/color/optional movement indicator from the snapshot;
- despawn its presentation entity when its ID is absent from a later snapshot;
- retain the ID-to-Bevy-entity mapping only as a derived cache.

The selected character is Cora, identified by stable `EntityId::new(3)`, not
by a Bevy entity or a client-side position.

Terrain has a separate presentation root. The client derives Cora's central
chunk from the authoritative `WorldCell` in the lightweight snapshot. It
requests exactly the nine chunks in radius one around it during initial load
and only when that central chunk changes; it does not query terrain each render
frame. Between changes, a disposable terrain presentation cache supplies the
rendered cells. Chunks are rendered as colored cell primitives: grass, water,
and rock have distinct colors. On a change of the central chunk, the terrain
root and its descendants are discarded and rebuilt from the newly requested
snapshot. This is presentation cache replacement, not authoritative chunk
residency or mutation.

The first client renders simple procedural colored shapes only: no authored
assets, sprite sheets, animation interpolation, audio, 3D, egui, selection
framework, or mutable client simulation mirror.

## 6. Camera and controls

The Bevy client creates an orthographic 2D camera. Pan and zoom alter only
camera presentation state. They never affect simulation position, visible-world
generation, command semantics, or tick cadence.

Controls are deliberately narrow:

- Arrow keys: submit `SetMovementDirection` for Cora only on an input edge or
  change of intended direction, with the corresponding cardinal direction.
  Holding an already selected arrow key does not resend that same command each
  render frame because movement direction is authoritative persistent state.
- Stop key: submit `StopMovement` only as its own input event.
- Mouse wheel / simple pan input: camera-only movement.

No movement route, pathfinding result, destination, or future world map is
stored by the client.

If `Application::execute` rejects a movement command because its first step is
blocked, the graphical client logs the error/debug diagnostic and leaves all
presentation state untouched. Its next authoritative snapshot remains the
source of truth; this increment adds no notification UI.

## 7. Testable policy and verification

The client crate keeps snapshot-to-presentation policy in pure, testable code
where possible, without starting Bevy's window or renderer. Coverage must
include:

1. the radius-one visible window contains the expected deterministic 3x3
   chunk coordinates;
2. an unchanged central chunk does not request/rebuild terrain, while moving
   Cora across a chunk boundary selects a new terrain window;
3. character synchronization preserves the stable `EntityId` key while the
   disposable presentation handle may be rebuilt;
4. a snapshot with a missing character produces a removal action, and an
   equivalent later snapshot is idempotent;
5. unchanged persistent-direction input emits no repeated movement command,
   and a rejected command leaves presentation derivation snapshot-driven;
6. the client manifest has no direct simulation/worldgen dependency and the
   dependency-boundary script proves Bevy is absent from the headless chain.

The completion gate is:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p progressus-headless -- --seed 42 --ticks 100000
cargo run -p progressus-headless -- --seed 0 --travel-chunks 64
./scripts/verify-core-dependency-boundary.sh
cargo check -p progressus-client
```

A graphical launch is a separate manual smoke check. It is not made a CI
requirement because automated tests must not require X11, Wayland, GPU, or a
display.

## 8. Explicit non-goals

This increment does not add Bevy below `progressus-client`, general
pathfinding, jobs/AI policy, command queues, selection complexity, crafting,
construction, save/load, chunk residency/cache, mutable chunks, animations,
collision, speed simulation, assets, audio, 3D, or UI tooling.

It advances Prototype 01's playable-client evidence only as a visualization
and input bootstrap. It does not complete the remaining Prototype 01 systems
or redefine the bootstrap movement model as finished navigation.
