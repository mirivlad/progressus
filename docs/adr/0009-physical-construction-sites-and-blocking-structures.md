# ADR-0009: Physical construction sites and blocking structures

Status: Accepted

## Context

Prototype 01 requires construction to consume delivered physical material and labor. A structure must not appear merely because matching resources exist elsewhere in the world.

The existing item model already provides stable stack IDs, exact Ground/Carried locations, reservations, and quantity consumption. Construction should reuse those semantics rather than introduce a global building inventory.

## Decision

A construction designation creates a stable-ID `ConstructionSite`. The first bootstrap structure is `StoneWall`, costing exactly `2 Stone` and fixed construction work.

A site reserves one concrete compatible physical stack. `DeliverConstruction` moves that same stack through `Ground -> Carried -> Ground` to a deterministic reachable position adjacent to the site. Only after delivery may a `Construct` job begin.

Completion consumes exactly the required quantity from the delivered stack. A partial remainder keeps the same item ID and remains physically accessible beside the new structure. The completed structure keeps the site's stable ID.

A completed `StoneWall` is authoritative world occupancy, separate from base terrain. It blocks both continuous movement transitions and A* pathfinding; the terrain cell underneath remains unchanged.

Construction sites do not block movement while unfinished. Placement is limited to explored, walkable, unoccupied cells and cannot be designated under a character.

Cancelling a site removes its child jobs and reservations. If material is currently carried, it is first dropped at the worker's current exact position; no material is silently deleted.

## Consequences

- Construction competes for the same physical stacks as Haul and Craft through explicit reservations.
- Camera/world discovery remains unchanged; structures do not reveal terrain.
- A line of walls can be designated with the client rectangle tool, but builders still require an accessible adjacent work position.
- Dense enclosed construction layouts may become unreachable and intentionally remain unfinished rather than teleporting workers/materials.
- Demolition, doors, multi-material recipes, stack splitting, construction priorities, and persistence remain future work.
- Workbench placement remains an explicit Prototype 01 bootstrap exception and is still instantaneous.
