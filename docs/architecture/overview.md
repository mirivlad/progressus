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

The current movement bootstrap makes this boundary concrete with:

- `SetMovementDirection { character_id, direction }` and `StopMovement { character_id }` application commands;
- authoritative `MovementState::{Idle, Moving { direction }}` on characters;
- exact authoritative `WorldPosition` values with signed integer `1024`-subunit cells, plus derived containing cells in detached snapshots;
- cardinal bootstrap movement at `256` subunits per tick; every coarse transition is checked against effective terrain, and water, rock, or the representable-world edge cause a normal exact-boundary idle stop;
- a headless-only least-visited walker that queries neighboring terrain through snapshots, submits ordinary commands, advances four default-speed ticks to the chosen center, and observes a fresh snapshot before selecting again.

The walker is an external deterministic test driver, not authoritative navigation: it stores no route, does not run BFS/Dijkstra/A*, and does not alter simulation or world-generation state. This bootstrap does not yet provide general obstacle navigation, jobs/AI movement policy, speed/collision, chunk residency, or persistence.

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

### Current effective-terrain and natural-resource bootstrap

Worldgen v1 now derives two independent untouched layers from `(seed, worldgen version, absolute cell)`: terrain and an optional natural-resource source. Resource sampling uses a separate deterministic domain from terrain, so adding the source layer does not alter the accepted terrain golden fixtures. Sources currently exist only on Grass, use `Tree` or `StoneOutcrop` kinds with small finite yields, and are absent from the forced spawn corridor. Generation order remains irrelevant.

`GeneratedChunk` remains the immutable raw result of deterministic base generation. `Simulation` owns that base-world identity together with a private canonical `ModifiedWorld`, whose sparse `BTreeMap` state records only terrain overrides that differ from the generated base.

`EffectiveChunk` is built on demand by applying those overrides to a raw generated chunk. Authoritative movement passability and application terrain snapshots query this effective terrain, so they agree on the current world rather than bypassing modifications.

This is an in-memory bootstrap only. It does not yet provide save/load, resident caching or unloading policy, or a terrain gameplay command.

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

### Current harvest-job bootstrap

`progressus-sim` now owns a deterministic `JobWorld` with stable Progressus job IDs, a source-to-harvest-job index, and an exclusive worker-to-job reservation index. Harvest jobs move through `Available`, `Reserved`, and `Working` states. Available jobs consider idle unreserved characters in deterministic distance/ID order, reserve one worker only after a route is available, travel through the existing bounded explored-world navigation, then perform a small fixed work amount. Completion depletes the natural source and creates exactly one physical Wood/Stone stack at the source position.

Cancellation removes both source and worker indexes. Manual `MoveTo`, Stop, or directional movement releases an assigned worker before applying the player command, returning the unfinished job to `Available`; a failed manual `MoveTo` leaves the existing assignment intact. Tests cover full completion, deterministic/exclusive assignment, invalid designations, cancellation, manual interruption, and reservation-index consistency.

`progressus-app` exposes `DesignateHarvest`/`CancelJob`, detached job snapshots, and a job revision. The Bevy bootstrap exposes Harvest as a persistent toolbar tool that designates every eligible source in a dragged rectangular area and draws a small state-colored bracket around designated sources. Shift+left-click remains a temporary single-source shortcut.

### Current stockpile/haul bootstrap

`progressus-sim` also owns deterministic stockpile areas and `Haul` jobs. A stockpile is a stable-ID set of discovered walkable ground cells with a unique cell-owner index. Eligible outside items are hauled into storage, but Haul is no longer restricted to the outside→inside transition: an underfilled stack already on stockpile ground may itself become a source when another compatible stack in the same stockpile can absorb it. Destination selection is merge-first and only falls back to an empty cell when no compatible merge exists. Internal consolidation moves higher stable IDs toward lower compatible IDs, which makes the direction canonical and prevents storage ping-pong. Job indexes reserve one item, one destination cell, and one worker; the worker travels to the exact item position, picks up the canonical stack, carries it to the destination cell center, and drops/merges the same stable stack there. Manual interruption or removal of an active destination releases reservations and drops a carried stack at the worker's current position.

`progressus-app` exposes stockpile commands/snapshots and carried-item snapshots. The Bevy bootstrap exposes `Stockpile +` and `Stockpile -` toolbar tools with rectangular drag designation, outlines stockpile cells, and parents carried item sprites to worker visuals so transport remains visible through presentation interpolation. Ctrl+left-click remains a temporary single-cell shortcut. Internal exact stack splitting now exists for production supply, but player-directed/general-purpose splitting is not exposed. Stockpile filters/priorities, containers, skills, and generalized job policy remain later work.

### Current workbench/craft bootstrap

`progressus-sim` owns stable-ID workstations, stable-ID production orders, a `ProductionLogisticsWorld`, and a typed `RecipeId::PrimitiveTool` definition (`2 Wood + 1 Stone -> 1 PrimitiveTool`, six work ticks). A production order has an explicit `Finite { remaining_runs }` or `Infinite` target. Only one Craft job can occupy a workstation at a time; pending orders are considered in creation order. Each producer owns explicit Input and Output ground-cell zones with unique cell ownership. The current one-cell Workbench specializes that generic ownership model into exactly two cardinal Input ports plus exactly two diagonal Output ports. Input and Output pairs each cycle independently through six canonical unordered layouts and neither can be edited cell-by-cell through the public simulation API. These cells cannot simultaneously be ordinary stockpile ground. A Craft job reserves concrete physical stacks only from its own Input zone. Missing quantities are split exactly from eligible ordinary stockpile stacks and carried by a dedicated `SupplyProduction` job into a reserved Input cell. Completion consumes exact quantities, preserving partial stack IDs or removing exhausted stacks, creates the physical `PrimitiveTool` in the producer's Output zone, and decrements only finite targets. Ordinary Haul can then move the output from Output to storage. Shared physical inputs cannot be double-reserved; a two-infinite-order regression verifies deterministic shared-source arbitration without starvation. See [`ADR-0012`](../adr/0012-production-input-output-logistics-zones.md).

`progressus-app` exposes workstation, production-order, and production-zone commands, detached workstation/order/logistics snapshots, and authoritative workstation/production/logistics revisions. The Bevy toolbar keeps Workbench only as a point placement tool. In Select mode, clicking a workbench opens a reusable `ModalKind` shell whose workstation view edits authoritative production orders, including an explicit `∞` control; removing a workbench is also handled from that modal. The modal logistics schematic reuses the procedural Workbench image, marks the two physical Input ports in red and the two physical Output ports in yellow, and exposes separate localized rotation actions rather than Workbench logistics paint tools. The same modal shell is intended for later containers, furnaces, research stations, and other inspectors. Shared UI strings use a typed localization layer with Russian as the default language and English as the second built-in language. Bevy's embedded ASCII-only font is not sufficient for Russian, so the bootstrap resolves a local system font with Cyrillic coverage. See [`ADR-0010`](../adr/0010-production-orders-and-localized-modal-ui.md). Workbench placement remains an instantaneous bootstrap exception; physical StoneWall construction is implemented separately through construction sites and delivery/construct jobs.

### Current physical-construction bootstrap

`progressus-sim` owns a stable-ID `ConstructionWorld` for unfinished sites and completed structures. `StructureKind::StoneWall` costs 2 Stone and fixed work. A site reserves one concrete compatible stack, excludes it from competing Haul/Craft use, and creates a `DeliverConstruction` job. The worker physically picks up that canonical stack and drops it at a deterministic reachable work position beside the site; only a delivered stack can enable `Construct`. Completion consumes exactly 2 Stone and converts the site to a finished structure with the same stable ID. A partial remainder stays on the ground beside the wall. Cancellation drops carried material before clearing jobs/reservations.

Finished StoneWall occupancy is separate from terrain. Movement transitions and A* both consult completed structures, so a wall blocks traversal without mutating the underlying generated/effective terrain. Unfinished blueprints remain passable. The Bevy client exposes a rectangular Stone-wall tool plus procedural blueprint/wall sprites; Cancel-jobs removes unfinished sites but does not demolish completed walls. See [`ADR-0009`](../adr/0009-physical-construction-sites-and-blocking-structures.md).

## 14. Items and inventories

Prototype 01 should prefer a simple physical item/stack model.

Items need a clear ownership/location state, for example one of:

- on ground at a position;
- carried by an entity;
- stored inside a container/stockpile representation;
- consumed/destroyed.

An item must not simultaneously exist in two locations.

Large-scale aggregation is a later concern and must conserve quantities when introduced.

### Current physical-item bootstrap

`progressus-sim` owns a deterministic `ItemWorld` keyed by the same global stable `EntityId` space as characters. Prototype item content is deliberately tiny (`Wood`, `Stone`, and `PrimitiveTool`), but every stack has a quantity in `1..=1024` and exactly one canonical location: exact fixed-point ground position or carrier character ID. Ground items are indexed by chunk and carried items by character; transfer operations update the canonical stack and its indexes atomically. Character interaction reach uses integer `InteractionRadius` geometry, and dropping is rejected onto non-walkable effective terrain.

`progressus-app` exposes only explored ground items in requested chunks as detached `GroundItemSnapshot` values plus an item revision. Natural-resource snapshots are likewise chunk-scoped and contain only explored source cells; a separate resource revision is reserved for authoritative depletion changes. Carried stacks are published as detached carrier-linked snapshots. The Bevy client reconciles disposable ground-item entities by stable Progressus ID and converts their exact positions relative to the nearby render origin, just like characters.

Harvested resources create ordinary physical ground stacks, and the haul bootstrap can move those stacks through the canonical `Carried` state into designated stockpile cells. A stockpile is intentionally not a container: delivered stacks remain `Ground` at exact positions inside the stockpile area, as recorded by [`ADR-0006`](../adr/0006-stockpiles-remain-physical-ground.md). Haul uses a merge-first destination policy: a compatible underfilled same-kind stack is preferred over an empty stockpile cell when the combined quantity is at most 1,024. Underfilled stacks already inside the same stockpile are also valid Haul sources and consolidate toward a lower stable-ID target, preventing ping-pong; the destination stack keeps its stable ID and the emptied source stack is removed. Exact internal stack splitting now exists for production supply, but there is still no player-directed/general-purpose splitting UI, real container/storage location, or residency; physical stacks and active logistics now round-trip through save format v1. Crafting and StoneWall construction now provide explicit quantity-consumption paths over physical stacks.

### Procedural presentation assets

Prototype visual art is source-controlled as Rust code under `assets/procedural/`. The Bevy client supplies a small integer RGBA canvas/rasterizer and lazily turns those recipes into nearest-filtered `Image` assets. Terrain, characters, ground-item stacks, primitive tools, workbenches, trees, and stone outcrops use bounded deterministic variants selected from `WorldCell` or stable `EntityId`; stack quantities use cached procedural bitmap labels rather than antialiased world-space font rendering. The registry caches each recipe/variant or quantity label rather than allocating a texture every frame.

This pipeline is strictly presentation-only. Procedural art code does not enter `progressus-sim`, `progressus-app`, or `progressus-worldgen`, and pixels never become authoritative state. See [`ADR-0005`](../adr/0005-procedural-visual-assets-as-code.md).

## 15. Production

Recipes are data definitions interpreted by generic systems.

A recipe conceptually includes:

- accepted inputs;
- output(s);
- required workstation/tool capability;
- work amount;
- optional skill/technology requirements.

Prototype 01 uses only a few recipes. The architecture should avoid hard-coding each product's behavior into bespoke UI or simulation classes. The first implemented typed recipe is `PrimitiveTool`: two Wood plus one Stone at a Workbench, consuming concrete stacks from that producer's Input zone and creating one physical output in its Output zone.

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


### Cardinal connectivity for cell networks

Cell-network presentation uses a generic four-bit N/E/S/W neighbour mask. StoneWall blueprints and finished walls are the first consumer: procedural arms extend to tile edges for all 16 isolated/end/straight/corner/T/cross masks, so adjacent wall cells read as one continuous structure. The helper is presentation-derived from authoritative cells and is intended for future roads, fences, pipes, or similar cardinal networks; see [`ADR-0011`](../adr/0011-cardinal-connectivity-autotiles.md).

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

## Persistence bootstrap

`progressus-sim` owns versioned save format v1 as an explicit DTO contract rather than serializing the internal `Simulation` layout. Untouched generated chunks are omitted; seed/worldgen identity plus sparse exploration, overrides, depleted resources, exact entities/items/jobs, reservations, production logistics, and construction reconstruct the authoritative world. `progressus-app` exposes immutable save bytes, metadata inspection, and validated construction of a new `Application`. The Bevy client owns only desktop slot storage and presentation reset; it never mutates a partially decoded simulation. Three bootstrap slots use recoverable temporary/backup replacement in the platform user-data directory. See [`ADR-0014`](../adr/0014-client-save-slots-and-atomic-load.md).

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

The executable bootstrap has five crates with these dependency directions:

```text
progressus-client --> Bevy
        |
        +--> progressus-app <-- progressus-headless
                 |
                 v
           progressus-sim --> progressus-worldgen
```

- `progressus-worldgen` owns deterministic versioned terrain generation and coordinate types.
- `progressus-sim` owns authoritative time, stable identities, characters, physical item stacks and their chunk/carrier indexes, base-world identity, and sparse modified-world state.
- `progressus-app` is the command/query boundary and returns detached read models.
- `progressus-headless` proves that the application can run and be inspected without a renderer.
- `progressus-client` is a native Bevy presentation consumer. Its only direct dependencies are Bevy and `progressus-app`; it must not directly depend on `progressus-sim` or `progressus-worldgen`.

The headless application chain is Bevy-free. The boundary guard scans that complete chain and verifies the client's direct-dependency rule. The Bevy client converts `ClientSnapshot` data into disposable entities mapped from stable Progressus IDs; those mappings are derived presentation caches, never authoritative or persistent state.

`progressus-sim` also owns the in-memory, monotonic `ExploredWorld`: every character reveals the Euclidean disk of cells within the provisional radius `5` around its containing cell at new-game creation and after each authoritative tick. Discovery is independent of selection and camera position. `progressus-app` publishes terrain as detached `KnownTerrain`; it omits entirely unknown chunks and never gives the client a terrain type for an undiscovered cell. Player `MoveTo` destinations and the bounded player A* are limited to explored cells, so an undiscovered terrain query cannot become a navigation side channel.

The client first requests a lightweight character snapshot, establishes a disposable render origin, and requests only chunks intersecting the camera viewport plus a small presentation margin. Those chunk-scoped snapshots also contain only explored ground-item stacks, so undiscovered item locations do not become a client-side information leak. It repeats that terrain request only when the viewport window, the authoritative exploration revision, or the authoritative item/resource revisions change, not at render-frame frequency. The camera can pan over unknown background but cannot discover or inspect terrain; no character, including Cora (`EntityId` 3), is special to discovery or terrain selection. Arrow and Stop input become ordinary application commands; pan and zoom alter only presentation camera state.

The presentation scheduler is non-authoritative: it requests at most one simulation tick every approximately 250 ms (nominally 4 Hz) and discards a long-frame backlog rather than catching up. Rendering frame time never becomes simulation input.

This bootstrap proves rendering, tool-based rectangle/point designation, snapshot-driven mapping, camera-driven explored-terrain refresh, effective-terrain snapshots, deterministic sub-cell living positions, and a bounded deterministic `MoveTo` route through the application boundary required by [`ADR-0004`](../adr/0004-grid-world-continuous-living-movement.md). A* remains cardinal and cell-topological; exact waypoints and per-tick motion traces are presentation-readable only on request. A trace is the path completed in that authoritative tick, including a one-point trace while idle, so presentation cannot replay stale arrival motion after a later snapshot. It does not implement diagonal or hierarchical navigation, auto-repath, generalized jobs/AI priorities, speed modifiers or collision, chunk residency, demolition/doors, autosaves, or save migration beyond explicit v1 rejection. Harvest, Haul, Craft, and physical StoneWall construction are the first complete job bootstraps: Wood/Stone/PrimitiveTool stacks remain physical across starting supplies, harvested outputs, carrying, stockpile delivery, same-kind stack merging up to the 1,024-unit capacity, local recipe consumption, and crafted output. The selected-character route and selection bracket are normal presentation; F3 only adds the technical authoritative-position marker. Coordinates and provisional chunk geometry remain specified by [`ADR-0003`](../adr/0003-bootstrap-world-coordinates.md).
