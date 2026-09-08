# Continuous settlement ambient

Date: 2026-09-08

Status: approved by the owner on 2026-09-08; implementation in progress.

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

The earlier audio specification remains the historical record of the previous version. This specification supersedes its 120/20 schedule. Do not claim the continuous generator exists until playback and examples have been verified.
