# Low-poly client implementation plan

**Goal:** A separately runnable, playable low-poly experiment and integration of the accepted continuous ambient generator.
**Architecture:** An opt-in `low-poly` Cargo feature and `progressus-low-poly` binary in the existing client crate. The experiment consumes only `progressus-app`; it shares fixed-point trace interpolation, tick scheduling and continuous audio. Its camera, input, HUD, mesh caches and procedural models are local to the experiment.
**Tech Stack:** Rust, Bevy 0.18.1, existing application commands/read models.

## Global constraints

- Authoritative simulation, WorldPosition, WorldCell, saves and gameplay remain two-dimensional and unchanged.
- Keep the default 2D executable working without requiring the 3D feature.
- Visual variants are deterministic, bounded and presentation-only.
- Unknown terrain stays unknown; camera queries never discover terrain.
- No decision to replace the current renderer and no transition ADR.
- Work on `codex/low-poly-client`; verify, commit and push completed work.

## Tasks

- [x] Integrate the accepted continuous neutral generator into runtime as a bounded streaming source. Preserve sound-effect observation and volume controls. Share the music plugin with the experiment. Verify continuity, bounded buffering, mute and missing-device behavior using focused tests.
- [x] Create procedural low-poly source under `assets/procedural/low_poly/`: flat shaded trees, rocks, bushes, people, items, workbenches, walls, doors and construction variants. Cache bounded model/mesh/material variants. Verify finite geometry and normals.
- [x] Implement a separate binary and experimental runtime. Reuse the application and trace interpolator. Batch terrain per chunk, reconcile static visuals by exact detached state, retain stable-ID pawn mappings, and rebase integer coordinates before float conversion. Refresh spatial layers only when viewport or corresponding revisions change.
- [x] Implement orthographic pan/zoom, ground-plane ray picking and screen-space pawn selection. Route commands through Application. Add selection/path markers and an experimental HUD with pause, harvest, stockpile, walls, doors, workbench and craft tools; support loading an existing save for evaluating real settlements.
- [x] Add focused tests for signed/far coordinate mapping, picking, hidden terrain, stable cache identity and gameplay command/interpolation integration.
- [x] Run fmt, workspace clippy/check/tests and the headless acceptance script. Build and launch both clients; capture and inspect native visuals, exercise camera/selection/movement/tools and fix issues. Document exact evidence and limitations, launch commands and production implications in `docs/experiments/low-poly-client.md`.

## Result

Implemented and validated. See `docs/experiments/low-poly-client.md` for evidence and limitations. Native review caught inverted prism/gem winding; a failing regression preceded the correction. Final visual-only berry placement was checked with the four procedural-model tests after the full workspace pass. No authoritative source files changed.
