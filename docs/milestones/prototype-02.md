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

- at least one deterministic renewable/gatherable food source;
- collection produces physical food;
- ordinary stockpile logistics can move it;
- five characters can remain fed in a long-run scenario when enough source capacity exists.

## 4. Stage B — Sleep and shelter

- authoritative fatigue state;
- autonomous Sleep job;
- a simple physical sleeping place/shelter structure;
- sleeping restores fatigue;
- shelter materially improves sleep/rest or prevents an environmental penalty;
- no hidden abstract housing inventory.

## 5. Stage C — Skills and practical knowledge

- small typed skill set, starting with skills actually used by implemented work;
- work grants deterministic practice/experience;
- skill has a measured gameplay effect such as work duration or yield;
- skill state persists and appears in the character inspector;
- knowledge remains partly attached to people as required by the vision.

## 6. Stage D — Mining and early metallurgy

- a non-renewable ore source distinct from ordinary Stone;
- physical ore item and extraction work;
- one furnace/smelting production object using the generic production-logistics contract;
- at least one fuel/material requirement;
- one metal intermediate/product;
- no teleporting inputs or outputs.

## 7. Stage E — Simple research/capability gating

Research is a prerequisite, not a magic production currency.

Prototype 02 needs only enough research/knowledge to prove that a capability can require both knowledge and physical prerequisites. Unlocking metallurgy must not itself create ore, fuel, furnaces, tools, or products.

## 8. Cross-cutting client usability pass

Status: **Complete (visual validation on owner PC pending)**

Before continuing the settlement systems, the Prototype 01/02 client received a usability pass: the flat toolbar became an icon-first HUD with Orders/Zones/Build palettes and localized hover help; middle-mouse drag pans the camera with grab-style inverted deltas; player move intent can continue into unexplored terrain without revealing it; stockpile zones render as a toggleable translucent layer and can be selected/configured; and stockpiles persist typed item acceptance filters that ordinary physical Haul obeys. A follow-up playtest pass made selection independent from the active tool (existing workstations/characters win a plain click without cancelling Build/Orders), fixed non-overlapping stockpile paint to create independent IDs/policies, moved tooltips beside/above the hovered HUD control, allowed Door designation to replace planned/completed StoneWall cells, and changed Water/Rock transition sprites to alpha-rounded overlays over a Grass underlay.

## 9. Persistence and determinism

Every new authoritative state introduced by Prototype 02 must either:

- be represented explicitly in the versioned save DTO; or
- be a documented derived cache rebuilt from authoritative state.

Save/load during an active need-satisfaction job must continue deterministically. Global stable-ID uniqueness and physical quantity conservation remain mandatory.

## 10. Headless acceptance

Add a `prototype-02` activity scenario that exercises at least food, sleep, skills, and metallurgy for a long run. It must verify:

- no crash or invariant violation;
- bounded raw chunk residency;
- no duplicate stable IDs;
- no item quantity creation outside explicit production/gather rules;
- characters actually consume food over time;
- save/load can occur while a need job is active;
- the settlement can reach a stable repeating loop under sufficient resources.

## 11. Non-goals

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

## 12. Definition of done

- [x] nutrition and autonomous physical eating work;
- [ ] renewable physical food can sustain the five-character settlement;
- [ ] sleep and shelter work;
- [ ] at least one practical skill changes work outcomes;
- [ ] ore extraction works;
- [ ] one early metallurgy chain works through physical production logistics;
- [ ] knowledge/research gates a capability without replacing physical prerequisites;
- [ ] all Prototype 02 authoritative state round-trips through persistence;
- [ ] long-run Prototype 02 activity smoke passes;
- [ ] performance impact is measured against the Prototype 01 baseline;
- [ ] architecture and gameplay documentation match the implementation.
