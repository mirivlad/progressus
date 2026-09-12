# ADR-0009: Physical construction sites and blocking structures

Status: Accepted

Amended: 2026-09-12 (passable automatic doors; wall-to-door replacement; physical site preparation)

## Context

Prototype 01 requires construction to consume delivered physical material and labor. A structure must not appear merely because matching resources exist elsewhere in the world.

The existing item model already provides stable stack IDs, exact Ground/Carried locations, reservations, and quantity consumption. Construction should reuse those semantics rather than introduce a global building inventory.

## Decision

A construction designation creates a stable-ID `ConstructionSite`. `StoneWall` costs exactly `2 Stone` and fixed construction work. The first passable structure is `Door`, costing exactly `2 Wood` and six construction work ticks; it reuses the same physical material-delivery and work lifecycle.

A site reserves one concrete compatible physical stack. `DeliverConstruction` moves that same stack through `Ground -> Carried -> Ground` to a deterministic reachable position adjacent to the site. Only after delivery may a `Construct` job begin.

Completion consumes exactly the required quantity from the delivered stack. A partial remainder keeps the same item ID and remains physically accessible beside the new structure. The completed structure keeps the site's stable ID.

Completed structures are authoritative world occupancy, separate from base terrain, and the terrain cell underneath remains unchanged. `StoneWall` blocks continuous movement and pathfinding. `Door` remains passable even while visually closed and has a higher navigation cost than ordinary ground, allowing A* to prefer a reasonable door-free route without treating a door as an obstacle.

Door open/closed state is authoritative but automatic: a character occupying the door cell opens it, and it stays open for a short deterministic hold interval after the cell is vacated. This state is exposed through detached snapshots and persisted as an additive optional v1 save field. It does not introduce locking, access control, or an inventory.

Construction sites do not block movement while unfinished. Designation is limited to explored, walkable cells without a permanent claim. A natural-resource source, ground stack, or character is removable occupancy and therefore does not reject the player's intent. Instead the stable-ID project owns an explicit `PrepareConstruction` job which, in deterministic order, harvests the existing source, physically carries each ground stack to a valid adjacent cell outside all claims, and moves an available occupying character out of the footprint. Existing unrelated harvest or item work retains its reservation; the project waits rather than stealing it. Material delivery and construction work remain disabled until the footprint is actually clear, and completion rechecks occupancy so a late arrival cannot be buried or deleted.

Preparation uses the same authoritative movement, capability checks, carrying limits, stable item IDs, and `Ground -> Carried -> Ground` transfers as ordinary work. Missing tools, workers, routes, carrying capacity, or valid drop cells leave pending work rather than teleporting anything. Harvest output is a normal physical stack and is also moved before building. A renewable source cleared for construction becomes permanently depleted because the completed claim prevents regrowth.

Workbench designation follows the same preparation contract when its center cell has removable occupancy. Its production-port layout and permanent-claim rules are validated before the deferred project is accepted; after preparation it becomes the authoritative workstation using the project's stable ID. An already clear Workbench cell retains immediate placement as the minimal bootstrap path. Door designation has one explicit replacement exception: a planned StoneWall is cancelled through the normal construction-cancellation path, or a completed StoneWall is removed from structure occupancy, before the Door site is created on the same cell. Replacing a completed wall does not introduce salvage in this pass.

Cancelling a site removes its child jobs and reservations. If material is currently carried, it is first dropped at the worker's current exact position; no material is silently deleted.

## Consequences

- Construction competes for the same physical stacks as Haul and Craft through explicit reservations.
- Camera/world discovery remains unchanged; structures do not reveal terrain.
- A line of walls can be designated with the client rectangle tool, but builders still require an accessible adjacent work position.
- Dense enclosed construction layouts may become unreachable and intentionally remain unfinished rather than teleporting workers/materials.
- General demolition/salvage, door locking/access policy, multi-material recipes, stack splitting, and construction priorities remain future work.
- Workbench placement remains instantaneous only on an already clear cell; removable occupancy creates a persisted, cancellable preparation project first.
