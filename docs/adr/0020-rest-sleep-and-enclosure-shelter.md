# ADR-0020 — Rest, sleep, and enclosure as shelter

Status: **Proposed**

Date: 2026-09-10

Decision owner: project owner, who chose real enclosed-space detection over a bed-only sleep bonus.

## Context

Prototype 02 Stage B requires authoritative fatigue, an autonomous Sleep job, a physical sleeping place, and shelter that *materially* improves rest. Stage A already established the pattern for a physical need: bounded authoritative satiety, deterministic tick decay, an explicit Eat job that reserves and consumes one real stack, and no abstract score refill ([`ADR-0016`](0016-authoritative-satiety-and-physical-eating.md)).

Shelter is harder than food because there is no obvious physical object to consume. Two models were considered:

1. A bed restores more than bare ground. Cheap and reversible, but shelter becomes a number attached to one structure rather than a fact about the built world.
2. Enclosure detection: a room is discovered from the walls and doors the player actually built, and sleeping inside one is what shelter means.

The owner chose the second. It matches the project's guiding question — progress as physical transformation of the world — and it makes the existing wall/door network mean something beyond blocking movement.

The obstacle is that the world is effectively unbounded. A naive "is this space closed?" flood fill over an open world does not terminate, and would force terrain generation for arbitrary chunks. Any enclosure rule must therefore be bounded before it is correct.

## Decision

### Rest is an authoritative need modeled like satiety

Each `Character` gains a bounded `rest` value in `0..=MAX_REST` (`100`), starting rested. It decreases by `1` every `REST_DECAY_INTERVAL_TICKS` (`48`) authoritative ticks, three times slower than hunger, so a full rest cycle spans roughly 4,800 ticks.

The milestone calls this state fatigue. The stored field is its complement, `rest`, so that every need in this codebase reads "higher is better" and decays over time. Mixing a decaying satiety with a rising fatigue in the same tick phase invites sign errors for no gain.

Decay depends only on the simulation tick, never on wall-clock time or frame timing. A character at or below `TIRED_REST` (`30`) is tired and becomes eligible for an autonomous Sleep job.

### The bed is an ordinary physical structure

`StructureKind::Bed` joins the existing construction lifecycle unchanged: designation creates a stable-ID `ConstructionSite`, the site reserves one concrete physical stack, `DeliverConstruction` carries that same stack to a reachable adjacent position, and only a delivered stack enables `Construct`. A bed costs `2 Wood` and fixed construction work ([`ADR-0009`](0009-physical-construction-sites-and-blocking-structures.md)).

A finished bed occupies its cell as authoritative structure occupancy and is passable with the same navigation cost as a door, so a sleeper can stand on it and a bed cannot silently wall off a room. A bed does **not** join the wall network: `connects_to_wall_network()` stays false, so bed geometry never participates in the cardinal connectivity mask of [`ADR-0011`](0011-cardinal-connectivity-autotiles.md).

### Sleep is an ordinary job with exclusive reservation

`JobKind::Sleep { character_id, bed_id }` reuses the existing job lifecycle: `Available`, `Reserved`, `Working`. A tired character reserves one concrete unoccupied bed, travels to it through ordinary bounded explored-world navigation, and performs `SLEEP_WORK_TICKS` (`64`) of work. One bed serves one sleeper at a time; the reservation is exclusive and is released by cancellation, manual interruption, or path failure exactly like every other job reservation.

If no reachable unoccupied bed exists, the character sleeps on the ground at its current position instead of deadlocking. Ground sleep is a real job with the same lifecycle; it simply has no bed to reserve.

Eat outranks Sleep. A character that is both hungry and tired eats first, and a Sleep job in progress is preempted by hunger, by a player order, and by nothing else. Tiredness never blocks work the way starvation does; there is no exhaustion collapse state in this prototype.

### Enclosure is a bounded, on-demand property, not a maintained world

Restoration depends on where the sleep happens, and that is decided by an explicit enclosure test evaluated **when a Sleep job completes**, against the cell the character slept in.

The test is a breadth-first fill over cardinal neighbours from that cell:

- The fill passes through a cell when its effective terrain is walkable and no completed wall-network structure occupies it.
- `StoneWall` and `Door` bound the room. A door bounds it whether it is currently open or closed; the automatic open/closed state of [`ADR-0009`](0009-physical-construction-sites-and-blocking-structures.md) describes passage, not shelter.
- Non-walkable terrain bounds the room, so a pocket enclosed by natural rock shelters as well as a built one.
- Unfinished construction sites do not bound anything. A planned wall is not a wall.
- Beds and workbenches do not bound anything. They are furniture inside a room, and treating them as boundaries would let furniture cut a room in half and report an open field as enclosed.
- A cell that does not exist, at the coordinate limit, bounds the fill.

The fill returns **enclosed** when its frontier empties, and **open** when it has visited more than `ENCLOSURE_CELL_BUDGET` (`256`) cells. The budget is what makes the rule total: an unbounded space always exhausts it, a room never can, and no query can walk off into arbitrary terrain. Like the radius-five discovery disk and the radius-eight forage bound, `256` is a provisional gameplay constant, not a claim about the largest room anyone should ever build.

The verdict is a property of a cell set, so it does not depend on visitation order. The fill reads effective terrain through the existing point lookup, which does not materialize chunks and must not expand raw-chunk residency. It reads real terrain, not `KnownTerrain`: whether a room is enclosed is a fact about the world, not about what the player has discovered, and the fill never touches `ExploredWorld` or reveals a cell.

Enclosure is therefore a derived answer computed at the moment it is needed, never a `RoomWorld`, never a cached room ID, and never a persisted field. There is nothing to invalidate when a wall is built, cancelled, replaced by a door, or when a terrain override changes, because nothing is stored. This is the smallest implementation that satisfies the requirement; a maintained room index remains available later if a room overlay or a measured cost justifies one.

### Shelter changes the outcome in three tiers

Completing a Sleep job restores rest by:

| Where the character slept | Restored |
| --- | ---: |
| In a bed, inside an enclosed space | to `MAX_REST` |
| In a bed, not enclosed | `50` |
| On the ground | `25` |

Both the bed and the enclosure independently change the result, which is what makes building a shelter worth the material rather than decorative.

### Persistence

`rest` is new authoritative character state and is written explicitly to the versioned save DTO as an additive optional field, defaulting to `MAX_REST` for saves written before this change, in the same way door state was added. Sleep jobs, bed reservations, and bed structures round-trip through the existing job, reservation, and construction persistence with no new mechanism.

Enclosure is never saved. It is recomputed from terrain and structures on demand, which is the documented derived-state option the milestone requires.

### Client

The Build palette gains Bed. The bed is a deterministic presentation-only low-poly mesh. The character inspector shows rest alongside satiety, and shows the sleeping state and whether the completed sleep counted as sheltered. Presentation never computes or owns the enclosure verdict.

## Consequences

- Shelter is earned from the wall and door network the player already builds, and the first real reason to enclose a space rather than build isolated walls.
- A natural rock pocket shelters. This is deliberate, and it makes the earliest settlement viable before anyone can afford stone walls.
- There is no roof and no weather in this prototype, so enclosure is the whole of shelter. Adding a roof later would refine which enclosed spaces count without changing what a room is.
- A space larger than the budget reads as open even if it is genuinely walled. Very large halls are not shelterable until the budget is revisited with evidence.
- The enclosure test costs at most a few hundred point terrain lookups per completed sleep, which is rare. If a future room overlay needs a continuous answer for the whole viewport, that is a new decision with its own measurement, not an extension of this one.
- No demolition, no bed ownership or assignment, no shared or double beds, no exhaustion collapse, no sleep schedule or day/night cycle, and no temperature. Rest decays continuously; characters sleep when tired, not when it is dark.

## Verification

- Deterministic decay: rest falls only on the interval, identical command sequences produce identical rest.
- A tired character with a reachable bed reserves exactly one, and a second tired character cannot reserve the same bed.
- Cancellation, manual interruption, and path failure leave no orphan bed reservation.
- Save and load during an active Sleep job continues deterministically, and a pre-change save loads rested.
- Enclosure: a closed wall ring reports enclosed; the same ring with one cell missing reports open; a ring closed by a door reports enclosed whether the door is open or closed; a bed or workbench inside a large open area does not make it enclosed; a rock pocket reports enclosed; an area over the budget reports open.
- The enclosure test does not change `ExploredWorld`, does not expand raw-chunk residency, and does not alter any authoritative state.
- A long-run headless scenario keeps five characters both fed and rested, with no item quantity creation and no duplicate stable IDs.
