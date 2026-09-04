# Task: UI/UX pass, exploration orders, stockpile policies

Status: **Implemented; owner-PC visual validation pending**
Date: 2026-09-04

## Goal

Remove the current prototype-toolbar feel without changing the authoritative/client boundary, make common mouse interactions natural, make stockpile zones legible and configurable, and let the player send characters toward unexplored territory without exposing hidden terrain through the UI.

## Scope

### 1. Bottom HUD hierarchy

Replace the current flat row of text buttons with compact icon-first controls.

Requirements:

- primary controls use icons instead of permanent long text labels;
- every icon has a hover tooltip containing:
  - localized action name;
  - one short localized sentence explaining the action;
  - hotkey when one exists;
- actions are grouped by purpose instead of sharing one flat row;
- gameplay tools and system controls are visually separated;
- current mode remains visible as status/context, not as another action button.

Initial hierarchy:

- **Selection**: Select;
- **Orders**: Harvest, Cancel jobs;
- **Zones**: Stockpile add, Stockpile remove;
- **Build**: Stone wall, Workbench;
- **System**: Pause/resume, Saves, Language.

`Zones` and `Build` must open compact secondary palettes/menus. The hierarchy must be data-driven enough that adding later zones/buildings does not require extending one ever-growing horizontal row.

### 2. Camera mouse pan

Keep WASD and wheel zoom. Add middle-mouse drag pan:

- hold middle mouse button and move the pointer to pan the camera;
- dragging the pointer down moves the map up on screen, and equivalently for the other axes;
- pan distance respects orthographic zoom;
- beginning a middle-button drag must not select, designate, or issue a move order;
- dragging remains presentation-only and must not discover terrain.

### 3. Move orders into unexplored territory

Right-click movement must accept an intended destination in unexplored space.

Required semantics:

- the intended destination is authoritative player intent;
- the client must not reveal hidden terrain or a hidden full route merely because the order was issued;
- navigation advances through currently reachable explored terrain toward the target frontier;
- as the character reveals more terrain, navigation may deterministically re-plan toward the same intended destination;
- if the exact requested point becomes reachable, the character reaches it;
- if progress becomes impossible, the character stops at the reachable point that got it closest to the requested target under the deterministic navigation policy;
- manual interruption/cancellation still clears the navigation order normally;
- no finite global map assumption may be introduced.

Add headless regression coverage for issuing a destination beyond the initial explored region and for stopping at the closest reachable frontier when an obstacle prevents further progress.

### 4. Route endpoint cleanup

Remove the visible hook/loop/backtrack near exact click destinations.

Requirements:

- cardinal authoritative movement is preserved;
- waypoint construction must not visit a target-cell center and then visibly backtrack merely to reach the exact sub-cell destination;
- route display must not append an extra segment when the last remaining waypoint already equals the displayed destination;
- add regression tests for final waypoint sequences, including destinations near each side/corner of the target cell.

### 5. Stockpile zone rendering

Replace the per-cell outline grid with a zone overlay.

Requirements:

- stockpile cells are rendered as a translucent fill, approximately 30% opacity;
- render layer is above ground/terrain and below physical items, characters, buildings, jobs and route overlays;
- do not draw four permanent borders around every stockpile cell;
- optionally draw only the external boundary of a stockpile, especially when selected;
- provide a HUD toggle for zone visibility;
- selected/edited stockpile may be emphasized without returning to the permanent cell-grid clutter.

### 6. Stockpile configuration

A stockpile remains a physical ground zone as required by ADR-0006. It gains policy metadata; it does not become an abstract inventory.

Interaction:

- single click in Select mode selects the stockpile zone;
- double-click a stockpile cell opens stockpile settings;
- selection UI also exposes an explicit Configure action so double-click is a shortcut, not a hidden-only feature.

Settings contain a RimWorld-like hierarchical allow list:

- groups/categories have tri-state checkboxes: all / none / partial;
- leaf rows are concrete item kinds;
- toggling a group applies to all children;
- toggling a child recomputes the parent state;
- new stockpiles allow all currently known item kinds by default;
- hauling may deliver an item only to a stockpile whose policy accepts that kind;
- changing policy never deletes or teleports items already on the ground;
- disallowed existing contents may later be hauled to another compatible stockpile through ordinary physical hauling.

The item category mapping belongs to simulation/application data, not hard-coded presentation-only button logic. Save/load must preserve stockpile policy.

A future priority field is anticipated, but priority UI/behavior is not required by this task unless needed to resolve an implementation ambiguity.

## Architecture constraints

- `progressus-sim` remains pure Rust and authoritative.
- Bevy UI/rendering state remains disposable presentation state.
- stockpile contents remain `ItemLocation::Ground`; filters only constrain hauling destinations.
- unexplored navigation must not turn camera visibility into simulation truth.
- persistence changes must be explicit and version-safe; do not silently reinterpret incompatible save data.

## Delivery plan

1. Fix route endpoint waypoint construction and add simulation regression coverage.
2. Implement unexplored-destination navigation intent/replanning and headless tests.
3. Add stockpile policy model, hauling eligibility, persistence and application read/command boundary.
4. Replace stockpile grid rendering with translucent zone overlay and visibility toggle.
5. Add stockpile selection/double-click settings UI.
6. Refactor bottom HUD into icon-first grouped palettes with localized tooltips.
7. Add middle-mouse drag camera pan.
8. Update milestone/architecture documentation to match final behavior.

## Acceptance

The work is accepted when all requested interactions exist, authoritative behavior has headless regression coverage where applicable, save/load preserves stockpile policies, and no client launch is required for automated verification.

## Implementation/verification note

Implemented on `codex/ui-ux-navigation-stockpiles`: grouped icon HUD with localized hover help, Orders/Zones/Build palettes, zone visibility control, middle-mouse camera drag, exploration-intent navigation/replanning, exact-route endpoint cleanup, translucent stockpile overlays and selected external boundary, stockpile selection/double-click/Configure flow, hierarchical item-category filters, authoritative hauling enforcement, and persistence.

Server-safe verification on `tomas` passes `cargo test -p progressus-sim -p progressus-app -j1` and formatting/diff checks. The graphical client is intentionally not compiled or launched there, so final Bevy type/link/runtime and visual-layout validation is performed by the owner after pulling on the normal development PC.

## Tomas machine safety rule

On host `tomas`, **do not launch `progressus-client` and do not run commands that compile/link the graphical client**. That workload can exhaust CPU/RAM and hang the machine. Limit verification there to source inspection, formatting, lightweight non-client checks known not to build the Bevy client, and repository operations.
