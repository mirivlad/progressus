# Prototype 02 Stage C — Practical Skills

Status: **Approved by project owner on 2026-09-15; implementation in progress**

## Purpose and scope

Stage C proves that named people retain practical knowledge gained through physical work. A skill changes the time needed to perform later work, not the amount a finite source yields or the materials a recipe consumes. Stage D will add player-directed excavation of ground and rock on the existing flat map; underground levels are outside Prototype 02. Stage E will add research/knowledge gating separately. A skill is not a technology unlock.

## Alternatives considered

- **Shorter work phase — selected.** Deterministic, visible in job duration, and conserves physical quantities.
- **Greater output.** Rejected for this stage: one deposit would create more material solely because of worker experience, complicating quantity accounting and depletion semantics.
- **Chance of success.** Rejected for this stage: it adds random outcomes and possible wasted inputs without demonstrating the basic people-to-work knowledge loop.

## Skill vocabulary and ownership

Three stable lowercase skill names are defined in the shared content vocabulary: `gathering`, `mining`, and `crafting`. The content registry defines their identities only; the client localizes those names, and neither layer owns experience or game time. The pure-Rust simulation owns a bounded practice value for each skill on each persistent `Character`. A new character, or a valid pre-skill save, starts at zero practice in every skill. Practice is an integer in `0..=5`; five is the current mastery cap. The registry is append-only, and saves use stable names rather than runtime handle indexes.

`Gathering` applies to ordinary Harvest of trees, stone outcrops and berry bushes, including harvesting one of those sources to prepare a construction cell. `Mining` applies to Harvest of copper veins and, once Stage D is implemented, to work that excavates rock or ground. This classification is based on the source's required mining capability or the excavation job kind, not on a client model. `Crafting` applies to completed Workbench Craft jobs, including the current primitive-tool and cart recipes. Haul, movement, Eat, Sleep, construction delivery and construction assembly do not grant these three skills. A finished job grants exactly one practice point to its actual worker, capped at five. A cancelled, failed, abandoned or merely reserved/working job grants none. Repeated tick processing and save/load must not award one completion twice.

The current tool/capability requirement is independent of skill. A master miner without a tool still cannot extract a copper vein or excavate tool-gated rock. No skill gates a recipe or creates an item.

## Work-time effect

At the transition into `JobState::Working`, the simulation reads the assigned worker's practice in the relevant skill and fixes that job's remaining work. The duration is the existing base work duration at practice `0..=4`; at practice `5` it is `max(1, base - 1)` ticks. Only the working phase changes. Travel, item transport, material quantities, source yield and job reservation order do not change.

The `remaining_ticks` already stored on a working job is authoritative. Finishing another job, loading a save, or reassigning an available job does not retroactively rescale work already in progress. If an interrupted job leaves `Working` and later begins work anew, the new worker's current practice determines that new work phase under the ordinary job lifecycle.

## Persistence, application and presentation

Save format v1 gains an additive per-character list of `{ name, practice }` entries in registry order; zero-practice skills may be omitted. A missing list means all zero; unknown skill names, duplicate entries and values above five are rejected rather than silently dropped or clamped. Saved names remain stable across content-registry append operations. Active jobs continue to save their exact remaining ticks, so a save during skilled work resumes byte-identically under the same inputs.

`progressus-app` publishes detached per-character skill practice and derived mastery through its existing character snapshot. The Bevy client shows three localized rows in the selected-character inspector, with progress `0/5` through `5/5`; it reads the snapshot and never awards practice or calculates a job's authoritative duration. No new player command is needed to train a skill: training is caused by completed physical work. The interface must not imply that skill alone meets a tool prerequisite.

## Verification and acceptance

- A new game and pre-skill save show three zeroed skills; a malformed save with an unknown, duplicate or out-of-range skill fails explicitly.
- Five completed jobs of one kind raise only the performing character's corresponding skill to five. Cancellation, path failure, manual interruption and a different worker do not miscredit practice; additional completions remain capped at five.
- For an identical source/recipe and input state, an untrained worker uses the existing duration and a mastered worker uses one fewer work tick, bounded below by one. The difference is measured at the work phase, not confused with travel time.
- Copper mining continues to require an equipped mining-capable tool at mastery five. Stone/berry/wood and recipe input/output quantities remain exactly the same at both proficiency levels.
- A save during skilled work round-trips character practice and job remaining work and resumes deterministically. Headless tests run without Bevy; app snapshots are detached; client localization and inspector tests pass.
- The final Stage C check runs the repository gate with client tests enabled and reports native inspector observation separately from automated results. Stage D/E, including excavation and research, remain incomplete until their own implementation and verification.
