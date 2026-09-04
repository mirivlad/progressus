# ADR-0016: Authoritative satiety and physical eating

- Status: Accepted
- Date: 2026-09-04

## Context

Prototype 02 introduces human needs. The first implemented need is food. Progressus must not satisfy material needs through an abstract colony-wide resource counter because that would bypass the project's physical-world and logistics invariants.

At the same time, satiety itself is a property of a person rather than a world item. It must be deterministic, bounded, visible through the application read model, and persistent across save/load.

## Decision

Each `Character` owns authoritative satiety in the inclusive range `0..=100`. New characters start at `100`. Satiety decays by one on deterministic global simulation-tick intervals; no wall-clock or client-frame time participates.

The first hunger threshold is `<= 50`. A character at zero satiety is excluded from ordinary worker assignment until food becomes available.
Hunger is satisfied by an explicit character-specific `Eat` job referencing one concrete physical food stack. For the bootstrap food, `Berries`, the scheduler splits exactly one unit from a larger ground stack when necessary, gives the portion its own stable `EntityId`, reserves that item against Haul/Craft/production/construction use, and routes the hungry character to it.

Eating consumes exactly one physical unit only when the Eat job completes. One Berry restores a fixed `+50` satiety, capped at `100`. Cancellation or manual interruption releases the reservation and consumes nothing.

Eat jobs use the ordinary `JobWorld` state machine and worker reservation indexes. They are character-specific: another worker cannot take over an Eat job created for a different person. Free physical food is considered for hunger before new ordinary Haul jobs, but food already being carried by an existing logistics job is not stolen.

Satiety and active Eat jobs are serialized in save format v1. Active Eat item/worker references are validated on restore. Save compatibility between arbitrary pre-release development builds is not a requirement; unsupported formats still fail explicitly.

`progressus-app` publishes detached satiety and Eat-job state. The Bevy client localizes the need/job/item names, renders Berries procedurally, and shows satiety in the selected-character inspector.

## Consequences

The simulation now has its first autonomous material demand loop without introducing an abstract food inventory. Large food stacks can feed multiple people concurrently because exact meal portions are split and independently reserved, while total physical quantity remains conserved.
The new-game `Berries x10` stack is only a small startup buffer. P02-N03 is completed separately by deterministic renewable BerryBush sources and ordinary physical Harvest/Haul logistics as recorded in ADR-0018.

This ADR does not define cooking, nutrition chemistry, preferences, meals with multiple ingredients, health damage from starvation, or mood effects. Those remain later gameplay layers.