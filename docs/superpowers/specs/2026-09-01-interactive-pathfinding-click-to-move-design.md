# Interactive pathfinding, exact click-to-move, and visual movement probe

**Status:** Approved for implementation
**Date:** 2026-09-01

## Goal

Add the first authoritative `MoveTo` flow: a Bevy client selects a character,
quantizes a right-click into an exact fixed-point destination, and sends an
ordinary application command. Pure Rust simulation builds and executes a
deterministic, bounded, cell-topological route; the client observes detached
snapshots and presents its route and authoritative/interpolated positions.

This builds on ADR-0004 and the implemented sub-cell bootstrap. World terrain,
chunks, passability, and A* nodes remain `WorldCell`; living positions and
route waypoints remain exact `WorldPosition` values. No authoritative float,
Bevy type, wall-clock delta, or renderer state enters simulation.

## Authoritative route model

`progressus-app::Command` gains:

```rust
MoveTo { character_id: EntityId, destination: WorldPosition }
```

`Character` has a private `NavigationRoute` with the requested destination and
remaining `VecDeque<WorldPosition>` waypoints. The public movement read state
becomes:

```rust
MovementState::Idle
MovementState::ManualDirectional(Direction)
MovementState::Navigating { destination: WorldPosition }
```

The route itself remains private. A successful `MoveTo` atomically replaces a
manual direction or route. A failed command leaves all previous movement and
route state unchanged. `StopMovement` clears a route without moving or
snapping; `SetMovementDirection` atomically clears a route and enters manual
directional mode. A successful replacement starts at the current exact
position.

`destination == current exact position` succeeds, clears old intent, and
leaves the character idle. Any other destination whose containing cell is not
effective-walkable is rejected. The command reports distinct errors for an
unknown character, blocked destination, exhausted frontier, and exhausted
search budget. `WorldPosition` is already canonical and in-domain, so no new
untrusted coordinate form is accepted by this API.

## Deterministic, bounded A*

`progressus-sim` owns an internal A* module. Its nodes are lazily opened
`WorldCell`s, cardinal neighbours are considered exactly in `East, North,
South, West` order, and the heuristic is Manhattan distance. Open and closed
state use ordered collections; each candidate carries a monotonically
increasing insertion number, so the priority key is `(f_score, h_score,
insertion_number, cell)` and never depends on hash iteration order.

`PATHFINDING_NODE_BUDGET` is `50_000`. Removing the final open-set candidate
increments the count. An empty frontier is `PathNotFound`; reaching the budget
before finding a goal is `SearchBudgetExceeded`.

Each search owns a temporary `BTreeMap<ChunkCoord, EffectiveChunk>`. Terrain
lookups split a cell, obtain a generated-effective chunk once for that search,
and read its local terrain. The cache is dropped when the search returns: it
is neither simulation residency nor a persistent/materialized world cache.

If an override makes a pawn's current cell blocked, A* still uses that cell as
its initial origin. It may expand outward from it, but no neighbour expansion
may re-enter it; all other path cells, including the destination, must be
effective-walkable.

## Exact route geometry and execution

A* produces coarse cells. Simulation converts them to fixed-point cardinal
waypoints without forcing the pawn to remain centered:

1. For a different destination cell, move from current position to the start
   cell center via X then Y legs; omit zero-length legs.
2. Follow the A* cell centers, excluding the already represented start center.
3. Move from destination cell center to exact destination via X then Y legs;
   omit zero-length legs.
4. For a destination in the same walkable cell, use only the direct local X
   then Y legs.

The executor consumes integer speed across waypoint legs in a tick. On
reaching a waypoint it appends it to the motion trace and spends any remaining
distance on the next leg. It never uses diagonal authoritative translation.
Arrival is only exact `position == requested destination`; then the route is
cleared and movement becomes idle.

Every coarse transition repeats the current effective-terrain check. If a
planned next cell becomes blocked, the existing half-open boundary rule is
used, the remaining tick distance is discarded, and the route is cleared with
an idle state. There is no auto-repath. A pawn already stranded in a blocked
source cell may move inside it and exit through a walkable transition, but is
never allowed to enter another blocked cell.

## Motion trace and read model

Each character retains a transient `last_tick_motion_trace` derived while the
last simulation tick runs. Its canonical idle/no-motion form is one point:
`[current_position]`; moving traces contain `[start, crossed_waypoint..., end]`.
`AdvanceTicks { count > 1 }` retains only the final executed tick's trace.
The trace is not a save format or gameplay/persistence state.

`SnapshotQuery` gains `navigation_for: Option<EntityId>`. When selected, the
ordinary detached `ClientSnapshot` includes at most one detached
`NavigationSnapshot`:

```rust
NavigationSnapshot {
    character_id: EntityId,
    destination: Option<WorldPosition>,
    remaining_waypoints: Vec<WorldPosition>,
    last_tick_motion_trace: Vec<WorldPosition>,
}
```

Normal lightweight and headless snapshots neither request nor copy routes.
The client passes its presentation-only selected ID into this query; it never
mutates the returned route values.

## Client interaction and presentation

Selection is a client-only resource. A left click selects the nearest visible
character inside a fixed presentation hit radius; equal distances break by
stable `EntityId`; an empty click clears it. A selected entity disappearing
from a later snapshot also clears selection.

For a selected character, a right-click converts its camera viewport point to
exact world position by subtracting/adding the current exact cell-center
presentation origin, scaling local render units by `SUBUNITS_PER_CELL`,
rounding to nearest subunit, checked-adding, and finally calling
`WorldPosition::from_subunits`. Floats exist only in this input-quantization
boundary. The resulting `MoveTo` is sent via `Application::execute`; command
rejection is logged and does not optimistic-edit selection, destination,
route, or authoritative position.

Client presentation creates disposable selected/destination/route debug
entities. F3 toggles them. The overlay shows the exact authoritative position,
the interpolated sprite, selected marker, destination marker, and a polyline
of remaining exact waypoints.

The character sprite interpolates along the selected character’s latest trace
by polyline distance during the 250 ms presentation interval, clamped at the
authoritative final point. It never interpolates across the chord of a
multi-leg tick, extrapolates past authority, or writes to simulation. Every
fixed-point point is converted after subtraction from the current cell-center
render origin, so chunk rebasing cannot produce a global-float or half-cell
jump.

## Verification and scope

Authoritative tests cover deterministic straight/equal-cost obstacle paths,
turns, same-cell and negative exact targets, chunk crossing, override
block/open, unreachable/budget outcomes, stranded escape, command replacement
and failure preservation, stop/manual interruption, invalidated next route
cell, exact arrival, tick remainder, and motion trace turns. Client tests cover
selection tie-breaking, exact quantization (center, arbitrary, negative,
boundary, pan/zoom-local invariance), trace-polyline interpolation, rebase,
overlay-from-snapshot, and rejected-command authority preservation.

The full Rust/headless/application/client boundary gates and both existing
headless scenarios remain required. A graphical smoke test is attempted when a
display and GPU are available; otherwise the absence of that manual evidence is
reported explicitly.

This increment does not add diagonal pathfinding, pawn collision or avoidance,
auto-repath, jobs/AI, reservations, multi-cell footprints, save/load,
hierarchical navigation, physics, inventory/resources, or an authoritative UI
selection state.
