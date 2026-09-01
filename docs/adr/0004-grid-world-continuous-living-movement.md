# ADR-0004: Grid world with continuous authoritative movement for living entities

Status: **Accepted**

Date: 2026-09-01

Decision owner: project owner

## Context

ADR-0003 deliberately selected integer world-cell coordinates and 32x32 bootstrap chunks without deciding how characters and other living entities should move inside that grid.

The first movement bootstrap represents a character only by an integer `WorldCell` and advances it by at most one cardinal cell per simulation tick. The first Bevy client therefore renders movement as discrete cell-to-cell jumps. This is useful for proving deterministic cross-chunk traversal, but it is not the intended final movement model.

Progressus needs a spatial model closer to games such as RimWorld: terrain, walls, buildings, resource deposits, work sites, and pathfinding topology are grid-aligned, while characters, animals, and other mobile living entities move continuously between traversable cells.

## Decision

Progressus keeps a **cellular world topology** but does **not** make living entities permanently cell-quantized.

### 1. The world remains grid based

The following concepts remain aligned to `WorldCell` or multi-cell footprints:

- terrain;
- walls and doors;
- buildings and construction footprints;
- static resource deposits and similar world objects;
- stockpile/designation regions where applicable;
- pathfinding nodes and occupancy/passability decisions.

Chunks remain groups of world cells as defined by ADR-0003.

### 2. Living movement is authoritative at sub-cell resolution

Characters, animals, and other mobile living entities may occupy positions between cell centers while moving.

A route may still be expressed as a sequence of traversable cells, but traversal of each route segment takes simulation time rather than teleporting the entity from one integer cell coordinate to the next.

Conceptually:

```text
path cells:       [10,10] -> [11,10] -> [12,10]
authoritative:    center ---- sub-cell positions ---- center ---- ...
```

The authoritative simulation, not Bevy interpolation, owns progress along the current movement segment.

### 3. Authoritative sub-cell coordinates use deterministic integer/fixed-point representation

Do not use renderer `f32`/`f64` coordinates as authoritative simulation state.

The implementation should use a Progressus-owned deterministic integer or fixed-point representation for sub-cell position/progress. The exact scale (for example, subunits per cell) is intentionally left to the implementation design and tests.

Required properties:

- exact deterministic updates for an identical command/tick sequence;
- stable behavior across platforms supported by the simulation;
- explicit overflow handling;
- lossless conversion of integer `WorldCell` centers into the chosen representation;
- no dependence on Bevy transforms, frame delta, or renderer interpolation.

### 4. Speed is simulation state

Movement speed controls how much authoritative distance an entity advances per simulation tick.

Different entities may later have different speeds, and gameplay systems may modify speed. A movement tick is therefore not synonymous with "move exactly one cell".

If a tick advances far enough to reach or pass a route waypoint, the movement system must deterministically consume the remaining distance according to the movement design rather than relying on render frames.

Exact acceleration, diagonal-cost rules, and multi-waypoint-per-tick policy remain implementation decisions and require tests before adoption.

### 5. Pathfinding remains grid-topological

This decision does not require continuous-space pathfinding.

The intended separation is:

```text
pathfinding:      sequence of traversable world cells / portals
movement:         continuous authoritative traversal between them
presentation:     rendering of authoritative position, optionally interpolated visually
```

Walls, blocked terrain, buildings, doors, and future reservations therefore continue to constrain traversal through the grid topology.

### 6. Entity visual size and physical footprint are separate concepts

A creature may be drawn smaller than, approximately equal to, or larger than one terrain cell without that rendering size defining simulation occupancy.

Normal characters and animals may initially navigate with a one-cell logical occupancy model while still moving continuously.

Future large creatures, vehicles, or machines may require a multi-cell footprint, radius, clearance class, or another explicit collision/occupancy model. That must be represented in authoritative simulation state and pathfinding rules; it must not be inferred from sprite dimensions.

### 7. Current movement is explicitly bootstrap-only

The existing `WorldCell + Direction`, one-cell-per-tick movement remains valid only as Prototype 01 bootstrap evidence for:

- deterministic ticking;
- passability checks;
- command/application boundaries;
- cross-chunk traversal;
- headless testing.

It is not the target movement model and must not be treated as the foundation for final speed, pathfinding, collision, animation, or creature-size semantics.

Before general pathfinding and gameplay movement are considered complete, the simulation must migrate to the sub-cell authoritative model described by this ADR.

## Consequences

### Positive

- Terrain and construction retain a simple deterministic grid representation.
- Pathfinding can remain finite/local over cells without requiring continuous-space navigation.
- Characters and animals can move smoothly and at different speeds.
- Rendering no longer needs to fake the entire notion of movement with presentation-only tweening.
- Future injuries, species differences, equipment, roads, doors, terrain costs, and similar systems can affect real simulation speed.
- Large visual sprites do not accidentally dictate collision rules.

### Costs

- Character position becomes more complex than a single `WorldCell`.
- Movement snapshots and save/load will eventually need sub-cell state.
- Movement across chunk/cell boundaries must be carefully canonicalized.
- Collision, reservations, interactions, and reaching work targets will need explicit definitions for which cell(s) a moving entity currently occupies or claims.

## Non-decisions

This ADR does not yet choose:

- the number of fixed-point subunits per cell;
- diagonal movement and its cost;
- acceleration/deceleration;
- exact collision geometry;
- multi-cell creature footprint representation;
- local avoidance between moving creatures;
- path reservation semantics;
- visual interpolation policy in the Bevy client;
- whether a very fast entity may consume multiple path segments in one simulation tick.

Those decisions should be made from the requirements of the first real navigation/speed increment and backed by deterministic tests.
