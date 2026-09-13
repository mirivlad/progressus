# Articulated settlement visuals — design

Status: **Approved for implementation**

Date: 2026-09-13

## Purpose and scope

Keep the accepted stylized low-poly 3D direction, but replace the current rigid,
visually weak character with a legible articulated character. The first playable
visual slice covers one shared person design with bounded variants, a primitive
tool, and a cart, in idle, walking, working, tool-bearing, and cart-pushing
situations. This is a quality baseline
for later procedural content, not a promise to rebuild every world model in the
same change. Sleep, skills, metallurgy, research, and other Prototype 02 systems
remain separate milestone work after this slice.

The acceptance question is visual as well as technical: at the game's default
camera and at a practical closer zoom, the person must read as a person in a
recognizable pose, the held tool must read as a tool, and the cart must read as
something being pushed rather than a floating inventory marker. Screenshots
and a short live motion capture require owner review; passing Rust tests alone
cannot establish aesthetic quality.

## Chosen approach

Use a small hierarchy of presentation-only Bevy transforms for torso/head,
arms, and legs, with explicit hand and cart attachment points. Improve the
existing source-controlled Rust mesh recipes for each part and the tool/cart.
Shared part meshes and bounded appearance variants are cached; per-character
entities only hold transforms and material/variant choices. The first version
uses transform animation rather than skinned meshes or an external model
pipeline. This keeps the accepted procedural-asset direction and is reversible
if the resulting screenshots do not meet the quality bar.

Rejected for this slice: a better single mesh with whole-body bobbing, because
limbs would remain rigid; and an authored skeleton/GLTF pipeline, because it
changes the asset workflow before a small in-game visual proof exists. Neither
is forbidden as a later reviewed choice.

## Data flow and behavior

- The pure Rust simulation and save format do not gain pose, animation clock,
  joints, sockets, or render entities. Progressus IDs remain the only stable
  identity; Bevy entities are disposable projections.
- The client derives a presentation pose from detached character movement
  traces and job snapshots. Moving takes precedence over idle motion; a
  `Working` Harvest, Craft, or Construct job selects a generic reach/swing
  gesture when the worker is not walking. Other, unknown, or interrupted jobs
  fall back to idle/walk without blocking gameplay. Visual timing may use
  client frame time, but it never advances or changes authoritative work.
- Walking alternates legs and arms and keeps feet close to the ground. Idle
  motion is restrained. Working has a visible reach/swing rhythm without
  pretending that each swing creates an item. The established root-position
  interpolation and facing continue to follow the authoritative trace.
- `InventoryItemSnapshot::location`, not a UI guess, decides whether a
  primitive tool or cart is attached. A tool in the `tool` slot follows the
  hand socket. A cart in that slot follows a drawbar/pushing anchor behind the
  worker; its wheels rotate visually while the bearer moves. Carried goods
  remain visible but may not be duplicated as ground goods. A parked cart is
  a ground object, keeps its contents in simulation, and never follows a
  character until actually equipped.
- Attachment transforms follow the animated character at render time, including
  camera-origin rebasing. Unequip, drop, character removal, chunk eviction, and
  save/load rebuild or remove attachments from current snapshots; no stale
  mesh may survive an ID reused by another loaded world.
- Picking and gameplay commands continue to use the existing authoritative
  character/item IDs and world coordinates. Animation and model geometry do
  not alter reach, collision, pathing, construction occupancy, or item ownership.

## Visual quality and performance contract

The person gets a clearer silhouette and proportions, differentiated face/head,
torso, hands, and feet, coherent clothing/color variants, and clean intersections
at ordinary poses. The tool and cart are redesigned at the same apparent scale
and palette, with correct ground contact and no visible clipping through the
worker in normal idle/walk/push frames. Exact shapes and colors are adjusted
against in-game captures rather than locked by numeric values in this spec.

No character mesh is regenerated every frame. The mesh cache stays bounded by
part kind and authored variant; only transforms change every frame. Distant or
off-screen characters need no added high-cost animation work beyond the visible
scene. Existing viewport eviction and origin-rebase behavior remain intact.

## Verification and delivery

1. Add client tests for pose precedence, attachment selection by physical
   location, shared mesh reuse, removal on unequip/load/eviction, and stable
   root motion through an origin rebase. Keep headless simulation tests Bevy-free.
2. Run focused client checks and tests, then the full prototype gate, including
   the explicitly enabled heavy client tests.
3. Capture the actual native client at default and close zoom for idle, walk,
   work, tool equipped, cart equipped, and cart parked/loaded. Review silhouettes,
   ground contact, clipping, transitions, and frame cost. Iterate on recipes
   and poses before claiming the visual slice complete.
4. Update client/milestone documentation only to the behavior actually proven.
   Commit and push each verified logical stage directly on `main`.

Native graphics or input that cannot run in the current environment is reported
as unverified and left for explicit owner-PC review; unit-test success is not
substituted for visual acceptance.
