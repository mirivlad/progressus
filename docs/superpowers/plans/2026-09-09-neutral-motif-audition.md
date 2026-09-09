# Neutral Motif Audition Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an isolated deterministic generator for neutral Progressus motif candidates and export listenable WAV examples without changing in-game audio.

**Architecture:** A new pure Rust module generates, validates and shortlists compact motif data, then renders each selected motif with procedural plucked-wood synthesis in isolation and in a short harmonic context. A standalone Cargo example writes review artifacts under `target/`; the module is deliberately not registered by the client library or playback systems.

**Tech Stack:** Rust 2024, standard library only for synthesis and WAV encoding, existing `progressus-client` example target.

## Global Constraints

- Do not modify the current runtime ambient generator, Bevy playback, audio controls or authoritative simulation.
- Generate 30 to 50 deterministic neutral candidates and shortlist 8 to 12 structurally distinct examples.
- Use only presentation-owned deterministic randomness.
- Avoid copied reference melodies, folk stylization, dance pulse, cosmic drones and continuous low oscillators.
- Render both isolated and harmonically contextual versions of every shortlisted motif.
- Keep all generated WAV files outside Git under `target/neutral-motif-audition-20260909/`.

---

### Task 1: Motif grammar and deterministic shortlist

**Files:**
- Create: `crates/progressus-client/src/motif_generation.rs`

**Interfaces:**
- Produces: `pub const SAMPLE_RATE: u32 = 24_000`
- Produces: `pub enum EndingRole { Open, Resting, Resolving }`
- Produces: `pub struct Motif { pub degrees: Vec<i8>, pub durations: Vec<u8>, pub role: EndingRole }`
- Produces: `pub fn candidate_pool(seed: u64) -> Vec<Motif>`
- Produces: `pub fn shortlist(seed: u64, count: usize) -> Vec<Motif>`

- [x] **Step 1: Add failing grammar tests**

Add module tests that require exactly 48 reproducible candidates, 3–7 notes, range at most six scale degrees, at most one leap larger than a third, matching duration counts, no immediate three-note repetition, role-compatible endings and no transposition-equivalent duplicates. Require a deterministic 10-item shortlist with distinct normalized `(degrees, durations)` signatures and coverage of every ending role.

```rust
#[test]
fn neutral_pool_is_reproducible_and_structurally_valid() {
    let pool = candidate_pool(0x5052_4f47_5245_5353);
    assert_eq!(pool.len(), 48);
    assert_eq!(pool, candidate_pool(0x5052_4f47_5245_5353));
    assert!(pool.iter().all(valid_motif));
    assert_eq!(normalized_signatures(&pool).len(), pool.len());
}

#[test]
fn shortlist_is_diverse_and_covers_ending_roles() {
    let selected = shortlist(0x5052_4f47_5245_5353, 10);
    assert_eq!(selected.len(), 10);
    assert!(EndingRole::ALL.into_iter().all(|role| selected.iter().any(|m| m.role == role)));
}
```

- [x] **Step 2: Run the focused test and observe failure**

Run:

```bash
rustc --test --edition=2024 crates/progressus-client/src/motif_generation.rs -o /tmp/progressus-motif-tests
```

Expected: failure because the new interfaces are not implemented.

- [x] **Step 3: Implement the bounded grammar**

Use a local SplitMix64-style RNG. Generate scale-degree walks from neutral Dorian/pentatonic-compatible degrees with weighted steps `[-2, -1, 0, 1, 2]`, permitting one bounded `±3` leap. Generate relative durations from `[2, 3, 4, 6]`, classify endings by final motion and reject candidates until 48 normalized unique valid motifs exist. Give every loop a hard attempt bound and panic with a clear invariant message if the grammar cannot fill the pool.

Shortlist deterministically by round-robin ending role, then maximize minimum distance from already selected normalized degree/rhythm signatures. Resolve equal scores by original candidate index.

- [x] **Step 4: Run focused tests**

Run:

```bash
rustc --test --edition=2024 crates/progressus-client/src/motif_generation.rs -o /tmp/progressus-motif-tests
/tmp/progressus-motif-tests
```

Expected: all motif grammar tests pass.

- [x] **Step 5: Commit and push the grammar**

```bash
git add crates/progressus-client/src/motif_generation.rs
git commit -m "feat: add neutral motif grammar"
git push origin main
```

### Task 2: Procedural audition rendering and exporter

**Files:**
- Modify: `crates/progressus-client/src/motif_generation.rs`
- Create: `crates/progressus-client/examples/motif_preview.rs`

**Interfaces:**
- Consumes: `shortlist(seed: u64, count: usize) -> Vec<Motif>`
- Produces: `pub enum AuditionKind { Isolated, Contextual }`
- Produces: `pub fn audition_wav(motif: &Motif, index: u64, kind: AuditionKind) -> Vec<u8>`
- Produces: CLI `cargo run -p progressus-client --example motif_preview -- <output-directory>`

- [x] **Step 1: Add failing synthesis tests**

Require valid 24 kHz stereo PCM16 RIFF output, deterministic bytes, distinct isolated/contextual renders, non-silence, peak below `0.82`, and no long stationary low-frequency foundation. Check that the contextual render has audible energy before and after the motif while its quietest one-second window remains materially below its loudest window.

```rust
#[test]
fn audition_wavs_are_deterministic_distinct_and_bounded() {
    let motif = &shortlist(NEUTRAL_SEED, 10)[0];
    let isolated = audition_wav(motif, 0, AuditionKind::Isolated);
    let contextual = audition_wav(motif, 0, AuditionKind::Contextual);
    assert_eq!(isolated, audition_wav(motif, 0, AuditionKind::Isolated));
    assert_ne!(isolated, contextual);
    assert_valid_pcm16(&isolated, SAMPLE_RATE, 2, 0.82);
    assert_valid_pcm16(&contextual, SAMPLE_RATE, 2, 0.82);
}
```

- [x] **Step 2: Observe the focused failure**

Run the same standalone `rustc --test` command and confirm failure on the absent rendering API.

- [x] **Step 3: Implement acoustic rendering**

Render a 9-to-14-second isolated example with a deterministic Karplus–Strong plucked-string voice layered quietly with a short modal wood resonator. Render the contextual version with the same motif plus two sparse upper-register harmonic responses and a shared synthetic room tail. Use excitation envelopes and decays; do not sustain a low oscillator. Encode fixed-headroom stereo PCM16 without per-file normalization.

The exporter writes:

```text
motif-01-isolated.wav ... motif-10-isolated.wav
motif-01-context.wav  ... motif-10-context.wav
neutral-motifs-isolated-sampler.wav
neutral-motifs-context-sampler.wav
manifest.txt
generation.log
```

The manifest records scale-degree intervals, relative durations and ending role for each numbered candidate. Samplers insert two seconds of silence between examples.

- [x] **Step 4: Run focused tests and export examples**

```bash
rustc --test --edition=2024 crates/progressus-client/src/motif_generation.rs -o /tmp/progressus-motif-tests
/tmp/progressus-motif-tests
cargo run -p progressus-client --example motif_preview -- target/neutral-motif-audition-20260909
```

Expected: tests pass and 22 WAV files plus `manifest.txt` and `generation.log` are written.

- [x] **Step 5: Verify review artifacts**

Parse every WAV and record duration, peak, RMS, non-finite sample count and clipping count in `target/neutral-motif-audition-20260909/verification.json`. Require stereo PCM16 at 24 kHz, nonzero RMS, zero non-finite samples, zero clipped samples and peak below `0.82`. Confirm `git diff` contains no runtime audio registration or playback changes.

- [x] **Step 6: Run project checks**

```bash
cargo fmt --all --check
cargo clippy -p progressus-client --all-targets -- -D warnings
cargo test -p progressus-client --lib
./scripts/verify-core-dependency-boundary.sh
```

Expected: every command exits zero.

- [x] **Step 7: Commit and push the renderer/exporter**

```bash
git add crates/progressus-client/src/motif_generation.rs crates/progressus-client/examples/motif_preview.rs docs/superpowers/plans/2026-09-09-neutral-motif-audition.md
git commit -m "feat: generate neutral motif auditions"
git push origin main
```

- [ ] **Step 8: Deliver the listening pack**

Present the isolated and contextual samplers inline, link the numbered WAV files and explain that signal checks passed while musical acceptance remains the owner's listening decision. Do not integrate any candidate into the game during this task.
