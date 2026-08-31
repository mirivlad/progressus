# ADR-0002: Rust Simulation Core + Bevy Client

Status: **Accepted**

Date: 2026-08-31

Decision owner: project owner

## Context

Progressus is intended to simulate a continuous, potentially unbounded world whose population may eventually grow from a handful of people to large towns, cities, and regions.

The main technical risk is therefore not rendering complexity but simulation scale: population, jobs, logistics, persistence, long time spans, chunk streaming, and future Simulation LOD.

A previous colony/civilization simulation experiment built around Godot showed poor scaling as population increased. Progressus is expected to require substantially larger simulation scale, so coupling authoritative game state to an engine scene/object model is considered an unnecessary architectural risk.

The project also intends to use procedural graphics by default. This reduces the value of choosing an engine primarily for scene editing and authored-asset workflows.

## Decision

Progressus will use:

- **Rust** as the implementation language for the authoritative simulation;
- a **pure Rust simulation core** that does not depend on Bevy, a renderer, a windowing system, or engine object identities;
- **Bevy** as the initial game client/rendering framework;
- a strict boundary between authoritative simulation state and presentation state.

Conceptually:

```text
+------------------------------------+
|       progressus-client            |
| Bevy / rendering / input / UI      |
| procedural visuals / audio         |
+------------------+-----------------+
                   |
          commands / read models
                   |
+------------------v-----------------+
|        progressus-sim              |
| pure Rust authoritative simulation |
| world / people / jobs / economy    |
| logistics / technology / history   |
+------------------+-----------------+
                   |
+------------------v-----------------+
| worldgen / persistence / data      |
+------------------------------------+
```

The exact crate split may evolve, but the dependency direction is normative: **the simulation must not depend on Bevy**.

## Consequences

### 1. Bevy is a client technology, not the game model

Bevy may be used for:

- rendering;
- windowing;
- input;
- camera handling;
- UI;
- audio;
- shaders;
- visual ECS/state used only by the client;
- batching and GPU-facing representation.

Bevy types must not become authoritative merely because they are convenient.

### 2. Bevy `Entity` is not a persistent Progressus identity

Persistent simulation entities use Progressus-owned stable identifiers.

Bevy `Entity`, scene IDs, renderer handles, memory addresses, array indexes, and other transient handles must never be serialized as authoritative identity.

The client may maintain temporary mappings such as:

```text
ProgressusId -> Bevy Entity
```

Those mappings are disposable and rebuildable.

### 3. Headless simulation is first-class

The core must be runnable directly without Bevy and without creating a graphics context.

Tests and benchmarks should be able to do conceptually:

```text
let mut sim = Simulation::new(seed);
for _ in 0..1_000_000 {
    sim.tick();
}
```

This is required for:

- deterministic regression tests;
- accelerated long-run simulation;
- profiling;
- population-scale benchmarks;
- save/load qualification;
- automated agent work and CI.

### 4. Simulation representation is owned by Progressus

The project is free to use structs, tables, sparse sets, custom ECS-like storage, indices, arenas, or hybrids inside `progressus-sim`.

No external ECS framework is mandated for the authoritative core by this ADR.

This is intentional because future Simulation LOD may need representations that differ radically between detailed and remote populations.

For example, a nearby settlement may contain detailed people while a distant region may use an aggregate population representation. That transition must be a simulation concern, not a renderer concern.

### 5. Not every game object must be an entity

Large homogeneous quantities should not automatically become one runtime entity per physical unit.

Examples that may use aggregated representations when gameplay permits include:

- item stacks;
- bulk materials;
- remote population groups;
- routine production flows.

Detail should exist where detail has gameplay consequences.

### 6. Rendering consumes simulation state

The simulation determines **what exists and what is true**.

The renderer determines **how that truth is shown**.

A simulation building may expose properties such as footprint, material, era, condition, and function. The Bevy client may procedurally derive roof shape, brick variation, grime, decorative details, animation, and other visual state.

Changing a visual-generation algorithm must not change authoritative simulation results.

### 7. Procedural graphics are the default direction

The initial visual architecture should favor deterministic procedural construction of terrain, vegetation, buildings, characters, vehicles, and decorative variation, with authored assets used when they provide clear value.

Visual randomness must use presentation-only seeds/state and must never consume authoritative simulation RNG.

At minimum, the architecture should conceptually separate:

- world-generation randomness;
- authoritative simulation randomness;
- visual/presentation randomness.

### 8. Bevy remains replaceable in principle

Bevy is the selected initial client framework, not a permanent dependency of saved worlds or simulation rules.

Replacing the renderer/client in the future should be expensive but architecturally possible without rewriting the simulation model.

## Rejected alternatives

### Godot as the primary simulation host

Rejected for Progressus.

Godot remains capable for many games, but tying a simulation with the intended population scale to the engine's scene/object lifecycle is considered unnecessary risk. Prior project experience also showed population scaling problems in a Godot-hosted simulation.

### Rust simulation core embedded in Godot

Rejected for the initial architecture.

This preserves simulation performance but introduces a permanent two-language/toolchain integration boundary, engine extension/FFI complexity, and two competing object models without a demonstrated benefit over a Rust-native client.

### Fully custom Rust renderer/engine

Rejected for now.

It offers maximum control but would push the project toward writing engine infrastructure rather than proving the game. Bevy provides the client-side systems we need while preserving a Rust-native toolchain.

### Bevy ECS as the authoritative simulation model

Not selected by this ADR.

It may be evaluated for limited internal use later, but the core must remain independent of Bevy itself. Authoritative storage decisions will be made from profiling and simulation requirements rather than renderer convenience.

## Performance policy

The stack decision is complete, but performance qualification is not.

Prototype development must establish representative headless benchmarks for increasing population and world activity. Performance work remains evidence-driven:

1. create a representative scenario;
2. measure;
3. identify the real bottleneck;
4. optimize;
5. re-measure.

The project should prefer simple deterministic single-threaded authoritative simulation until measurements justify parallelism. Any parallel authoritative simulation model still requires a separate ADR.

## Initial repository direction

Expected shape:

```text
crates/
  progressus-sim/       # pure Rust authoritative simulation
  progressus-worldgen/  # deterministic procedural world generation
  progressus-client/    # Bevy client / renderer / input / UI
  progressus-app/       # executable integration layer if useful
```

This layout is directional rather than mandatory. A coding task may propose a smaller initial split if it preserves the dependency boundary.

## Acceptance rule for future work

A change violates this ADR if authoritative simulation correctness requires Bevy to exist.

In particular, agents must stop and report a conflict before introducing any requirement that:

- makes `progressus-sim` depend on Bevy;
- uses Bevy `Entity` as persistent game identity;
- requires a renderer/window/GPU for simulation tests;
- makes animation/frame timing authoritative;
- lets visual RNG alter simulation outcomes.
