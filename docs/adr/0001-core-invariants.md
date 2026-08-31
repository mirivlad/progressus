# ADR-0001: Core Simulation Invariants

Status: **Accepted**

Date: 2026-08-31

## Context

Progressus is intended to grow from a small colony simulation into a large civilization simulation without switching to a separate abstract overworld. That vision creates architectural constraints that must be preserved from the first prototype.

Coding agents and future contributors need a small set of explicit rules that take precedence over convenient local implementations.

## Decision

The following invariants are accepted.

### INV-001 — One continuous world

All persistent physical entities exist in one shared world coordinate system.

A feature may use abstraction internally for performance, but it must not create a second authoritative strategic world whose state contradicts or replaces the physical world.

### INV-002 — Physical logistics

Material goods do not teleport between independent locations.

Movement may later be aggregated at coarse simulation LOD, but quantities, travel constraints, and causal transport links must still be represented.

### INV-003 — Deterministic procedural base world

Untouched world content is reproducible from stable inputs including world seed, world-generation version, and coordinates.

Chunk visitation order must not change canonical untouched generation.

### INV-004 — Chunk-based world lifecycle

The implementation must not require the whole traversed world to remain loaded or fully materialized.

World storage and simulation operate through independently loadable regions/chunks or an equivalent bounded spatial partition.

### INV-005 — Persistent identity

Long-lived simulation entities have stable identities independent of memory addresses, rendering objects, container indexes, or chunk residency.

Moving, unloading, saving, or loading an entity does not silently change its identity.

### INV-006 — Simulation independent from rendering

Authoritative game state is owned by the simulation, not the visual client.

The simulation must be executable headlessly.

### INV-007 — Deterministic authoritative time

Game logic is advanced by simulation time, not renderer frame timing or wall-clock timing.

The same authoritative initial state and command sequence should produce the same result on supported deterministic builds.

### INV-008 — Material technology

Research knowledge alone is insufficient for advanced production.

Advanced capabilities may require resources, tools, machines, infrastructure, skills, institutions, and earlier industrial capabilities.

### INV-009 — Management must scale

Future population growth must not require proportional manual control of every individual.

Interfaces and systems must permit transition from direct task assignment toward policies, stock rules, routes, production plans, and automation.

### INV-010 — Simulation LOD must conserve state

When future performance systems aggregate detailed simulation, the transition must preserve meaningful quantities, identities, obligations, and causal constraints.

Simulation LOD is allowed to reduce computational detail. It is not allowed to become a source of free goods, deleted shortages, impossible travel, or unexplained entity replacement.

### INV-011 — Persistent world history

Player-caused changes to the physical world are authoritative state.

Unloading a region must not reset meaningful construction, extraction, destruction, or other persistent modifications back to procedural defaults.

### INV-012 — No silent invariant violation

During development and testing, impossible authoritative states must be surfaced explicitly rather than silently repaired whenever practical.

Examples include duplicated item ownership, dangling persistent references, invalid chunk addresses, and conflicting job reservations.

## Consequences

These decisions make some shortcuts unavailable.

For example:

- distant cargo cannot simply be added to another inventory after a timer with no transport representation;
- save files cannot assume a finite pre-generated world;
- character state cannot live only in scene/render objects;
- a future performance optimization cannot freely replace named people with anonymous population counters without a defined transition model;
- research cannot be the only prerequisite for advanced manufacturing.

The benefit is that early prototypes remain compatible with the defining long-term idea of Progressus.

## Agent stop rule

If an implementation task appears to require violating any invariant above, the coding agent must stop that part of the work and report:

1. which invariant is in conflict;
2. why the requested implementation conflicts with it;
3. at least one alternative that preserves the invariant, if known.

The agent must not silently weaken this ADR.

Changing an invariant requires a new ADR that explicitly supersedes or amends ADR-0001.
