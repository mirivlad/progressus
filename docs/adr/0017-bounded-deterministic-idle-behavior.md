# ADR-0017: Bounded deterministic idle behavior

Status: **Accepted**
Date: 2026-09-04

## Context

Prototype 02 has five persistent characters, physical work, logistics and autonomous eating, but a character with no job or urgent need otherwise remains motionless forever. That is mechanically valid but makes the settlement read as a set of waiting robots rather than inhabited space.

Deep psychology, recreation, relationships and personality remain explicit Prototype 02 non-goals. The project needs only enough autonomous low-priority behavior to keep idle people visibly alive without creating a second job system or allowing free exploration.

## Decision

Idle life is authoritative simulation behavior, not presentation animation.

- `MovementState::Wandering` represents low-priority autonomous movement and uses the existing exact fixed-point navigation route machinery.
- Wandering is not a `Job` and owns no work reservation.
- A wandering character remains available to the ordinary worker scheduler; real work may replace the wander route immediately when assigned.
- Hunger and other higher-priority needs may likewise interrupt wandering.
- Player movement commands always replace wandering normally.
- Idle decisions are deterministic from world seed, stable character ID and simulation tick cycle; no wall-clock or client RNG participates.
- Characters spend substantial time standing between idle decisions rather than performing a continuous random walk.
- A character keeps an authoritative `idle_anchor`, initially its spawn cell and later the location where meaningful non-wandering movement ends.
- Idle destinations and every cell of an idle route must remain inside Manhattan radius 3 of that anchor and must already be explored and walkable.
- A rare social idle choice may approach an adjacent cell of another currently idle nearby character, but only when that destination also satisfies the same local-anchor bound.
- Wandering never moves the idle anchor. It therefore cannot become an unbounded random walk or migrate the settlement by repeatedly treating each idle destination as a new origin.
- Active wandering state and `idle_anchor` persist in save format v1 through additive fields/variants; older v1 saves without an idle anchor default it to the restored character position.

The small amount of incidental visibility gained while walking around an already known local area is acceptable. Idle behavior must never intentionally route toward unknown terrain, and the fixed anchor prevents repeated idle decisions from becoming autonomous scouting.

## Consequences

The settlement remains visibly active even when the player issues no commands, while work/needs retain priority. Later beds, homes, fires, tables, workplaces, social spaces or recreation systems may become richer idle destinations, but they should extend this priority model rather than turn idle behavior into hidden mandatory work.
