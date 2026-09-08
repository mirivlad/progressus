# Continuous settlement ambient

Date: 2026-09-08

Status: approved on 2026-09-08; implemented and technically validated on 2026-09-09. Subjective listening assessment remains with the owner.

## Requested change

Replace the previous 120-second composition / 20-second silence schedule with continuous locally generated acoustic ambient. Individual layers may rest, but the background must not deliberately fall silent. The supplied space-ambient algorithm is a design reference, not a verified account of EVE Online's implementation.

Keep nature and settlement work as the sound vocabulary. Deliver listenable examples before judging the result finished.

## Approach

Use independently mixed layers, with bounded audio buffers prepared on a worker and overlapping playback transitions in Bevy. This reuses the current generated PCM integration while allowing layers to change independently.

Alternatives:

- Crossfade complete compositions: smallest code change, but retains the existing piece structure and couples the layers. Does not sufficiently address the requested generator.
- Synthesize every sample directly on the audio thread: flexible and low in buffer memory, but introduces a new realtime DSP path with stricter performance and underrun requirements. Unnecessary for this iteration.

The recommended buffered approach must not perform synthesis or wait for a worker in a frame update. Keep at most current and prepared-next material per continuous layer, plus outgoing voices during bounded transitions. If generation is late, retain an explicitly seamless safety bed and omit optional phrases until ready; do not introduce the old silent interval or accumulate queued phrases.

## Three layers

### Harmonic foundation

Quiet sustained string/air-like textures, restrained low frequencies, soft attacks and overlapping releases. Begin with open fifths, suspended harmony and warm Dorian/major colours rather than a permanently threatening minor drone.

Independent slow amplitude/timbre motion may use periods such as 17, 23 and 29 seconds. These periods delay repetition; they do not make a finite oscillator system mathematically non-repeating.

Choose compatible harmonic changes over approximately 45–90 seconds. Over several minutes, change register, voicing, instrument balance and phrase density. Keep common tones or fade conflicting voices before a new harmony; simultaneous unrelated random chords are not the intended result.

### Sparse acoustic phrases

Three to six connected notes from the current harmony/scale, with approximately 10–25 seconds between the end of one phrase and the start of another. Use a small melodic contour with stepwise movement and occasional resolved leaps rather than independently picking every note.

Soft string, harp-like and breath-like timbres; short restrained echoes and a diffuse tail. Phrase seeds and arrangement choices change over time. Prevent immediately repeated phrase contours. Long foreground melodies, constant arpeggios and a fixed waltz pulse are outside this direction.

### Environment and settlement textures

Nature: low-level filtered noise shaped as air movement and leaf-like rustling, with slow independent movement and restrained stereo width. These are atmospheric presentation textures; do not imply a new authoritative weather system. Water and animal calls are omitted until their presence can be grounded in existing visible world facts.

Industry at the current milestone means construction and workbench activity: wood, stone and tool sounds. Retain the existing observed work cues and modest spatial panning. Any longer work texture must be driven by actually visible, observed construction/crafting activity and fade away when that activity ends. Do not invent engines, machinery or an always-running workshop.

No danger, combat, electricity or future technology state is introduced. The available distinction is quiet surroundings versus an active settlement, using detached current snapshots only.

## Controls and boundaries

- Music level controls the harmonic foundation, phrases and decorative nature bed. Effects level controls actual work/handling sounds and any activity-driven work texture. Either category can be muted independently.
- Musical time continues during simulation pause. No new work sounds are triggered while paused; existing tails may finish. On load, reset observation as today and do not replay historic work.
- Preserve the existing work limits: eight simultaneous effect voices, at most four new work cues per observed tick, visibility culling before admission. Bound additional ambient voices and assets explicitly in the implementation and test those bounds over a long run.
- All arrangement state, audio clocks and seeds remain client-owned. Authoritative RNG, simulation, saves and dependency boundaries are unchanged.
- Retain graceful startup without an output device. Do not infer absence of an audio device solely from a PulseAudio environment override; check the backend actually used in the runtime test.

## Acceptance and examples

1. Export a four-minute quiet ambient example and a four-minute example with explicitly scripted demonstration work activity. Label scripted activity as a demo, not a capture of gameplay. Export isolated layers for listening if the mix needs adjustment.
2. Demonstrate multiple harmony/arrangement transitions without the previous full-background gaps, including a transition with melody silent while the foundation continues.
3. Validate finite PCM, headroom of the combined mix, smooth buffer boundaries and transitions, presentation-seed reproducibility, changing phrase/arrangement choices, bounded voice/asset counts and late-worker behavior.
4. Verify native playback across several transitions, separate level/mute controls, pause/load behavior and activity fading. A generated WAV alone does not prove runtime continuity.
5. Retain the existing real command/tick sequence comparison proving identical authoritative saves with and without audio observation/synthesis. Run focused client tests and existing required project gates.
6. Record raw generation times, asset/voice counts and native runtime observations. Distinguish technical checks from the owner's subjective listening assessment.

The earlier audio specification remains the historical record of the previous version. This specification supersedes its 120/20 schedule.

## Implementation and validation record — 2026-09-09

- Three independently prepared/mixed layers: 64-second harmonic foundation, 64-second non-looping melody, 43-second nature texture. Foundation/nature replacements start after 58/37 seconds with six-second linear crossfades; seamless source loops sustain the background if the next buffer is late. Outgoing voices fade only after the replacement sink is observed ready.
- Nature uses actual filtered random noise in wind/leaf bands with slow envelopes. Harmony uses compatible D Dorian voicings; melodic phrases change contour, length, register and timbre. Three phrases fit each melody buffer; internal rests remain 10–25 seconds and normal cross-buffer rests 12–15 seconds. A late worker may extend the melody's rest without silencing the background.
- Existing observed harvest/construction/crafting/pickup/drop cues provide the settlement activity layer. No fictional machinery or new gameplay state was added. Work voices have a fixed 0.08 mix gain before the Effects setting, reserving headroom for the eight-voice limit.
- Exported 23 WAV files, including four-minute quiet and explicitly scripted work demonstrations, 45-second excerpts, isolated layers and work samples. PCM metadata, duration, headroom and non-silence checks passed. Both four-minute mixes had peak 0.047913 and minimum one-second RMS after fade-in 0.003922. Scripted effect panning approximates placement; it is not an exact reproduction of the native spatial backend.
- `PROGRESSUS_RUN_CLIENT_TESTS=1 ./scripts/check-prototype-01.sh` passed: format, strict Clippy, 180 authoritative/app/headless/worldgen tests, 65 client-library tests plus one client CLI test, dependency boundaries and the three existing long-run headless scenarios. The real audio-observation command sequence preserves identical authoritative saves.
- Eight pure synthesis tests passed independently. Regressions include 100 adjacent melody-buffer boundaries, combined headroom, finite raw float PCM, loop boundaries and variation. An actual Bevy-system test exercises 40 transition cycles with a deliberately pending worker and simulated backend-start observations; assets and entities retire correctly. This test does not claim to reproduce physical output-device behavior.
- Native X11/PipeWire capture lasted 250.0007 seconds, covering four foundation, three melody and six nature replacements. Simulation was paused during the capture and ambient continued. No one-second bin after startup was silent; the longest near-zero stereo run after startup was 0.000125 seconds. Capture peak was 0.134979 with no clipping. Only the test process's own sink input was routed and recorded; the default system sink was not changed.
- Fifteen-second native samples showed three or four ambient voices and 21/22 total audio assets, including the 15 cached work sounds; counts returned after overlaps. Static maximum is five ambient voices and eight ambient assets, plus the work bank. Pending-task/asset/voice cleanup on unavailable output and volume/fade independence are covered by client tests.
- Native raw generation times included foundation index 0: 632.546065 ms, index 1: 494.501036 ms; melody index 0: 99.783216 ms; nature index 0: 136.931745 ms. Raw process-memory/CPU samples are retained separately and include renderer/allocator costs; they are not an audio-overhead measurement or a performance-improvement claim.
- An ALSA configuration pointing at unavailable hardware produced the actual backend warning `No audio device found`, followed by the client's silent-continuation diagnostic. The application remained alive until the explicit 12-second timeout.
- Independent source review found and verified the cross-buffer-rest fix; no actionable source findings remained. Music/Effects/mute and changing crossfade gain were verified through the real level-application system. A fresh native click-through of every Sound-modal control was not completed in this run; the UI itself is unchanged from the preceding audio implementation.

Local evidence and playable outputs: `target/continuous-ambient-20260908/` in the main checkout. It contains `generation.log`, `preview-verification.json`, `native-client.log`, `native-verification.json`, `native-process-samples.txt`, `no-device.log`, `final-gates.log`, previews and the native capture. These generated artifacts are not repository assets.

Recovery note: the former `/tmp` worktree disappeared between sessions before implementation was committed. The successful source patches were recovered from the session journal into the persistent `.worktrees/continuous-ambient` checkout, then reviewed and verified again. The approved design commit had already been pushed.

## Listening revision — 2026-09-09

The owner reported that the continuous background resembled a refrigerator hum. Isolated-layer and spectrum checks traced this to the foundation's uninterrupted D2/A2 fundamentals at approximately 73/110 Hz. Low-pass RMS below 130 Hz was 0.013840, exceeding the 0.011786 upper-band RMS.

The foundation now starts at D3, reduces its sustained gain and gives its four voices independent 17/23/29/31-second swells. This keeps the nature layer continuous while the harmonic voices breathe instead of holding an appliance-like low tone. The regenerated foundation measures 0.002659 below 130 Hz versus 0.005884 above it; overall foundation RMS fell from 0.018179 to 0.006457. A regression checks all four voicings for restrained low-frequency energy and meaningful four-second energy variation. Subjective acceptance depends on the owner's review of the regenerated example.
