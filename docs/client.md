# Native low-poly client

Run `./scripts/run-client.sh` from the repository. On Linux this checks and installs missing ALSA build dependencies using the system package manager (sudo may request your password). Subsequent runs skip installation. With dependencies installed, `cargo run -p progressus-client` also works. Add `--seed 73` or `--diagnostics` to the launcher as needed.

## Controls and workflows

- WASD or middle drag: pan. Wheel: zoom. Q/E: orbit. F: focus selected person.
- Left click: select a person, inspect a stockpile, or open a workstation. Right click: exact movement order, or leave the active tool. Escape: close modal/leave tool. P: pause.
- Arrows/Space retain direct move/stop control for Cora. F3 shows residency and authoritative-position diagnostics.
- Orders, Zones and Build open grouped tool palettes. Harvest, stockpile editing, wall and cancellation tools support rectangular drag selection with cell grid feedback. Door and workbench use point placement.
- Character inspector shows identity, location, satiety, movement, carried goods and current job/state.
- Stockpile inspector and configuration retain item/category filters and priority. Double-click a stockpile to configure it. Zones can be hidden.
- Workstation modal manages finite/infinite production orders, order quantity/cancellation and independent input/output port rotation. Port cells are marked red/yellow in the world.
- Saves has three slots with seed/tick metadata. Loading resets disposable presentation while preserving the pause setting.
- Sound settings control music and effects separately. Working, gathering, construction and physical item transfers generate bounded on-screen sound cues.
- Ground and carried stacks display their quantities. Walls, doors and scaffolds share cardinal connectivity; door axes follow neighboring walls. Rocky terrain is raised presentation geometry, not a new vertical simulation layer.

The UI supports Russian and English. All authoritative gameplay and save compatibility remain governed by existing simulation/application contracts.

The accepted visual prototype is preserved in Git at `f4093e2`; its separate runtime and review exports are no longer part of the current checkout.
