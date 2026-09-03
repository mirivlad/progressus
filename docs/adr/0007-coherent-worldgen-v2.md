# ADR-0007: Coherent world generation v2

- Status: Accepted
- Date: 2026-09-03

## Context

Worldgen v1 intentionally proved deterministic, order-independent infinite chunk generation with the smallest possible implementation. It independently classified each absolute cell, which made the rendered world read as a checkerboard of isolated water, rock, grass, trees, and stone outcrops rather than as terrain.

Changing that algorithm in place would make the same `(seed, version, coordinate)` identity produce different terrain and would defeat the purpose of versioned regeneration.

## Decision

`CURRENT_WORLDGEN_VERSION` is version 2. Version 1 remains supported and retains its golden fixtures unchanged.

Worldgen v2 remains stateless and deterministic from `(seed, version, absolute WorldCell)`, but uses deterministic regional feature centers and integer geometry to form connected water/rock bodies and clustered forest regions. Stone outcrops are biased toward rock-region edges. The bootstrap spawn clearing remains walkable and deterministic starter source placement guarantees access to both Wood and Stone around the initial colony without turning the rest of the world into per-cell noise.

`WorldGenerator` also exposes deterministic point queries for terrain and natural resources. Point queries and materialized chunks must agree exactly. Systems that need one cell must not generate an entire 32×32 chunk merely to inspect that cell; full chunk materialization remains available for chunk read models and rendering.

## Consequences

The current map has larger recognizable terrain/resource regions while preserving chunk-order independence and negative-coordinate behavior. Old v1 worlds remain reproducible when their explicit worldgen version is supplied.

The headless traversal fixture changes under v2 because the walkable topology changes. Seed 0 currently crosses 64 positive chunk boundaries in 2,239 chosen-cell steps and ends at coarse cell `(2048, 89)`.

This ADR does not introduce biomes, climate, rivers, elevation, seasons, finite map bounds, chunk residency, or persistence. Those remain later systems.
