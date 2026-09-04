# Task: P02-N03 renewable food

- Date: 2026-09-05
- Status: Complete
- Milestone: Prototype 02 — Sustainable Settlement

## Goal

Close P02-N03 with a renewable source that feeds the existing physical food/eating loop without introducing abstract food inventory or a special logistics path.

## Implemented

- current worldgen advances to v3 while preserving v1/v2 generator identity;
- v3 adds deterministic `BerryBush` resources on grass, including four guaranteed starter bushes;
- each harvest produces a physical `Berries` stack with deterministic yield 3–5;
- harvested bushes regrow after 512 simulation ticks through sparse authoritative regrowth state;
- regrowth state persists across save/load;
- hungry characters with no available Berries may autonomously designate a reachable explored bush for ordinary Harvest;
- ordinary Haul moves harvested Berries into accepting stockpile floor cells;
- bootstrap Berries are reduced from 700 to 10 so renewable gathering, rather than a giant starter cache, carries long-run survival;
- the Bevy presentation has a procedural berry-bush sprite.

## Acceptance

- point/chunk worldgen queries remain deterministic and agree under v3;
- worldgen v2 coherent-terrain/resource behavior remains tested separately;
- a BerryBush disappears after Harvest, produces physical food, and returns at its exact deterministic deadline;
- an in-progress regrowth deadline round-trips through persistence and resumes deterministically;
- harvested Berries reach a normal Berries-accepting stockpile through ordinary Haul;
- with the bootstrap Berries removed, four starter bushes keep all five characters above zero satiety for 10,000 ticks;
- headless activity smoke rejects any sampled character starvation during its 100,000-tick run;
- dependency boundaries and authoritative pure-Rust simulation remain unchanged.

## Scope boundary

No farming plots, planting, seasons, crop growth stages, cooking, nutrition chemistry, preferences, spoilage, or abstract food inventory are introduced here. Those remain later gameplay choices.