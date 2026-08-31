# Progressus

**Progressus** is a bottom-up civilization simulator: a colony-scale game that grows into a simulation of settlements, cities, industrial regions, and eventually a technological civilization — all inside one continuous physical world.

The project starts from a familiar colony-sim scale, but its long-term goal is different from a traditional RimWorld-like game. There is no separate strategic map and no abstract expedition layer. People, cargo, roads, mines, factories, vehicles, settlements, and infrastructure remain part of the same world.

> Start with a few people and primitive tools. End with a civilization whose railways, power grids, factories, cities, and abandoned ruins all grew from that first settlement.

## Core pillars

1. **One continuous world** — no separate expedition maps or teleporting logistics.
2. **Potentially unbounded procedural map** — generated in chunks as the world is explored.
3. **Physical production and logistics** — resources exist somewhere and must physically move.
4. **Deep technological progression** — technologies unlock capabilities, not merely numerical bonuses.
5. **Infrastructure matters** — advanced technology requires the industrial base that can actually produce it.
6. **Management scales with civilization** — the player gradually moves from assigning individual jobs to defining production, logistics, and economic rules.
7. **Persistent history** — old buildings, roads, settlements, and infrastructure remain part of the world's history.
8. **Simulation LOD** — nearby regions are simulated in detail; distant regions may be safely aggregated.

## Current status

**Pre-production / architecture.**

The immediate goal is not to build the entire game. The first prototype exists to prove the dangerous fundamentals:

- deterministic chunk-based world generation;
- effectively unlimited traversal;
- persistent entities;
- basic characters and jobs;
- physical resource movement;
- simple crafting and construction;
- save/load of a sparse, growing world;
- headless deterministic simulation tests.

See [`docs/milestones/prototype-01.md`](docs/milestones/prototype-01.md).

## Documentation

- [`docs/vision.md`](docs/vision.md) — game vision and design bible.
- [`docs/architecture/overview.md`](docs/architecture/overview.md) — initial technical architecture and boundaries.
- [`docs/milestones/prototype-01.md`](docs/milestones/prototype-01.md) — first executable milestone and acceptance criteria.
- [`docs/adr/0001-core-invariants.md`](docs/adr/0001-core-invariants.md) — architectural invariants that implementations must preserve.
- [`AGENTS.md`](AGENTS.md) — rules for Codex, Claude, and other coding agents.

## Development principle

Each milestone should produce a smaller but coherent playable game. Architecture should permit future scale, but code for hypothetical late-game systems should not be written before it is needed.

If an implementation seems to require violating a documented invariant, stop and document the conflict instead of silently changing the game model.

## Name

*Progressus* is a working project name. It reflects the central idea: progress is not a menu of unlocks, but a physical transformation of the world and the society living in it.
