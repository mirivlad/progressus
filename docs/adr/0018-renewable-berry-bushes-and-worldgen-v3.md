# ADR-0018: Renewable berry bushes and worldgen v3

- Status: Accepted
- Date: 2026-09-05

## Context

Prototype 02 P02-N03 requires a deterministic renewable food source that still obeys Progressus' physical-world rules. Food must be gathered into physical items, moved by ordinary logistics, and consumed by the existing Eat jobs; hunger must not draw from an abstract colony inventory.

Adding a new generated resource also changes the result of `(seed, worldgen version, absolute cell)`. ADR-0007 makes that identity stable, so the existing worldgen v2 cannot be reinterpreted in place even though compatibility between arbitrary pre-release save builds is not required.

## Decision

`CURRENT_WORLDGEN_VERSION` becomes version 3. Worldgen v3 keeps v2 terrain generation unchanged and layers `BerryBush` natural resources onto grass. Worldgen v1 and v2 remain unchanged generators.

Every v3 new game has four guaranteed berry bushes near the starting clearing at the diagonal cells `(±3, ±3)`. Additional wild bushes are sparse and deterministic. A bush yields 3–5 physical `Berries` according to seed/cell identity.

Harvesting a BerryBush uses the ordinary `Harvest` job and creates an ordinary physical Berries ground stack. The bush then becomes unavailable for 512 authoritative simulation ticks instead of entering permanent depletion. Its regrowth deadline is authoritative sparse state keyed by world cell and participates in save/load and resource revision tracking.

When a hungry character has no unreserved physical Berries available, nutrition maintenance may designate a reachable explored BerryBush for harvest. This does not create a special food-production shortcut: the designation enters the same Harvest scheduler, reserves an ordinary worker, requires travel/work, and only completion creates food. Existing player Harvest designations remain valid and use the same path.

Ordinary Haul and stockpile acceptance handle harvested Berries without a food-specific storage mechanism. The small new-game Berries stack is reduced to 10 units and is only a startup buffer; long-run sustainability must come from renewable sources.

## Consequences

The five-character settlement can sustain itself indefinitely when enough bushes are reachable, while all food remains spatial and physical. Bushes can be exhausted temporarily, logistics can become a bottleneck, and a character may still starve if sufficient food capacity is unavailable.

Worldgen v3 exists to preserve deterministic generator identity, not to promise migration of arbitrary development saves. Pre-release save compatibility is not a milestone requirement; unsupported formats must still fail explicitly rather than being silently reinterpreted.

P02-N03 tests cover physical harvest output, deterministic regrowth, save/load during regrowth, ordinary stockpile hauling, and a 10,000-tick five-character run with the bootstrap Berries removed.