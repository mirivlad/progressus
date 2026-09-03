# ADR-0010 — Production orders and localized modal UI

Status: **Accepted**

Amended by [ADR-0012](0012-production-input-output-logistics-zones.md): dedicated production Input/Output zones replace adjacent ordinary-stockpile staging.

## Context

The first crafting bootstrap used a map `Craft tool` that toggled one job on a workbench. That is too narrow once a workstation can expose several recipes, quantities, repeat policies, priorities, or later stock-target orders.

Production intent is gameplay state, not presentation state. The Bevy client must not own a hidden counter that decides how many items are produced.

The first real UI also needs localization before English strings spread through rendering and interaction systems. Russian is the project's default UI language; English is the second built-in language.

## Decision

`progressus-sim` owns stable-ID `ProductionOrder` entities. An order belongs to one workstation and one typed recipe and has an explicit target:

- `Finite { remaining_runs }`;
- `Infinite`.
A finite order remains present at zero and can be edited or restarted. An infinite order never decrements and continues to request work whenever physical inputs and a worker are available. `Infinite` is a real enum state, never a sentinel quantity.

Only one Craft job may be active for a workstation at a time. Pending orders are considered in stable creation order. A Craft job reserves concrete physical input stack IDs before work; another job cannot reserve the same stack concurrently.

When two workstations can use the same stockpiled input, the first job that successfully reserves it owns that input until release or consumption. The other job waits. Completed infinite work creates a later job ID while an already-waiting competing job keeps its earlier ID, which prevents the simple shared-input case from starving one workstation indefinitely. This behavior has a regression test.

Crafting does not teleport ingredients or shuttle a whole shared source stack between workstations. Craft consumes physical stacks on designated stockpile ground within Manhattan distance 1 of the workstation. If a pending job lacks a local ingredient, production supply splits only the missing recipe quantity from an available distant stockpiled stack and sends that split batch through an ordinary Haul job to an adjacent staging stockpile cell. Stable job ordering plus concrete item reservations preserve deterministic arbitration between competing finite or infinite orders.

## UI decision

The map `Craft tool` is removed. In Select mode, clicking a workstation opens a generic modal shell keyed by `ModalKind`. Workbench production is the first modal content implementation; future containers, furnaces, research stations, characters, and similar inspectors should reuse the same shell rather than add bespoke overlay mechanisms.
The workstation modal exposes recipe creation, finite quantity adjustment, an explicit `∞` control, order deletion, workstation removal, and close. These actions issue application commands and mutate authoritative orders; the modal never owns production truth.

The client has an explicit `Locale` resource with `ru` and `en`. `ru` is the default. Shared UI strings are selected by typed keys instead of being embedded throughout systems. The toolbar provides a runtime language toggle.

Bevy 0.18's embedded `FiraMono-subset.ttf` contains only basic ASCII, so it cannot render Russian. The bootstrap client resolves a local system font with Cyrillic coverage on Linux, Windows, and macOS and logs a warning before falling back to Bevy's default font. No font binary is added to the repository by this bootstrap decision.

## Consequences

Production orders can be persisted independently from transient Craft jobs, and future order policies such as "make until stock reaches X" can extend the target model without replacing the workstation UI contract.

The current order model does not yet include priorities, pause, drag reordering, stock targets, quality filters, ingredient filters, skill constraints, or bill-specific allowed stockpiles. Those are later policy layers, not reasons to bypass physical reservations now.
