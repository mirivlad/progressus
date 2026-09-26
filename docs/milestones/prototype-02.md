# Milestone: Prototype 02 — Sustainable Settlement

Status: **In progress**

## 1. Purpose

Prototype 02 must prove that the Prototype 01 physical-world foundation can support a small settlement that sustains people over time rather than only executing player-issued work.

The central loop is:

```text
human need
→ physical demand
→ gathering / production
→ storage / logistics
→ consumption
→ changed character state
```

Needs are not abstract score drains: satisfying them must consume or use physical world state whenever the need is material.

## 2. Target player-visible scenario

The same five persistent characters can establish a primitive settlement that:

1. obtains and stores food;
2. becomes hungry over simulation time;
3. autonomously interrupts ordinary work to eat physical food;
4. resumes useful work after eating;
5. sleeps and benefits from shelter;
6. develops simple practical skills through work;
7. extracts a non-renewable ore resource;
8. performs one early metallurgy chain;
9. unlocks that capability through simple knowledge/research prerequisites;
10. survives save/load and long-run headless simulation without quantity creation.

## 3. Stage A — Food and nutrition

### P02-N01 — Character nutrition

Status: **Complete**

- authoritative bounded satiety/nutrition state per character;
- deterministic decay based only on simulation ticks;
- detached snapshot + localized character inspector;
- save/load round trip;
- zero nutrition is observable and prevents endless normal work.

### P02-N02 — Physical food and autonomous eating

Status: **Complete**

- at least one concrete physical food item;
- hungry characters create/receive an explicit Eat job;
- the food stack is reserved against competing jobs;
- the character physically travels to it;
- exactly one unit is consumed;
- nutrition rises by a fixed deterministic amount;
- cancellation/interruption releases reservations without consuming food.

### P02-N03 — Renewable food source

Status: **Complete**

- at least one deterministic renewable/gatherable food source;
- collection produces physical food;
- ordinary stockpile logistics can move it;
- five characters can remain fed in a long-run scenario when enough source capacity exists.

Worldgen v3 adds deterministic `BerryBush` sources, including four guaranteed bushes around the starting clearing. Harvest creates 3–5 physical Berries, the bush regrows after 512 authoritative ticks, regrowth state round-trips through persistence, ordinary Haul can stockpile the food, and hungry characters with no free Berries may autonomously designate an explored reachable bush for ordinary Harvest. A 10,000-tick test removes the bootstrap food entirely and verifies that all five characters remain above zero satiety. See [`ADR-0018`](../adr/0018-renewable-berry-bushes-and-worldgen-v3.md).

## 4. Stage B — Sleep and shelter

Status: **Implemented; native behavior observed, owner aesthetic acceptance pending**

- Each character has persistent authoritative rest (`0..=100`), decaying by one every 48 ticks. At 30 or below, an exclusive autonomous Sleep job can reserve a finished Bed; without a reachable free bed, the character sleeps on the ground. Hunger and direct player orders preempt Sleep and release its reservation.
- Bed is an ordinary physically delivered `2 Wood` construction, passable and separate from wall connectivity. At Sleep completion, a bounded 256-cell cardinal fill derives whether real walls, doors, or impassable terrain enclose the sleeping cell. An enclosed bed restores rest to 100; an open bed adds 50; ground sleep adds 25. No housing inventory or stored room state exists.
- Save v1 persists rest, active Sleep and bed ownership, and the most recent shelter verdict. Detached snapshots expose them to the localized inspector and the Bed build tool/low-poly presentation. Headless tier, reservation, save/load and 10,000-tick five-person food/rest tests pass. Native bed designation, completed bed geometry, and a character visibly lying on it during Sleep were observed on 2026-09-26. Owner aesthetic acceptance remains open. See [`ADR-0020`](../adr/0020-rest-sleep-and-enclosure-shelter.md).

## 5. Stage C — Skills and practical knowledge

Implemented: the typed, stable-name Gathering, Mining and Crafting registry gives each person 0–5 persistent practice. A completed physical Harvest or Craft job awards one point only to its worker; sources requiring the mining capability train Mining, other sources train Gathering. At five points the matching work phase takes one tick less (minimum one), without changing output/input quantities or bypassing tools. Save v1 accepts old characters with no skill field, validates named entries and retains active work ticks; detached snapshots expose localized progress in the character inspector. Native inspector appearance still needs a visual smoke.

## 6. Stage D — Mining and early metallurgy

Status: **Rock excavation and a basic furnace recipe implemented; ground excavation remains open**

- a non-renewable ore source distinct from ordinary Stone;
- physical ore item and extraction work;
- player-directed physical excavation of ground and rock on the existing flat map: work changes the authoritative cell and creates physical output that must be transported; no underground levels in Prototype 02;
- one furnace/smelting production object using the generic production-logistics contract;
- at least one fuel/material requirement;
- one metal intermediate/product;
- no teleporting inputs or outputs.

The non-renewable copper-vein worldgen layer yields physical CopperOre, with mining gated by equipped tool capabilities. A headless regression proves physical inputs become a tool at the workbench, a named character fetches/equips it, then a revealed copper vein is depleted and all resulting ore is physically hauled to a stockpile. The fixture stages that character near the distant vein; it does not prove long-distance player travel or native UI operation. The furnace uses the existing fixed physical input/output ports and production orders: two CopperOre plus one Wood consumed as fuel produce one CopperIngot on an output port. The new content has its own low-poly models and localized Build/production UI. Native inspection on 2026-09-26 confirmed the placed furnace model, separate furnace icon, localized recipe, order and port controls; the ingot model and end-to-end smelting still need native observation.

D1 adds exclusive persisted `ExcavateRock` work over explored Rock cells. A worker must physically equip a mining-capable tool and approach from reachable cardinal ground; completion alone changes `Rock -> Grass`, creates exactly physical `Stone x1`, and awards one Mining practice point. The saved terrain revision remeshes the changed visible cell, and the Orders palette filters only known Rock cells while drawing its preview/job marker above raised rock geometry. Headless lifecycle, interruption, path failure, save/load, ordinary Haul, application boundary and client tests pass. Native click/worker/remesh/haul observation remains pending. Ordinary ground excavation and pits/levels are not implemented.

## 7. Stage E — Simple research/capability gating

Research is a prerequisite, not a magic production currency.

Prototype 02 needs only enough research/knowledge to prove that a capability can require both knowledge and physical prerequisites. Unlocking metallurgy must not itself create ore, fuel, furnaces, tools, or products.

The Workbench offers a one-time `Study copper ore` order. A worker reserves one physical CopperOre at an Input port, spends 32 base work ticks studying it, and returns one CopperOre at an Output port. Completion records persistent settlement knowledge of metallurgy. Before that, a furnace smelting order is rejected and its recipe is marked locked in the localized workstation window. Knowledge does not supply the two ore, one wood fuel, furnace, worker, or free output port required by each smelt. Save v1 defaults missing knowledge to unknown and rejects unknown or duplicate knowledge names.

## 8. Cross-cutting client usability pass

Status: **Complete**

Before continuing the settlement systems, the Prototype 01/02 client received a usability pass: the flat toolbar became an icon-first HUD with Orders/Zones/Build palettes and localized hover help; middle-mouse drag pans the camera with grab-style inverted deltas; player move intent can continue into unexplored terrain without revealing it; stockpile zones render as a toggleable translucent layer and can be selected/configured; and stockpiles persist typed item acceptance filters that ordinary physical Haul obeys. Follow-up playtests fixed non-overlapping stockpile paint to create independent IDs/policies, moved tooltips beside/above the hovered HUD control, allowed Door designation to replace planned/completed StoneWall cells, and changed Water/Rock transition sprites to alpha-rounded overlays over a Grass underlay. Select-mode clicks now resolve exact physical objects before stockpile ground (workstation/character, then item/source, then stockpile), while an active construction tool keeps the click for designation. Construction intent on passable ground is accepted over removable sources, loose stacks, and characters; explicit persisted preparation clears them physically before walls, doors, or deferred workbenches complete. Owner-PC visual validation of the earlier terrain/door/HUD pass was completed on 2026-09-04; the 2026-09-12 selection/preparation extension still requires a fresh native visual smoke.

## 9. Minimal idle life pass

Status: **Complete**

Owner-PC visual validation completed on 2026-09-05. Characters who have no job and no urgent need now exhibit deterministic low-priority life rather than standing forever. Idle people periodically take short authoritative `Wandering` routes through explored walkable cells, and occasionally approach another nearby idle person. Wandering is not a job: real work, hunger and player orders may preempt it. Each character keeps a persistent local idle anchor and every idle route stays within Manhattan radius three of that anchor, so repeated wandering cannot become autonomous migration/scouting. See [`ADR-0017`](../adr/0017-bounded-deterministic-idle-behavior.md).

## 9a. Articulated visual slice

Status: **In progress**

The client now renders procedural articulated people with detached-snapshot idle/walk/work poses and physical tool/cart attachments; shared meshes and the cart's wheel geometry remain client-owned. Headless simulation and saves are unchanged. Automated scene tests cover attach/drop/load/eviction and native default/close character captures exist. Native equipped-tool/cart captures, live picking validation, and owner aesthetic review remain open; this visual slice is not yet accepted as complete.

## 10. Persistence and determinism

Every new authoritative state introduced by Prototype 02 must either:

- be represented explicitly in the versioned save DTO; or
- be a documented derived cache rebuilt from authoritative state.

Save/load during an active need-satisfaction job must continue deterministically. Global stable-ID uniqueness and physical quantity conservation remain mandatory.

## 11. Headless acceptance

Add a `prototype-02` activity scenario that exercises at least food, sleep, skills, and metallurgy for a long run. It must verify:

- no crash or invariant violation;
- bounded raw chunk residency;
- no duplicate stable IDs;
- no item quantity creation outside explicit production/gather rules;
- characters actually consume food over time;
- save/load can occur while a need job is active;
- the settlement can reach a stable repeating loop under sufficient resources.

## 12. Non-goals

Prototype 02 does not require:

- deep mood/personality simulation;
- relationships or families;
- combat;
- complex cooking/nutrition chemistry;
- diseases;
- seasons/weather agriculture;
- electricity;
- vehicles;
- multi-settlement Simulation LOD;
- a large research tree.

## 13. Definition of done

- [x] nutrition and autonomous physical eating work;
- [x] renewable physical food can sustain the five-character settlement;
- [x] sleep and shelter work in authoritative simulation and client plumbing (native behavior observed; owner aesthetic acceptance pending);
- [x] at least one practical skill changes work outcomes (Stage C headless/client checks; native inspector smoke pending);
- [x] ore extraction works in the headless simulation (native acceptance pending);
- [x] one early metallurgy chain uses physical production ports (native acceptance pending);
- [x] knowledge/research gates metallurgy without replacing physical prerequisites;
- [ ] all Prototype 02 authoritative state round-trips through persistence;
- [ ] long-run Prototype 02 activity smoke passes;
- [ ] performance impact is measured against the Prototype 01 baseline;
- [ ] architecture and gameplay documentation match the implementation.
