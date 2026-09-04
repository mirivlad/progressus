# ADR-0011 — Cardinal connectivity autotiles

Status: **Accepted**

Amended: 2026-09-04 (doors join wall topology)

## Context

Cell-sized structures such as walls must read as one continuous object rather than a row of isolated sprites with gaps. The same presentation problem will later apply to roads, fences, pipes, rails, and similar network-like objects.

## Decision

The Bevy presentation layer derives a four-bit cardinal connectivity mask for a cell from matching neighbouring cells: north, east, south, and west. The 16 possible masks cover isolated pieces, ends, straight segments, corners, T-junctions, and four-way intersections.

`StoneWall` is the first consumer, and `Door` now joins the same wall-network set. Construction blueprints and finished wall/door cells participate in one intended topology, so a partially built run still previews as a connected run and a door interrupts a wall without visual gaps. The procedural recipes extend connected arms/frames to the tile edge. Door passability/open state remains authoritative structure behavior and is not encoded in the connectivity mask.

The connectivity helper is generic presentation code rather than wall-specific simulation state. Future roads and other network visuals should reuse the same cardinal-mask mechanism when their gameplay topology is cell-cardinal.

## Consequences

Connectivity is derived from authoritative cell occupancy and is not separately persisted. Adding or removing a neighbouring segment changes only presentation selection; it does not alter wall identity, movement blocking, or construction semantics.

Diagonal neighbours do not connect in this cardinal network contract. Natural terrain smoothing uses a separate eight-neighbour presentation mask because coast/mountain corner shaping needs diagonal context; it does not change the four-bit wall/door topology.
