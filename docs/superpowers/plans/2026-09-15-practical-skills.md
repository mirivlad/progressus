# Prototype 02 Practical Skills Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give persistent individual practice in gathering, mining and crafting a deterministic one-tick work-phase effect without changing physical quantities or tool requirements.

**Architecture:** `progressus-content` defines stable skill identities, while `progressus-sim` owns practice, job timing, completion awards and save validation. `progressus-app` publishes detached values and the Bevy client only localizes and displays them. Reuse existing `Working.remaining_ticks` for active-job persistence.

**Tech Stack:** Rust workspace, serde save v1 DTOs, content registry, Bevy client, headless tests.

## Global Constraints

- Follow [the approved spec](../specs/2026-09-15-practical-skills-design.md) and ADR-0001/0002/0021/0024.
- Three stable skill names: `gathering`, `mining`, `crafting`; registry append-only, save names not handles.
- Practice is `0..=5`, one point only for completed physical work; at five practice, work is `max(1, base - 1)` ticks.
- No yield/input changes, no tool-requirement bypass, no Bevy dependency in the authoritative chain.
- Work directly on `main`; commit and push each independently verified task. No subagents are used for this execution.

## File map

- `crates/progressus-content/src/skill.rs` defines the identity registry; `lib.rs` exports it and registry tests assert unique names/order.
- `crates/progressus-sim/src/entity.rs` owns each character's practice; `simulation/persistence.rs` validates named save entries.
- `crates/progressus-sim/src/simulation/work.rs` chooses work duration and awards Harvest practice; `simulation/crafting.rs` awards Craft practice.
- `crates/progressus-app/src/read_model.rs` and `lib.rs` publish detached practice. `crates/progressus-client/src/i18n.rs`, `ui.rs` and synthetic fixtures render it.
- `docs/milestones/prototype-02.md`, `docs/architecture/overview.md`, `docs/client.md`, `README.md` describe only verified behavior.

---

### Task 1: Stable skill identity, character practice and save validation

**Files:** Create `crates/progressus-content/src/skill.rs`; modify `crates/progressus-content/src/lib.rs`, `crates/progressus-sim/src/lib.rs`, `crates/progressus-sim/src/entity.rs`, `crates/progressus-sim/src/simulation/persistence.rs`.

**Interfaces:** Produce `SkillId`, `skill::{GATHERING, MINING, CRAFTING}`, `MAX_SKILL_PRACTICE: u8 = 5`, `Character::skill_practice(SkillId) -> u8`, and `Character::record_skill_practice(SkillId)`.

- [ ] **Step 1: Write failing tests.** Add registry tests that resolve all three names and reject an unknown name. Add a character test with this exact expectation:

```rust
#[test]
fn skill_practice_starts_empty_and_caps_at_five() {
    let id = EntityId::new(1).unwrap();
    let position = WorldPosition::from_cell_center(WorldCell::new(0, 0)).unwrap();
    let mut person = Character::new(id, "Ada", position);
    assert_eq!(person.skill_practice(skill::GATHERING), 0);
    for _ in 0..6 { person.record_skill_practice(skill::GATHERING); }
    assert_eq!(person.skill_practice(skill::GATHERING), 5);
    assert_eq!(person.skill_practice(skill::MINING), 0);
}
```

In persistence tests, remove `skills` from a saved character and assert all skills zero; set `skills` to unknown-name, duplicate-name and practice-six arrays and assert `Simulation::load_json` fails.

- [ ] **Step 2: Run red.** `cargo test -p progressus-sim --lib skill_practice -- --nocapture` must fail to compile on the missing method. Registry tests must fail before the registry is added.
- [ ] **Step 3: Implement the bounded state and DTO.** Add a content-handle registry with `SkillDefinition { name: &'static str }` and the three rows. Store `BTreeMap<SkillId, u8>` on `Character`, default empty, read absent as zero, and saturate awards at five. Add a defaulted `skills: Vec<SkillPracticeSave>` of stable `{ name: String, practice: u8 }` entries to `CharacterSave`; save positive entries in registry order. During restore, resolve every name, reject unknown/duplicate/value-over-five, then construct `CharacterRestoreState` with the map. Keep existing save version and old-field defaults.

```rust
pub const MAX_SKILL_PRACTICE: u8 = 5;
pub fn skill_practice(&self, skill: SkillId) -> u8 {
    self.skills.get(&skill).copied().unwrap_or(0)
}
pub(crate) fn record_skill_practice(&mut self, skill: SkillId) {
    let practice = self.skills.entry(skill).or_default();
    *practice = practice.saturating_add(1).min(MAX_SKILL_PRACTICE);
}
```

- [ ] **Step 4: Run green.** `cargo test -p progressus-content && cargo test -p progressus-sim --lib skill` and `cargo test -p progressus-sim --lib save_v1_without` must pass. Run `cargo fmt --all -- --check` and `cargo clippy -p progressus-sim --all-targets -- -D warnings`.
- [ ] **Step 5: Commit and push.** `git add crates/progressus-content/src/skill.rs crates/progressus-content/src/lib.rs crates/progressus-sim/src/lib.rs crates/progressus-sim/src/entity.rs crates/progressus-sim/src/simulation/persistence.rs && git commit -m "sim: persist bounded individual skill practice" && git push origin main`.

### Task 2: Skilled work duration and completion awards

**Files:** Modify `crates/progressus-sim/src/simulation/work.rs`, `crates/progressus-sim/src/simulation/crafting.rs`, `crates/progressus-sim/src/simulation/persistence.rs`; add focused tests beside existing Harvest/Craft/save tests.

**Interfaces:** Consume Task 1's `SkillId` and `Character::skill_practice/record_skill_practice`; produce `Simulation::skilled_work_ticks(worker_id: EntityId, skill: SkillId, base: u32) -> u32` and classification of natural resource requirements.

- [ ] **Step 1: Write failing tests.** Build same-position fixtures with five completed Gathering, Mining and Crafting jobs assigned to one named worker. Compare the `Working.remaining_ticks` of identical later work with a zero-practice worker (Harvest 4 versus 3; current primitive-tool Craft 6 versus 5). Assert each successful completion gives only its worker one matching practice, cancellation gives none, and physical output quantities equal the original source/recipe definitions. A mastered miner without the `mine` capability must still be unable to start copper extraction.

```rust
#[test]
fn skilled_worker_gets_one_less_work_tick() {
    let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
    let trained = EntityId::new(3).unwrap();
    let untrained = EntityId::new(1).unwrap();
    for _ in 0..5 {
        simulation.characters.get_mut(&trained).unwrap()
            .record_skill_practice(skill::GATHERING);
    }
    assert_eq!(simulation.skilled_work_ticks(untrained, skill::GATHERING, HARVEST_WORK_TICKS), 4);
    assert_eq!(simulation.skilled_work_ticks(trained, skill::GATHERING, HARVEST_WORK_TICKS), 3);
}
```

- [ ] **Step 2: Run red.** `cargo test -p progressus-sim --lib skilled_ -- --nocapture` must fail on the unmodified work duration or missing award.
- [ ] **Step 3: Implement only work-phase effects.** At Harvest/PrepareConstruction-NaturalResource `start_working`, classify a source requiring `capability::MINE` as Mining and other current sources as Gathering. At Craft `start_working`, use Crafting. Read the assigned worker's practice before setting remaining work. Award once after a successful `complete_harvest` or `complete_craft` has removed its job; the Craft output-capacity early return awards nothing. Reuse `remaining_ticks` unchanged on save/load.

```rust
fn skilled_work_ticks(&self, worker_id: EntityId, skill: SkillId, base: u32) -> u32 {
    let mastered = self.characters[&worker_id].skill_practice(skill) == MAX_SKILL_PRACTICE;
    if mastered { base.saturating_sub(1).max(1) } else { base }
}
```

- [ ] **Step 4: Run green and conservation.** `cargo test -p progressus-sim --lib skilled_`, the existing `crafted_tool_enables_copper_extraction_and_physical_ore_delivery` regression, `cargo test -p progressus-sim --lib`, and a save/load-during-skilled-Working regression must pass. Assert job and item reservation indexes remain consistent.
- [ ] **Step 5: Commit and push.** `git add crates/progressus-sim/src/simulation/work.rs crates/progressus-sim/src/simulation/crafting.rs crates/progressus-sim/src/simulation/persistence.rs && git commit -m "sim: train skills on completed work and shorten skilled work" && git push origin main`.

### Task 3: Detached read model, localized inspector and stage evidence

**Files:** Modify `crates/progressus-app/src/lib.rs`, `crates/progressus-app/src/read_model.rs`, `crates/progressus-app/tests/client_boundary.rs`, `crates/progressus-client/src/i18n.rs`, `crates/progressus-client/src/ui.rs`, `crates/progressus-client/src/presentation.rs`, `docs/milestones/prototype-02.md`, `docs/architecture/overview.md`, `docs/client.md`, `README.md`.

**Interfaces:** Produce `SkillPracticeSnapshot { kind: SkillId, practice: u8, mastered: bool }` and `CharacterSnapshot.skills: Vec<SkillPracticeSnapshot>` in stable registry order. Client uses only this detached snapshot.

- [ ] **Step 1: Write failing boundary and localization tests.** After a public `Application::new_game`, require every character snapshot to have Gathering/Mining/Crafting with zero practice. Mutate a detached snapshot and confirm a fresh one remains unchanged. Extend `every_content_definition_is_translated_in_every_language` to enumerate `SkillId::all()`; before adding localization rows this must fail for `skill gathering`.

```rust
#[test]
fn skill_snapshot_starts_zero_and_is_detached() {
    let application = Application::new_game(NewGameOptions { seed: WorldSeed::new(0) }).unwrap();
    let mut snapshot = application.snapshot(SnapshotQuery::default()).unwrap();
    assert_eq!(snapshot.characters[0].skills.len(), 3);
    assert!(snapshot.characters[0].skills.iter().all(|entry| entry.practice == 0 && !entry.mastered));
    snapshot.characters[0].skills.clear();
    assert_eq!(application.snapshot(SnapshotQuery::default()).unwrap().characters[0].skills.len(), 3);
}
```

- [ ] **Step 2: Run red.** `cargo test -p progressus-app --test client_boundary skill_snapshot` and `cargo test -p progressus-client --lib every_content_definition_is_translated` must fail before the read model/localization implementation.
- [ ] **Step 3: Publish and render.** Export `SkillId` and `skill` through `progressus-app`; build the detached skill vector from `SkillId::all()` and `Character::skill_practice`. Add Russian/English rows for all three skills. Append localized `name: practice/5` lines to the selected-character inspector; update the synthetic `CharacterSnapshot` fixture. No client code may change practice or work ticks.

```rust
let skills = SkillId::all().map(|kind| {
    let practice = character.skill_practice(kind);
    SkillPracticeSnapshot { kind, practice, mastered: practice == MAX_SKILL_PRACTICE }
}).collect();
```

- [ ] **Step 4: Verify and document.** Run `cargo test -p progressus-app`, `cargo check -p progressus-client --all-targets -j 1`, then `PROGRESSUS_RUN_CLIENT_TESTS=1 ./scripts/check-prototype-01.sh`. Inspect a native selected-person inspector if available; report any unverified visual behavior honestly. Update only the proved Stage C checklist and relevant docs. Run `git diff --check`.
- [ ] **Step 5: Commit, push and verify.** `git add crates/progressus-app crates/progressus-client/src/i18n.rs crates/progressus-client/src/ui.rs crates/progressus-client/src/presentation.rs docs/milestones/prototype-02.md docs/architecture/overview.md docs/client.md README.md && git commit -m "client: show persistent practical skills" && git push origin main`; require a clean `git status --short` and identical `git rev-parse HEAD` / `git rev-parse origin/main`.
