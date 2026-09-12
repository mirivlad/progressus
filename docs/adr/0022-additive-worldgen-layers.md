# ADR-0022 — Additive worldgen layers

Status: **Accepted**

Date: 2026-09-10

Decision owner: project owner, who set the framing that Progressus grows through content updates rather than through new worlds.

## Context

World generation was versioned as a whole. `WorldGenerator` held one `WorldgenVersion`, every save recorded it, and every version stayed supported forever, so a world created at version 3 kept generating exactly what version 3 generates. That is the right instinct — it means an update can never turn a player's forest into a lake — but it has a consequence nobody chose:

**a world created today can never receive a resource added tomorrow.**

Adding copper ore would require a version 4. Existing saves stay pinned to 3 and never see copper anywhere, however far their people explore. The player's only way to reach new content is to abandon the settlement and start over. For a game meant to grow through updates, that is the update model failing at its first real test.

The naive fix — always generate with the newest version — is worse. It rewrites terrain under a standing settlement and breaks [`INV-003`](0001-core-invariants.md) and [`INV-011`](0001-core-invariants.md) together.

## Decision

### A world has one pinned base and a growing set of layers

`WorldGenerator` now carries two things:

- a **base version**, which is the world's terrain plus the resources that existed when it was created. It is chosen once, recorded in the save, and never changes for that world.
- a set of **resource layers**, applied over that base.

The base keeps the existing v1/v2/v3 functions untouched. Those are not layers and never become layers: each is one world's whole original generation. Restructuring them into layers would change what existing worlds generate, which is the thing this decision exists to prevent.

### A layer may only fill what nothing else claimed

`natural_resource_at` asks the base first. If the base placed something, that is the answer. Only when the base left the cell empty are layers consulted, in registry order, and the first layer that claims the cell wins.

This ordering is the whole mechanism. It makes adding a layer **monotone**: a cell that had a resource keeps exactly that resource, and a cell that had none may gain one. No update can move, remove, or substitute anything a world already contains.

Layers never touch terrain. Terrain is one value per cell, so any new terrain type would have to replace an existing one, and no ordering rule can make that additive. A new terrain kind therefore reaches only worlds created after it. This is a real limitation and it is accepted: terrain is the world's shape, and reshaping a settled world is not an update, it is a different world.

`RESOURCE_LAYERS` is append-only and never reordered, for the same reason the content registries are: a layer's position decides which of two layers claims a contested cell.

### Layers are recorded, but loading applies every layer the build knows

A save records the layer names active when it was written. Loading does **not** restrict itself to that list — it applies every layer this build defines, which is precisely how an update reaches an existing world.

The recorded list is the guard in the other direction. A save naming a layer this build does not define came from a newer build, and loading it would silently omit resources that world already has. That is refused with the same error unknown content gets, per [`INV-012`](0001-core-invariants.md) and [`ADR-0021`](0021-content-registry.md).

Layers are addressed at runtime by a bitmask over the registry, so a generator stays `Copy`; the mask holds at most 64 layers and a test pins that ceiling against the registry's size.

### A layer may not grow a resource under something already built

Construction designation may now overlap a resource that already exists, but only as a temporary explicit preparation state: the source remains queryable and is physically harvested before completion. A layer added later must not create a different source under a wall, a stockpile, a workbench, a production port, or a construction project that has already claimed the cell.

The simulation therefore reports no newly generated natural resource for a claimed cell through either the point query or the chunk query. A construction or deferred-workbench project records whether its source predated the claim; only that recorded source stays visible until its preparation job harvests it. The flag is persisted, so save/load cannot either erase the source early or mistake a later layer for pre-existing occupancy.

Cells the player merely walked over, explored, or dropped items on are not claims. A new deposit appearing in explored ground is intended: the fiction is that prospecting finds what was always there.

## Consequences

- An update that adds a resource reaches every existing world, including settled ones, without touching terrain, buildings, or anything already generated.
- The first shipped layer is `copper_veins`, which places a non-renewable copper source that no base generation had. It reaches worlds created before it existed, including v1 and v2 worlds, and it is the end-to-end proof that a content update can extend a settled world. Copper favours the edges of rocky ground, stays clear of the starting clearing, and is rare enough that a settlement must go looking: roughly one cell in two hundred across a 240×240 sample, with the nearest vein 15 to 19 cells from spawn on the bootstrap seeds.
- Additivity is enforced for **every** registered layer, on every base version, by a test that compares generation with and without each layer: terrain must be untouched, existing resources must be unchanged, and the layer must actually place something. Ordering and the mask ceiling are proven separately with layers defined under `cfg(test)`.
- Worldgen golden fixtures now pin the **base** generation explicitly, with no layers. That is what they were always for: they guard against accidental drift in v1/v2/v3, and layers are additive by construction and tested separately, so they must not be able to move those values.
- New terrain kinds still require a new world. So do changes to how existing resources are placed.
- A resource may appear in ground the player has explored but not built on. Players who cleared an area and left it empty may find something new in it.
- Worlds diverge by build: two players with the same seed but different game versions have different resources. The seed alone no longer identifies a world; the seed, base version, and layer set do, and all three are in the save.
- Nothing here changes existing generation. The golden fixture values are unmodified, and the deterministic headless smokes produce byte-identical output even with copper added, because the activity scenario designates harvests from the explored area around spawn and copper begins beyond it. That is the intended shape of a content update: an existing settlement carries on untouched, and the new resource is something to go and find.
