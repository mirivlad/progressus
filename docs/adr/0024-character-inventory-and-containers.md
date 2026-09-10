# ADR-0024 — Character inventory, equipment slots, and containers

Status: **Accepted**

Date: 2026-09-11

Decision owner: project owner, who asked for a character inventory covering equipment, what a person carries in their hands, and tools such as a cart that hold several times more.

## Context

A character has no inventory. `ItemLocation` has exactly two variants, `Ground` and `Carried`, and `carried_by_character` is an unbounded set that the simulation never reads: `carried_items_by` exists only for tests. `pick_up_item` checks reach and that the stack is on the ground, and nothing else.

**There is therefore no carrying limit at all.** A character may carry any number of stacks of any size. The activity scenario reports at most two only because its jobs never ask for more.

That single fact sets the order of this work: "a cart holds more than hands" has no meaning until hands hold a definite amount, so the cart is not the first step.

Two other things push in the same direction. `PrimitiveTool` is crafted from `2 Wood + 1 Stone`, hauled to a stockpile, and then does nothing: the only production chain in the game ends in an object with no purpose. And [`INV-008`](0001-core-invariants.md) already states that advanced capability may require tools, machines and skills — requiring a tool to mine is the first application of an accepted invariant rather than a new idea.

Two measures were considered and rejected before the one below. **Per-material mass** was rejected as more simulation than the game needs: with the production chains the design bible describes it ends in weighing nails and bearings, and every new definition would carry a balanced number forever. **A uniform load, where a full stack of anything weighs the same,** was rejected once the arithmetic was checked against the actual economy: a tree yields four to eight wood and a wall costs two stone, while `MAX_STACK_QUANTITY` is 1024. A limit three orders of magnitude above anything the game produces would never bind, and the code enforcing it would never run.

## Decision

The work is one architecture delivered in three stages. `ItemLocation` grows from two variants to four — on the ground, in the hands, equipped in a slot, or inside a container — and everything else follows.

### Stage A — how much fits, expressed in the item's own units

Every item definition declares a **hand load**: how many of it one pair of hands holds. Wood ten, stone five, iron ore four, uranium one. The number is authored directly in the units of the thing itself, which is what a designer can reason about; "wood weighs 2.5" is not.

A mixed load shares one pair of hands rather than filling independent buckets:

```
sum(quantity of item i / hand load of item i)  ≤  capacity multiplier
```

Bare hands are a multiplier of one. Five wood and two stone is `0.5 + 0.4 = 0.9` and fits; six and three is `1.2` and does not. Independent per-item limits were rejected: with a dozen resource kinds a person would become a walking warehouse and every container after them pointless.

This is the same expressive power as mass, parameterised so the authored number means something on its own. It introduces no new unit of measurement, and `MAX_STACK_QUANTITY` goes back to being what it always was — the ceiling on one stack in storage, unrelated to what a person can lift.

Behaviour changes here, and that is the point. Fifteen wood is two trips; thirty-five wood for a construction site is four. Jobs take what fits and split the remainder, using the stack splitting that production supply already performs. Splitting preserves total quantity, and the conservation tests must be extended to cover the split-on-pickup path.

### Stage B — equipment slots, and extraction that requires a tool

`ItemLocation::Equipped { character_id, slot }` joins the enum. A character holds at most one item per slot. Slots are a content registry with stable names, exactly like items and structures ([`ADR-0021`](0021-content-registry.md)), and item definitions declare which slot they occupy.

**No system matches on a specific slot.** This is the property that makes later slots cheap, and it is a rule rather than an accident: simulation code asks whether a character has something equipped that provides a required capability, never whether a named slot holds a named item.

Capability is the indirection that carries it. A small registry of capability names — `mine`, `chop`, `dig` — sits alongside the others. An item definition lists what it *provides*; a natural resource definition lists what extracting it *requires*. `PrimitiveTool` provides `mine` and `chop`; a copper vein requires `mine`.

That indirection is what stops a better tool from breaking anything: a future iron pick declares the same capability and every resource needing it accepts the pick, with no edit anywhere else. It is also where a skill requirement attaches when the milestone's skill stage arrives, because a requirement is a list and a skill can satisfy an entry in it.

**One slot is defined: `tool`.** A cart occupies it, because a cart is what the character has hold of. The consequence is deliberate and worth stating plainly: **a character cannot mine and push a cart at the same time.** Extraction and haulage become either different people or the same person at different times, which is the first real specialisation pressure in the game.

With this, stone becomes minable from rock terrain itself. Surface outcrops stay what they should always have been — the source available to someone with no tools yet — and the tool the game already crafts finally does something.

### Stage C — containers

`ItemLocation::Contained { container_item_id }` joins the enum. An item definition may declare a **capacity multiplier**, which makes it a container. The multiplier is relative to hands and the same fraction rule applies inside it: a cart at four holds forty wood or twenty stone or four uranium, a large cart at eight holds twice that.

Hands and container are counted separately, not summed. What a character holds directly obeys a multiplier of one; what the cart holds obeys the cart's. A person pushing a cart has their hands on it.

**A cart can be set down loaded.** This is the requirement that makes containment necessary rather than optional: if a container's contents merely raised its bearer's limit, the goods would belong to the person and could never be parked. Contents therefore have their own location — the cart — and the cart has its own, the ground or a slot.

Containment turns item location from a flat set into a tree, so the rules that keep it physical are settled here rather than discovered later:

- **No cycles.** An item may not be inside itself, directly or transitively. Enforced by walking the containment chain before every insertion, which is cheap because of the depth bound.
- **Bounded depth.** Containers nest at most two deep: a bucket may ride in a cart, a cart may not ride in a cart. Provisional, like the discovery radius, and stated as a number rather than left implicit.
- **Contents obey capacity.** The fraction sum inside a container may not exceed its multiplier.
- **Setting a container down does not spill it.** Contents keep the container as their location; only the container's own location changes.
- **Nothing is destroyed silently.** Container destruction is not in scope, but when it arrives the contents must be placed on the ground, never deleted. [`INV-012`](0001-core-invariants.md) applies.
- **Reservation follows access.** An item inside a container may be reserved when that container rests on the ground or is equipped by the worker reserving it. Material inside a cart another person is pushing is not available to anyone else.
- **Spatial queries surface the container, not its contents.** Goods inside a cart standing on the ground are not published as ground items; the client sees the cart.

The flat `ItemWorld` map continues to hold every stack whatever its location, so existing conservation scans keep working. They must additionally count what sits inside containers, or a container becomes a place where quantity accumulates unaudited.

### What is deliberately not built

- **Weapon and clothing slots.** Combat is an explicit non-goal of the current milestone, and clothing without temperature or protection is decoration. A slot no system reads is the placeholder [`AGENTS.md` §16](../../AGENTS.md) forbids. The design admits them; the registry does not list them yet.
- **Goods that hands cannot hold at all.** The shape is a hand load that stays the reference figure plus a flag saying hands are not allowed, because a hand load of zero would multiply to zero and make the goods unmovable by cart as well. No content needs it yet, so the flag waits.
- **A parked cart as an automatic haul destination.** A cart on the ground can be taken from and put into by hand. Whether haul jobs may route goods into one turns carts into mobile stockpiles, which is a logistics feature with its own design, not a consequence of this one.
- **Vehicles.** A cart is a container its bearer moves. Anything with its own speed, pathing or movement rules is transport, which the milestone defers.
- **Per-material bulk beyond the hand load, tool durability, tool ownership policy, and skills.** The requirement list is shaped to accept skills; the skill system is its own milestone stage.

### Adding a slot later

The cost is stated so it can be checked: a new slot is one row in the slot registry, one localized name, and the `equip_slot` value on the definitions that go there. `ItemLocation`, the equip and unequip commands, the persistence DTO, the reservation rules and every existing system stay untouched, because none of them names a slot. If adding a slot ever requires editing a system, this decision has been violated.

## Consequences

- The first production chain in the game acquires a purpose: the tool it makes is what lets a settlement mine.
- Stone stops competing for open ground. It comes from rock terrain, which is not a cell anything else wanted, and surface outcrops become the pre-tool bootstrap. This is the shape that keeps the map from filling as resource kinds multiply.
- Carrying becomes a real constraint and the logistics ladder from the vision gets its first rungs: hands, cart, large cart. Each rung is one number on one definition.
- A character cannot mine and haul in the same trip. Small settlements will feel this as walking; that is the pressure that makes a cart worth building.
- Hauling numbers change, and long-run scenario output changes with them. That is a gameplay change rather than drift, and the conservation and reservation tests are what must confirm nothing leaks.
- Save format gains two `ItemLocation` variants and character equipment. The project is pre-alpha, so this is written directly with no migration, per [`ADR-0021`](0021-content-registry.md).
- Containment is the largest piece and the one that can quietly break physical accounting. It is last for that reason, and it does not begin until Stage A and Stage B are complete with their tests green.
