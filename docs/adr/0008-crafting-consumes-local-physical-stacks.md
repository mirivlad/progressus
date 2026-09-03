# ADR-0008: Crafting consumes local physical stacks

- Status: Accepted
- Date: 2026-09-03

## Context

Progressus requires production to remain physical. A tempting craft bootstrap would deduct abstract Wood/Stone counters from anywhere on the map, but that would bypass hauling, spatial stockpiles, reservation conflicts, and the ownership model already established for items.

## Decision

Prototype Craft jobs reserve a stable workbench, one worker, and the concrete input stack IDs. The first recipe is `2 Wood + 1 Stone -> 1 PrimitiveTool`. Eligible inputs must be ordinary ground stacks in designated stockpile cells within Manhattan distance 1 of the workbench. Completion decrements those exact stacks or removes a fully consumed stack and creates a new physical output stack beside the workbench.

The workbench itself is currently placed instantly as a bootstrap workstation; this ADR does not treat placement as completed construction. Future Construct jobs must deliver physical materials and work before a building becomes complete.

## Consequences

Crafting cannot silently consume material from the other side of the world. Reservation conflicts are explicit, quantity conservation remains testable, and crafted outputs immediately participate in ordinary hauling. Large input-delivery jobs, bills/queues, filters, skills, tools, power, and physical construction remain later work.
