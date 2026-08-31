# Architecture Overview v0.1

Status: **Proposed for Prototype 01**

This document defines architectural boundaries and constraints, not a final engine/language choice. Engine, language, renderer, ECS library, storage implementation, and asset pipeline remain separate decisions until evaluated against Prototype 01.

## 1. Primary architectural goal

Progressus must support a simulation that can grow in spatial extent, entity count, and temporal duration without coupling the game rules to a particular renderer or requiring the entire world to remain resident in memory.

The first implementation should optimize for correctness, determinism, observability, and testability before visual richness.

## 2. Major layers

Conceptually the project is split into:

```text
+-----------------------------+
|          Game Client        |
| rendering / input / UI      |
+--------------+--------------+
               |
               v
+-----------------------------+
|       Application Layer     |
| commands / queries / modes  |
+--------------+--------------+
               |
               v
+-----------------------------+
|       Simulation Core       |
| world / entities / jobs     |
| production / logistics      |
+--------------+--------------+
               |
               v
+-----------------------------+
| World Generation/Persistence|
| chunks / seed / saves       |
+-----------------------------+
```

The boundaries do not require separate processes or repositories. They are logical dependency directions.

The Simulation Core must not depend on rendering APIs.

## 3. Simulation Core

The core owns authoritative game state.

Prototype 01 responsibilities:

- simulation clock;
- entity identities;
- positions;
- characters;
- tasks/jobs;
- resources/items;
- stockpiles;
- simple buildings;
- crafting/construction state;
- chunk activation requests;
- deterministic state transitions.

Later systems such as education, power, advanced transport, markets, social institutions, and simulation LOD should extend this model rather than replace it.

## 4. Commands and queries

The client should not mutate core state directly.

Player intent should enter through explicit commands, for example:

```text
DesignateHarvest(area)
CreateStockpile(area, filters)
PlaceConstruction(definition, position)
SetRecipe(workshop, recipe)
SetJobPriority(character, job_type, priority)
```

The UI reads state through queries/read models.

This creates a useful seam for:

- deterministic tests;
- replay/debugging;
- headless simulation;
- future AI-controlled test players;
- future networking experiments if ever desired.

Prototype 01 does not require a network protocol or event-sourcing framework.

## 5. Time model

Simulation time should be discrete and deterministic.

The exact tick rate is not fixed here. It must be chosen through measurement.

Rules:

1. Game logic may not depend on wall-clock timing.
2. Given the same initial state and command sequence, the core should produce the same authoritative result on supported deterministic builds.
3. Rendering frame rate must be independent of simulation tick rate.
4. Pausing and accelerated headless stepping must be supported by architecture even if early UI exposes only basic speeds.

Systems that do not need per-tick updates should eventually use scheduled or coarse updates rather than blindly scanning all entities every tick.

## 6. Coordinates

All physical entities exist in one world coordinate space.

Prototype 01 should use chunk-local coordinates plus a stable chunk coordinate, avoiding assumptions that the entire world fits inside a small fixed grid.

Example conceptual address:

```text
WorldPosition {
    chunk_x
    chunk_y
    local_x
    local_y
}
```

The exact numeric types are an ADR-level implementation decision.

Coordinate representation must permit negative coordinates and traversal arbitrarily far from the origin within practical technical limits.

## 7. Chunks

The world is divided into fixed-size chunks.

A chunk can be in states such as:

- nonexistent/unvisited;
- deterministically generatable;
- generated and currently unloaded;
- loaded inactive;
- active detailed simulation;
- modified and persisted.

The state machine should be explicit rather than inferred from renderer visibility.

Chunk size is not fixed by this document. It must be benchmarked against generation, pathfinding, persistence, and rendering costs.

## 8. Deterministic world generation

Untouched terrain is generated from at least:

- world seed;
- world-generation version;
- chunk coordinates.

Generation must not depend on chunk visitation order.

Generating chunk `(10, -4)` before `(10, -3)` must not change either chunk's canonical untouched result.

Features crossing chunk boundaries must therefore derive from stable world-space inputs rather than mutable neighboring generation order.

## 9. Persistence

Saving the whole procedural world as a giant materialized map is forbidden.

The save model should conceptually contain:

```text
Save
 ├─ metadata
 │   ├─ save format version
 │   ├─ world seed
 │   ├─ worldgen version
 │   └─ simulation time
 ├─ global simulation state
 ├─ persistent entity records
 └─ modified chunk records
```

Untouched chunks should normally be regenerated from seed instead of stored in full.

Modified chunks store enough information to reconstruct their authoritative state.

Save format versioning must exist from the first persistent prototype. Backward compatibility between all development builds is not required, but unsupported versions must fail explicitly rather than silently corrupt state.

## 10. Entity identity

Long-lived entities require stable identifiers independent of memory addresses, array indices, or chunk load order.

At minimum this applies to:

- characters;
- significant buildings;
- jobs/orders that survive multiple ticks;
- later vehicles and institutions.

An entity may move between chunks without changing identity.

Temporary rendering objects do not require simulation identities.

## 11. Entity storage

No ECS framework is mandated yet.

The implementation may use ECS, sparse sets, tables, ordinary structs/classes, or a hybrid. Selection must be based on measured fit to the prototype.

Regardless of implementation, the model must support:

- stable IDs;
- bulk iteration by relevant component/state;
- entity creation/destruction;
- serialization;
- chunk-aware spatial indexing;
- separation between simulation data and presentation data.

## 12. Spatial indexing

Do not search the entire entity population to answer local spatial questions.

Prototype 01 should have a chunk-aware spatial index sufficient for:

- locating nearby resources;
- finding occupants/items in a cell or area;
- pathfinding queries;
- loading/unloading decisions.

Its implementation can begin simple and be replaced after profiling.

## 13. Jobs and work

Work should be represented as explicit world tasks rather than hidden animation states.

A conceptual flow:

```text
player designation / system demand
        ↓
available job
        ↓
worker selection
        ↓
reservation
        ↓
travel
        ↓
perform work
        ↓
produce/modify physical state
        ↓
complete/release reservation
```

Prototype 01 must define cancellation and reservation cleanup behavior. Lost reservations are a common source of colony-sim deadlocks and should be tested explicitly.

## 14. Items and inventories

Prototype 01 should prefer a simple physical item/stack model.

Items need a clear ownership/location state, for example one of:

- on ground at a position;
- carried by an entity;
- stored inside a container/stockpile representation;
- consumed/destroyed.

An item must not simultaneously exist in two locations.

Large-scale aggregation is a later concern and must conserve quantities when introduced.

## 15. Production

Recipes are data definitions interpreted by generic systems.

A recipe conceptually includes:

- accepted inputs;
- output(s);
- required workstation/tool capability;
- work amount;
- optional skill/technology requirements.

Prototype 01 uses only a few recipes. The architecture should avoid hard-coding each product's behavior into bespoke UI or simulation classes.

## 16. Data definitions

Game content should be data-driven where practical:

- resource definitions;
- item definitions;
- recipes;
- building definitions;
- terrain types;
- later technology definitions.

"Data-driven" does not mean inventing a scripting language during Prototype 01. A typed declarative format or code-defined immutable tables are both acceptable initially if they keep content separate from algorithms.

## 17. Pathfinding

Prototype 01 only needs correct local movement over generated terrain.

However, the pathfinding API must not assume the entire future world is represented by one finite in-memory grid.

Long-distance hierarchical routing, roads, rail networks, and regional logistics are later milestones.

Early pathfinding should therefore be encapsulated behind a service/interface boundary rather than embedded in character logic.

## 18. Headless mode

The Simulation Core must be runnable without creating a graphics window.

Required uses:

- unit tests;
- integration tests;
- deterministic reproduction of bugs;
- accelerated simulation runs;
- future performance qualification.

A useful target shape is conceptually:

```text
progressus-test run scenario.json --ticks 100000
```

The exact executable layout is not mandated.

## 19. Observability

Simulation failures should be inspectable.

Prototype 01 should support structured diagnostics for important operations such as:

- chunk generation/loading;
- entity creation/destruction;
- job assignment/cancellation;
- save/load failures;
- invariant violations.

Tests should be able to request state summaries without scraping graphical UI.

## 20. Invariants over silent repair

When impossible state is detected during development, fail loudly in debug/test builds.

Examples:

- an item exists in two locations;
- a worker reserves two exclusive jobs;
- a persisted entity references an impossible chunk;
- deterministic regeneration disagrees with expected worldgen version behavior.

Silent repair hides architectural defects.

## 21. Performance strategy

Optimization order:

1. establish correctness;
2. build representative headless benchmarks;
3. measure;
4. optimize proven hotspots;
5. re-measure.

Do not introduce complicated multithreading, distributed simulation, custom allocators, or low-level storage solely because the final vision sounds large.

The architecture should avoid obvious global scans and renderer coupling now, while leaving deeper optimization until profiling justifies it.

## 22. Concurrency

Prototype 01 should prefer deterministic single-threaded authoritative simulation unless benchmarks demonstrate an immediate blocker.

Background work may later be appropriate for rendering preparation, generation, pathfinding, persistence, or coarse simulation, but concurrency must not make authoritative state order-dependent.

Any move to parallel authoritative simulation requires an ADR.

## 23. Simulation LOD seam

Prototype 01 does not implement population aggregation, but APIs should avoid assumptions that every entity is always at maximum detail.

Future LOD work will need explicit transitions between detailed and aggregated representations.

The key future invariant is conservation: aggregation and expansion must not create or destroy meaningful quantities, identities, or obligations without a simulation rule explaining the change.

## 24. Client

The client is responsible for:

- camera;
- rendering;
- selection;
- placement previews;
- UI panels;
- input mapping;
- sound and animation.

It sends commands to the application/simulation layer and presents resulting state.

Visual interpolation may exist between simulation ticks but must never become authoritative gameplay state.

## 25. Prototype 01 architecture deliverables

Before Prototype 01 is considered complete, the codebase should contain:

- a documented project/module layout;
- deterministic world seed handling;
- chunk coordinate and lifecycle implementation;
- stable simulation entity IDs;
- a simulation clock;
- headless stepping;
- basic job/reservation model;
- item/resource ownership model;
- persistence with version metadata;
- automated tests for core invariants;
- at least one long-run headless smoke test.

## 26. Explicit non-goals for now

Do not implement during architectural bootstrap unless required by an accepted milestone:

- multiplayer;
- networking architecture;
- mod scripting language;
- politics;
- diplomacy;
- combat depth;
- advanced psychology;
- full technology tree;
- electricity simulation;
- trains;
- vehicles;
- procedural civilizations;
- orbital gameplay;
- premature population aggregation.

These are future game systems, not prerequisites for proving the core.
