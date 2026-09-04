# Task: terrain transitions and doors

Status: **Implemented; owner-PC visual/client validation pending**
Date: 2026-09-04

## Goal

Make natural terrain read as continuous lakes and mountain masses rather than stamped square cells, while keeping terrain semantics authoritative and cell-based. Add the first passable wall-network structure: an automatically opening physical door.

## Terrain presentation

- `Grass / Water / Rock` remain the only authoritative terrain semantics in this pass.
- Coast and foothill visuals are derived client presentation; they are not new terrain kinds.
- Water and Rock inspect all eight neighbouring known cells.
- Unknown neighbours must not reveal hidden terrain: an unknown neighbour is rendered as visually continuous until discovered.
- Water edges gain shallow water, a narrow shore strip and rounded cardinal/diagonal corners.
- Rock edges gain talus/soil/moss foothill treatment and rounded cardinal/diagonal corners.
- Diagonal-only unlike neighbours between two connected cardinal neighbours use an opaque curved shore/foothill bridge inside the Water/Rock tile; presentation must never expose a fake Grass wedge inside authoritative water or rock.

## Door mechanics

- Add `StructureKind::Door` to the physical construction system.
- A basic door costs 2 Wood and 6 construction work ticks.
- Doors share wall connectivity with StoneWall for presentation.
- Completed doors are passable; StoneWall remains blocking.
- A door cell has a navigation cost of 2 versus 1 for ordinary walkable ground.
- Closed doors therefore remain routeable but a reasonable door-free route may be preferred.
- A door opens authoritatively while a character occupies its cell and remains open for a short deterministic hold window after the character leaves.
- Door open/closed presentation state crosses `progressus-app` and persists in save format v1 through an additive optional field.
- Existing v1 saves without door state remain valid and restore doors closed.

## Client interaction

- Door is a point-placement item in the hierarchical Build palette.
- Door, wall and their blueprints form one visual cardinal wall network.
- Closed and open door sprites are procedurally generated.
- No manual locking, ownership/access policy, hold-open toggle, demolition conversion, or powered-door behavior is part of this pass.
