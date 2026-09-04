# Task: client spatial refresh performance regression

- Date: 2026-09-05
- Status: Implemented; owner-PC CPU/frame validation pending

## Problem

After renewable food landed, a small visible settlement could drive the client above two CPU cores and character interpolation became visibly jerky. Item/resource/exploration revisions shared one presentation invalidation path, so changing one Berry stack or bush could regenerate a full chunk terrain snapshot and despawn/recreate the entire visible terrain entity tree.

Autonomous berry harvesting also searched the complete explored set and could chain through newly revealed wild bushes, increasing both exploration and presentation invalidations while acting as unintended free scouting.

## Fix

- `SnapshotQuery` can independently include terrain, ground-item and natural-resource spatial layers.
- Item-only revisions request/reconcile ground items without generating or rebuilding terrain.
- Resource-only revisions request/reconcile resources without generating or rebuilding terrain.
- Terrain rebuild is limited to viewport/exploration changes.
- Autonomous food harvesting searches only the bounded bootstrap forage area; manual Harvest remains unrestricted.
- Regression coverage verifies selective application snapshots and that item/resource revisions do not replace a static terrain root.
- The long-run renewable-food test also verifies autonomous food gathering stays spatially bounded.

## Validation

Server-safe checks must include fmt, Clippy, app/sim/headless tests, client test compilation, dependency boundaries and the existing 100k smoke scenarios. The graphical client is not linked/run on `tomas`; CPU percentage and movement smoothness must be rechecked on the owner PC against the reported ~246% regression.
