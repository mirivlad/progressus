- Status: Accepted
- Date: 2026-09-03

## Context

Prototype 01 now needs stockpiles and hauling. The item model already guarantees that every physical stack has exactly one canonical location, currently either exact fixed-point ground position or carrier character ID.

A tempting shortcut is to make a stockpile another abstract item location such as `Stored { stockpile_id }`. That would make hauling easy to model but would also remove the stack from the physical world as soon as it reached a designated floor area. Progressus explicitly wants resources, logistics, obstruction, workshops, and later construction to remain physical rather than becoming hidden counters.

## Decision

A stockpile is an authoritative designated set of `WorldCell` ground cells. It is not a container and it does not create a new item-location state.

A stack delivered to a stockpile remains `ItemLocation::Ground { position }` at an exact fixed-point position inside a stockpile cell. During hauling it temporarily becomes `ItemLocation::Carried { character_id }`, then returns to `Ground` at the reserved destination cell.

Each stockpile cell has at most one stockpile owner. Prototype 01 reserves at most one haul job per item and per destination cell. An occupied destination accepts another stack only when it is the same item kind and the combined quantity does not exceed the hard per-stack capacity of 1,024; otherwise it remains unavailable. Stockpile cells must be discovered, walkable, and free of a live natural-resource source when designated.

Future real containers such as crates, chests, shelves, vehicles, or building inventories may introduce an explicit stored/container location if their gameplay requires it. Such storage must not retroactively redefine ordinary stockpile floor areas as abstract containers.

## Consequences

Hauling remains a visible physical transition: ground stack → carried stack → ground stack. Stable item identity and quantity are preserved while hauling. When compatible stacks merge, the destination stack keeps its stable ID, receives the summed quantity, and the now-empty source stack is explicitly removed. Stockpile contents remain spatially queryable, and later systems can reason about the exact positions of stored resources.

The bootstrap keeps at most one ground stack per stockpile cell, uses a hard capacity of 1,024 units per stack, and performs deterministic same-kind merging during Haul delivery. Stack splitting, filters, priorities, container inventories, and large-scale logistics remain later work.
