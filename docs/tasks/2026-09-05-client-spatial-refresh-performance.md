# Task: client spatial refresh/render performance regression

- Date: 2026-09-05
- Status: Implemented; owner-PC CPU/frame validation pending

## Problem

After renewable food landed, a small visible settlement drove the client above two CPU cores and character interpolation became visibly jerky. The first investigation found real but secondary invalidation bugs: item/resource revisions shared terrain refresh work, and autonomous berry gathering could chain into unintended exploration. Fixing those did not materially reduce the reported CPU load (~356% on the owner PC).

The deeper rendering problem was the terrain representation itself. Every explored cell was a separate Bevy `Sprite` entity, with Water/Rock sometimes requiring a second Grass-underlay entity. A modest explored viewport therefore produced thousands of persistent render entities that Bevy had to extract/prepare every frame even while the terrain was static.
## Fix

- `SnapshotQuery` independently selects terrain, ground-item and natural-resource spatial layers.
- Item/resource-only revisions no longer request or rebuild terrain.
- Autonomous food harvesting is bounded to the bootstrap forage area; manual Harvest remains unrestricted.
- Terrain presentation is now chunk-batched: one visible authoritative chunk is one Bevy sprite backed by one composed procedural texture, rather than up to 1,024+ per-cell sprite entities.
- Terrain chunk textures preserve the existing procedural Grass/Water/Rock variants, eight-neighbour shoreline/foothill topology and Grass underlay semantics.
- The terrain root stays stable. Each chunk has a presentation fingerprint including a one-cell neighbour ring; exploration updates regenerate only chunks whose visible terrain/topology actually changed.
- Leaving the camera window despawns only stale chunk sprites/images. Static chunks remain alive and their image handles are updated in place when needed.
- The workspace dev profile now uses optimization (`opt-level=1`, dependencies `opt-level=3`) so `cargo run -p progressus-client` does not execute the Bevy/WGPU stack as fully unoptimized debug code.

## Validation

Server-safe validation includes fmt, Clippy, client all-target typecheck, app/sim/headless tests, dependency boundaries and existing 100k smoke scenarios. Client regression tests now assert that terrain roots survive exploration/camera refreshes and that a full known chunk produces one terrain-chunk presentation entry rather than per-cell entities.

The graphical client is not linked/run on `tomas`. Owner-PC validation must compare CPU percentage and movement smoothness against the reported ~356% case using the same seed, zoom and approximate explored area.
