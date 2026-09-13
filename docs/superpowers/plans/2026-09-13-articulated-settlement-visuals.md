# Articulated Settlement Visuals Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the rigid character with a better procedural low-poly articulated character, animate idle/walk/work, and show physically equipped tools and carts following the bearer.

**Architecture:** All mesh recipes, hierarchy, pose clocks, and attachments live in `progressus-client`. They consume detached `progressus-app` character, job, and inventory snapshots; `progressus-sim`, saves, stable IDs, and gameplay timing are unchanged. Shared bounded meshes are cached while per-character Bevy entities contain only disposable transforms.

**Tech Stack:** Rust workspace, Bevy 0.17, existing source-controlled procedural meshes, `progressus-app` read model.

## Global Constraints

- Work directly on `main`; commit and push each verified logical task to `origin`.
- Never put Bevy, animation phase, mesh identity, or wall-clock timing in authoritative simulation or saves.
- Keep the existing low-poly client, Progressus ID mapping, camera-origin rebase, viewport eviction, item ownership, and character picking.
- No mesh generation per frame; mesh count is bounded by part kind and variant.
- Real native captures and owner review are required before claiming aesthetic completion; a green test suite alone is insufficient.
- The first slice is the person, primitive tool, and cart; no sleep, skill, metallurgy, research, or wholesale world-art rewrite.

---

## File map

- `assets/procedural/low_poly/models.rs`: character part and tool/cart mesh recipes; no runtime animation state.
- `crates/progressus-client/src/low_poly/character.rs` (new): rig parts, spawn hierarchy, pose selection, and transform animation.
- `crates/progressus-client/src/low_poly/mod.rs`: part mesh cache and handoff from existing root interpolation to the rig animator.
- `crates/progressus-client/src/low_poly/scene.rs`: spawn/despawn rig roots and reconcile equipped visual objects from detached snapshots.
- `crates/progressus-client/src/navigation.rs`: optional client-only animation phase, retained across authoritative tick updates and cleared on load.
- `docs/client.md`, `docs/milestones/prototype-02.md`, and this plan: only verified workflow/status updates.

## Task 1: Shared procedural parts and character hierarchy

**Files:** `assets/procedural/low_poly/models.rs`, `crates/progressus-client/src/low_poly/{mod.rs,character.rs,scene.rs}`.

**Interfaces:** `models::CharacterPart` identifies `Torso`, `Head`, `Arm`, and `Leg`; `models::character_part_mesh(part, variant) -> Mesh` is a pure presentation recipe. `character::spawn(&mut Commands, &mut Palette, &mut Assets<Mesh>, Handle<StandardMaterial>, EntityId, WorldPosition, WorldCell) -> Entity` returns the existing `Pawn`/`MotionTarget` root with a `CharacterRig` of child handles.

- [ ] **Step 1: Red test.** Add a model test that calls `character_part_mesh` for every `CharacterPart` and all four variants, asserts finite nonempty positions/normals, and checks torso/head/limb bounds fit the current one-cell pawn scale. Add a scene test that one visible character has exactly one `Pawn` root and articulated child handles, and that an idle second sync reuses the same mesh asset IDs.
- [ ] **Step 2: Confirm red.** Run `cargo test -p progressus-client character_part -j 1 -- --nocapture` and `cargo test -p progressus-client articulated_character -j 1 -- --nocapture`; expect failure before production changes.
- [ ] **Step 3: Mesh recipes.** Add the enum and dispatch to `models.rs` using the existing `Geometry` primitives. Each part is local to its pivot, not a world-space complete person:

  ```rust
  #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
  pub enum CharacterPart { Torso, Head, Arm, Leg }

  pub fn character_part_mesh(part: CharacterPart, variant: u8) -> Mesh {
      let mut g = Geometry::default();
      let variant = variant % 4;
      match part {
          CharacterPart::Torso => character_torso(&mut g, variant),
          CharacterPart::Head => character_head(&mut g, variant),
          CharacterPart::Arm => character_arm(&mut g, variant),
          CharacterPart::Leg => character_leg(&mut g, variant),
      }
      g.mesh()
  }

  fn character_torso(g: &mut Geometry, variant: u8) {
      let cloth = [[0.16, 0.36, 0.58, 1.], [0.54, 0.25, 0.16, 1.],
                   [0.24, 0.48, 0.28, 1.], [0.47, 0.28, 0.56, 1.]][variant as usize];
      g.prism(Vec3::new(0., 0.35, 0.), 0.34, 0.17, 0.22, 6, cloth);
  }
  fn character_head(g: &mut Geometry, variant: u8) {
      let skin = [0.69 + variant as f32 * 0.045, 0.49 + variant as f32 * 0.035,
                  0.33 + variant as f32 * 0.025, 1.];
      g.gem(Vec3::new(0., 0.12, 0.), Vec3::new(0.14, 0.15, 0.12), 7, skin);
      g.gem(Vec3::new(0., 0.22, -0.01), Vec3::new(0.145, 0.07, 0.13), 7,
            [0.13 + variant as f32 * 0.035, 0.075, 0.035, 1.]);
  }
  fn character_arm(g: &mut Geometry, variant: u8) {
      let skin = [0.69 + variant as f32 * 0.045, 0.49 + variant as f32 * 0.035,
                  0.33 + variant as f32 * 0.025, 1.];
      g.beam(Vec3::ZERO, Vec3::new(0., -0.31, 0.), 0.05, skin);
      g.gem(Vec3::new(0., -0.32, 0.), Vec3::splat(0.055), 6, skin);
  }
  fn character_leg(g: &mut Geometry, _variant: u8) {
      g.beam(Vec3::ZERO, Vec3::new(0., -0.36, 0.), 0.065, [0.19, 0.15, 0.12, 1.]);
      g.cuboid(Vec3::new(0., -0.38, 0.055), Vec3::new(0.14, 0.07, 0.19),
               [0.16, 0.13, 0.11, 1.]);
  }
  ```

  Shape the recipes for a legible clothed silhouette and clean neck/shoulder/hip joints at the existing orthographic scale; retain four deterministic appearance variants keyed by stable character ID. Keep the old `Character` recipe until scene migration passes.
- [ ] **Step 4: Cache and rig.** Add `Palette.character_parts: BTreeMap<(CharacterPart, u8), Handle<Mesh>>` and `Palette::character_part` with the same `entry(...).or_insert_with(|| meshes.add(...))` pattern as `Palette::get`. In `character.rs`, spawn one root with `Pawn(id)` and `MotionTarget(id, Vec3::ZERO)`, then torso/head/two arms/two legs as children, storing child `Entity` handles on `CharacterRig`. Replace only the character root spawn in `scene::sync`; leave terrain, items, workstation, and resource reconciliation untouched. Update the scene test's `Palette` fixture with the new cache field.
- [ ] **Step 5: Green and commit.** Run the two focused tests, `cargo check -p progressus-client --all-targets -j 1`, and `cargo fmt --all -- --check`. Review only these files, commit `client: build articulated procedural character rig`, push `main`, and verify `HEAD == origin/main`.

## Task 2: Client-only idle, walk, and work poses

**Files:** `crates/progressus-client/src/low_poly/character.rs`, `crates/progressus-client/src/low_poly/mod.rs`, `crates/progressus-client/src/navigation.rs` if a phase must persist across tick refreshes.

**Interfaces:** `character::pose_kind(&CharacterSnapshot, &[JobSnapshot], &[WorldPosition]) -> PoseKind` yields `Walk`, `Work`, or `Idle`. `character::pose(PoseKind, phase_seconds: f32) -> CharacterPose` is pure and bounded; `character::animate_rigs(...)` applies its angles to the child transforms after authoritative root interpolation.

- [ ] **Step 1: Red test.** Test `pose_kind`: a nonzero interpolated movement trace wins over a simultaneous `JobState::Working`; an idle character with `Working` Harvest/Craft/Construct chooses Work; other jobs choose Idle. Test `pose`: the two legs swing in opposite directions during Walk, feet/limb angles remain finite and bounded, Work moves arms without changing root position, and Idle does not resemble Walk.
- [ ] **Step 2: Confirm red.** Run `cargo test -p progressus-client pose_kind -j 1 -- --nocapture` and `cargo test -p progressus-client character_pose -j 1 -- --nocapture`; expect the new assertions to fail before the implementation.
- [ ] **Step 3: Pure pose rules.** Define exact state and finite transforms in `character.rs`, with all angles in radians and phase wrapped to a fixed period:

  ```rust
  #[derive(Clone, Copy, Debug, Eq, PartialEq)]
  pub enum PoseKind { Idle, Walk, Work }

  pub struct CharacterPose {
      pub left_leg: f32, pub right_leg: f32,
      pub left_arm: f32, pub right_arm: f32,
      pub torso_bob: f32,
  }

  pub fn pose(kind: PoseKind, phase_seconds: f32) -> CharacterPose {
      let wave = (phase_seconds * std::f32::consts::TAU * 1.6).sin();
      match kind {
          PoseKind::Idle => CharacterPose { left_leg: 0., right_leg: 0., left_arm: 0., right_arm: 0., torso_bob: wave * 0.008 },
          PoseKind::Walk => CharacterPose { left_leg: wave * 0.38, right_leg: -wave * 0.38, left_arm: -wave * 0.25, right_arm: wave * 0.25, torso_bob: wave.abs() * 0.025 },
          PoseKind::Work => CharacterPose { left_leg: 0., right_leg: 0., left_arm: -0.25 + wave * 0.3, right_arm: -0.25 + wave * 0.3, torso_bob: wave * 0.012 },
      }
  }
  ```

  Adjust amplitudes only after in-game review; never use a swing as an authoritative completion callback.
- [ ] **Step 4: Apply pose.** Run root interpolation as today, then rotate the rig's limb child `Transform`s around their pivots. Keep an animation phase per visible character across the four-per-second authoritative snapshot refreshes; advance it using `Time::delta_secs()` only while `TickScheduler` is unpaused, and clear it with the disposable scene on load/eviction. Derive Work only from the detached `jobs` read model. Do not allocate mesh or make a per-frame snapshot copy.
- [ ] **Step 5: Green and commit.** Run focused pose/scene tests, all client tests, `cargo check -p progressus-client --all-targets -j 1`, and format. Check a native walk/work frame if display is available; report if not. Commit `client: animate articulated idle walk and work poses`, push, and verify remote equality.

## Task 3: Physical tool and cart attachments

**Files:** `assets/procedural/low_poly/models.rs`, `crates/progressus-client/src/low_poly/{character.rs,scene.rs,mod.rs}`.

**Interfaces:** `character::equipped_visual(items: &[InventoryItemSnapshot], character_id: EntityId) -> Option<(EntityId, ItemId)>` selects only `ItemLocation::Equipped` for that bearer. Scene attachment entities are keyed by physical item ID and parented under the rig; parked carts remain `ObjectKey::Item` ground objects.

- [ ] **Step 1: Red test.** Make a detached row for each `Ground`, `Carried`, `Equipped`, and `Contained` location. Assert only `Equipped { character_id, slot: tool }` selects the tool/cart; another bearer's row never selects it. In a scene test, equip a tool/cart through the public application command, sync, and assert one attached model exists and no ground duplicate; then drop, load a different world at the same tick, and evict/revisit its chunk to verify correct removal/rebuild and stable ID ownership.
- [ ] **Step 2: Confirm red.** Run `cargo test -p progressus-client equipped_visual -j 1 -- --nocapture` and the focused scene attachment test; expect failure before production changes.
- [ ] **Step 3: Reconcile attachments.** Implement the selection from detached rows, keyed by `EntityId`, for example:

  ```rust
  fn equipped_visual(items: &[InventoryItemSnapshot], bearer: EntityId) -> Option<(EntityId, ItemId)> {
      items.iter().find_map(|item| match item.location {
          ItemLocation::Equipped { character_id, slot }
              if character_id == bearer && slot == progressus_content::slot::TOOL => Some((item.id, item.kind)),
          _ => None,
      })
  }
  ```

  Use the existing scene revision/visible-chunk pass to create/remove only the matching child visuals. PrimitiveTool follows the animated hand; Cart follows a rig drawbar anchor behind the worker. Do not turn contained cargo into separately rendered ground items or change any item location.
- [ ] **Step 4: Cart and tool presentation.** Keep the ground cart recipe, but split the equipped cart visual into cached body and wheel meshes so its two wheels rotate with visual travel distance; make the primitive tool upright at the hand socket rather than reusing its ground-resting orientation. Animate the cart as a child of the bearer root so it follows turns, motion interpolation, and origin rebases. The cart uses the same palette and scale as a parked cart, with wheel ground contact in normal walking frames.
- [ ] **Step 5: Green, visual review, docs, commit.** Run attachment, scene, and all client tests; then `PROGRESSUS_RUN_CLIENT_TESTS=1 ./scripts/check-prototype-01.sh`. Capture default/close native frames for Idle, Walk, Work, Tool, Cart pushing, and parked/loaded Cart, plus a short motion clip. Review ground contact, clipping, silhouettes, transitions, cache count, and whether native clicking still selects the person. Iterate if deficient. Update `docs/client.md` and `docs/milestones/prototype-02.md` only with proven behavior; explicitly mark owner aesthetic acceptance pending if that review cannot run. Commit `client: render equipped tools and moving carts`, push, and verify `HEAD == origin/main`.

## Final audit

- [ ] Confirm the spec's pose, ownership, attachment, cleanup, cache, rebase, and visual-acceptance clauses each have either passing evidence or a named open limitation.
- [ ] Confirm `git diff --check`, focused red/green evidence, full gate result, native capture status, and `git status --short --branch` are recorded in the final report.
