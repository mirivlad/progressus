# ADR-0008: Crafting consumes local physical stacks

- Status: Accepted
- Date: 2026-09-03

## Context

Progressus requires production to remain physical. A tempting craft bootstrap would deduct abstract Wood/Stone counters from anywhere on the map, but that would bypass hauling, spatial stockpiles, reservation conflicts, and the ownership model already established for items.

## Decision

Prototype Craft jobs reserve a stable workbench, one worker, and the concrete input stack IDs. The first recipe is `2 Wood + 1 Stone -> 1 PrimitiveTool`. Craft itself consumes only ordinary ground stacks in designated stockpile cells within Manhattan distance 1 of the workbench. When a pending order lacks a local input, production supply may split exactly the missing recipe quantity from an unreserved stack elsewhere in a stockpile and create an ordinary physical Haul job into a compatible adjacent stockpile staging cell. The remainder stays in the source stack; Craft then reserves and consumes the delivered local stack. Completion decrements those exact stacks or removes a fully consumed stack and creates a new physical output stack beside the workbench.

The workbench itself is currently placed instantly as a bootstrap workstation; this ADR does not treat placement as completed construction. Future Construct jobs must deliver physical materials and work before a building becomes complete.

## Consequences

Crafting cannot silently consume material from the other side of the world. Input movement is visible and uses the same `Ground -> Carried -> Ground` hauling path as other logistics; only the exact recipe quantity is split for delivery, so one workbench does not drag a large shared stack away from competing production. Reservation conflicts and quantity conservation remain testable, and crafted outputs immediately participate in ordinary hauling. Production orders now provide finite/infinite bills; filters, skills, tools, power, and richer delivery policy remain later work.
