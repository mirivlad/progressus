# ADR-0021 — Content is a registry of definitions, not enums

Status: **Accepted**

Date: 2026-09-10

Decision owner: project owner, who set the framing that Progressus is a finished indie game grown through content updates, not a prototype, and who chose to convert every content kind in one pass.

## Context

Progressus is built to be played and then extended: an update should add items, resources, recipes, structures and technologies without reworking the systems that consume them. The code was not written for that.

Content is currently expressed as closed Rust enums — `ItemKind`, `Terrain`, `NaturalResourceKind`, `StructureKind`, `WorkstationKind`, `RecipeId` — and the systems match on identity rather than on properties. Three consequences follow, all measured against the code as of `33f60f8`:

**Adding one item means editing six files.** The `ItemKind` enum and its category mapping, the save DTO enum and its two conversions, both localized names, the client's kind-to-model mapping, and the model enum with its mesh function. There are 135 references to specific `ItemKind::` variants across the workspace, for four items.

**Behavior is attached to identity instead of properties.** "Is this food" is `item.kind() == ItemKind::Berries` in three places in the production code, so a second food would need all three found by hand — and because these are equality tests rather than exhaustive matches, the compiler reports nothing. "What does harvesting this yield" is a three-arm match duplicated in three places. Nutrition, regrowth and yields are global constants belonging to one specific resource.

[`docs/vision.md`](../vision.md) §7–§8 describe production chains down to pistons and bearings and name a dozen raw resources. Several hundred content entries against six files each is not a viable shape.

## Decision

### A dedicated content crate holds definitions

`progressus-content` is a new crate at the bottom of the dependency graph. It owns only definitions: no authoritative state, no coordinates, no Bevy, no simulation logic. `progressus-worldgen`, `progressus-sim` and `progressus-app` depend on it.

Six registries move there: items, terrain, natural resources, structures, workstations and recipes.

### A stable name is the identity; the runtime handle is an index

Each definition carries a stable lowercase name — `"wood"`, `"stone_wall"`, `"berry_bush"`. That name is what saves store and what localization keys off. It is a permanent commitment: a name may be added, but renaming or removing one is a content-breaking change.

At runtime a definition is addressed by a small copyable handle that indexes its registry, so comparisons and map keys stay cheap. Handles are never persisted.

Named constants are derived from the registry by a `const fn` name lookup, so `items::WOOD` is resolved at compile time and a misspelled name fails the build rather than panicking at runtime.

Definitions are **appended, never reordered**. Registry order determines handle order, which determines `BTreeMap` iteration order, which is part of deterministic simulation outcomes. Reordering a registry is a behavior change even when no definition's content changed.

### Systems read properties, not identity

The generalization is the point of the change, not a side effect:

| Was | Becomes |
| --- | --- |
| `kind == ItemKind::Berries` | `definition.nutrition > 0` |
| `BERRIES_MEAL_SATIETY` constant | `nutrition` on the item definition |
| three-arm match for harvest output | `yields` on the resource definition |
| `kind == NaturalResourceKind::BerryBush` | `regrow_ticks` present on the definition |
| `BERRY_BUSH_REGROW_TICKS` constant | `regrow_ticks` on the definition |
| `terrain != Terrain::Grass` means blocked | `walkable` on the terrain definition |
| per-structure match arms for cost and work | fields on the structure definition |

After this, a new food, a new tree that drops a different log, or a new passable structure is one definition plus its presentation, and no system needs to learn about it.

### Unknown content fails loudly

Loading a save that names content this build does not define is an error, never a silently dropped entity or a substituted default. This is INV-012 applied to content.

Presentation is the one exception: the client renders an unknown or not-yet-authored definition with a neutral placeholder mesh rather than refusing to draw the world. A missing model is a visual defect; a missing definition is a corrupt world.

### The client may depend on the content crate

[`scripts/verify-core-dependency-boundary.sh`](../../scripts/verify-core-dependency-boundary.sh) is extended to permit `progressus-content` as a direct `progressus-client` dependency, alongside Bevy and `progressus-app`. The ban on `progressus-sim` and `progressus-worldgen` is unchanged.

The boundary exists so the client cannot read or mutate authoritative state, and so the headless chain stays Bevy-free. Content definitions are neither state nor authority; they are the shared vocabulary both sides already speak. Routing them through a re-export layer in `progressus-app` would add surface without protecting anything.

### Save compatibility begins at beta

The owner set the release policy that makes this change affordable:

- **Alpha versions may break save compatibility.** An update is allowed to invalidate existing saves.
- **From beta onward, saves must survive updates.** Breaking a save format after that requires a migration, not a version bump.

The project is pre-alpha, so this refactor changes the save format directly and writes no migrations. The full version policy belongs in the roadmap document; it is recorded here because it is what licenses the format change.

## Consequences

- One definition in one place replaces six coordinated edits, and the compiler resolves named constants, so a typo cannot reach a running build.
- Exhaustive `match` over content is given up. A new item can no longer make the compiler point at every place that must handle it. In exchange, most of those places stop existing, because behavior reads a field. Where a genuine per-kind branch remains it is guarded by a registry test rather than by the type system.
- Content definitions stay Rust source compiled into the binary. There are no runtime data files and no modding, consistent with [`ADR-0005`](0005-procedural-visual-assets-as-code.md) treating authored content as source. Moving definitions to loadable files later does not change any consumer, because consumers already read a registry.
- Per-item stack limits, item durability, tool requirements and technology gating are not introduced here. They are fields to add to definitions when the systems that use them exist, not placeholders to add now.
- Worldgen keeps its own versioning and its existing placement logic. Which resources exist becomes registry data; where they are placed does not change, and no generated world shifts as a result of this change.
- Content additions still require a rebuild and a release. This targets the maintainer's cost per content item, not player-side data loading.
