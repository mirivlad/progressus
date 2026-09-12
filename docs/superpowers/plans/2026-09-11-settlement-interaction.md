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
- [x] Strengthen mining test to assert actual ore production and physical delivery, including crafted tool prerequisites (remote-source exploration is staged in the headless fixture).
- [x] Run `cargo test -p progressus-sim -p progressus-app -p progressus-content` and review diff.

## Task 2: Inventory client and resource information

Files: client inventory/resource UI module, runtime.rs, ui.rs, low_poly/scene.rs,
localization and application boundary fixtures.

- [x] Add localized inventory and source/item inspection text from snapshots.
- [x] Add reachable item/container actions, visible rejection feedback, click selection and hover identification.
- [x] Make selected-character right-click on a ground tool/cart create a persisted physical fetch-and-equip job, while ordinary ground right-click remains `MoveTo`.
- [ ] Preserve active tool actions and existing selection priorities; clear stale selection on load.
- [ ] Show equipped items in presentation, including parked/borne carts and their contents in the inspector.
- [x] Test selection/text/actions and run client checks plus executable client tests when feasible.
- [ ] Review and publish the complete inventory/resource interaction stage.

### Selection-priority regression

- [ ] Extract the cell-level selectable choice into a pure helper in
  `crates/progressus-client/src/runtime.rs` and first add a failing test proving
  that a ground `Cart` on a stockpile cell resolves to the item inspector rather
  than `SelectedStockpile`.
- [ ] Make the helper choose workstation, character, inspectable ground object,
  then stockpile in Select mode; keep the existing screen-body pawn hit first.
- [ ] When a construction tool is active, skip selection dispatch and send the
  click to `apply_point_tool`; retain Alt-click inspection for non-destructive
  inspection while other designation tools are active.
- [ ] Run the focused client library test and
  `cargo check -p progressus-client --all-targets -j 1`, then commit and push.

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

- [x] Add failing tree/stack/pawn designation and completion regressions.
- [x] Implement explicit site-owned preparation, physical clearing jobs and late occupancy checks.
- [x] Preserve existing resources and layer-claim protection; wait visibly for tools/routes/destination.
- [x] Test cancellation, unrelated jobs, full loads, renewable sources, blocked paths, and mid-job save/load conservation.
- [x] Cover workbench designation using the smallest compatible deferred placement path.
- [x] Review authority, integrate status UI and publish verified stage.

### TDD execution order

- [x] Add focused simulation regressions proving a site can be designated on a
  walkable cell containing a tree, a ground stack, or a character, while
  structures, workstations, stockpiles and production zones remain forbidden.
- [x] Add `ConstructionPreparation` state owned by the site and explicit
  preparation job kinds for harvesting a source and relocating a concrete stack;
  expose the state through detached snapshots and the save DTO.
- [x] Reuse the existing deterministic harvest and capacity-aware physical item
  transfer paths. Preparation output and pre-existing stacks must be dropped on
  explored walkable cells outside all planned footprints, with stable IDs and
  exact quantity conservation.
- [x] Add deterministic character-vacating behavior that waits for unrelated
  reserved work, then moves an available occupant to the first reachable
  E/N/S/W cell outside planned footprints. Construction material delivery and
  work remain disabled until the target cell is actually clear.
- [x] Recheck the target cell immediately before completion. If a removable
  occupant entered late, return the site to preparation rather than deleting,
  burying or teleporting it.
- [x] On cancellation remove only preparation jobs whose `site_id` matches the
  cancelled site, release their reservations, and drop any carried item at the
  worker's exact position through the existing cancellation path.
- [x] Add round-trip tests during resource harvesting and item relocation, plus
  cancellation, blocked-drop and late-occupancy regressions.
- [x] Route workbench placement through the same deferred preparation owner when
  only removable occupancy blocks the cell; retain immediate rejection for
  permanent claims and validate its port layout before accepting the project.
- [x] Add localized preparation/waiting status to the construction snapshot UI,
  run focused sim/app/client tests, then the full Prototype 01 gate.
- [x] Update ADR-0009, ADR-0022, README, client guide and milestone status to the
  behavior actually proven; review, commit and push the stage.

## Task 4: Documentation and acceptance

Files: README.md, architecture/overview.md, milestones/prototype-02.md, docs/client.md,
accepted ADR amendments and this progress checklist.

- [ ] Replace outdated flat inventory/2D/completion claims with current behavior and explicit remaining work.
- [x] Run `PROGRESSUS_RUN_CLIENT_TESTS=1 ./scripts/check-prototype-01.sh` if linking is feasible; otherwise run default gate and report limitation.
- [ ] Review whole change, resolve material findings, perform native UI smoke if available.
- [ ] Commit/push, integrate verified branch into main with fast-forward and verify remote head.

## Subsequent milestone work

Sleep and shelter remain the next subsystem, governed by ADR-0020. Do not silently
implement proposed foundational decisions as part of this interaction pass.
