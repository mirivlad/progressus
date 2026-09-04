# AGENTS.md

This file defines working rules for Codex, Claude, and other coding agents contributing to Progressus.

## 1. Read first

Before changing architecture or simulation behavior, read:

1. `README.md`
2. `docs/vision.md`
3. `docs/adr/0001-core-invariants.md`
4. `docs/adr/0002-rust-core-bevy-client.md`
5. the current milestone under `docs/milestones/`
6. relevant files under `docs/architecture/`

Project documentation is part of the specification, not optional background material.

## 2. Current development target

The current target is **Prototype 02 — Sustainable Settlement**.

Do not implement future systems merely because they appear in the Design Bible.

In particular, avoid speculative implementation of:

- electricity;
- trains;
- vehicles;
- diplomacy;
- politics;
- deep psychology;
- combat systems;
- late technology trees;
- multiplayer;
- mod scripting;
- full Simulation LOD aggregation.

Unless an accepted task/ADR explicitly changes scope, work only on what is needed to satisfy the active milestone.

## 3. Invariants and accepted ADRs are hard constraints

`docs/adr/0001-core-invariants.md` and accepted later ADRs contain binding project decisions.

If a requested task conflicts with one, **STOP that part of the task** and report:

- the invariant/ADR;
- the conflict;
- why it cannot be implemented honestly without changing the architecture;
- a preserving alternative if one is known.

Do not silently reinterpret an invariant or accepted ADR.

A fundamental change requires a new ADR.

## 4. Accepted implementation boundary

ADR-0002 is already decided:

- authoritative simulation is written in **pure Rust**;
- `progressus-sim` (or its equivalent authoritative core) must **not depend on Bevy**;
- **Bevy is the initial client/rendering framework**;
- headless simulation must run without initializing Bevy, a window, a graphics context, or audio;
- persistent identity belongs to Progressus, not Bevy.

Do not reopen the engine/language decision during ordinary implementation work.

In particular, do not:

- move authoritative gameplay state into Bevy scene/ECS objects merely for convenience;
- use Bevy `Entity` as a persisted simulation ID;
- make simulation correctness depend on a renderer system, animation, frame timing, camera visibility, or Bevy schedule timing;
- require Bevy to run simulation unit/integration tests.

The client may maintain disposable mappings such as `ProgressusId -> Bevy Entity`.

## 5. Do not invent design decisions unnecessarily

When the specification leaves a choice open, prefer the smallest reversible implementation that satisfies the current milestone.

Do not turn implementation details into permanent project rules unless the task requires it.

Examples of choices that still normally require evidence or an ADR before becoming foundational:

- authoritative simulation storage/ECS strategy;
- chunk dimensions;
- authoritative tick rate;
- save storage technology;
- deterministic RNG strategy;
- coordinate numeric representation;
- parallel simulation model;
- long-distance pathfinding architecture;
- Simulation LOD transition model.

If a choice is difficult to reverse or affects multiple subsystems, propose an ADR rather than burying the decision in code.

## 6. Simulation owns game truth

Rendering/UI objects are not authoritative simulation state.

Do not make gameplay correctness depend on:

- frame rate;
- animation completion callbacks;
- scene visibility;
- camera position except as an explicit input to loading/detail policy;
- renderer object identity;
- Bevy `Entity` identity;
- presentation-only procedural generation.

The core simulation must remain runnable headlessly as ordinary Rust code.

## 7. Determinism and RNG ownership

Do not use uncontrolled randomness in authoritative simulation.

Random decisions must derive from an explicit deterministic RNG source/state suitable for reproduction.

Do not use wall-clock time as simulation input unless an accepted design explicitly requires external real time.

World generation must remain independent of chunk visitation order.

Presentation randomness is separate from authoritative randomness. Procedural visuals must use client-owned deterministic seeds/state and must never consume authoritative simulation RNG.

Changing a tree renderer, roof generator, character appearance algorithm, shader, or other visual system must not change simulation outcomes.

## 8. Persistence

Assume saves will outlive process memory and chunk residency.

Persistent references must use stable Progressus simulation IDs, not pointers, Bevy entities, scene IDs, vector indexes, renderer handles, or other transient handles unless those handles are translated to stable IDs at the persistence boundary.

Never silently load an unsupported save format as though it were compatible.

## 9. Physical item ownership

At any authoritative instant, a physical item/stack must have one valid ownership/location state.

Transfers should be explicit and testable.

When modifying hauling, storage, crafting, cancellation, destruction, or character interruption, add/maintain tests that detect duplication and unexplained loss.

Not every individual unit needs to be a distinct entity. Aggregated stacks are acceptable when their semantics preserve the required physical ownership and gameplay consequences.

## 10. Job reservations

Jobs and scarce resources should have explicit reservation ownership where contention is possible.

Cancellation, worker death/removal, path failure, target destruction, and save/load must not leave permanent orphan reservations.

Prefer explicit lifecycle state over scattered booleans.

## 11. Performance work

Do not assume Rust automatically solves the intended scale.

First create a representative benchmark or headless scenario, measure it, then optimize the observed bottleneck.

Population scaling should be measured explicitly at useful orders of magnitude as the simulation grows.

Avoid adding complex concurrency before evidence justifies it.

Any parallel authoritative simulation design requires an ADR.

## 12. Tests

Simulation features should be testable without graphical interaction and without Bevy initialization.

For every non-trivial bug in authoritative simulation, add a regression test when practical.

Important classes of tests include:

- deterministic world generation;
- generation-order independence;
- cross-chunk movement;
- save/load round trips;
- item conservation/ownership;
- reservation cleanup;
- entity identity preservation;
- chunk unload/reload behavior;
- long-run headless scenarios;
- increasing-population performance scenarios;
- confirmation that presentation changes cannot affect authoritative state.

Do not replace meaningful assertions with snapshot churn merely to make CI green.

## 13. Procedural graphics policy

Procedural-by-default visuals are the selected direction, but they belong to the Bevy client/presentation layer.

Prefer generating visual variation from stable simulation facts plus presentation-only seeds, for example:

- terrain appearance;
- vegetation shape;
- building geometry/details;
- character appearance;
- vehicle appearance;
- wear, dirt, and decorative variation.

Do not generate expensive visual content every frame when it can be generated once and cached/batched.

Authored assets are allowed where they provide clearer or better results; procedural generation is a default direction, not a dogma.

## 14. Documentation must track reality

If implementation changes an accepted architectural behavior, update the relevant documentation in the same change.

If documentation and code disagree, do not assume code automatically wins. Determine whether implementation drifted from the design or the design was intentionally changed.

## 15. Keep changes reviewable

Prefer focused commits and PRs.

A change should have a clear reason and acceptance condition.

Avoid opportunistic repository-wide refactors during unrelated feature work unless they are required to complete the task safely.

## 16. No fake completion

Do not report a milestone item as complete merely because a type, stub, interface, placeholder UI, or TODO exists.

A requirement is complete only when its observable behavior and required tests/acceptance conditions exist.

If only part is implemented, report it as partial.

## 17. Failure reporting

When blocked, report concrete evidence:

- failing command/test;
- relevant error;
- smallest known reproduction;
- files/components involved;
- what was attempted.

Do not hide uncertainty behind confident status language.

## 18. Default task completion report

For substantial work, end with a concise report containing:

- what changed;
- tests/checks run and their result;
- milestone requirements advanced;
- known limitations or follow-up items;
- commit/PR reference when applicable.

## 19. Guiding question

When multiple implementations appear valid, prefer the one that best supports this project test:

> Can a few people in one physical world eventually become a large civilization without replacing that world with abstraction that breaks causality, logistics, or history?
