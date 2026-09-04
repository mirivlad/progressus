# ADR-0005: Procedural visual assets are first-class source code

- Status: Accepted
- Date: 2026-09-02
- Amended: 2026-09-04 (derived terrain transition topology and alpha-corner overlays)

## Context

Progressus prefers procedural graphics where authored bitmap art is not required. The Prototype 01 client initially represented terrain, characters, items, and natural resources as solid-color rectangles. That was useful for validating simulation boundaries but is no longer adequate for visual playtesting.

The project also needs visual assets to remain readable, versionable, and modifiable as source rather than existing only as opaque generated PNG files.

## Decision

Procedural visual definitions live as Rust source under the repository-level `assets/procedural/` tree. `progressus-client` owns the rasterizer, Bevy `Image` creation, caching, and rendering integration; the asset source code only describes how to draw onto the client's deterministic integer canvas.

Procedural art remains presentation-only. Simulation, world generation, application read models, pathfinding, and gameplay decisions must not depend on generated pixels or Bevy image handles.

A world cell or stable Progressus entity ID may deterministically select a visual variant. Presentation may also derive a bounded neighbour-topology key from already published authoritative cells. Water/Rock terrain use an eight-neighbour mask to draw coast/foothill transitions; unknown neighbours are deliberately treated as continuous so raster selection cannot become a terrain-discovery side channel. Known non-Grass cells are rendered over a deterministic Grass underlay, allowing Water/Rock procedural overlays to use transparent convex/diagonal corner pixels for rounded silhouettes without exposing the world clear colour or changing authoritative terrain. Variant/topology spaces must be bounded so a large or unbounded world cannot create one GPU texture per cell or entity. Generated images are cached and reused.

PNG and other authored assets remain allowed when they provide clear value, but generated PNG files are not the canonical source for procedural assets.

## Consequences

Visual assets can be reviewed and changed as ordinary code, and generated presentation remains reproducible. The client can progressively replace bootstrap art without changing authoritative state. Runtime generation adds a small startup/lazy cost, but bounded caching prevents generation from becoming an unbounded residency policy.
