# Architecture Overview v0.2

Status: **Accepted for Prototype 01**

The implementation boundary is fixed by [`ADR-0002`](../adr/0002-rust-core-bevy-client.md): the authoritative simulation is pure Rust and does not depend on Bevy; Bevy is the initial client/rendering framework. Renderer details, authoritative storage layout, chunk dimensions, persistence technology, and other lower-level choices remain separate decisions unless explicitly accepted elsewhere.

## 1. Primary architectural goal

Progressus must support a simulation that can grow in spatial extent, entity count, and temporal duration without coupling the game rules to a particular renderer or requiring the entire world to remain resident in memory.

The first implementation should optimize for correctness, determinism, observability, and testability before visual richness.

## 2. Major layers

Conceptually the project is split into:

```text
+-----------------------------+
|        Bevy Game Client     |
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
|   Pure Rust Simulation Core |
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

The Simulation Core must not depend on Bevy or any rendering API.

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

Long-lived entities require stable identifiers independent of memory addresses, array indices, chunk load order, or client/runtime handles.

At minimum this applies to:

- characters;
- significant buildings;
- jobs/orders that survive multiple ticks;
- later vehicles and institutions.

An entity may move between chunks without changing identity.

Bevy `Entity` is never a persistent simulation identity. The client may keep disposable mappings from Progressus stable IDs to Bevy entities.

Temporary rendering objects do not require simulation identities.

## 11. Authoritative entity/storage model

The authoritative core is pure Rust. No external ECS framework is mandated for simulation storage.

The implementation may use tables, sparse sets, ordinary structs, arenas, indexes, a custom ECS-like representation, or a hybrid. Selection must be based on measured fit to the simulation rather than convenience for the renderer.

Bevy ECS may be used freely for presentation/client state, but `progressus-sim` must not depend on Bevy.

Regardless of implementation, the simulation model must support:

- stable IDs;
- bulk iteration by relevant component/state;
- entity creation/destruction;
- serialization;
- chunk-aware spatial indexing;
- separation between simulation data and presentation data;
- future transitions between detailed and aggregated Simulation LOD representations.

Not every physical unit must be a separate runtime entity. Stacks, bulk resources, or future remote-population representations may be aggregated when gameplay semantics permit it.

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

Production complexity follows [`docs/gameplay/production.md`](../gameplay/production.md): chains exist to support believable development, not to turn Progressus into a conveyor/ratio optimization game.

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

The Simulation Core must be runnable directly as pure Rust without initializing Bevy, a window, renderer, graphics device, or audio system.

Required uses:

- unit tests;
- integration tests;
- deterministic reproduction of bugs;
- accelerated simulation runs;
- performance qualification;
- long-span and population-scale experiments.

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

The accepted Rust-core architecture is chosen to keep this path open, not because performance is assumed to be automatically solved by the language choice.

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

## 24. Bevy client

Bevy is the selected initial client framework.

The client is responsible for:

- camera;
- rendering;
- selection;
- placement previews;
- UI panels;
- input mapping;
- sound and animation;
- procedural visual generation and caching.

It sends commands to the application/simulation layer and presents resulting state.

Visual interpolation may exist between simulation ticks but must never become authoritative gameplay state.

The client should treat simulation read models as input data. Bevy scene/ECS state must be disposable and reconstructable from authoritative state where necessary.

### 24.1 Procedural graphics boundary

Procedural-by-default graphics are a presentation policy, not a simulation rule.

The simulation may expose meaningful visual facts such as:

- material;
- age/era;
- condition;
- footprint;
- species;
- equipment;
- functional state.

The client may derive from those facts:

- texture variation;
- geometric detail;
- vegetation shape;
- roof/decorative form;
- character appearance;
- dirt/wear;
- animation;
- other non-authoritative visual variation.

Visual generation must use presentation-owned deterministic seeds/state. It must not consume authoritative simulation RNG or change world/simulation outcomes.

Generated visual results should be cached/batched where appropriate rather than regenerated every frame.

## 25. Prototype 01 architecture deliverables

Before Prototype 01 is considered complete, the codebase should contain:

- a documented Rust crate/module layout preserving the ADR-0002 dependency direction;
- deterministic world seed handling;
- chunk coordinate and lifecycle implementation;
- stable simulation entity IDs;
- a simulation clock;
- headless stepping without Bevy initialization;
- basic job/reservation model;
- item/resource ownership model;
- persistence with version metadata;
- automated tests for core invariants;
- at least one long-run headless smoke test;
- a minimal Bevy client proving that simulation state can be presented without becoming authoritative client state.

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

## 27. Prototype 01 bootstrap implementation

The executable bootstrap uses this concrete Cargo dependency chain:

```text
progressus-headless
        |
        v
progressus-app
        |
        v
progressus-sim
        |
        v
progressus-worldgen
```

- `progressus-worldgen` owns deterministic versioned terrain generation and coordinate types.
- `progressus-sim` owns authoritative time, stable identities, characters, and simulation state.
- `progressus-app` is the client-facing command/query boundary and returns detached read models.
- `progressus-headless` proves that the application can run and be inspected without a renderer.

A Bevy client should depend on `progressus-app`, not on `progressus-sim`. It may translate a `ClientSnapshot` into disposable Bevy entities and send `Command` values back through `Application`. It must not retain mutable access to authoritative simulation storage.

The bootstrap currently presents deterministic terrain and five characters. It does not yet implement movement, jobs, items, persistence, or the visual client itself. Coordinate and provisional chunk-geometry decisions are specified by [`ADR-0003`](../adr/0003-bootstrap-world-coordinates.md).
