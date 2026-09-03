# ADR-0011 — Cardinal connectivity autotiles

Status: **Accepted**

## Context

Cell-sized structures such as walls must read as one continuous object rather than a row of isolated sprites with gaps. The same presentation problem will later apply to roads, fences, pipes, rails, and similar network-like objects.

## Decision

The Bevy presentation layer derives a four-bit cardinal connectivity mask for a cell from matching neighbouring cells: north, east, south, and west. The 16 possible masks cover isolated pieces, ends, straight segments, corners, T-junctions, and four-way intersections.

`StoneWall` is the first consumer. Construction blueprints and finished wall cells participate in the same intended wall topology, so a partially built run still previews as a connected run. The procedural wall recipe extends connected arms to the tile edge; adjacent cardinal wall sprites therefore meet without a visual gap.

The connectivity helper is generic presentation code rather than wall-specific simulation state. Future roads and other network visuals should reuse the same cardinal-mask mechanism when their gameplay topology is cell-cardinal.

## Consequences

Connectivity is derived from authoritative cell occupancy and is not separately persisted. Adding or removing a neighbouring segment changes only presentation selection; it does not alter wall identity, movement blocking, or construction semantics.

Diagonal neighbours do not connect in this bootstrap. If a future road or pipe model needs diagonal topology, that requires an explicit extension rather than overloading the four-bit wall contract.
