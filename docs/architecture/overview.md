# Architecture Overview v0.3

Status: **Prototype 01 complete; Prototype 02 extensions in progress**

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

Player intent enters through explicit commands. Prototype 01 currently includes commands such as:

```text
SetMovementDirection / MoveTo / StopMovement
DesignateHarvest / CancelJob
CreateStockpile / SetStockpileCell / SetStockpileItemAllowed
PlaceWorkstation / AddProductionOrder
CycleWorkstationInputs / CycleWorkstationOutputs
DesignateConstruction / CancelConstruction
```

The UI reads state through queries/read models.

The current movement bootstrap makes this boundary concrete with:

- `SetMovementDirection { character_id, direction }` and `StopMovement { character_id }` application commands;
- authoritative `MovementState::{Idle, ManualDirectional { direction }, Navigating { destination }}` on characters;
- exact authoritative `WorldPosition` values with signed integer `1024`-subunit cells, plus derived containing cells in detached snapshots;
- cardinal bootstrap movement at `256` subunits per tick; every coarse transition is checked against effective terrain, and water, rock, or the representable-world edge cause a normal exact-boundary idle stop;
- a headless-only least-visited walker that queries neighboring terrain through snapshots, submits ordinary commands, advances four default-speed ticks to the chosen center, and observes a fresh snapshot before selecting again.

The travel64 walker is an external deterministic test driver rather than authoritative navigation: it stores no route and chooses the next explored grass cell through public queries. Job navigation uses bounded deterministic cardinal A* over explored effective terrain and finished-structure occupancy. Player `MoveTo` additionally preserves the exact requested destination as authoritative intent even when it lies beyond the explored boundary: it follows a deterministic route to the currently closest reachable explored frontier, then replans toward the same intent as movement reveals new cells. If the requested point becomes reachable it is reached exactly; if progress becomes impossible the character remains at the closest reachable point discovered by that policy. Waypoints stay fixed-point and hidden terrain is never exposed to the client as a route. Prototype 01 still does not provide diagonal or hierarchical routing, generalized job priorities, speed modifiers, or pawn collision.

This creates a useful seam for:

- deterministic tests;
- replay/debugging;
- headless simulation;
- future AI-controlled test players;
- future networking experiments if ever desired.

Prototype 01 does not require a network protocol or event-sourcing framework.

## 5. Time model

Simulation time should be discrete and deterministic.

The authoritative model remains tick-count based. The Prototype 01 Bevy scheduler currently requests at most one tick every 250 ms (nominal 4 Hz), while headless callers may advance ticks as fast as possible.

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

The current bootstrap chunk side is 32 cells. Raw chunk residency follows [`ADR-0015`](../adr/0015-character-centered-derived-chunk-residency.md): `Simulation` keeps only the deduplicated radius-one neighborhoods around authoritative character chunk positions. Camera visibility does not define residency, and reads outside that set are ephemeral.

## 8. Deterministic world generation

Untouched terrain is generated from at least:

- world seed;
- world-generation version;
- chunk coordinates.

Generation must not depend on chunk visitation order.

Generating chunk `(10, -4)` before `(10, -3)` must not change either chunk's canonical untouched result.

Features crossing chunk boundaries must therefore derive from stable world-space inputs rather than mutable neighboring generation order.

### Current effective-terrain and natural-resource bootstrap

World generation derives two independent untouched layers from `(seed, worldgen version, absolute cell)`: terrain and an optional natural-resource source. Resource sampling uses a separate deterministic domain from terrain. Worldgen v1 and v2 retain their accepted identities; current worldgen v3 keeps v2 terrain unchanged and adds `BerryBush` beside the existing `Tree` and `StoneOutcrop` source kinds. All sources exist only on Grass. Tree/StoneOutcrop have small finite yields and permanently deplete when harvested; BerryBush yields 3–5 physical Berries, becomes temporarily unavailable, and regrows after 512 authoritative ticks through sparse simulation state. Four diagonal starter bushes at `(±3, ±3)` guarantee renewable food capacity near a new colony, while additional wild bushes remain sparse and deterministic. Generation order remains irrelevant. See [`ADR-0018`](../adr/0018-renewable-berry-bushes-and-worldgen-v3.md).

`GeneratedChunk` remains the immutable raw result of deterministic base generation. `Simulation` owns that base-world identity together with a private canonical `ModifiedWorld`, whose sparse `BTreeMap` state records only terrain overrides that differ from the generated base.

`EffectiveChunk` is built on demand by applying those overrides to a raw generated chunk. Authoritative movement passability and application terrain snapshots query this effective terrain, so they agree on the current world rather than bypassing modifications.

Save format v1 persists sparse authoritative changes rather than materialized base chunks. A private derived `ChunkResidency` caches only raw deterministic chunks near characters and unloads them when those character-centered neighborhoods move away; loading rebuilds that cache from restored character positions. A terrain gameplay command remains outside the current bootstrap.

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

Prototype 01 uses purpose-specific spatial indexes rather than one global ECS/grid index: ground items are indexed by chunk, carried items by character, stockpiles/workstations/construction have cell-owner indexes, and jobs maintain source/destination/worker reservation indexes. Natural resources remain deterministic worldgen plus sparse depletion, while pathfinding queries effective terrain/structure occupancy directly. Raw chunk loading/unloading is handled by the character-centered residency cache. A more general spatial index is deferred until population/structure scale measurements justify it.

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

Cancellation removes both source and worker indexes. An accepted player `MoveTo`, Stop, or directional movement releases an assigned worker before applying the player command, returning the unfinished job to `Available`. `MoveTo` now accepts a physical intent even when the exact point is hidden or blocked and resolves it through the closest-reachable player-navigation policy rather than treating that ordinary world condition as a rejected command. Tests cover full completion, deterministic/exclusive assignment, invalid designations, cancellation, manual interruption, and reservation-index consistency.

`progressus-app` exposes `DesignateHarvest`/`CancelJob`, detached job snapshots, and a job revision. The Bevy client exposes Harvest and Cancel jobs through the compact Orders palette; Harvest designates every eligible source in a dragged rectangular area and draws a small state-colored bracket around designated sources. Shift+left-click remains a temporary single-source shortcut.

### Current stockpile/haul bootstrap

`progressus-sim` also owns deterministic stockpile areas and `Haul` jobs. A stockpile is a stable-ID set of discovered walkable ground cells with a unique cell-owner index plus authoritative per-`ItemKind` acceptance policy; item categories (`Resources`, `Food`, `Products`) are typed simulation/application data used to edit groups without turning storage into an abstract inventory. New stockpiles accept all known kinds. Eligible items are hauled only to accepting stockpiles. If policy changes, active now-invalid deliveries are cancelled and an already stored disallowed stack can become an ordinary Haul source for another compatible stockpile; nothing is deleted or teleported. Haul is also not restricted to the outside→inside transition: an underfilled stack already on accepting stockpile ground may itself become a source when another compatible stack in the same stockpile can absorb it. Destination selection is merge-first and only falls back to an empty cell when no compatible merge exists. Internal consolidation moves higher stable IDs toward lower compatible IDs, preventing storage ping-pong. Job indexes reserve one item, one destination cell, and one worker; the worker travels to the exact item position, picks up the canonical stack, carries it to the destination cell center, and drops/merges the same stable stack there. Manual interruption or removal of an active destination releases reservations and drops a carried stack at the worker's current position.

`progressus-app` exposes stockpile cell/policy commands, detached stockpile snapshots including disallowed item kinds, and carried-item snapshots. The Bevy client keeps zone editing in a compact Zones palette rather than a permanently flat toolbar. Visible stockpiles use a translucent ~30% ground overlay above terrain but below physical entities; overlay cells meet without presentation gaps, a selected stockpile gets only its external boundary, and the HUD can hide/show the zone layer. A plain click can select a stockpile while ordinary non-zone tools remain active, and double-click opens its configuration modal. The modal presents category tri-state controls and concrete item toggles backed by that exact stable stockpile ID. Stockpile Add extends an existing stockpile only when the painted rectangle overlaps exactly one existing zone; a non-overlapping painted region creates a new stable ID, and painting across multiple existing stockpiles never implicitly merges their independent policies. Ctrl+left-click remains a temporary single-cell shortcut and creates an independent one-cell stockpile on empty ground. Carried item sprites remain parented to worker visuals so transport stays visible through interpolation. Internal exact stack splitting exists for production supply and meal allocation, but player-directed/general-purpose splitting is not exposed. Stockpile priorities, containers, skills, and generalized job policy remain later work.

### Current nutrition/eating bootstrap

Prototype 02 adds bounded authoritative satiety to each character. New characters start at `100`; satiety decays by one on global 16-tick intervals, so the need depends only on authoritative simulation time. At `<= 50`, a character may receive a character-specific `Eat` job. At zero satiety the character is excluded from ordinary worker assignment until food is available.

Food remains physical. The first concrete food item is `Berries`. When a hungry character needs a meal, the scheduler selects an eligible ground Berry stack, splits exactly `x1` when needed, gives that portion its own stable ID, reserves it against competing item jobs, and routes that specific character to the portion. Completion consumes exactly one Berry and restores `+50` satiety capped at `100`; cancellation or manual interruption consumes nothing. Free food is considered before creating new Haul jobs, but already active logistics are not stolen. Multiple hungry characters can therefore reserve distinct `x1` portions from one larger stack concurrently while total quantity remains conserved. See [`ADR-0016`](../adr/0016-authoritative-satiety-and-physical-eating.md).

Renewable supply is also physical. If a hungry character has no free ground Berries, nutrition maintenance may designate an explored reachable `BerryBush` for ordinary Harvest. That designation does not itself create food: a worker must travel, work, and complete Harvest, which creates one Berries ground stack. Ordinary Haul can move the output to an accepting stockpile. The harvested bush is absent for 512 simulation ticks and then reappears from persisted sparse regrowth state. The bootstrap Berries stack is only 10 units; a no-bootstrap-food regression verifies that the four guaranteed starter bushes can sustain all five characters over 10,000 ticks. See [`ADR-0018`](../adr/0018-renewable-berry-bushes-and-worldgen-v3.md).

`progressus-app` publishes satiety in detached character snapshots and publishes Eat jobs through the ordinary job read model. The Bevy client renders Berries procedurally, localizes food/Eat labels, and shows `Satiety/Сытость` in the selected-character inspector. Satiety and active Eat jobs round-trip through save v1.

### Current workbench/craft bootstrap

`progressus-sim` owns stable-ID workstations, stable-ID production orders, a `ProductionLogisticsWorld`, and a typed `RecipeId::PrimitiveTool` definition (`2 Wood + 1 Stone -> 1 PrimitiveTool`, six work ticks). A production order has an explicit `Finite { remaining_runs }` or `Infinite` target. Only one Craft job can occupy a workstation at a time; pending orders are considered in creation order. Each producer owns explicit Input and Output ground-cell zones with unique cell ownership. The current one-cell Workbench specializes that generic ownership model into exactly two cardinal Input ports plus exactly two diagonal Output ports. Input and Output pairs each cycle independently through six canonical unordered layouts and neither can be edited cell-by-cell through the public simulation API. These cells cannot simultaneously be ordinary stockpile ground. A Craft job reserves concrete physical stacks only from its own Input zone. Missing quantities are split exactly from eligible ordinary stockpile stacks and carried by a dedicated `SupplyProduction` job into a reserved Input cell. Completion consumes exact quantities, preserving partial stack IDs or removing exhausted stacks, creates the physical `PrimitiveTool` in the producer's Output zone, and decrements only finite targets. Ordinary Haul can then move the output from Output to storage. Shared physical inputs cannot be double-reserved; a two-infinite-order regression verifies deterministic shared-source arbitration without starvation. See [`ADR-0012`](../adr/0012-production-input-output-logistics-zones.md).

`progressus-app` exposes workstation, production-order, and production-zone commands, detached workstation/order/logistics snapshots, and authoritative workstation/production/logistics revisions. The Bevy toolbar keeps Workbench only as a point placement tool. Clicking an existing workbench opens a reusable `ModalKind` shell even when another build/designation tool remains active; the active tool is not cleared, so closing the inspector returns directly to that workflow. The workstation view edits authoritative production orders, including an explicit `∞` control; removing a workbench is also handled from that modal. The modal logistics schematic reuses the procedural Workbench image, marks the two physical Input ports in red and the two physical Output ports in yellow, and exposes separate localized rotation actions rather than Workbench logistics paint tools. The same modal shell is intended for later containers, furnaces, research stations, and other inspectors. Shared UI strings use a typed localization layer with Russian as the default language and English as the second built-in language. Bevy's embedded ASCII-only font is not sufficient for Russian, so the bootstrap resolves a local system font with Cyrillic coverage. See [`ADR-0010`](../adr/0010-production-orders-and-localized-modal-ui.md). Workbench placement remains an instantaneous bootstrap exception; physical StoneWall construction is implemented separately through construction sites and delivery/construct jobs.

### Current physical-construction bootstrap

`progressus-sim` owns a stable-ID `ConstructionWorld` for unfinished sites and completed structures. `StructureKind::StoneWall` costs 2 Stone and fixed work. A site reserves one concrete compatible stack, excludes it from competing Haul/Craft use, and creates a `DeliverConstruction` job. The worker physically picks up that canonical stack and drops it at a deterministic reachable work position beside the site; only a delivered stack can enable `Construct`. Completion consumes exactly 2 Stone and converts the site to a finished structure with the same stable ID. A partial remainder stays on the ground beside the wall. Cancellation drops carried material before clearing jobs/reservations.

Finished structure occupancy is separate from terrain. `StoneWall` blocks movement and A*, while `Door` is passable with navigation cost 2 versus ordinary ground cost 1. Door open/closed state is automatic and authoritative: occupancy opens it and a short deterministic hold window closes it after passage. Unfinished blueprints remain passable. Door designation may replace either a planned StoneWall (through normal cancellation/cleanup) or a completed StoneWall (removing that occupancy before creating the Door site); this narrow conversion does not add general demolition or salvage. Stone walls and doors share the same presentation connectivity network, and the door leaf uses one canonical vertical screen orientation regardless of the surrounding wall run. The Bevy client exposes Stone wall plus Door in the Build palette; Cancel-jobs removes unfinished sites but does not demolish completed structures. See [`ADR-0009`](../adr/0009-physical-construction-sites-and-blocking-structures.md).

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

`progressus-sim` owns a deterministic `ItemWorld` keyed by the same global stable `EntityId` space as characters. Current item content is deliberately tiny (`Wood`, `Stone`, `PrimitiveTool`, and `Berries`), but every stack has a quantity in `1..=1024` and exactly one canonical location: exact fixed-point ground position or carrier character ID. Ground items are indexed by chunk and carried items by character; transfer operations update the canonical stack and its indexes atomically. Character interaction reach uses integer `InteractionRadius` geometry, and dropping is rejected onto non-walkable effective terrain.

`progressus-app` exposes only explored ground items in requested chunks as detached `GroundItemSnapshot` values plus an item revision. Natural-resource snapshots are likewise chunk-scoped and contain only explored currently-available source cells; a separate resource revision covers authoritative depletion and renewable regrowth visibility changes. Carried stacks are published as detached carrier-linked snapshots. The Bevy client reconciles disposable ground-item entities by stable Progressus ID and converts their exact positions relative to the nearby render origin, just like characters.

Harvested resources create ordinary physical ground stacks, including Berries from renewable BerryBush sources, and the haul bootstrap can move those stacks through the canonical `Carried` state into designated stockpile cells. A stockpile is intentionally not a container: delivered stacks remain `Ground` at exact positions inside the stockpile area, as recorded by [`ADR-0006`](../adr/0006-stockpiles-remain-physical-ground.md). Haul uses a merge-first destination policy: a compatible underfilled same-kind stack is preferred over an empty stockpile cell when the combined quantity is at most 1,024. Underfilled stacks already inside the same stockpile are also valid Haul sources and consolidate toward a lower stable-ID target, preventing ping-pong; the destination stack keeps its stable ID and the emptied source stack is removed. Exact internal stack splitting now exists for production supply and per-character meal portions, but there is still no player-directed/general-purpose splitting UI or real container/storage location; physical stacks and active logistics round-trip through save format v1, while raw generated chunks use bounded derived residency. Crafting, StoneWall construction, and autonomous eating now provide explicit quantity-consumption paths over physical stacks.

### Procedural presentation assets

Prototype visual art is source-controlled as Rust code under `assets/procedural/`. The Bevy client supplies a small integer RGBA canvas/rasterizer and lazily turns those recipes into nearest-filtered `Image` assets. Terrain, characters, ground-item stacks including Berries, primitive tools, workbenches, doors, trees, stone outcrops, and berry bushes use bounded deterministic variants selected from `WorldCell` or stable `EntityId`; stack quantities use cached procedural bitmap labels rather than antialiased world-space font rendering. Water and Rock additionally carry a presentation-only eight-neighbour topology mask: known unlike neighbours derive shore/foothill strips and rounded cardinal/diagonal corners, while unknown neighbours are treated as visually continuous so terrain art cannot reveal undiscovered worldgen. Every known non-Grass terrain cell has a deterministic Grass presentation underlay; Water/Rock overlays may therefore clear alpha in convex and diagonal corner pixels so rounded silhouettes reveal grass rather than the world background. The registry caches each recipe/variant/topology or quantity label rather than allocating a texture every frame.

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

Prototype 01 implements deterministic bounded cardinal A* over `WorldCell` positions. The search reads effective terrain, completed structure occupancy/cost, and the monotonically explored-world boundary; it never requires a finite global map or probes hidden terrain. Ordinary walkable ground costs 1, a Door costs 2, and a StoneWall is impassable; Manhattan distance remains an admissible minimum-cost heuristic. Equal-cost tie breaking is deterministic and regression-tested, and resulting cell paths are converted to exact fixed-point waypoints owned by character navigation state. Player navigation can additionally select the currently closest reachable explored frontier toward a hidden or blocked intent and deterministically replan after discovery while retaining that original intent. Every movement transition is revalidated, so a newly blocked cell stops the character at the canonical boundary rather than allowing stale-route penetration. Final waypoint construction approaches the exact sub-cell destination from the actual arrival side rather than entering the target-cell center and visibly backtracking.

The search budget is deliberately local/bounded. There is no diagonal search, hierarchical long-distance routing, road/rail graph routing, or generalized dynamic-obstacle repath. The current replan is narrowly the player exploration-intent behavior. Later routing layers can replace/compose above this encapsulated pathfinding service without changing authoritative character identity or the application command boundary.

## 18. Headless mode

The Simulation Core must be runnable directly as pure Rust without initializing Bevy, a window, renderer, graphics device, or audio system.

Required uses:

- unit tests;
- integration tests;
- deterministic reproduction of bugs;
- accelerated simulation runs;
- performance qualification;
- long-span and population-scale experiments.

The implemented executable is `progressus-headless`, with deterministic `--ticks`, `--travel-chunks`, and `--activity-smoke` scenarios. `scripts/check-prototype-01.sh` runs the complete server-safe acceptance gate and the activity smoke performs save/load during physical logistics.

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

- deterministic world launch selection (`--seed`, default `0`);
- camera;
- rendering;
- selection;
- placement previews;
- UI panels and the selected-character inspector;
- input mapping;
- sound and animation;
- procedural visual generation and caching.

It sends commands to the application/simulation layer and presents resulting state.

Visual interpolation may exist between simulation ticks but must never become authoritative gameplay state.

The client should treat simulation read models as input data. Bevy scene/ECS state must be disposable and reconstructable from authoritative state where necessary.


### Cardinal connectivity for cell networks

Cell-network presentation uses a generic four-bit N/E/S/W neighbour mask. StoneWall and Door blueprints/finished structures now participate in one wall-network set: procedural arms extend to tile edges for all 16 isolated/end/straight/corner/T/cross masks, so doors join neighbouring walls cleanly. The helper remains presentation-derived from authoritative cells and is intended for future roads, fences, pipes, or similar cardinal networks; see [`ADR-0011`](../adr/0011-cardinal-connectivity-autotiles.md). Natural terrain uses a separate eight-neighbour presentation mask because diagonal information is needed to round coast and mountain corners.

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
- `progressus-sim` owns authoritative time, stable identities, characters/navigation/needs, jobs/reservations, physical items and stockpiles, workstations/production logistics, construction, sparse world modifications, persistence DTOs, and the derived raw-chunk residency cache.
- `progressus-app` is the command/query boundary and returns detached read models.
- `progressus-headless` proves that the application can run and be inspected without a renderer.
- `progressus-client` is a native Bevy presentation consumer. It launches seed `0` by default or a requested `--seed`, owns pause timing/save-slot files/localization/selection/inspectors, the icon-first HUD and its Orders/Zones/Build palettes, zone-layer visibility, tooltips, and stockpile/workstation modal presentation, and renders only detached application read models. Its only direct dependencies are Bevy and `progressus-app`; it must not directly depend on `progressus-sim` or `progressus-worldgen`.

The headless application chain is Bevy-free. The boundary guard scans that complete chain and verifies the client's direct-dependency rule. The Bevy client converts `ClientSnapshot` data into disposable entities mapped from stable Progressus IDs; those mappings are derived presentation caches, never authoritative or persistent state.

`progressus-sim` also owns the in-memory, monotonic `ExploredWorld`: every character reveals the Euclidean disk of cells within the provisional radius `5` around its containing cell at new-game creation and after each authoritative tick. Discovery is independent of selection and camera position. `progressus-app` publishes terrain as detached `KnownTerrain`; it omits entirely unknown chunks and never gives the client a terrain type for an undiscovered cell. A player may submit an exact `MoveTo` intent in unknown space, but route planning expands only through already explored walkable cells and publishes only that explored route segment. As the character reveals more cells the simulation replans internally toward the same intent, so accepting the order cannot become a hidden-terrain query side channel.

The client first requests a lightweight character snapshot, establishes a disposable render origin, and requests only chunks intersecting the camera viewport plus a small presentation margin. Those chunk-scoped reads contain only explored state, so undiscovered terrain/items/resources do not become a client-side information leak. Spatial snapshot layers are independently selectable: viewport/exploration changes may request terrain plus visible items/resources, while item-only or resource-only revisions explicitly skip terrain generation. Presentation therefore reconciles ground items and natural resources without despawning/recreating the visible terrain tree. Terrain is rebuilt only when the visible chunk window or explored-terrain state can actually have changed, not merely because one stack was split/eaten or one bush entered/regrew from cooldown. The camera can pan over unknown background but cannot discover or inspect terrain; no character, including Cora (`EntityId` 3), is special to discovery or terrain selection. Keyboard pan and middle-mouse drag plus wheel zoom alter only presentation camera state; middle-button drag is explicitly excluded from selection/designation/move-order handling. Mouse-drag deltas are inverted relative to camera translation, so dragging the pointer right/down moves the visible world left/up.

The presentation scheduler is non-authoritative: it requests at most one simulation tick every approximately 250 ms (nominally 4 Hz) and discards a long-frame backlog rather than catching up. Rendering frame time never becomes simulation input.

This bootstrap proves rendering, tool-based rectangle/point designation, snapshot-driven mapping, camera-driven explored-terrain refresh, effective-terrain snapshots, deterministic sub-cell living positions, and bounded deterministic `MoveTo` intent/navigation through the application boundary required by [`ADR-0004`](../adr/0004-grid-world-continuous-living-movement.md). A* remains cardinal and cell-topological; player exploration intent supports the narrow frontier replanning described above, but generalized dynamic-obstacle/hierarchical routing is not implemented. Exact route waypoints remain presentation-readable only for an explicitly requested character, while every lightweight `CharacterSnapshot` carries only its detached last-tick motion trace so all visible characters can interpolate smoothly between authoritative ticks. A trace is the path completed in that authoritative tick, including a one-point trace while idle, so presentation cannot replay stale arrival motion after a later snapshot. It does not implement diagonal or hierarchical navigation, generalized jobs/AI priorities, speed modifiers or pawn collision, demolition, door locking/access policy, autosaves, entity sleeping/Simulation LOD, or general save migration across arbitrary pre-release development formats; unsupported versions are rejected explicitly. Raw deterministic chunk residency and manual versioned save/load are implemented. Harvest, Haul, Craft, physical StoneWall construction, and Prototype 02 Eat are complete job bootstraps: Wood/Stone/PrimitiveTool/Berries stacks remain physical across starting supplies, harvested outputs, carrying, filtered stockpile delivery, same-kind stack merging up to the 1,024-unit capacity, local recipe consumption, crafted output, exact meal splitting, and food consumption. Character satiety is authoritative, deterministic, persistent, and exposed through the localized inspector. The selected-character route, selection bracket, localized character-state inspector, stockpile selection/configuration UI, zone overlay/toggle, grouped icon HUD, and hover tooltips are presentation state. Plain left-click gives existing workstations/characters priority over the active build/designation tool without clearing that tool; a stockpile click also selects its exact stable-ID zone, while an actual Stockpile Add/Remove drag remains a zone-edit gesture. HUD tooltips follow the hovered pointer/control and are placed above the bottom palette rows instead of occupying one fixed panel location. F3 adds resident-chunk outlines and the selected character's technical authoritative-position marker. Coordinates and provisional chunk geometry remain specified by [`ADR-0003`](../adr/0003-bootstrap-world-coordinates.md).
