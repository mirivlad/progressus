# Authoritative sub-cell spatial model

**Status:** Approved for planning
**Date:** 2026-09-01

## 1. Goal

Keep Progressus world topology coarse and cellular while giving living entities
and future small world objects exact, deterministic positions within those
cells. The model must support a future playable movement probe: a player
selects a pawn, clicks an exact destination, the authoritative simulation finds
a path over `WorldCell` nodes, and the client visualizes the resulting movement
without owning any simulation truth.

This specification replaces the previously considered cell-center/navigation-
segment model as the target spatial model. Cell centers remain useful route
waypoints, but they are not the only valid authoritative positions.

## 2. Scope and non-goals

This increment defines the spatial contract and the minimum migration needed
to make exact positions authoritative. It does not implement the next
interactive-pathfinding increment.

It does not add:

- a sub-cell navigation graph, A*, Dijkstra, or any other pathfinder;
- a physics engine, pawn-pawn collision, local avoidance, or full object
  collision geometry;
- jobs, AI, reservations, inventory interaction, or resource gameplay;
- save/load or a save representation for positions;
- diagonal movement rules, acceleration, inertia, or a final creature
  footprint model;
- Bevy authority, Bevy dependencies below `progressus-client`, or renderer
  frame timing as simulation input.

The next increment may add cardinal A* over `WorldCell` and a `MoveTo` command,
but it must consume this spatial model rather than create a second one.

## 3. World topology remains cellular

`WorldCell` remains the sole coarse/topological world coordinate. It continues
to define:

- terrain and effective-terrain overrides;
- chunk geometry and world-generation queries;
- water, rock, walls, buildings, resources with large footprints, and other
  future static topology;
- the initial pathfinding graph and its cardinal neighbours.

No global six-by-six, twelve-by-twelve, or other smaller terrain grid is
introduced. `CHUNK_SIDE`, raw world generation, `EffectiveChunk`, and sparse
modified terrain keep their existing `WorldCell` semantics.

## 4. Exact authoritative position

`progressus-sim` introduces a Progressus-owned public value type:

```rust
pub struct WorldPosition { /* private canonical i128 coordinates */ }
```

It represents a point in global fixed-point world space. Its coordinates are
signed `i128` subunits, but they are private: callers cannot construct an
invalid or non-canonical position by writing arbitrary public fields.

```text
SUBUNITS_PER_CELL = 1024
cell N on an axis = [N * 1024, (N + 1) * 1024)
cell center = N * 1024 + 512
```

`1024` is a named simulation-coordinate contract, not a repeated magic
literal. It is deliberately independent of movement speed. The initial
bootstrap speed may later be `256 subunits/tick` (one cell of distance in four
ticks), but changing that parameter must not change this coordinate format.

The type provides only controlled operations, conceptually:

```rust
pub const SUBUNITS_PER_CELL: i128 = 1024;

impl WorldPosition {
    pub fn from_cell_center(cell: WorldCell) -> Result<Self, PositionError>;
    pub fn from_subunits(x: i128, y: i128) -> Result<Self, PositionError>;
    pub fn x_subunits(self) -> i128;
    pub fn y_subunits(self) -> i128;
    pub fn containing_cell(self) -> Result<WorldCell, PositionError>;
    pub fn checked_translate(self, delta: WorldDelta) -> Result<Self, PositionError>;
}
```

The final names may differ, but the following properties are required:

1. conversion of a `WorldCell` center uses checked multiplication and addition;
2. a valid position is restricted to the representable `WorldCell(i64)` domain
   on both axes, so every valid position has exactly one containing cell;
3. `containing_cell()` uses Euclidean division by `1024`, including for negative
   coordinates; and
4. all movement translation, delta, and distance operations use checked integer
   arithmetic and return an explicit error or deterministic normal stop rather
   than wrapping.

For example, `WorldCell(-1, 0)` has center `(-512, 512)`. A position with
`x = -1` belongs to `WorldCell(-1, _)`; a position with `x = 0` belongs to
`WorldCell(0, _)`. This follows directly from `div_euclid`, not a special
negative-coordinate branch.

`WorldPosition`, related deltas, and any later interaction-radius value derive
the ordinary value traits needed for deterministic snapshots and comparisons
(`Clone`, `Copy`, `Debug`, `Eq`, and `PartialEq` at minimum). They contain no
authoritative `f32` or `f64`.

## 5. Three deliberately separate spatial concepts

The following concepts must not be conflated:

| Concept | Authoritative meaning | Consumer |
| --- | --- | --- |
| `WorldPosition` | exact sub-cell point owned by the simulation | movement, objects, interactions, snapshots |
| containing `WorldCell` | coarse cell computed from a position through Euclidean division | terrain lookup, chunk selection, topology/pathfinding, coarse gameplay rules |
| presentation position | disposable render-space value derived from a snapshot | Bevy transform and optional visual smoothing |

The client may convert a nearby `WorldPosition` to Bevy `f32` coordinates after
subtracting a local presentation origin. That conversion is never stored in
`Simulation`, sent back as authoritative state, or used to decide movement or
passability. A click is quantized at the client/application boundary into an
explicit `WorldPosition`; once a command contains that value, authoritative
results depend only on that command sequence and ticks, not on FPS.

## 6. Living entities and movement migration

`Character.position: WorldCell` is replaced by an authoritative
`WorldPosition`. Bootstrap spawn positions are the centers of the existing
walkable spawn cells, preserving their stable IDs and initial terrain
invariants.

`CharacterSnapshot` publishes the owned `WorldPosition`, not a Bevy vector or
a float. It also retains or gains a derived containing `WorldCell` only when a
consumer needs a cheap explicit coarse value; the two values must be generated
from the same authoritative position and cannot diverge. The preferred
read-model shape is:

```rust
pub struct CharacterSnapshot {
    pub id: EntityId,
    pub name: String,
    pub position: WorldPosition,
    pub containing_cell: WorldCell,
    pub movement: MovementState,
}
```

Publishing both is a detached read-model convenience, not duplicated
authoritative state. Snapshot construction derives `containing_cell` from
`position` and treats a conversion failure as an impossible invariant
violation.

The present cardinal `SetMovementDirection` / `StopMovement` boundary remains
the only movement-control boundary during this migration. A later `MoveTo`
command belongs to the pathfinding increment, not this one.

The replacement movement implementation advances an exact position by an
integer `speed_subunits_per_tick`, never by one cell merely because a tick
occurred. It keeps cardinal bootstrap control, checks effective terrain before
admitting any coarse-cell transition, preserves partial distance and remaining
tick distance, and permits multiple checked coarse-cell transitions in a tick
when speed requires it. Water, rock, coordinate overflow, and a target that
becomes blocked remain normal deterministic stops; none may wrap coordinates
or cause an implicit snap to a cell center.

### Blocked-cell boundary invariant

An authoritative point pawn must never have a `containing_cell()` whose
effective terrain is impassable after a movement step. Cells use the canonical
half-open axis intervals from section 4. Therefore, when an attempted cardinal
transition would first enter a blocked cell, movement consumes only the largest
integer distance that leaves the point in its current walkable cell, then
becomes `Idle` and discards the remainder of that tick's movement distance.

For a boundary coordinate `B` between two cells on the moving axis:

- moving positive toward a blocked destination stops at `B - 1`;
- moving negative toward a blocked destination stops at `B`, because `B`
  belongs to the source cell under the half-open convention.

The perpendicular coordinate is unchanged. This one-subunit directional
asymmetry is not a second occupancy model: it follows solely from the
canonical `containing_cell()` rule. It is preferred to representing a pawn at
the geometric boundary of a terrain cell it is forbidden to occupy.

The implementation examines each coarse-cell boundary in travel order. If a
large speed would cross several cells, it consumes the valid portions and
checks every next cell through `effective_terrain_at`; the first blocked cell
ends the tick. Reverse movement after a boundary stop begins from the retained
exact position with no snap. Existing direction-replacement semantics retain
their atomic validation rule: an invalid replacement does not erase a valid
ongoing movement state.

This specification deliberately does not impose a new final route/turning
geometry. The next `MoveTo` increment will define route waypoints as exact
positions, with interior cardinal path waypoints normally at `WorldCell`
centers and the last waypoint at the requested exact destination. It must
remain valid for a pawn that begins at an arbitrary sub-cell position.

## 7. Coarse pathfinding connected to exact destinations

The planned first pathfinder remains a lazy, bounded graph over `WorldCell`:

```text
start containing cell -> cardinal walkable cells -> destination containing cell
```

It does not search a sub-cell lattice and does not materialize a finite global
map. A destination is valid only when its containing cell is walkable through
effective terrain.

For the future executor, the path turns into exact waypoints:

1. the pawn starts at its actual `WorldPosition`;
2. it follows a local segment inside its current walkable cell when needed;
3. it follows cardinal cell-center waypoints for interior path cells;
4. it ends at the exact requested destination inside the final walkable cell.

Arrival is exact: the destination is reached only when the authoritative
position equals the requested `WorldPosition`, not merely when the pawn enters
the destination cell. An interaction system may later choose an approach
position inside interaction range instead of an object's center; that is a
separate deterministic command/planning decision.

If topology changes so that a future route's next cell is no longer walkable,
the first pathfinding bootstrap may stop and invalidate the route rather than
replan. It must never resume the old one-cell-per-tick teleport model.

## 8. Point objects, small footprints, and interaction radius

Items and small resources are not implemented in this increment, but their
spatial contract is fixed now:

- a point-like object has one `WorldPosition`;
- an object that needs a small local reach extent has a center `WorldPosition`
  plus a circular `InteractionRadius` measured in subunits;
- a point-like item has `InteractionRadius::zero()`;
- `InteractionRadius` is reach/interaction geometry only. It is not a physical
  collision radius, pawn-collision body, terrain blocker, or multi-cell
  navigation footprint.

`InteractionRadius` is a named integer value type with private representation.
The initial interaction predicate is exact closed-circle reach:

```text
distance_squared(actor.position, object.position)
    <= (actor.interaction_radius + object.interaction_radius)^2
```

No float square roots are used. The implementation must avoid overflow by
first proving each absolute axis distance is no greater than the finite summed
radius, then computing the squares with checked `i128` arithmetic. A pair too
far on either axis is simply out of range and does not need a giant global
distance square. This gives corners and any apparent triangular approach area
their natural geometric result rather than encoding cell sectors.

Large buildings, walls, water, rock, and future multi-cell creatures retain
explicit topological/footprint models; they must not be silently represented as
small circles merely because the interaction predicate exists. If an object
later needs real physical extent or collision, that will be a separate
authoritative concept; `InteractionRadius` must not become a hidden substitute
for it.

## 9. Determinism, passability, and errors

- `Simulation` remains pure Rust and headless; no rendering clock, Bevy type,
  RNG, or wall-clock delta affects its spatial transitions.
- Passability reads only `effective_terrain_at`, never raw generated terrain.
- The same seed/worldgen version, modified-world state, ordered commands,
  exact destination values, and tick count produce equal authoritative state.
- Public constructors and checked arithmetic reject out-of-domain positions and
  return explicit simulation/application errors where a command is invalid.
- A normal movement stop caused by an impassable future cell or representable
  coordinate limit is deterministic and leaves the last exact valid position
  intact.
- Presentation interpolation may converge to a newer snapshot position but
  may not extrapolate indefinitely, alter a command, or write a value back to
  the simulation.

## 10. Required tests

The implementation plan must include focused RED/GREEN tests for at least:

1. center conversion and round-trip containment for zero, positive, and
   negative cells, including both sides of a cell and chunk boundary;
2. exact Euclidean containment at `-1`, `0`, and other cell boundaries;
3. checked center construction, translation, and position-domain overflow
   behaviour without wraparound;
4. movement by less than one cell per tick, exactly one cell after enough
   ticks, different speeds, and exact remainder preservation;
5. a large speed consuming multiple sequential coarse-cell transitions in one
   tick;
6. positive and negative chunk transitions, with the same `EntityId` and an
   exact sub-cell position;
7. deterministic equality for identical seeds, effective terrain mutations,
   commands, and tick counts;
8. blocked destinations and terrain changed to blocked while a pawn is moving,
   with no illegal snap or raw-worldgen lookup;
9. `StopMovement` and directional replacement at a non-center exact position;
10. snapshots carrying the exact `WorldPosition` and a containing cell derived
    from it, without a simulation/client mutable alias;
11. point and radius interaction geometry across cell interiors and cell
    borders, including a safe far-apart/overflow-resistant comparison; and
12. all existing modified-world effective-terrain, app-boundary, headless,
    client, worldgen, and Bevy dependency-boundary tests.

Blocked-boundary coverage must separately prove blocked transitions east, west,
north, and south; repeat them near negative coordinates; verify the resulting
`containing_cell()` is always walkable; verify the tick remainder is discarded
after the first blocked boundary; and verify reverse movement resumes from the
retained exact position without a snap.

The subsequent pathfinding/client increment additionally needs authoritative
tests for exact final destinations, route invalidation on changed terrain,
bounded lazy search, and manual visual checks of click movement, turns,
chunk crossings, negative positions, and presentation smoothing against an
authoritative marker.

## 11. Documentation and milestone status after implementation

Implementation documentation must state precisely that the bootstrap has
advanced from cell-teleport movement to deterministic exact positions. It must
not claim that navigation is complete: A*, click-to-move, jobs/AI policy,
pawn collision, multi-cell clearance, persistence, residency, and final
speed/animation rules remain unfinished.

ADR-0004 remains normative: living entities are continuous while terrain and
pathfinding topology are cellular. This specification adds local precision to
the existing world; it does not shrink or replace the world grid.
