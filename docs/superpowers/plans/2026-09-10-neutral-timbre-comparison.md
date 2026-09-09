# Neutral Foreground Timbre Comparison Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Export three 75-second comparisons with one continuous neutral background and identical Western-diatonic phrase events, changing only the foreground between felt piano, bowed strings and alternating voices.

**Architecture:** A new standalone pure Rust score-audition module owns the fixed comparison score, background rendering, two foreground physical models, room processing and PCM encoding. A path-including Cargo example exports the three review WAVs; neither file is registered by the client library or runtime. Once verified, the rejected motif-audition source and generated files are removed.

**Tech Stack:** Rust 2024 standard library, deterministic offline PCM synthesis, 24 kHz stereo PCM16 WAV.

## Global Constraints

- Keep the existing in-game ambient playback byte-identical.
- Render exactly 75 seconds per comparison.
- Use identical background and note events in all three versions.
- Use diatonic harmony-aware phrases; remove Dorian/pentatonic candidate logic from the active audition.
- Maintain a mid-register evolving background without a silent one-second window or constant low oscillator.
- Store final listening files under `/home/mirivlad/git/progressus-audio-review/neutral-timbres-20260910/`, outside Cargo `target`.
- Delete rejected preview artifacts and run `cargo clean` only after final verification and export.

---

### Task 1: Shared score and continuous background

**Files:**
- Create: `crates/progressus-client/src/score_audition.rs`

**Interfaces:**
- Produces: `pub const SAMPLE_RATE: u32 = 24_000`
- Produces: `pub const COMPARISON_SECONDS: usize = 75`
- Produces: `pub struct NoteEvent { pub start: f32, pub midi: f32, pub duration: f32 }`
- Produces: `pub fn phrase_events(seed: u64) -> Vec<NoteEvent>`
- Produces: `pub fn background_pcm(seed: u64) -> Vec<f32>`

- [x] **Step 1: Write failing score tests**

Require deterministic events in five separated phrases, only C-major/A-minor diatonic pitch classes, at least one `B -> C` resolution, every phrase ending on a current chord tone, and no event below A3. Require a 75-second stereo background whose one-second RMS windows after the initial fade are all nonzero and vary by at least 1.5×, with energy below 130 Hz lower than upper-band energy.

```rust
#[test]
fn score_is_diatonic_harmony_aware_and_reproducible() {
    let events = phrase_events(COMPARISON_SEED);
    assert_eq!(events, phrase_events(COMPARISON_SEED));
    assert_eq!(phrase_count(&events), 5);
    assert!(events.iter().all(is_allowed_diatonic_note));
    assert!(contains_leading_tone_resolution(&events));
    assert!(phrase_endings_are_chord_tones(&events));
}

#[test]
fn background_evolves_for_the_full_comparison_without_low_hum() {
    let pcm = background_pcm(COMPARISON_SEED);
    assert_eq!(pcm.len(), COMPARISON_SECONDS * SAMPLE_RATE as usize * 2);
    assert!(one_second_rms(&pcm)[1..].iter().all(|rms| *rms > 0.0002));
    assert!(dynamic_range_ratio(&pcm) > 1.5);
    assert!(low_band_energy(&pcm, 130.0) < upper_band_energy(&pcm, 130.0));
}
```

- [x] **Step 2: Run the standalone test and observe missing-interface failures**

```bash
rustc --test --edition=2024 crates/progressus-client/src/score_audition.rs -o /tmp/progressus-score-audition-tests
```

- [x] **Step 3: Implement the deterministic score and background**

Use five 15-second harmonic regions with close-position C/Am/F/G-derived voicings above A3. Crossfade breath/ensemble voices across every boundary, vary their amplitude envelopes on nonmatching periods, and high-pass the shared room return. Generate five four-to-six-note phrases near seconds 7, 21, 35, 50 and 64. Select chord tones plus stepwise diatonic passing notes, cap leaps at a third except one fourth per complete excerpt, and force one final `B4 -> C5` resolution.

- [x] **Step 4: Run the focused tests until they pass**

Run the command from Step 2 followed by `/tmp/progressus-score-audition-tests` and require zero failures.

### Task 2: Fair A/B/C foreground renderer and export

**Files:**
- Modify: `crates/progressus-client/src/score_audition.rs`
- Create: `crates/progressus-client/examples/timbre_preview.rs`
- Delete after replacement verification: `crates/progressus-client/src/motif_generation.rs`
- Delete after replacement verification: `crates/progressus-client/examples/motif_preview.rs`

**Interfaces:**
- Produces: `pub enum ForegroundTimbre { FeltPiano, BowedStrings, Alternating }`
- Produces: `pub fn comparison_wav(seed: u64, timbre: ForegroundTimbre) -> Vec<u8>`
- Produces CLI: `cargo run -p progressus-client --example timbre_preview -- <output-directory>`

- [x] **Step 1: Write failing comparison tests**

Require three deterministic, distinct, valid 75-second WAVs. Decode them and subtract independently rendered foregrounds to prove the background input is identical. Require peak below `0.82`, no clipped/non-finite samples, audible foreground energy around every phrase and no silent one-second window.

- [x] **Step 2: Observe failures for the absent timbre API**

Run the standalone score-audition test command and confirm the new tests fail for missing interfaces.

- [x] **Step 3: Implement the foreground voices**

Felt piano uses a soft hammer envelope driving three lightly detuned strings per note, high partials decaying faster than the fundamental and a restrained soundboard response. Bowed strings use slow attack, three detuned ensemble voices, bounded vibrato, harmonic rolloff and low bow noise. Alternating assigns complete phrases, rather than individual notes, to piano/string in sequence. All variants mix the same `background_pcm(seed)` and `phrase_events(seed)` with fixed gains before one common high-passed room stage.

- [x] **Step 4: Implement the exporter and generate the comparison**

Export `01-felt-piano.wav`, `02-bowed-strings.wav`, `03-alternating.wav`, `manifest.txt`, `generation.log` and `verification.json` under the requested external review directory.

- [x] **Step 5: Verify output and project boundaries**

```bash
cargo fmt --all --check
rustc --test --edition=2024 crates/progressus-client/src/score_audition.rs -o /tmp/progressus-score-audition-tests
/tmp/progressus-score-audition-tests
cargo clippy -p progressus-client --all-targets -- -D warnings
cargo test -p progressus-client --lib
./scripts/verify-core-dependency-boundary.sh
```

Parse all three WAVs independently and record duration, format, peak, RMS and clipping counts. Confirm no changes to `lib.rs`, runtime audio, simulation or saves.

- [x] **Step 6: Remove replaced artifacts and build products**

Delete `target/neutral-motif-audition-20260909/` after the new external review files pass. Commit and push the verified source, then run `cargo clean` in the main checkout. Report exact reclaimed disk space and preserve only the external review files.
