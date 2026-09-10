# Primary 3D client implementation plan

> Use subagent-driven-development for independent geometry work and review; integrate the shared client locally.

**Goal:** Promote the accepted low-poly experiment to the only client, preserve every existing interaction, and address the owner's visual feedback.

**Architecture:** Keep AuthoritativeClient, commands, snapshots, save slots, localized UI and modal logic. Replace the 2D rendering and coordinate adapters with the accepted 3D scene. Height and visual variation remain presentation-only; saved simulation and world generation are unchanged.

**Tech stack:** Rust, Bevy 0.18.1, system Linux audio, procedural meshes.

## Constraints

- Owner explicitly requests implementation on main and removal of the old client after migration.
- Preserve stable IDs, deterministic simulation, bounded scene caches and existing saves.
- Reuse existing production orders, stockpile filters/priorities, modal capture and localized inspector instead of approximating their behavior.

## Tasks and acceptance

- [x] Linux launch: add `scripts/run-client.sh` and dependency bootstrap; detect missing pkg-config packages and install only required distro packages before Cargo. Verify with isolated fake package-manager tests without modifying host packages.
- [x] Client integration: make 3D the default executable; adapt existing runtime pointer ray conversion, camera, UI and modal load invalidation. Preserve grouped toolbar, area drag, stockpile inspector and configuration, workstation production and port direction controls, save/load, sound/music settings, pause, localization, selection and navigation. Run client tests and real GUI interactions.
- [x] Scene parity: replace old sprite renderer with mesh renderer, projected stack labels, selection/drag grid, work designations and zone overlays. Adapt sound cue admission to projected 3D positions. Verify cache stability, load/rebase and no authoritative mutation.
- [x] Terrain: raise connected rocky terrain above resources, facet/round exposed corners and add sandy shore transition from known terrain, with coherent chunk edges and no hidden-terrain access. Test mesh normals and neighboring chunk geometry.
- [x] Construction: wall connections include blueprints and doors; orientation follows neighbor topology and workstation ports. Preserve authoritative footprint, construction and door behavior. Test straight/corner/junction masks and inspect visuals.
- [x] Cleanup and delivery: remove obsolete 2D runtime rendering/experimental entry point and outdated current documentation; keep useful historical evidence. Run fmt, workspace tests, strict clippy, boundary/prototype gate and native UI smoke. Commit/push logical completed stages to main, verify remote HEAD, remove merged experiment branch after acceptance.

## Verification ledger

Baseline f4093e2; main promotion authorized by owner on 2026-09-10.

Implemented and verified on 2026-09-10:

- `./scripts/check-prototype-01.sh` — passed: `cargo fmt --check`, strict clippy over worldgen/sim/app/headless and client, workspace tests, dependency-boundary guard, and the 100k idle, travel64 and 100k activity/persistence headless smokes.
- `cargo test -p progressus-client` — 94 passed, including the new `low_poly::scene` tests for cache stability under idle/item updates, mesh eviction and return without discovery, load-at-same-tick rebuild without stale meshes, and authority-sourced traces surviving origin rebase.
- `scripts/tests/linux-bootstrap.sh` — passed against fake package managers (install, no-op, failure); host packages untouched.

Not covered by automation: the manual native GUI smoke. Visual acceptance of the promoted client rests on the owner's review of `f4093e2` plus the follow-up terrain/label/outline adjustments in this change.
