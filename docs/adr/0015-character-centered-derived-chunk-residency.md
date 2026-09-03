# ADR-0015: Character-centered derived chunk residency

- Status: Accepted
- Date: 2026-09-04

## Context

Progressus has deterministic chunk world generation, sparse authoritative modifications, stable persistent entities, and versioned save/load. Untouched terrain never needs to be persisted because it can be regenerated from `(seed, worldgen version, absolute coordinate)`.

The simulation nevertheless needs a bounded notion of loaded world data. Keeping every generated chunk forever would make long-distance traversal consume memory in proportion to exploration history. Renderer visibility must not define simulation residency: moving the camera is presentation-only and must not load authoritative world state.

## Decision

`Simulation` owns a private `ChunkResidency` cache of immutable raw `GeneratedChunk` values.

Residency is derived, not authoritative:

- authoritative character positions define interest centers;
- each distinct character chunk requests a Chebyshev radius of one chunk, a 3×3 neighborhood;
- neighborhoods are unioned and deduplicated;
- with `N` distinct character centers the raw upper bound is `9 × N` resident chunks;
- reconciliation runs after authoritative movement/jobs for each tick and returns immediately when the set of character chunk centers is unchanged;
- chunks leaving the requested union are dropped from the cache;
- newly requested chunks are regenerated deterministically.

Raw/effective chunk reads outside the resident set are ephemeral. Camera snapshots, pathfinding probes, tests, or other read-only queries may generate a chunk for that call, but they do not insert it into `ChunkResidency`.

Resident raw chunks are only a cache. Sparse terrain overrides, depleted resources, physical items, jobs, stockpiles, production objects, construction, exploration, and other authoritative state are not evicted by this Prototype 01 policy.

The cache is not serialized in save format v1. Loading reconstructs residency from restored character positions. This keeps save bytes independent of cache history and allows residency policy to evolve without migrating authoritative saves.

At extreme signed-coordinate edges a full 32×32 materialized chunk may be unrepresentable even when an individual point remains valid. Residency may skip such a derived cache entry; point worldgen remains the fallback and therefore remains authoritative.

`progressus-app` publishes sorted resident chunk coordinates and a derived residency revision for diagnostics. F3 in the Bevy client renders resident chunk boundaries; camera movement itself does not change the set.

## Consequences

Long traversal has bounded raw-chunk memory rather than memory proportional to visited distance. The current five-character bootstrap has a theoretical maximum of 45 resident raw chunks; overlapping centers normally use fewer. The seed-0 64-boundary headless travel scenario reaches 21 resident chunks and never exceeds 21.

This is not Simulation LOD, entity sleeping, remote-settlement aggregation, or a general chunk-owned entity store. Those remain later scaling layers. Prototype 01 proves the narrower invariant that deterministic base chunks can load, unload, regenerate, and reapply sparse authoritative modifications without renderer ownership or save-format coupling.
