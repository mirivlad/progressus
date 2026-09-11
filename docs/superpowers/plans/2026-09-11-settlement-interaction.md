# Settlement Interaction Implementation Plan

> **For agentic workers:** Use subagent-driven-development and test-driven-development. Implement tasks in order; independent read-only audits may run concurrently.

**Goal:** Make inventory, carts, construction preparation and physical resource inspection usable and verifiable.

**Architecture:** Rust simulation owns all state/transfers/jobs. Application commands and detached snapshots carry player intent and information. Bevy owns input, inspection and visuals only.

**Tech Stack:** Rust workspace, Bevy, existing JSON save DTOs.

## Global Constraints

- Authoritative simulation must not depend on Bevy.
- Persistent references use stable Progressus IDs.
- Physical goods move through the world and have exactly one location.
- Unknown objects remain hidden; camera and presentation never discover cells.
- Keep scope within Prototype 02 and accepted ADRs; no automatic mobile stockpiles.
- Run focused regressions and strict final gates; commit and push each completed stage.

## Task 1: Inventory authority and application boundary

Files: simulation item operations, item_world.rs, simulation/work.rs, persistence.rs,
app/lib.rs and read_model.rs, relevant simulation/application tests.

- [ ] Reproduce cart logistics, release-index, equipment-drop and malformed-save failures with focused tests.
- [ ] Enforce holder/access/capacity/slot semantics at runtime and save boundaries.
- [x] Expose explicit commands and detached inventory/container snapshots; reject reserved mutation without side effects.
- [ ] Prove ordinary cart transport, interruption and deterministic save/load.
- [ ] Strengthen mining test to assert actual ore production and physical delivery, including crafted tool prerequisites.
- [x] Run `cargo test -p progressus-sim -p progressus-app -p progressus-content` and review diff.

## Task 2: Inventory client and resource information

Files: client inventory/resource UI module, runtime.rs, ui.rs, low_poly/scene.rs,
localization and application boundary fixtures.

- [x] Add localized inventory and source/item inspection text from snapshots.
- [x] Add reachable item/container actions, visible rejection feedback, click selection and hover identification.
- [ ] Preserve active tool actions and existing selection priorities; clear stale selection on load.
- [ ] Show equipped items in presentation, including parked/borne carts and their contents in the inspector.
- [x] Test selection/text/actions and run client checks plus executable client tests when feasible.
- [ ] Review and publish the complete inventory/resource interaction stage.

## Status after the inventory and boundary pass

Player-facing inventory is implemented end to end and covered by the gate,
including the heavy client link. What the remaining Task 1/2 bullets still
owe, and why they are not ticked:

- save boundaries do not yet re-validate location, slot and capacity rules,
  and no malformed-save regression exists;
- cart transport is proven through completion but not through interruption
  or a mid-haul save/load;
- the mining test still does not assert actual ore production and physical
  delivery behind a crafted tool;
- equipped items are not drawn on the character in the 3D scene — a parked
  cart renders as an ordinary ground stack, and its contents appear only in
  the inspector;
- stale selection is not explicitly cleared on load.

## Task 3: Construction preparation

Files: construction.rs, simulation/building.rs, work.rs, job.rs, persistence.rs,
world resource queries, app snapshots, client status text, ADR-0009/0022.

- [ ] Add failing tree/stack/pawn designation and completion regressions.
- [ ] Implement explicit site-owned preparation, physical clearing jobs and late occupancy checks.
- [ ] Preserve existing resources and layer-claim protection; wait visibly for tools/routes/destination.
- [ ] Test cancellation, unrelated jobs, full loads, renewable sources, blocked paths, and mid-job save/load conservation.
- [ ] Cover workbench designation using the smallest compatible deferred placement path.
- [ ] Review authority, integrate status UI and publish verified stage.

## Task 4: Documentation and acceptance

Files: README.md, architecture/overview.md, milestones/prototype-02.md, docs/client.md,
accepted ADR amendments and this progress checklist.

- [ ] Replace outdated flat inventory/2D/completion claims with current behavior and explicit remaining work.
- [ ] Run `PROGRESSUS_RUN_CLIENT_TESTS=1 ./scripts/check-prototype-01.sh` if linking is feasible; otherwise run default gate and report limitation.
- [ ] Review whole change, resolve material findings, perform native UI smoke if available.
- [ ] Commit/push, integrate verified branch into main with fast-forward and verify remote head.

## Subsequent milestone work

Sleep and shelter remain the next subsystem, governed by ADR-0020. Do not silently
implement proposed foundational decisions as part of this interaction pass.
