# ADR-0013 — Workbench uses two rotatable input ports and two rotatable output ports

Status: **Accepted**

## Context

ADR-0012 introduced generic physical Input/Output production buffers. Free-form painting is useful for some future producers, but it is unnecessary micromanagement for the one-cell bootstrap Workbench and makes its material interface visually ambiguous.

## Decision

A Workbench has exactly two physical Input cells on cardinal sides of its footprint and exactly two physical Output cells on diagonal sides. Placement succeeds only when a complete 2-Input + 2-Output layout can be created; this is checked before allocating the Workbench stable ID.

Input layouts form a fixed six-position cycle: north-south, west-east, north-east, east-south, south-west, west-north. Output layouts independently form the six unordered pairs of north-west, north-east, south-east, and south-west: NW-SE, NE-SW, NW-NE, NE-SE, SE-SW, SW-NW.

The player cannot add or remove Workbench Input or Output cells individually through the simulation/application API. Separate rotate commands atomically advance each pair by one position. Input and Output rotations are independent. If the next pair is unavailable because a target cell is blocked, occupied, stored, or owned by another production zone, rotation is rejected and the current pair remains authoritative.

The orientations are not separately persisted. They are derived from the authoritative Input/Output cells, so save format v1 remains unchanged. Legacy/non-canonical sets can still be loaded; the first successful rotate action canonicalizes the affected pair to the first layout in its cycle.

The workstation modal shows the same procedural Workbench image used on the map. Red points show the two Input ports and yellow points show the two Output ports. Separate localized buttons rotate inputs and outputs.

Craft output uses the current Output pair deterministically: it prefers a compatible partial stack, then an empty Output cell. If neither output can accept the product, production waits rather than teleporting or dropping the result elsewhere.

## Consequences

The generic `ProductionLogistics` ownership model remains reusable. Other production objects may choose another fixed port count, a footprint-derived perimeter, or explicitly paintable zones without inheriting Workbench-specific geometry.

Changing input orientation cancels active Craft and supply reservations before replacing the pair. Changing output orientation cancels active Craft before replacing the pair. Physical stacks already on former port cells remain ordinary ground items and re-enter normal logistics rather than teleporting with the port.
