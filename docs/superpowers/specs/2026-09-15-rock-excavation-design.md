# Prototype 02 Stage D1 — Physical Rock Excavation

Status: **Implemented with automated headless/application/client evidence on 2026-09-17; native visual acceptance pending.**

## Purpose and boundary

The player can designate an explored `Rock` terrain cell for physical excavation on the existing flat map. A character approaches from traversable ground, works with mining-capable equipment, and removes that whole rock cell. Completion changes its authoritative effective terrain to `Grass` and places exactly `Stone x1` on the newly traversable cell. Ordinary Haul may then move the stone; there is no teleporting output. This is the first part of Stage D. Excavating ordinary ground, pits/height levels, smelting and research remain separate later work.

## Approaches considered

- **Whole-cell removal — selected.** `Rock -> Grass` plus one physical Stone stack uses the existing sparse terrain-override and item contracts. It is visible, passable and reversible at this milestone scale.
- **Pit or stepped relief after removal.** Would require a new authoritative terrain/height and navigation contract before this first rock job, and the owner chose the simpler passable result.
- **Harvest-like stone source without terrain change.** Does not meet the agreed requirement that excavation change the cell, so it is not used.

The `Stone x1` quantity is a deliberately small first content value, not a geological scale or a permanent rule for all rock. The work phase starts at four ticks, matching current Harvest; Mining mastery at five practice shortens it to three. Neither proficiency nor presentation changes the output quantity or tool requirement.

## Authoritative flow

`progressus-sim` owns a persisted `ExcavateRock { cell }` job and an exclusive cell-to-job index. The application publishes a `DesignateRockExcavation { cell }` command and detached job snapshot; Bevy does not own the job, terrain or item. Designation accepts an explored effective `Rock` cell not already held by another excavation or a structure/site/zone. It does not require an available worker or immediately reachable neighboring cell, so a temporarily inaccessible designation remains `Available` and can be cancelled. A changed/non-Rock target is cancelled without output or skill credit.

Assignment considers available workers deterministically. `capability::MINE` must come from equipped physical equipment; the existing automatic tool-fetch policy also sees available rock-excavation work. For each qualified worker, the simulation chooses a reachable explored, walkable cardinal neighbor in a fixed order, reserves the worker, and navigates to that neighbor. A quarter-cell target interaction radius lets an adjacent cell-center worker reach the rock face while keeping the rock cell itself impassable. Path failure leaves the job available, with no orphan worker reservation.

On arrival the simulation fixes `Working.remaining_ticks` from that actual worker's Mining practice. Work is authoritative and independent of animation/frame time. A target no longer effective `Rock` cancels the job; worker removal or loss of the equipped `mine` capability releases the worker and returns the unchanged target job to `Available`. None of those paths completes work. A successful completion preflights the stable item ID and terrain revision, applies the sparse `Grass` override, creates one ground `Stone x1` stack at the cell center, removes the job/index, and awards exactly one Mining practice point to the completing worker (capped at five). No award occurs at designation, reservation, start, interruption, cancellation or failed output.

Rock excavation does not erase a natural resource or another physical item. Generated resource layers currently place resources only on traversable ground; designation still checks that the chosen cell has no preserved source or conflicting claim. Existing loose goods elsewhere are untouched. The newly created Stone has one ordinary ground ownership state and is eligible for ordinary Haul/storage policy.

## Terrain refresh and persistence

The existing low-poly terrain cache refreshes on exploration revision, not on an override. A separate authoritative terrain revision is therefore advanced only when an effective override changes. It is published in `ClientSnapshot` and used by the Bevy spatial cache to request/remesh visible terrain; item revision continues to refresh the new Stone. This revision is an invalidation counter, not geology or gameplay truth. Save v1 gains an additive defaulted terrain-revision field so save/load of an active game keeps detached snapshot behavior deterministic; old saves remain readable. Existing sparse terrain overrides and exact `Working.remaining_ticks` remain the authoritative persisted state. The `ExcavateRock` save kind uses its stable world cell and Progressus job/worker IDs, never Bevy entities.

## Client workflow

The Orders palette adds a localized rock-excavation designation tool. Area selection targets known effective `Rock` cells; it never probes undiscovered terrain. Its drag preview and pending/working designation marker sit above the procedural rock mesh rather than at the hidden ground plane; the existing Cancel Jobs gesture removes the job. Help text states that a mining-capable tool is required and that completion clears the cell and produces physical Stone. A live native check must confirm the click, marker, worker approach, cell remesh and subsequent haul. The client cannot grant a skill point or change terrain on its own.

## Verification

- Headless designation checks reject hidden, non-Rock, duplicate and claimed cells without allocating a second job; an unreachable Rock remains available and cancellable.
- A worker without `mine` cannot start; a reachable physical tool triggers the existing exclusive fetch path. A master miner with no tool remains ineligible.
- A real worker approaches from adjacent traversable ground, works four or three ticks as appropriate, and completion yields exactly one `Stone x1`, one Mining practice point and an effective/passable `Grass` cell. Item and job indexes remain consistent. Repeated advancement never duplicates output or practice.
- Cancellation, manual interruption, target change, tool loss, path failure and worker removal leave no output/credit/orphan reservation. Save/load during `Available`, `Reserved` and skilled `Working` resumes deterministically; the terrain override and item round-trip after completion. Pre-scope saves load with terrain revision zero.
- An application boundary test proves the command and detached terrain/job/item snapshots. Client tests prove localization, known-cell designation filtering and revision-triggered visible terrain refresh. The full repository gate runs with client tests enabled. Native visual operation is reported separately, never inferred from unit tests.

ADR-0001/0002/0003/0004/0022 remain binding: the worldgen base and layers are not rewritten, physical ownership and stable IDs remain authoritative, continuous movement stays in pure Rust, and Bevy is presentation only.
