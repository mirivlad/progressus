# Settlement interaction completion

Status: Accepted by the project owner in the 2026-09-11 conversation.

## Scope and sequence

Finish playable inventory and carts, prove the physical tool/mining/transport loop,
synchronize documentation, add construction preparation (owner's item 3.5), and
identify ground items and natural sources (item 3.6). Sleep/shelter follows this
interaction pass under its own ADR-0020; no metallurgy, skills or future transport
systems are introduced by this pass.

## Inventory and carts

Keep ADR-0024's physical locations, stable IDs, shared hand capacity and independent
container capacity. Expose owned snapshots and explicit application commands for
pickup, equip, unequip, drop, loading and unloading. Commands operate only within
physical reach, reject reservations they do not own, and do not teleport goods.
The player can select a character, inspect hands/equipment and operate reachable
items and containers. A loaded cart may be equipped, used by ordinary logistics,
parked and inspected. Feedback explains rejected actions. Do not add automatic
mobile stockpiles or automatic equipment-switching policy.

Validate saves against the same location, slot and capacity rules as runtime.
Test hauling with a cart through completion and interruption, plus manufacture,
tool acquisition, actual copper extraction and physical delivery.

## Construction preparation

Designation expresses intent and succeeds on explored, suitable terrain occupied
by removable natural sources, ground stacks or characters. Existing permanent
structures, stockpiles and production occupancy retain their restrictions and
the accepted wall-to-door exception remains. Workbenches must also respect this
preparation requirement when removable occupants prevent their placement.

An explicit preparation lifecycle owned by the site harvests the existing source,
physically relocates initial and harvested stacks outside planned footprints,
and lets characters physically vacate before completion. Materials and workers
never teleport. Construction waits when the tool, worker, path or drop destination
is missing and publishes an understandable reason. Preparation must not suppress
or erase the source merely because a site now claims its cell. Newly introduced
worldgen layers must still not appear underneath old claimed sites.

Cancellation releases only preparation work owned by this project, preserves
pre-existing independent work, drops transported goods safely and conserves all
quantities. Save/load mid-preparation continues deterministically. Final completion
rechecks occupancy, so someone or something entering the cell cannot be buried.

This explicitly amends ADR-0009's designation restriction and the treatment of
pre-existing sources at construction sites in ADR-0022.

## Resource information

Provide a localized hover tooltip and pinned click inspector for explored ground
stacks and natural sources. Show names and quantities; for sources, show output,
tool requirements and assigned work; for stacks show reservations and container
contents where applicable. Remain usable while a designation tool is active:
hover inspection never steals a build/harvest action. Preserve character and
workstation selection and UI input capture. Unknown objects remain hidden.

## Verification

Use focused failing regressions before changes, headless conservation/reservation
and persistence tests, application-boundary tests, client interaction tests, strict
format/Clippy/dependency gates and existing long-run smokes. Validate actual native
UI if the environment permits it; report unavailable visual validation honestly.
Publish focused commits after verified stages. No milestone completion is inferred
from a type, command or isolated fixture alone.
