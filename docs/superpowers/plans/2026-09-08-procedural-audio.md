# Procedural Audio Implementation Plan

> **For agentic workers:** Use subagent-driven-development for bounded tasks and reviews. Execute continuously within the approved specification.

**Goal:** Fix the audited correctness defects and deliver local acoustic music, work sounds and playable examples.

**Architecture:** Pure sample synthesis and a disposable snapshot observer live in the client; Bevy owns playback. A background task prepares a new 120-second composition; playback completion starts a 20-second silence before the next composition. Authoritative RNG/save state is untouched.

**Tech Stack:** Rust, pinned Bevy 0.18.1 audio, generated PCM16 WAV.

## Global Constraints

- The simulation core must not depend on Bevy.
- Stable Progressus identity is independent of renderer identity.
- Audio uses presentation-only seeds and clocks.
- Music is approximately 120 seconds, followed by 20 seconds of silence, then a changed composition.
- At most eight effect voices and four new effect voices per observed tick.
- Client direct dependencies remain Bevy and progressus-app.
- Commit/push logically completed tasks; integrate verified work into main.

## Task 1 — Correctness repairs

Files: `crates/progressus-sim/src/simulation.rs`, `crates/progressus-client/src/interaction.rs`, focused tests.

- [ ] Add a regression which reaches Craft Working/1, blocks both output ports, advances ordinary ticks successfully without consuming ingredients, saves/loads, unblocks, and completes exactly once. Cancellation releases reservations.
- [ ] Observe the existing ProductionOutputBlocked failure, then make absent output retain work/inputs and return normally.
- [ ] Add scheduler tests using exact Duration inputs and expected `elapsed / 250ms`, while preserving at-most-one-tick and pause/backlog behavior; observe failure.
- [ ] Preserve only `elapsed % TICK_INTERVAL` after emission. Replace the two equivalent divisibility expressions flagged by Clippy.
- [ ] Run focused tests, authoritative suite, and strict lint checks; commit/push.

## Task 2 — Synthesis and previews

Files: new `crates/progressus-client/src/audio_synthesis.rs`; standalone example `crates/progressus-client/examples/audio_preview.rs` (path-includes the pure generator).

Interfaces: `music_wav(index: u64) -> Vec<u8>`, `effect_wav(kind: EffectKind, variant: u64) -> Vec<u8>`, `EffectKind::{Harvest, Construct, Craft, Pickup, Drop}`. Public constants define sample rate, music duration and gap.

- [ ] Write tests for RIFF PCM size, non-silence, no clipping, deterministic seed, distinct composition families, fades and effect duration. Run the pure module with `rustc --test` before implementation.
- [ ] Generate notes from three distinct family-specific progressions, rhythmic structures, melodic contours and timbral balances. Render plucks, soft bowed/pad partials and wooden percussion with envelopes into bounded stereo buffers. Normalize with headroom and encode WAV.
- [ ] Export three complete pieces, separate short excerpts, effect sampler and a two-piece 120/20 transition demonstration. Verify all samples/metadata and retain generation timings.
- [ ] Run synthesis tests and commit/push the generator/examples.

## Task 3 — Playback, work cues and controls

Files: new `audio.rs`, `audio_observer.rs`; modify Cargo features, runtime/lib registration and localized UI/modal controls.

- [ ] Test observation: baseline emits nothing; unchanged tick emits nothing; working progress creates a cue; load reset discards history; carrier transition creates a handling cue. Cull by visible camera bounds and cap admission.
- [ ] Implement a bounded background generation task and current/next asset lifecycle. Start the silence timer only when the music voice ends; late generation extends silence. Muting retains schedule without audible playback.
- [ ] Observe authoritative snapshots after successful ticks, reset on load, and spawn finite despawning work voices from cached WAV samples.
- [ ] Add a Sound modal with separate Music/Effects levels including zero; preserve gameplay input capture and localization. Apply volume changes to existing voices too.
- [ ] Run client tests (or clearly report linkage limits), fmt, clippy, core tests and boundary checks. Verify no-device behavior and authoritative save equality with/without observing audio.

## Task 4 — Review, integration and report

- [ ] Review the complete diff independently for lifecycle, memory, playback gaps, cancellation/load false cues, native UI and core boundaries.
- [ ] Fix substantive findings and rerun covering checks.
- [ ] Update specification/milestone notes with actual validation and remaining listening risks.
- [ ] Merge/fast-forward verified commits to main, push, verify remote hash; deliver playable local files and report exact test evidence.

Stage B fatigue/shelter design remains the next gameplay stage, not part of this audio implementation.
