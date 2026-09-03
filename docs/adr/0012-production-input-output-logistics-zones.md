# ADR-0012 — Production input/output logistics zones

Status: **Accepted**

## Context

The first Craft bootstrap staged missing ingredients in ordinary stockpile cells beside a Workbench. That proved physical delivery, but it makes production depend on the player drawing storage next to every machine and does not generalize cleanly to furnaces, kitchens, workshops, or later industrial buildings.

Production still must not teleport ingredients or turn a workstation into an abstract inventory. Shared stockpile stacks must remain physical and competing producers must not shuttle whole source stacks back and forth.

## Decision

Every production object owns authoritative `ProductionLogistics` with separate **Input** and **Output** ground-cell zones. A zone cell has one production owner and one zone kind. It cannot simultaneously be ordinary stockpile ground or another production object's zone.

For the current one-cell Workbench, valid production-zone cells are the eight neighbouring cells around its footprint. Placement seeds a deterministic bootstrap layout of up to three Input cells and one Output cell when valid cells exist. The workstation modal exposes localized Input/Output add/remove tools; editing uses the shared rectangle designation mechanism but applies only to the production object's valid local perimeter. Future multi-cell producers may define their valid logistics perimeter from their footprint without changing the zone ownership model.

A pending Craft job may reserve and consume inputs only while the concrete stacks physically lie in that workstation's Input zone. Missing recipe quantities create a distinct `SupplyProduction` job. Supply splits exactly the missing quantity from an eligible ordinary stockpile stack when necessary, carries that physical stack through `Ground -> Carried -> Ground`, and delivers it to a reserved Input cell. Supply never consumes from another producer's Input zone.

Craft output is created physically in the producer's Output zone. It then participates in ordinary Haul and may be moved into a normal stockpile. Thus the generic flow is:

`Stockpile -> Input -> production -> Output -> Stockpile`.

Input/output zones are floor buffers, not hidden inventories. Items in them remain ordinary physical ground stacks with stable IDs and normal quantity rules.

## Consequences

Production no longer requires ordinary stockpile cells beside the machine. Exact recipe batches remain visible and reservable, while a large shared source stack stays in central storage except for the quantity actually needed by a job.

`SupplyProduction` and ordinary `Haul` are intentionally separate job categories: production supply has a specific producer and Input destination, while Haul targets general storage. This leaves room for later production priorities, fuel delivery, allowed-input policies, and dedicated logistics workers without turning generic Haul into a collection of special cases.

This ADR supersedes the adjacent-stockpile staging details in ADR-0008 and ADR-0010; their decisions about concrete physical stacks, production orders, deterministic reservations, finite/infinite targets, modal UI, and localization remain in force.
