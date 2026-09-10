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

- [ ] Linux launch: add `scripts/run-client.sh` and dependency bootstrap; detect missing pkg-config packages and install only required distro packages before Cargo. Verify with isolated fake package-manager tests without modifying host packages.
- [ ] Client integration: make 3D the default executable; adapt existing runtime pointer ray conversion, camera, UI and modal load invalidation. Preserve grouped toolbar, area drag, stockpile inspector and configuration, workstation production and port direction controls, save/load, sound/music settings, pause, localization, selection and navigation. Run client tests and real GUI interactions.
- [ ] Scene parity: replace old sprite renderer with mesh renderer, projected stack labels, selection/drag grid, work designations and zone overlays. Adapt sound cue admission to projected 3D positions. Verify cache stability, load/rebase and no authoritative mutation.
- [ ] Terrain: raise connected rocky terrain above resources, facet/round exposed corners and add sandy shore transition from known terrain, with coherent chunk edges and no hidden-terrain access. Test mesh normals and neighboring chunk geometry.
- [ ] Construction: wall connections include blueprints and doors; orientation follows neighbor topology and workstation ports. Preserve authoritative footprint, construction and door behavior. Test straight/corner/junction masks and inspect visuals.
- [ ] Cleanup and delivery: remove obsolete 2D runtime rendering/experimental entry point and outdated current documentation; keep useful historical evidence. Run fmt, workspace tests, strict clippy, boundary/prototype gate and native UI smoke. Commit/push logical completed stages to main, verify remote HEAD, remove merged experiment branch after acceptance.

## Verification ledger

Pending implementation. Existing baseline is f4093e2; main promotion authorized by owner on 2026-09-10.
