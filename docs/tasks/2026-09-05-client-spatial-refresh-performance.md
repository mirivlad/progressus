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
- The focused Winit event loop uses reactive low-power scheduling matched to the refresh rate reported for the monitor containing the window (for example 60 Hz or 200 Hz) instead of a hard-coded cap. If refresh metadata is unavailable it falls back to Bevy's continuous mode rather than silently capping a high-refresh display. The unfocused window drops to 15 Hz.
- Diagnostics now include tick-only authority timing plus spatial snapshot and terrain/item/resource refresh timings, so periodic 4 Hz spikes are not hidden by 60-frame averages.

## Validation

Server-safe validation includes fmt, Clippy, client all-target typecheck, app/sim/headless tests, dependency boundaries and existing 100k smoke scenarios. Client regression tests now assert that terrain roots survive exploration/camera refreshes and that a full known chunk produces one terrain-chunk presentation entry rather than per-cell entities.

The graphical client is not linked/run on `tomas`. Owner-PC validation reduced the reported load from ~356% to ~70% after chunk batching, which confirms the main terrain-entity regression but leaves a material steady-state client cost. A temporary `--diagnostics` mode now logs FPS/frame time, Bevy entity count, Progressus update/authority/presentation timing, WGPU render-pass diagnostics, and the selected render adapter so the remaining cost can be measured on the owner PC instead of inferred from `htop`.
## Spatial snapshot spike follow-up

Diagnostics on the owner PC exposed a separate 28-30 ms `spatial_snapshot` spike while authority, presentation reconciliation and GPU work remained sub-millisecond. The application boundary was generating natural-resource data for every requested camera-window chunk and only filtering by exploration afterwards. A viewport containing four explored chunks could therefore regenerate dozens of completely unknown chunks. `ExploredWorld` now maintains an authoritative explored-chunk index, and `Application::snapshot` intersects requested spatial chunks with that index before terrain, ground-item or natural-resource work. A 63-chunk synthetic query with four explored chunks fell from roughly 85-92 ms to 0.6-0.85 ms on `tomas`; the resource-only path fell from roughly 82 ms to 0.16-0.22 ms.
