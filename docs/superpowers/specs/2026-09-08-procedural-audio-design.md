# Procedural settlement audio

Date: 2026-09-08

Status: implemented and technically validated on 2026-09-08. Owner listening review of the generated material remains requested.

Historical version: the owner subsequently approved [continuous layered ambient](2026-09-08-continuous-ambient-design.md), which supersedes the music schedule and composition requirements below. Work observation and authoritative boundaries remain applicable.

## Goal

Give the existing settlement a quiet acoustic background and readable sounds of physical work. The owner selected locally generated acoustic ambient over orchestral music or externally generated AI tracks.

## Alternatives considered

1. Local procedural composition and synthesis: offline, reproducible, small distribution, controllable performance. Selected. Instrument realism is limited by the synthesis and must be judged by listening.
2. Pre-generated AI recordings: potentially richer timbres, but introduces an external production workflow and recording/licensing decisions. Not selected.
3. Recorded instrument stems with procedural arrangement: a useful later quality upgrade, but requires an authored sound library. Not needed for this first pass.

## Music

- Sparse plucked-string motifs, soft sustained harmonic support and restrained wooden percussion. No vocals or constant foreground melody.
- Play a newly composed approximately 120-second piece, then 20 seconds of silence, then a different piece. Work effects remain independent of this music silence.
- Adjacent pieces must change composition family, harmonic progression, melody, arrangement and instrumentation; merely transposing or ornamenting one looping phrase does not satisfy this requirement.
- Use several bounded composition families (for example open pastoral phrases, flowing harp-like figures, and spacious modal responses), with deterministic presentation-only variation inside each family. Return to a family only with a new composition seed.
- Generate one next piece on a worker task, retain at most current and next PCM assets, and never synthesize a full piece in a frame update. If generation is late, extend the silence rather than blocking a frame or replaying the old piece.
- Use 24 kHz stereo PCM16, about 11.52 MB per 120-second encoded piece. Measure backend/decoder memory separately. Fade into/out of each piece to avoid abrupt boundaries.
- Deliver at least three independently listenable music examples and an effect sampler, plus an example of the 120-second music / 20-second silence / changed music sequence.
- Music continues during simulation pause; it does not follow simulation acceleration. Exact musical phase is not saved.

## Work effects

Initial vocabulary:

- Harvest: short soft cut/rustle impact; material-specific tree/stone/berry variants may be used only when that source kind is already available from an explored snapshot.
- Construct: muted stone/wood taps.
- Craft: restrained workbench/tool strikes.
- Physical pickup/drop: small handling sound only when the carrier/location transition is actually observed.

Effects describe observed physical activity. A designation click is not a completed building or a finished harvest. A disappeared job alone does not prove completion: cancellation also removes jobs. The first pass does not need completion fanfares.

Derive work effects from detached `JobSnapshot` state/progress and worker positions at successfully observed authoritative ticks. Keep the previous observed tick/job progress in disposable client state. Do not invent an authoritative event log for audio.

Do not replay effects on an ordinary frame, repeated snapshot, camera pan, startup or save load. Loading resets the observation baseline even when the loaded tick equals the previous tick. Pausing suppresses new work effects; existing short tails may finish.

Cull work effects outside the visible camera area before allocating voices. Use modest horizontal panning within that area, rather than a new world-scale acoustic simulation. Bound work effects to eight simultaneous voices and four new voices per observed tick; drop excess cues rather than queueing an audible backlog. Reuse a small generated sample bank.

## Controls and failure behavior

- Add a localized Sound entry to the existing HUD/modal system.
- Provide separate Music and Effects levels, each with a zero/mute position. Defaults must leave work effects readable without dominating the scene.
- Settings are client session state initially. Do not add presentation settings to authoritative saves.
- If an output device is unavailable, continue playing silently with a diagnostic. Audio initialization must not make a valid headless simulation depend on an audio device.

## Boundaries

All synthesis, arrangement, audio resources, voice limits and settings belong to `progressus-client`. Continue using `progressus-app` snapshots; no Bevy dependency, audio clock or audio RNG enters `progressus-sim` or `progressus-worldgen`.

Use Bevy's audio integration with generated in-memory PCM/WAV assets. Check the locally pinned Bevy API and Linux audio build dependencies before selecting feature flags. Keep the client's direct dependencies constrained to Bevy and `progressus-app` as required by the existing boundary script.

Likely files: new client `audio.rs` for Bevy playback/observation, new client `audio_synthesis.rs` for pure sample generation, client module/runtime registration, localized HUD/modal controls and Cargo audio features. No changes to authoritative save format or gameplay timing are needed for audio.

## Acceptance

1. Generate preview WAVs for the music and each effect family; verify valid PCM, finite samples, non-silence, no clipping and bounded durations. The owner can listen to these independently of launching Bevy.
2. Test presentation-seed reproducibility and variation. This is a presentation guarantee within the supported build, not a cross-platform authoritative floating-point contract.
3. Test that repeated frames/snapshots, pause, load and camera movement do not duplicate work effects. Test voice admission limits and off-screen culling.
4. Compare authoritative save bytes for the same command/tick sequence with audio enabled and disabled; they must match.
5. Verify independent mute/level controls, graceful no-device behavior, the 120/20 music schedule, changed compositions, fades, pickup/drop and work sounds in the native client. Compilation alone is not listening validation.
6. Record generation wall time, steady-state frame/update cost and asset/voice counts. Retain raw observations; do not claim a performance improvement from this feature.

## Order of work

First fix and verify the audit's blocked-output and tick-remainder defects, keeping them in focused commits. Then implement the sample generator and previews, playback/observation, and controls. Run audio acceptance and the existing core/client gates. Stage B sleep/shelter starts after this pass, with its own need-priority and physical-rest design.

## Validation record

- Generated three complete 120-second 24 kHz stereo PCM16 compositions (`01-pastoral`, `02-harp-waltz`, and `03-modal-rest`), their 30-second excerpts, five three-variant effect families, an effect sampler, and a `120 s music → 20 s silence → changed 120 s music` WAV example. Every exported file was parsed as WAV, had finite non-silent PCM under the clipping limit, and had the expected duration.
- Unit tests cover score reproducibility and structural differences across composition families; PCM duration, fades, headroom and effects; observation baseline/load/pause behavior; cue culling and voice limits; and two matching real authoritative command/tick sequences with and without audio observation/synthesis. The sequences serialize to identical save bytes throughout.
- Native client validation ran against the active X11 and PipeWire sessions. The visual Sound modal showed independently adjustable Music and Effects levels. A 160-second isolated audio capture contained an 80-second tail of a running piece, 23 seconds of digital silence, and the beginning of the next generated piece. The capture began after the current piece had already started, hence the partial first piece.
- With its PulseAudio server environment deliberately pointed at a nonexistent socket, the native client remained alive for nine seconds and did not terminate. This checks graceful startup under that unavailable-server configuration; it does not prove behavior for every native backend or physical device failure.
- Backend/decoder memory and subjective listening quality remain platform- and owner-dependent. The generated preview files are the review artifact for timbre, mix and musical variety.
