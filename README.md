# Progressus

**Progressus** is a bottom-up civilization simulator: a colony-scale game that grows into a simulation of settlements, cities, industrial regions, and eventually a technological civilization — all inside one continuous physical world.

The project starts from a familiar colony-sim scale, but its long-term goal is different from a traditional RimWorld-like game. There is no separate strategic map and no abstract expedition layer. People, cargo, roads, mines, workshops, factories, vehicles, settlements, and infrastructure remain part of the same world.

> Start with a few people and primitive tools. End with a civilization whose railways, power grids, industries, cities, and abandoned ruins all grew from that first settlement.

## Core pillars

1. **One continuous world** — no separate expedition maps or teleporting logistics.
2. **Potentially unbounded procedural map** — generated in chunks as the world is explored.
3. **Physical goods, lightweight production chains** — resources exist somewhere and must physically move, but production is not an optimization puzzle about perfectly matching conveyor-like chains.
4. **Deep technological progression** — technologies unlock capabilities, not merely numerical bonuses.
5. **Infrastructure matters** — advanced technology requires the industrial base that can actually support it, without requiring the player to micromanage every intermediate component.
6. **Management scales with civilization** — the player gradually moves from assigning individual jobs to defining workshops, supplies, transport, services, and broader economic rules.
7. **Persistent history** — old buildings, roads, settlements, and infrastructure remain part of the world's history.
8. **Simulation LOD** — nearby regions are simulated in detail; distant regions may be safely aggregated.

## Implementation architecture

The initial implementation stack is an accepted project decision:

- **pure Rust authoritative simulation core**;
- **Bevy client** for rendering, input, UI, audio, and presentation;
- the simulation core **must not depend on Bevy**;
- stable Progressus IDs, not Bevy `Entity`, define persistent simulation identity;
- headless simulation and tests must run without initializing a renderer or window;
- procedural graphics are the default visual direction, with authored assets used where they clearly provide value.

In short: **Rust decides what exists; Bevy shows it.**

See [`docs/adr/0002-rust-core-bevy-client.md`](docs/adr/0002-rust-core-bevy-client.md) for the normative decision and [`docs/architecture/overview.md`](docs/architecture/overview.md) for architectural boundaries.

## Production philosophy

Production exists to make technological and territorial development believable, not to turn Progressus into a factory-layout game.

A product should require the materials, tools, facilities, knowledge, and infrastructure that make sense for its era. Intermediate parts are modeled when they create useful choices, dependencies, specialization, trade, maintenance, or historical character. They should not be added merely to make a longer recipe.

Throughput, stock levels, transport capacity, and bottlenecks can matter, especially in an industrial society, but optimizing them is one possible activity rather than the central fantasy of the game.

The central fantasy remains **building a living civilization in one continuous world**.

See [`docs/gameplay/production.md`](docs/gameplay/production.md) for the normative production-design rules.

## Current status

**Prototype 01 — foundation in progress.**

The first executable headless foundation is implemented:

- deterministic versioned 32×32 chunk generation;
- signed positive/negative world coordinates;
- a checked simulation clock and stable Progressus entity IDs;
- a deterministic five-character new-game scenario;
- a detached client read model exposed through `progressus-app`;
- a headless consumer that runs without Bevy, a window, or a graphics context.

Movement, jobs, physical items, modified chunk persistence, save/load, and the Bevy client remain Prototype 01 work. The current boundary is intentionally ready for a visual client to depend on `progressus-app` without taking ownership of simulation truth.

The immediate goal is not to build the entire game. The first prototype exists to prove the dangerous fundamentals:

- deterministic chunk-based world generation;
- effectively unlimited traversal;
- persistent entities;
- basic characters and jobs;
- physical resource movement;
- simple crafting and construction;
- save/load of a sparse, growing world;
- headless deterministic simulation tests;
- clean separation between the Rust simulation and the Bevy client.

See [`docs/milestones/prototype-01.md`](docs/milestones/prototype-01.md).

## Development

Run the complete automated suite:

```bash
cargo test --workspace
```

Run a deterministic headless scenario:

```bash
cargo run -p progressus-headless -- --seed 42 --ticks 100000
```

Verify that Bevy has not entered the application/simulation/worldgen dependency boundary:

```bash
./scripts/verify-core-dependency-boundary.sh
```

The application API and its external-consumer contract test live in `crates/progressus-app`. Bootstrap coordinate and chunk decisions are recorded in [`docs/adr/0003-bootstrap-world-coordinates.md`](docs/adr/0003-bootstrap-world-coordinates.md).

## Documentation

- [`docs/vision.md`](docs/vision.md) — game vision and design bible.
- [`docs/gameplay/production.md`](docs/gameplay/production.md) — lightweight production-chain philosophy and guardrails.
- [`docs/architecture/overview.md`](docs/architecture/overview.md) — technical architecture and boundaries.
- [`docs/milestones/prototype-01.md`](docs/milestones/prototype-01.md) — first executable milestone and acceptance criteria.
- [`docs/adr/0001-core-invariants.md`](docs/adr/0001-core-invariants.md) — architectural invariants that implementations must preserve.
- [`docs/adr/0002-rust-core-bevy-client.md`](docs/adr/0002-rust-core-bevy-client.md) — accepted Rust simulation / Bevy client decision.
- [`AGENTS.md`](AGENTS.md) — rules for Codex, Claude, and other coding agents.

## Development principle

Each milestone should produce a smaller but coherent playable game. Architecture should permit future scale, but code for hypothetical late-game systems should not be written before it is needed.

If an implementation seems to require violating a documented invariant or accepted ADR, stop and document the conflict instead of silently changing the game model.

## Name

*Progressus* is a working project name. It reflects the central idea: progress is not a menu of unlocks, but a physical transformation of the world and the society living in it.
