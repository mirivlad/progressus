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

Placement already refuses to build on a cell holding a resource, so today a resource and a structure cannot coexist. A layer added later could break that from the other side, by placing a resource under a wall, a stockpile, a workbench or a production port that has stood there for hours.

The simulation therefore reports no natural resource for a cell claimed by a structure, a construction site, a stockpile, a workstation or a production zone, through both the point query and the chunk query. For every world that exists today this changes nothing, because such a cell can never hold a resource; it is the guard that keeps the invariant true once layers carry content.

Cells the player merely walked over, explored, or dropped items on are not claims. A new deposit appearing in explored ground is intended: the fiction is that prospecting finds what was always there.

## Consequences

- An update that adds a resource reaches every existing world, including settled ones, without touching terrain, buildings, or anything already generated.
- The shipped layer registry is empty. This change delivers the mechanism, not content; the first real layer arrives with the first resource that the base generations did not have. Additivity, ordering, and the mask are proven by layers defined under `cfg(test)`, so the mechanism under test is the mechanism that ships.
- New terrain kinds still require a new world. So do changes to how existing resources are placed.
- A resource may appear in ground the player has explored but not built on. Players who cleared an area and left it empty may find something new in it.
- Worlds diverge by build: two players with the same seed but different game versions have different resources. The seed alone no longer identifies a world; the seed, base version, and layer set do, and all three are in the save.
- Nothing here changes existing generation. The worldgen golden fixtures pass unmodified, and the deterministic headless smokes produce byte-identical output.
