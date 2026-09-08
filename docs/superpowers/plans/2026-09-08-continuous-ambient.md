# Continuous Ambient Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development for independent synthesis work and review; the controller integrates playback and verifies the native client.

**Goal:** Replace 120/20 playback with continuous, independently changing acoustic layers and deliver listening examples.

**Architecture:** Pure client synthesis generates bounded PCM layer buffers on worker tasks. Bevy mixes three independent layer players, overlapping looping foundation/nature replacements and leaving melodic rests; existing observed work effects remain independent.

**Tech Stack:** Rust, pinned Bevy 0.18.1 audio, stereo PCM16 at 24 kHz; no new direct dependencies.

## Global Constraints

- Approved specification: `docs/superpowers/specs/2026-09-08-continuous-ambient-design.md`.
- Pure Rust authoritative core and saves remain unchanged; all audio randomness stays in the client.
- Never synthesize in a frame update. At most one prepared-next buffer/task and two playing voices per layer; release retired assets.
- Foundation/nature buffers must loop seamlessly if generation is late; melody must not repeat a stale phrase buffer.
- Existing Music/Effects controls remain independent; existing work admission limits and observation semantics remain.

## Task 1: Pure synthesis and previews

Files: create `crates/progressus-client/src/ambient_synthesis.rs`; modify `crates/progressus-client/examples/audio_preview.rs`. Controller handles module registration and playback.

Interfaces to export:
```rust
pub enum AmbientLayer { Foundation, Melody, Nature }
impl AmbientLayer {
    pub const ALL: [Self; 3];
    pub fn seconds(self) -> f64;
    pub fn looping(self) -> bool;
}
pub fn layer_wav(layer: AmbientLayer, index: u64) -> Vec<u8>;
```

Layer PCM has fixed mix headroom; do not normalize each buffer independently. Foundation and nature are seamless loops; melody includes soft tails and rests. Use one compatible modal palette with changing voicing/register/timbre; independently selected buffers must stay harmonically compatible. Changing instruments and density must be audible beyond random transposition. Keep synthesis code independent of Bevy and simulation. Reuse existing effects without changing their observer.

- [ ] Write focused failing synthesis tests for audible continuous foundation/nature boundaries, bounded finite PCM, changed deterministic buffers and phrase rest/contour rules; run `rustc --test --edition=2024 -O crates/progressus-client/src/ambient_synthesis.rs -o /tmp/progressus-ambient-tests` and execute it.
- [ ] Implement the generator and verify these tests, including combined-mix headroom. Use 17/23/29-second modulation and compatible harmonic colour changes; no promise of mathematically infinite non-repetition.
- [ ] Update the standalone preview exporter to create four-minute quiet and explicitly scripted work mixes using the same layer timing/transitions as playback, retaining raw generation times. Export short excerpts and isolated layers. Preserve existing effect samples.
- [ ] Report exact APIs, timing, bounds, test results and preview paths to the controller; do not modify playback files or commit another worker's edits.

## Task 2: Bounded independent playback

Files: `crates/progressus-client/src/audio.rs`, `audio_observer.rs`, `lib.rs`; new pure `ambient_playback.rs` if needed to share timing with previews.

- [ ] Replace the old gap scheduler test with tests exercising continuous transition gains, delayed-next behavior, readiness-before-fade, long-run state/resource bounds, independent mute scaling and no repeated melodic backlog.
- [ ] Implement looping current/next foundation and nature with six-second linear crossfades. Start retiring the old voice only after the new sink is ready. If a frame stalls or generation is late, the old looping bed continues. For melody, play once and start fresh material after completion; do not loop or overlap conflicting phrases.
- [ ] Remove the retired 120/20 scheduler and full-piece production path after updating save-independence tests to call actual ambient generation.
- [ ] Retain real work effects as the activity layer; do not add an invented continuous machine texture. Apply volume changes through each voice's fade gain so the settings system cannot overwrite transitions.
- [ ] Run focused client tests and strict client Clippy. Check actual pinned sink API locally, and verify startup/asset cleanup without an output device using an actual unavailable backend.

## Task 3: Review, native validation and publication

- [ ] Review the complete diff for approved scope, worker/resource lifecycle, continuity, audio quality limits and authoritative independence; resolve findings with regression checks.
- [ ] Run `PROGRESSUS_RUN_CLIENT_TESTS=1 ./scripts/check-prototype-01.sh` with the shared Cargo target cache and bounded jobs.
- [ ] Build/run the native client and capture only its audio in an isolated sink for multiple transitions. Check pause, mute, settings, continuous bed, process/voice/asset counts and raw generation times. Clearly distinguish runtime evidence from exported previews.
- [ ] Update both audio specs and this plan with actual validation status. Commit each verified logical stage and push; fast-forward the main checkout only if its state still permits this, then verify remote HEAD.
- [ ] Deliver playable absolute-path examples, concise verification results, commit reference and any remaining subjective or platform limitations.
