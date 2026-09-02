# Physical Items & Ownership Foundation — Design

Status: **Approved for implementation**

## Goal

Add the smallest authoritative physical-item model needed before harvest/haul jobs. Items have exact sub-cell positions, stable Progressus IDs, quantities, and exactly one location/owner state. The client may render known ground items, but no job automation, stockpiles, harvesting, crafting, or persistence is added in this increment.

## Decisions

- `ItemKind::{Wood, Stone}` is the Prototype 01 bootstrap content set.
- `ItemQuantity` is a positive `u32` value type; zero-quantity stacks cannot exist.
- `ItemLocation` is a single enum, initially `Ground { position: WorldPosition }` or `Carried { character_id: EntityId }`. Stored/consumed states are not fake placeholders: storage will extend the enum when a real stockpile exists; consumption removes or changes a stack through an explicit rule later.
- Items use the same global `EntityIdAllocator` as characters so IDs remain unique across authoritative entities.
- `ItemWorld` owns the canonical item map and derived deterministic indexes for ground items by `ChunkCoord` and carried items by character. Transfers update canonical location and indexes atomically.
- Characters expose a provisional interaction reach of `768` subunits (3/4 cell). Items are point-like for reach checks. The parameter is gameplay/bootstrap data, not part of the fixed-point coordinate contract.
- `Simulation::pick_up_item` requires a ground item within character reach. `Simulation::drop_item` requires the item to be carried by that character, destination within reach, and effective `Grass` at the destination cell. Failed transfers preserve all prior state.
- The new-game scenario contains a few deterministic **starting supply stacks** on the forced-grass spawn corridor, placed at non-center sub-cell positions. These are scenario supplies, not procedural natural deposits. Natural resource sources/harvesting are the next increment and must not be confused with these stacks.
- `item_revision` increments only when item membership/location changes. It is a cheap read-model invalidation signal.
- Application chunk queries return detached `GroundItemSnapshot`s only for requested chunks and only for explored cells. Lightweight snapshots do not globally enumerate all ground items.
- Bevy renders returned ground items as disposable small bootstrap markers. Presentation never owns item truth.

## Invariants

1. Every live item exists exactly once in `ItemWorld` and has exactly one `ItemLocation`.
2. A ground item appears in exactly one ground-chunk index and no carried index.
3. A carried item appears in exactly one character-carried index and no ground index.
4. Failed pickup/drop changes neither location, indexes, quantity, nor revision.
5. Pickup/drop preserve stable item ID, kind, and quantity.
6. Client/read-model queries cannot reveal a ground item in an undiscovered cell.
7. Item exact positions use `WorldPosition`; no authoritative float is introduced.

## Non-goals

- natural trees/deposits or worldgen resource distribution;
- harvest/haul jobs or reservations;
- stockpile/storage ownership state;
- stack split/merge or carrying capacity;
- crafting/consumption;
- item collision/physics;
- save/load or chunk residency;
- item selection UI.

## Follow-up

Next increment: natural resource sources + explicit harvest/haul job lifecycle + stockpile designation, using this ownership model rather than inventing a second item representation.
