# ADR-0013 — Workbench uses two rotatable cardinal input ports

Status: **Accepted**

## Context

ADR-0012 introduced generic physical Input/Output production buffers. Free-form Input painting is useful for some future producers, but it is unnecessary micromanagement for the one-cell bootstrap Workbench and makes its material interface visually ambiguous.

## Decision

A Workbench has exactly two physical Input cells on cardinal sides of its footprint. The allowed pair layouts form a fixed cycle: north-south, west-east, north-east, east-south, south-west, west-north.

The player cannot add or remove Workbench Input cells individually through the simulation/application API. A single rotate command atomically changes the pair. If the next pair is unavailable because a target cell is blocked, occupied, stored, or owned by another production zone, the rotation is rejected and the current pair remains authoritative.

The orientation is not separately persisted. It is derived from the two authoritative Input cells, so save format v1 remains unchanged. Legacy/non-canonical input sets can still be loaded; the first successful rotate action canonicalizes them to the first pair in the cycle.

Workbench Output remains a physical production buffer and is restricted to diagonal neighbour cells, leaving all four cardinal positions available to the input-port cycle.

The workstation modal shows the same procedural Workbench image used on the map. Red points show the current two Input ports; Output points are shown separately in orange. The control is localized in Russian and English.

## Consequences

The generic `ProductionLogistics` ownership model remains reusable. Other production objects may choose another fixed port count, a footprint-derived perimeter, or explicitly paintable zones without inheriting Workbench-specific geometry.

Changing input orientation cancels the Workbench's active Craft and supply reservations before replacing the pair. Physical stacks already on former Input cells remain ordinary ground items and re-enter normal logistics rather than teleporting with the port.
