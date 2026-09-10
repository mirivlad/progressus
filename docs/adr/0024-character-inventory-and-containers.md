# ADR-0024 — Character inventory, equipment slots, and containers

Status: **Proposed**

Date: 2026-09-11

Decision owner: project owner, who asked for a character inventory covering equipment, what a person carries in their hands, and tools such as a cart or bucket that hold more than hands can.

## Context

A character has no inventory. `ItemLocation` has exactly two variants, `Ground` and `Carried`, and `carried_by_character` is an unbounded set that the simulation never reads: `carried_items_by` exists only for tests. `pick_up_item` checks reach and that the stack is on the ground, and nothing else.

**There is therefore no carrying limit at all.** A character may carry any number of stacks of any size. The activity scenario reports at most two only because its jobs never ask for more.

That single fact sets the order of this work. "A cart holds more than hands" has no meaning until hands hold a definite amount, so the cart is not the first step — the limit is.

Two other things push in the same direction. `PrimitiveTool` is crafted from `2 Wood + 1 Stone`, hauled to a stockpile, and then does nothing: the only production chain in the game ends in an object with no purpose. And [`INV-008`](0001-core-invariants.md) already states that advanced capability may require tools, machines and skills — requiring a tool to mine is the first application of an accepted invariant rather than a new idea.

The owner asked to defer weapon and clothing slots, but to design so that adding them later costs almost nothing.

## Decision

The work is one architecture delivered in three stages. `ItemLocation` grows from two variants to four — on the ground, in the hands, equipped in a slot, or inside a container — and everything else follows from that.

### Stage A — mass, and a limit on hands

Every item definition declares a `unit_mass`. A character declares a `carry_capacity` in the same unit. What a character holds in their hands must not exceed it.

Mass rather than a count of stacks: a count would make one berry and a thousand stone equal cargo, which is precisely the abstraction [`INV-002`](0001-core-invariants.md) rejects. The vision's logistics chapter asks for capacity to matter; this is where it starts mattering.

This changes existing behaviour and the change is the point. A haul job that meets a stack heavier than the worker can lift splits it and moves what fits, using the stack splitting that production supply already performs. Item conservation is unaffected: splitting preserves total quantity, and the existing conservation tests must be extended to cover the split-on-pickup path.

### Stage B — equipment slots, and extraction that requires a tool

`ItemLocation::Equipped { character_id, slot }` joins the enum. A character holds at most one item per slot. Slots are a content registry with stable names, exactly like items and structures ([`ADR-0021`](0021-content-registry.md)), and item definitions declare which slot they occupy.

**No system matches on a specific slot.** This is the property that makes later slots cheap and it is a rule, not an accident: simulation code asks whether a character has something equipped that provides a required capability, never whether a particular slot is filled with a particular item.

Capability is the indirection that carries it. A small registry of capability names — `mine`, `chop`, `dig` — sits alongside the others. An item definition lists what it *provides*; a natural resource definition lists what its extraction *requires*. `PrimitiveTool` provides `mine` and `chop`; a copper vein requires `mine`.

That indirection is what keeps a better pick from breaking anything: a future iron pick declares the same capability and every resource that needs mining accepts it, with no edit anywhere else. It is also where a skill requirement will attach when Stage C of the milestone arrives, because a requirement is a list and a skill can satisfy an entry in it.

Only two slots are defined now: `tool` and `carried_container`. They are separate deliberately — picking up a basket should not disarm a miner.

With this, stone becomes minable from rock terrain itself. Surface outcrops remain what they should always have been: the source available to someone who has no tools yet. The tool the game already crafts finally does something.

### Stage C — containers

`ItemLocation::Contained { container_item_id }` joins the enum. An item definition may declare a `capacity`, in the same mass unit, which makes it a container. A character's effective carrying capacity is their hands plus whatever their equipped container holds.

Containment turns item location from a flat set into a tree, so the rules that keep it physical are stated here rather than discovered later:

- **No cycles.** An item may not be inside itself, directly or transitively. Enforced by walking the containment chain before every insertion, which is cheap because of the depth bound.
- **Bounded depth.** Containers nest at most two deep: a bucket may ride in a cart, a cart may not ride in a cart. Provisional, like the discovery radius, and stated as a number rather than left implicit.
- **Contents obey capacity.** The combined mass inside a container may not exceed its declared capacity.
- **Dropping a container drops it loaded.** Contents stay inside and are not spilled onto the ground. Their location remains the container; the container's location becomes the ground.
- **Nothing is destroyed silently.** Container destruction is not in scope, but when it becomes possible the contents must be placed on the ground, never deleted. [`INV-012`](0001-core-invariants.md) applies.
- **Reservation follows access.** A job may reserve an item inside a container only when that container is on the ground or equipped by the worker doing the reserving. Material inside a cart another person is pushing is not available to anyone else.
- **Spatial queries surface the container, not its contents.** Items inside a container standing on the ground are not published as ground items; the client sees the container.

The flat `ItemWorld` map continues to hold every stack whatever its location, so existing conservation scans keep working unchanged. They must additionally learn to count mass held inside containers, or a container would become a place where quantity quietly accumulates unaudited.

### What is deliberately not built

- **Weapon and clothing slots.** Combat is an explicit non-goal of the current milestone, and clothing without temperature or protection is decoration. A slot that no system reads is the placeholder that [`AGENTS.md` §16](../../AGENTS.md) forbids. The design admits them; the registry does not list them yet.
- **Vehicles.** A hand cart is a carried container and nothing more. Anything with its own movement rules, speed, or pathing is transport, which the milestone defers.
- **Tool durability, tool assignment policy, and per-character tool ownership.** A tool is reserved and equipped like any other physical item.
- **Skills.** The requirement list is shaped to accept them; the skill system itself is Stage C of the milestone.

### Adding a slot later

The cost is stated so it can be checked: a new slot is one row in the slot registry, one localized name, and the `equip_slot` value on the item definitions that go there. `ItemLocation`, the equip and unequip commands, the persistence DTO, the reservation rules and every existing system are untouched, because none of them names a slot. If adding a slot ever requires editing a system, this decision has been violated.

## Consequences

- The first production chain in the game acquires a purpose: the tool it makes is what lets a settlement mine.
- Stone stops competing for open ground. It comes from rock terrain, which is not a cell that anything else wanted, and surface outcrops become the pre-tool bootstrap. This is the shape that keeps the map from filling up as resource kinds multiply.
- Carrying becomes a real constraint, and the logistics ladder from the vision has its first two rungs: hands, then a container.
- Haul behaviour changes in Stage A, and long-run scenario output will change with it. That is a genuine gameplay change, not drift, and the milestone's conservation and reservation tests are what must confirm nothing leaks.
- Save format gains two `ItemLocation` variants and character equipment. The project is pre-alpha, so this is written directly with no migration, per [`ADR-0021`](0021-content-registry.md).
- Containment is the largest piece and the one that can quietly break physical accounting. It is last for that reason, and it does not begin until Stage A and Stage B are complete and their tests are green.
