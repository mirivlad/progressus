# Continuous Neutral Ambient Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a deterministic three-minute audition from an unbounded alternating piano-and-strings timeline with a 20-percent quieter background, smooth phrase ducking and sparse phrase-specific reverb or delay.

**Architecture:** A bounded-state `AmbientTimeline` emits absolute-time harmony regions and phrase plans without a fixed cycle. The existing standalone score renderer consumes those plans into separate background, foreground and wet buses, then mixes and encodes a review WAV. The code remains example-only and is not registered in client runtime audio.

**Tech Stack:** Rust 2024 standard library, deterministic offline PCM synthesis, 24 kHz stereo PCM16 WAV.

## Global Constraints

- Keep `crates/progressus-client/src/lib.rs`, runtime audio, authoritative simulation and saves unchanged.
- Use presentation-only deterministic randomness that never consumes simulation RNG.
- Multiply the accepted dry background by exactly `0.8` before ducking.
- Apply a smooth additional 8-12 percent background reduction around foreground phrases.
- Alternate felt piano and bowed strings by complete phrase.
- Apply extra reverb or stereo delay only to selected foreground phrases; never process the background bus with these sends.
- Generate an absolute-time timeline that can extend beyond 180 seconds without replaying the first 75 seconds.
- Keep only 12 recent motif fingerprints in generator state.
- Export the review WAV outside Cargo `target` under `/home/mirivlad/git/progressus-audio-review/continuous-neutral-20260910/`.
- Delete `/home/mirivlad/git/progressus-audio-review/neutral-timbres-20260910/` and run `cargo clean` only after the replacement passes all checks.

---

### Task 1: Bounded unbounded-timeline planner

**Files:**
- Create: `crates/progressus-client/src/ambient_timeline.rs`
- Modify: `crates/progressus-client/src/score_audition.rs`

**Interfaces:**
- Produces: `pub enum PhraseTimbre { FeltPiano, BowedStrings }`
- Produces: `pub enum PhraseEffect { Dry, Reverb { send: f32, tail: f32 }, Delay { send: f32, seconds: f32, feedback: f32, repeats: u8, pan: f32 } }`
- Produces: `pub struct HarmonyRegion { pub index: u64, pub start: f32, pub duration: f32, pub chord: [f32; 3], pub gain: f32 }`
- Produces: `pub struct PlannedNote { pub start: f32, pub midi: f32, pub duration: f32 }`
- Produces: `pub struct PhrasePlan { pub index: u64, pub notes: Vec<PlannedNote>, pub timbre: PhraseTimbre, pub effect: PhraseEffect, pub fingerprint: MotifFingerprint }`
- Produces: `pub struct AmbientPlan { pub regions: Vec<HarmonyRegion>, pub phrases: Vec<PhrasePlan> }`
- Produces: `pub struct AmbientTimeline` with `pub fn new(seed: u64) -> Self`, `pub fn next_region(&mut self) -> HarmonyRegion`, `pub fn next_phrase(&mut self) -> PhrasePlan`, and test-only `fn recent_fingerprint_count(&self) -> usize`.
- Produces: `pub fn collect_plan(seed: u64, seconds: f32) -> AmbientPlan`

- [x] **Step 1: Write failing deterministic timeline tests**

Add tests in `ambient_timeline.rs` that collect regions and phrases until 240 seconds and require deterministic same-seed output, changed different-seed output, strictly increasing absolute times, alternating timbres, legal MIDI pitch classes and bounded recent history:

```rust
#[test]
fn timeline_extends_deterministically_with_bounded_state() {
    let first = collect_plan(TEST_SEED, 240.0);
    assert_eq!(first, collect_plan(TEST_SEED, 240.0));
    assert_ne!(first, collect_plan(TEST_SEED ^ 1, 240.0));
    assert!(first.regions.windows(2).all(|w| w[0].start < w[1].start));
    assert!(first.phrases.windows(2).all(|w| w[0].notes[0].start < w[1].notes[0].start));
    assert!(first.phrases.iter().enumerate().all(|(i, phrase)| {
        phrase.timbre == if i % 2 == 0 { PhraseTimbre::FeltPiano } else { PhraseTimbre::BowedStrings }
    }));
    let mut timeline = AmbientTimeline::new(TEST_SEED);
    for _ in 0..100 { timeline.next_phrase(); }
    assert_eq!(timeline.recent_fingerprint_count(), 12);
}
```

- [x] **Step 2: Run the standalone test and observe missing-interface failures**

Run:

```bash
rustc --test --edition=2024 crates/progressus-client/src/ambient_timeline.rs -o /tmp/progressus-ambient-timeline-tests
```

Expected: compilation fails because `AmbientTimeline` and plan types do not exist.

- [x] **Step 3: Implement deterministic counters and harmony transitions**

Use a local SplitMix-style RNG with independently derived streams. Keep only current timing/index/chord state and a `VecDeque<MotifFingerprint>` of length 12. Choose region duration in `[13.0, 19.0]`, overlap its start by `3.5-5.5` seconds, reject the current chord, and select from these close-position voicings:

```rust
const VOICINGS: [[f32; 3]; 6] = [
    [60.0, 64.0, 67.0], // C
    [60.0, 64.0, 69.0], // Am/C
    [60.0, 65.0, 69.0], // F/C
    [62.0, 67.0, 71.0], // G/D
    [62.0, 65.0, 69.0], // Dm
    [59.0, 64.0, 67.0], // Em/B
];
```

Derive region gain from a slow sine energy arc plus bounded random variation. Prevent tonic-cadence regions from occurring less than three regions apart.

- [x] **Step 4: Implement phrase generation and recent-history rejection**

Start phrases after gaps in `[10.0, 22.0]`. Build four-to-six-note phrases from chord tones and adjacent diatonic passing tones, require a chord-tone ending, cap each leap at five semitones and permit at most one five-semitone leap. Normalize intervals and quantized duration ratios into `MotifFingerprint`; retry candidates that exactly match recent history or match intervals while also retaining rhythm and ending role. Select effects with a deterministic weighted distribution and reject the previous additional effect treatment:

```rust
match effect_roll {
    0..=49 => PhraseEffect::Dry,
    50..=84 => PhraseEffect::Reverb { send, tail },
    _ => PhraseEffect::Delay { send, seconds, feedback, repeats, pan },
}
```

The fixed three-minute fixture must contain all three treatments; use deterministic fallback assignment by phrase index if weighted selection has not produced a missing treatment by the final fixture phrase.

- [x] **Step 5: Run planner tests and commit**

Run:

```bash
rustfmt --edition 2024 crates/progressus-client/src/ambient_timeline.rs
rustc --test --edition=2024 crates/progressus-client/src/ambient_timeline.rs -o /tmp/progressus-ambient-timeline-tests
/tmp/progressus-ambient-timeline-tests
```

Expected: all timeline tests pass. Commit with:

```bash
git add crates/progressus-client/src/ambient_timeline.rs crates/progressus-client/src/score_audition.rs
git commit -m "audio: add unbounded neutral ambient timeline"
git push
```

### Task 2: Layer balance, ducking and sparse spatial sends

**Files:**
- Modify: `crates/progressus-client/src/score_audition.rs`

**Interfaces:**
- Consumes: `AmbientTimeline`, `HarmonyRegion`, `PhrasePlan`, `PhraseTimbre`, `PhraseEffect`
- Produces: `pub struct RenderedLayers { pub unscaled_background: Vec<f32>, pub background: Vec<f32>, pub foreground: Vec<f32>, pub wet: Vec<f32> }`
- Produces: `pub fn continuous_layers(seed: u64, seconds: usize) -> RenderedLayers`
- Produces: private `fn continuous_pcm(seed: u64, seconds: usize) -> Vec<f32>` and test-only `fn render_overlapping_windows(seed: u64, window_seconds: usize, total_seconds: usize) -> Vec<f32>`
- Produces: `pub fn continuous_wav(seed: u64, seconds: usize) -> Vec<u8>`

- [ ] **Step 1: Write failing balance, effects and continuity tests**

Add focused tests in `score_audition.rs`:

```rust
#[test]
fn continuous_mix_keeps_background_behind_foreground() {
    let layers = continuous_layers(COMPARISON_SEED, 180);
    assert_eq!(layers.background.len(), 180 * SAMPLE_RATE as usize * 2);
    for (raw, reduced) in layers.unscaled_background.iter().zip(&layers.background) {
        assert!(reduced.abs() <= raw.abs() * 0.800_01 + 1e-7);
    }
    assert!(phrase_windows_are_ducked_smoothly(&layers));
    assert!(foreground_phrase_rms_exceeds_background(&layers));
}

#[test]
fn effects_are_sparse_bounded_and_foreground_only() {
    let plan = collect_plan(COMPARISON_SEED, 180.0);
    assert!(plan.phrases.iter().any(|p| matches!(p.effect, PhraseEffect::Dry)));
    assert!(plan.phrases.iter().any(|p| matches!(p.effect, PhraseEffect::Reverb { .. })));
    assert!(plan.phrases.iter().any(|p| matches!(p.effect, PhraseEffect::Delay { .. })));
    let layers = continuous_layers(COMPARISON_SEED, 180);
    assert!(wet_peak(&layers) < foreground_dry_peak(&layers));
    assert_eq!(layers.unscaled_background, background_without_phrase_effects(COMPARISON_SEED, 180));
}

#[test]
fn arbitrary_windows_match_one_shot_render_at_the_join() {
    let whole = continuous_pcm(COMPARISON_SEED, 180);
    let joined = render_overlapping_windows(COMPARISON_SEED, 30, 180);
    assert_pcm_close(&whole, &joined, 1e-6);
    assert!(maximum_join_delta(&joined, 30) < 0.01);
}
```

- [ ] **Step 2: Run focused tests and observe failures**

Run:

```bash
rustc --test --edition=2024 crates/progressus-client/src/score_audition.rs -o /tmp/progressus-score-audition-tests
```

Expected: compilation fails for absent continuous rendering APIs.

- [ ] **Step 3: Generalize background and foreground rendering to absolute plans**

Parameterize PCM allocation by `seconds`, render `HarmonyRegion` values instead of the fixed five-region array, and preserve the existing air/ensemble and instrument synthesis functions. Keep an unscaled background buffer. Create the audible background as:

```rust
let duck = background_duck_gain(absolute_time, &phrases); // 0.88..=1.0
background[index] = unscaled_background[index] * 0.8 * duck;
```

Use a cosine attack/release envelope at least 600 ms long around each phrase so adjacent-sample gain changes remain below the test threshold. Render foreground timbre from each complete `PhrasePlan`.

- [ ] **Step 4: Implement bounded reverb and stereo delay sends**

Copy only the selected phrase's dry foreground interval and instrument tail to a temporary send. For `Reverb`, use three decorrelated delay taps plus bounded feedback for the requested 2.0-4.5 second tail. For `Delay`, alternate stereo taps at the planned 280-620 ms interval for exactly `repeats` iterations and multiply each repeat by the planned feedback. Scale the complete wet bus if necessary so its peak remains below the dry foreground peak; do not alter the background bus.

- [ ] **Step 5: Implement overlap-window equivalence and WAV output**

Render windows using absolute plan/event time and a six-second overlap margin. Keep only the requested center frames and use equal-power overlap weights. `continuous_wav(seed, seconds)` renders the same timeline as the windowed path and encodes exactly `seconds * SAMPLE_RATE * 2` samples.

- [ ] **Step 6: Run focused tests and commit**

Run:

```bash
rustfmt --edition 2024 crates/progressus-client/src/score_audition.rs
rustc --test --edition=2024 crates/progressus-client/src/score_audition.rs -o /tmp/progressus-score-audition-tests
/tmp/progressus-score-audition-tests
```

Expected: all legacy comparison and new continuous-render tests pass. Commit with:

```bash
git add crates/progressus-client/src/score_audition.rs
git commit -m "audio: render balanced continuous ambient"
git push
```

### Task 3: Three-minute review export and cleanup

**Files:**
- Modify: `crates/progressus-client/examples/timbre_preview.rs`
- Modify: `docs/superpowers/plans/2026-09-10-continuous-neutral-ambient.md`

**Interfaces:**
- Consumes: `continuous_wav(seed: u64, seconds: usize) -> Vec<u8>` and timeline plan data
- Produces CLI: `cargo run -p progressus-client --example timbre_preview -- <output-directory>`
- Produces: `continuous-alternating-180s.wav`, `manifest.txt`, `generation.log`, `verification.json`

- [ ] **Step 1: Change exporter to one continuous 180-second artifact**

Set `PREVIEW_SECONDS: usize = 180`, call `continuous_wav(COMPARISON_SEED, PREVIEW_SECONDS)`, and record phrase count plus dry/reverb/delay counts in the manifest. Write independent decoded statistics for duration, channels, sample rate, peak, RMS and clipping count into `verification.json`.

- [ ] **Step 2: Generate the review artifact**

Run:

```bash
CARGO_TARGET_DIR=/home/mirivlad/git/progressus/target cargo run -p progressus-client --example timbre_preview -- /home/mirivlad/git/progressus-audio-review/continuous-neutral-20260910
```

Expected: the new directory contains one 180-second WAV plus the three text verification files.

- [ ] **Step 3: Run final source and artifact gates**

Run:

```bash
cargo fmt --all --check
rustc --test --edition=2024 crates/progressus-client/src/ambient_timeline.rs -o /tmp/progressus-ambient-timeline-tests
/tmp/progressus-ambient-timeline-tests
rustc --test --edition=2024 crates/progressus-client/src/score_audition.rs -o /tmp/progressus-score-audition-tests
/tmp/progressus-score-audition-tests
CARGO_TARGET_DIR=/home/mirivlad/git/progressus/target cargo clippy -p progressus-client --all-targets -- -D warnings
CARGO_TARGET_DIR=/home/mirivlad/git/progressus/target cargo test -p progressus-client --lib
./scripts/verify-core-dependency-boundary.sh
```

Expected: every command exits zero. Independently parse the WAV and require 180.000 seconds, stereo, 24 kHz, PCM16, peak below `0.82`, zero clipped samples, no silent one-second window and low-band energy below upper-band energy at 130 Hz.

- [ ] **Step 4: Review, publish and clean generated iterations**

Request independent review of the complete diff from `origin/main`. Fix all Critical and Important findings, rerun affected checks, then commit and push. Fast-forward verified commits into `main` and confirm `HEAD == origin/main`.

Delete `/home/mirivlad/git/progressus-audio-review/neutral-timbres-20260910/` only after the replacement artifact passes. Run `cargo clean`, preserve `/home/mirivlad/git/progressus-audio-review/continuous-neutral-20260910/`, mark every plan checkbox complete, and publish the documentation-only completion commit.
