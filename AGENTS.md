# AGENTS.md

This file defines working rules for Codex, Claude, and other coding agents contributing to Progressus.

## 1. Read first

Before changing architecture or simulation behavior, read:

1. `README.md`
2. `docs/vision.md`
3. `docs/adr/0001-core-invariants.md`
4. the current milestone under `docs/milestones/`
5. relevant files under `docs/architecture/`

Project documentation is part of the specification, not optional background material.

## 2. Current development target

The current target is **Prototype 01 — Continuous World**.

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

## 3. Invariants are hard constraints

`docs/adr/0001-core-invariants.md` contains accepted project invariants.

If a requested task conflicts with one, **STOP that part of the task** and report:

- the invariant ID;
- the conflict;
- why it cannot be implemented honestly without changing the architecture;
- a preserving alternative if one is known.

Do not silently reinterpret an invariant.

Do not edit ADR-0001 to make an implementation easier.

A fundamental change requires a new ADR.

## 4. Do not invent design decisions unnecessarily

When the specification leaves a choice open, prefer the smallest reversible implementation that satisfies the current milestone.

Do not turn implementation details into permanent project rules unless the task requires it.

Examples of choices that should normally be documented before becoming foundational:

- game engine;
- primary implementation language;
- ECS/framework dependency;
- chunk dimensions;
- authoritative tick rate;
- save storage technology;
- deterministic RNG strategy;
- coordinate numeric representation;
- parallel simulation model.

If a choice is difficult to reverse or affects multiple subsystems, propose an ADR rather than burying the decision in code.

## 5. Simulation owns game truth

Rendering/UI objects are not authoritative simulation state.

Do not make gameplay correctness depend on:

- frame rate;
- animation completion callbacks;
- scene visibility;
- camera position except as an explicit input to loading/detail policy;
- renderer object identity.

The core simulation must remain runnable headlessly.

## 6. Determinism

Do not use uncontrolled randomness in authoritative simulation.

Random decisions must derive from an explicit deterministic RNG source/state suitable for reproduction.

Do not use wall-clock time as simulation input unless an accepted design explicitly requires external real time.

World generation must remain independent of chunk visitation order.

## 7. Persistence

Assume saves will outlive process memory and chunk residency.

Persistent references must use stable simulation IDs, not pointers, scene IDs, vector indexes, or transient handles unless those handles are translated to stable IDs at the persistence boundary.

Never silently load an unsupported save format as though it were compatible.

## 8. Physical item ownership

At any authoritative instant, a physical item/stack must have one valid ownership/location state.

Transfers should be explicit and testable.

When modifying hauling, storage, crafting, cancellation, destruction, or character interruption, add/maintain tests that detect duplication and unexplained loss.

## 9. Job reservations

Jobs and scarce resources should have explicit reservation ownership where contention is possible.

Cancellation, worker death/removal, path failure, target destruction, and save/load must not leave permanent orphan reservations.

Prefer explicit lifecycle state over scattered booleans.

## 10. Performance work

Do not optimize based only on the final ambition of "tens of thousands of people".

First create a representative benchmark or headless scenario, measure it, then optimize the observed bottleneck.

Avoid adding complex concurrency before evidence justifies it.

Any parallel authoritative simulation design requires an ADR.

## 11. Tests

Simulation features should be testable without graphical interaction.

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
- long-run headless scenarios.

Do not replace meaningful assertions with snapshot churn merely to make CI green.

## 12. Documentation must track reality

If implementation changes an accepted architectural behavior, update the relevant documentation in the same change.

If documentation and code disagree, do not assume code automatically wins. Determine whether implementation drifted from the design or the design was intentionally changed.

## 13. Keep changes reviewable

Prefer focused commits and PRs.

A change should have a clear reason and acceptance condition.

Avoid opportunistic repository-wide refactors during unrelated feature work unless they are required to complete the task safely.

## 14. No fake completion

Do not report a milestone item as complete merely because a type, stub, interface, placeholder UI, or TODO exists.

A requirement is complete only when its observable behavior and required tests/acceptance conditions exist.

If only part is implemented, report it as partial.

## 15. Failure reporting

When blocked, report concrete evidence:

- failing command/test;
- relevant error;
- smallest known reproduction;
- files/components involved;
- what was attempted.

Do not hide uncertainty behind confident status language.

## 16. Default task completion report

For substantial work, end with a concise report containing:

- what changed;
- tests/checks run and their result;
- milestone requirements advanced;
- known limitations or follow-up items;
- commit/PR reference when applicable.

## 17. Guiding question

When multiple implementations appear valid, prefer the one that best supports this project test:

> Can a few people in one physical world eventually become a large civilization without replacing that world with abstraction that breaks causality, logistics, or history?
