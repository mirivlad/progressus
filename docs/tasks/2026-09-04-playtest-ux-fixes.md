# Task: playtest UX fixes after terrain/door pass

Status: **Implemented; owner-PC client/visual validation pending**
Date: 2026-09-04

## Scope

Fix issues found in the first owner-PC playtest of terrain transitions, doors, stockpiles, camera input, object selection and HUD tooltips.

## Input and selection

- Invert middle-mouse camera drag relative to the current implementation.
- Active tools and selected objects remain independent state.
- Plain left-click on an existing workstation or character always selects/opens that object first, regardless of the active tool; a stockpile click likewise selects it, while a real Stockpile Add/Remove drag still edits the zone.
- Selecting an object must not cancel the active tool.
- If the click hits no selectable object, the active point/area tool handles it normally.
- Placing a new workstation does not auto-open it; clicking an existing workstation while Workbench mode is active opens it instead of trying to place another one there.

## Stockpiles

- Separate painted stockpile regions must receive separate stable IDs and independent item policies.
- Stockpile Add extends an existing stockpile only when the painted rectangle actually overlaps that stockpile.
- A fresh non-overlapping painted region creates a new stockpile rather than extending the first global stockpile.
- Never merge distinct stockpiles implicitly because their item policies may differ.
- The selected inspector and configuration modal must always use the same selected stockpile ID.
- Adjacent overlay cells meet without presentation gaps; only the external selected-zone outline remains.

## Doors

- Door placement may replace a completed StoneWall cell.
- Door placement may replace a planned StoneWall construction site.
- Replacing an unfinished wall must clean its jobs/material reservation through normal cancellation semantics.
- Replacing a completed wall creates the Door construction designation on that cell; no salvage policy is introduced in this pass.
- Door artwork uses one canonical vertical-door presentation regardless of whether neighbouring walls run horizontally or vertically.
- Wall/door cardinal connectivity remains intact around the canonical door drawing.

## Terrain presentation

- Keep authoritative terrain unchanged (`Grass / Water / Rock`).
- Render Grass as the base layer under every known terrain cell.
- Water/Rock become overlays whose convex/diagonal corner pixels can be transparent.
- Rounded transparent corners expose the Grass base instead of the world clear colour.
- Preserve existing shore/foothill bands and hidden-terrain non-disclosure.

## Tooltips

- Remove the fixed tooltip location that intersects open palette rows.
- Position the tooltip near the hovered control, normally above the pointer for bottom HUD controls.
- Clamp tooltip position to the window so it remains readable without covering the hovered palette row.

## Validation

- Add/extend sim/app regressions for wall→door replacement and independent stockpile policies where applicable.
- Static-review client click priority, stockpile creation, tooltip positioning and terrain/door raster code.
- Run fmt/diff checks and sim/app/headless tests only on `tomas`; do not build or launch `progressus-client` there.
