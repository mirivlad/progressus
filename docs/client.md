# Native low-poly client

Run `./scripts/run-client.sh` from the repository. On Linux this checks and installs missing ALSA build dependencies using the system package manager (sudo may request your password). Subsequent runs skip installation. With dependencies installed, `cargo run -p progressus-client` also works. Add `--seed 73` or `--diagnostics` to the launcher as needed.

## Controls and workflows

- WASD or middle drag: pan. Wheel: zoom. Q/E: orbit. F: focus selected person.
- Left click in Select mode prioritizes a workstation or person, then an inspectable ground item/resource, then the stockpile beneath it. While a construction tool is active, the construction click wins instead. With a person selected, right-click a visible ground tool or cart to order that person to fetch and equip it; right-click elsewhere gives an exact movement order or leaves the active tool. Escape closes a modal/leaves the tool. P pauses.
- Arrows/Space retain direct move/stop control for Cora. F3 shows residency and authoritative-position diagnostics.
- Orders, Zones and Build open grouped tool palettes. Harvest, stockpile editing, wall and cancellation tools support rectangular drag selection with cell grid feedback. Door and workbench use point placement. A wall, door or workbench may be designated on passable ground occupied only by a source, loose stack, or person; the preparation job clears that occupancy physically before completion.
- Character inspector shows identity, location, satiety, movement, carried goods and current job/state.
- Stockpile inspector and configuration retain item/category filters and priority. Double-click a stockpile to configure it. Zones can be hidden.
- Workstation modal manages finite/infinite production orders, order quantity/cancellation and independent input/output port rotation. Port cells are marked red/yellow in the world.
- Saves has three slots with seed/tick metadata. Loading resets disposable presentation while preserving the pause setting.
- Sound settings control music and effects separately. Working, gathering, construction and physical item transfers generate bounded on-screen sound cues.
- Ground and carried stacks display their quantities. Walls, doors and scaffolds share cardinal connectivity; door axes follow neighboring walls. Rocky terrain is raised presentation geometry, not a new vertical simulation layer.

## Character visual slice

The current client renders people from shared procedural torso, head, arm, and leg meshes. Their idle, walking, and working poses derive only from detached movement and job snapshots; animation is not gameplay state. A physically equipped primitive tool follows the hand, and a physically equipped cart follows the bearer with distance-driven wheel rotation. A parked cart remains a ground model. Scene tests cover attachment, drop, loading another world, and viewport eviction; the simulation retains item ownership and quantity.

Native default- and close-zoom character frames were captured on 2026-09-13. The person remains visually provisional, and native frames with a tool and cart actually equipped, plus owner aesthetic acceptance, are still pending. Automated tests alone do not establish visual quality or native click UX.

The UI supports Russian and English. All authoritative gameplay and save compatibility remain governed by existing simulation/application contracts.

The accepted visual prototype is preserved in Git at `f4093e2`; its separate runtime and review exports are no longer part of the current checkout.
