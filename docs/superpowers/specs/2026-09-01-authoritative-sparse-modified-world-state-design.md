# Authoritative Sparse Modified World State Design

**Status:** Approved for planning

## 1. Goal

Add the smallest authoritative modified-world mechanism above deterministic worldgen. A current world resolves from two deliberately distinct sources:

1. immutable base terrain from `seed + worldgen_version + ChunkCoord`;
2. sparse authoritative terrain overrides owned by simulation.

A current chunk is materialized only on demand as base plus sparse overrides. This is a bootstrap for modified world state, not a terraforming feature, residency cache, or persistence system.

## 2. Generated terrain and effective terrain are different values

`GeneratedChunk` remains exclusively the untouched, pure output of `progressus-worldgen`. It is never mutated, populated from modified state, or presented as a current authoritative chunk.

`progressus-sim` introduces a separate public `EffectiveChunk` value type: one coordinate, row-major terrain cells, and bounded `terrain_at` lookup. It is materialized on demand, returned by value, and never retained by `Simulation` merely because it was read.

The raw `Simulation` method is explicitly named `generated_chunk`. Its semantics remain `seed + worldgen version + coordinate -> untouched GeneratedChunk`; it is for diagnostics, worldgen tests, and base/effective comparison, never an authoritative gameplay shortcut. Movement, application snapshots, and later jobs/resources/construction use effective terrain only.

## 3. Authoritative ownership and sparse representation

```text
Simulation
 ├─ WorldGenerator                    // seed + worldgen version
 └─ ModifiedWorld                     // private, simulation-owned overlay
     └─ BTreeMap<ChunkCoord, ChunkDelta>
         └─ BTreeMap<LocalCell, Terrain>
```

`ModifiedWorld`, `ChunkDelta`, and their maps stay private inside `progressus-sim`. No mutable-map accessor, mutable delta accessor, or direct insertion API is exported. `BTreeMap` supplies deterministic canonical ordering and a future chunk-local persistence form; it is not a performance claim.

The overlay contains no seed, version, untouched terrain, or transient generated/effective chunks. It has meaning only relative to the `WorldGenerator` of its owning `Simulation`; it is not independently transferable between arbitrary world identities.

## 4. Domain API and canonical mutation

```rust
pub fn set_terrain_override(
    &mut self,
    position: WorldCell,
    terrain: Terrain,
) -> Result<(), SimulationError>;

pub fn effective_terrain_at(
    &self,
    position: WorldCell,
) -> Result<Terrain, SimulationError>;

pub fn effective_chunk(
    &self,
    coordinate: ChunkCoord,
) -> Result<EffectiveChunk, SimulationError>;
```

`set_terrain_override` immediately mutates authoritative state. It is neither a simulation tick nor a `progressus-app::Command`. For one base-world identity and ordered mutation sequence it is deterministic. Failure to obtain raw base terrain continues through `SimulationError::Worldgen`; no gameplay-specific mutation error is added.

The method resolves raw base terrain first and then atomically canonicalizes:

1. a requested value different from base is stored as the local override;
2. a requested value equal to base removes the local override;
3. an empty `ChunkDelta` removes its chunk entry;
4. untouched chunks never appear in `ModifiedWorld`.

Thus `Grass(base) -> Rock -> Water -> Grass` produces the same canonical modified state as no mutation. No public API lets callers construct noncanonical deltas.

Public Rust visibility makes this a simulation/world-state primitive, not permission for presentation or application layers to bypass the command boundary. Future construction, mining, destruction, and terrain systems invoke it only from deterministic authoritative execution flow. This increment adds no terrain-mutation command or route to `progressus-app`, `progressus-client`, or `progressus-headless`.

## 5. One effective-terrain resolution rule

The private resolver is exactly:

```text
effective terrain = local override, if present; otherwise raw base terrain
```

`effective_terrain_at` splits the `WorldCell`, checks the delta, and reads raw base only if the override is absent; it need not materialize a whole chunk. `effective_chunk` generates an untouched `GeneratedChunk`, applies that same resolver to every local cell, and constructs a distinct `EffectiveChunk`. It never mutates or rebrands `GeneratedChunk`.

Movement passability calls point effective terrain during both new-direction validation and every persistent step. Grass is walkable; water and rock block movement. The application snapshot path calls `effective_chunk`, converting it into the existing detached `ChunkSnapshot`.

## 6. Application and presentation boundary

`Application::snapshot(SnapshotQuery)` retains ordered, deduplicated, detached snapshots but obtains terrain through `Simulation::effective_chunk`, not `generated_chunk`. No snapshot type, client API, or Bevy knowledge changes. A subsequent normal 3x3 client query therefore receives effective terrain through the existing boundary and rebuilds only its disposable presentation cache under existing central-chunk rules.

There is no `SetTerrain` command, client terrain-editing UI, or headless mutation option.

For proof of the production snapshot path, `progressus-app` may contain a private `#[cfg(test)]` unit-test constructor/injection seam from a prepared `Simulation`. It is not public, has no Cargo feature, and is unavailable to client/headless consumers. The test changes terrain through the production domain primitive, invokes ordinary production `Application::snapshot`, and never mutates a returned `ClientSnapshot` to manufacture an assertion.

## 7. Determinism, sparsity, and persistence preparation

Generating or reading unrelated base/effective chunks never creates an overlay record. Two distant modified chunks need only two delta records; chunks between them need not be materialized or retained. This prepares sparse persistence and residency work, but does not implement an LRU, resident cache, unload policy, serialization, save format, or disk save/load.

A future save can represent only:

```text
ChunkCoord -> [(LocalCell, Terrain override), ...]
```

in stable `BTreeMap` ordering; save metadata will separately own seed and worldgen version. The representation depends on no Bevy entity, address, hash iteration order, or materialized chunk.

## 8. Required tests

The implementation plan must include:

1. untouched `EffectiveChunk` structural equivalence (coordinate and row-major cells) to raw `GeneratedChunk`;
2. one override changes exactly one effective cell;
3. base-equal assignment removes its override and an empty delta removes its chunk record;
4. `Grass -> Rock -> Water -> Grass` is canonically identical to no modification;
5. distant modified chunks remain independent;
6. generating/reading other chunks does not affect modifications;
7. equal seed/version and equal ordered mutations produce identical authoritative effective state;
8. point lookup and `effective_chunk(...).terrain_at(...)` agree for modified and untouched cells;
9. base grass overridden to rock/water blocks movement, base rock/water overridden to grass permits it, and restoring base restores base passability;
10. an app unit test prepares a real `Simulation`, calls production `set_terrain_override`, injects it through the test-only seam, proves normal `Application::snapshot` returns effective terrain, and proves `generated_chunk` remains raw base;
11. existing worldgen repeatability/order-independence, movement, app boundary, headless traversal, Bevy client, and dependency-boundary checks stay green.

## 9. Documentation status after implementation

Implementation marks `P01-WORLD-05` only **partially advanced (bootstrap)**: sparse in-memory terrain overrides are authoritative for simulation and snapshots, but persistence is absent.

`P01-WORLD-04` remains incomplete: no explicit resident cache or unload policy exists. `P01-SIM-04` remains incomplete: this increment adds no navigation/pathfinding, jobs/AI policy, speed, collision, or persistence.

## 10. Explicit non-goals

No disk save/load, serialization/save format, items, resources, jobs, stockpiles, construction, buildings, crafting, pathfinding, Bevy terrain-editing UI, terrain gameplay command, chunk LRU/residency cache, database/filesystem storage, or mutable `GeneratedChunk` semantics are added.
