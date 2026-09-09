# Structured procedural settlement score

Date: 2026-09-09

Status: approved on 2026-09-09. Stage 1 motif audition implemented and technically validated on 2026-09-10; owner listening review remains pending.

## Problem

The current continuous ambient implementation is technically continuous but does not form a coherent piece of music. A 64-second harmonic buffer, a 64-second melodic buffer and a 43-second nature buffer are generated and replaced independently. Sustained oscillator voices provide continuity while phrases have no shared harmonic or dramatic plan. The owner first heard the low foundation as an appliance-like hum; raising and modulating it removed that specific low-frequency defect but did not make the result resemble the supplied musical reference.

The supplied reference, Jón Hallur's five-minute `Wonderlands of the Mind`, is a fixed soundtrack composition rather than evidence of a real-time generator. Its useful lesson is therefore the presence of an intentional musical journey, not a particular space-themed timbre or melody. Progressus must not copy its melody, recording or arrangement.

## Goal

Replace the independent-loop music model with a client-owned procedural score that:

- plays continuously without scheduled silent gaps;
- develops over coherent five-to-seven-minute spans;
- sounds neutral, contemplative, natural and lightly acoustic;
- avoids cosmic drones, folk stylization and constant mechanical hum;
- produces original material specifically for Progressus;
- varies across spans without becoming random or directionless;
- keeps existing physical work effects separate and readable;
- provides short listenable examples before replacing runtime music.

## Approaches considered

### Pure unconstrained real-time synthesis

Generate every note and sound from independent random processes. This minimizes authored data but reproduces the main weakness of the current implementation: local variation without musical direction. Not selected.

### Fixed rendered compositions

Author and render several complete tracks. This gives the greatest control over musical quality, but a small library will repeat and does not meet the preferred procedural direction. Not selected for the main system, though fixed renders remain useful as review artifacts.

### Structured procedural score

Use a small set of original motif seeds, a constrained harmonic graph, phrase-transition rules and an authored macro-form. Render the chosen score locally with procedural acoustic instruments and textures. This combines musical direction with bounded variation and is selected.

The design follows established generative-music practice: pitch choices satisfy scale, harmony and next-note constraints together; phrases transition through an allowed state graph; game adaptation combines sequential musical segments with changing instrumental layers. References:

- [Wotja Music Engine rules](https://intermorphic.com/archive/wotja/22/guide/)
- [SuperCollider phrase patterns](https://doc.sccode.org/Tutorials/A-Practical-Guide/PG_04_Words_to_Phrases.html)
- [Wwise interactive music structure](https://www.audiokinetic.com/en/public-library/2024.1.8_8893/?id=creating_interactive_music&source=Help)
- [Cycling '74 granular synthesis tutorial](https://docs.cycling74.com/legacy/max7/tutorials/11_polychapter02)

## Musical hierarchy

Randomness operates only inside decisions made by the level above it.

1. A **score span** lasts five to seven minutes and owns the global key, mode, energy curve and deterministic presentation seed.
2. A span contains four to six **sections** lasting roughly 45 to 90 seconds: settling, opening, development, thinning and transition. These are roles rather than a fixed repeated order.
3. Each section selects a route through a small **harmonic graph**. Chord changes normally last 12 to 32 seconds and use common tones or short voice movement.
4. Harmony makes one or more **phrase states** eligible: opening, continuation, answer, cadence or rest.
5. A phrase transforms one selected **motif seed** while respecting the current harmony, register and recent-history exclusions.
6. **Orchestration** chooses which voices present the harmony and phrase. Instruments may rest completely; continuity comes from overlapping releases, room response and a very quiet environment texture rather than a permanent oscillator.

The next span is prepared before the current transition section. It either retains the tonal centre or moves through a shared chord. An 8-to-16-second musical overlap prevents a deliberate full-background gap.

## Neutral motif set

The first audition set is neutral and contemplative. It deliberately avoids recognizable folk scales, dance rhythms, heroic cadences, danger cues and the melodic material of the reference.

A motif is compact data rather than recorded music:

```text
scale-degree intervals + relative durations + contour role + ending tendency
```

The candidate generator creates 30 to 50 motifs under these rules:

- three to seven notes;
- mostly stepwise movement, with at most one leap larger than a third;
- total range no greater than a sixth;
- no immediate three-note repetition;
- no fixed pulse repeated across every phrase;
- endings classified as open, resting or resolving;
- Dorian, major-pentatonic and suspended modal material, with the mode chosen per score span;
- no motif is accepted solely because it satisfies the numeric rules.

Automated checks remove duplicates, close transpositions, excessive leaps, repeated contours and unresolved phrases presented as cadences. The first review artifact contains 8 to 12 surviving motifs, each rendered twice: once on a neutral plucked/wood voice and once in a short harmonic context. The owner selects four to six seeds. Runtime transformations may transpose within the active mode, omit a non-structural note, lengthen durations, change octave or assign another approved voice. They may not reorder notes arbitrarily or mutate a motif until its contour is unrecognizable.

## Harmony and repetition control

The harmony generator uses an explicit directed graph rather than choosing any chord from one scale. Each edge carries a weight and a role such as continue, open, darken, brighten or resolve. A section specifies its desired start and end roles, then chooses a bounded path between them.

The score state remembers:

- the last eight motif transformations;
- the last six harmonic transitions;
- the last four instrument/register combinations;
- the preceding section and score-span shapes.

Immediate exact repetition is forbidden. Recently used choices have reduced weight. History changes probabilities but never overrides harmonic validity. A fallback deterministic path always exists so restrictive history cannot stall generation.

## Procedural acoustic palette

The first version remains locally generated and adds no licensed music assets.

- Soft strings use a damped plucked-string model with a low transient and restrained upper harmonics.
- Wood voices use a short excitation through a small modal resonator bank.
- Air voices combine a tonal component with breath noise and slow natural pitch movement; they appear sparsely.
- Muted metal uses inharmonic resonators with a short excitation and long quiet decay. It is an occasional colour, not an industrial drone.
- Environment texture uses sparse shaped wind/leaf events with genuine quiet intervals. It does not contain a constant low tone.

All voices feed shared early-reflection, delay and diffuse-tail buses. The reverberation return is high-pass filtered to avoid accumulated rumble. Per-voice gain remains fixed-headroom; a final limiter protects exceptional overlaps without normalizing every generated buffer to the same loudness.

Existing harvest, construction, crafting, pickup and drop effects remain on the Effects control and continue to represent observed physical activity. The music may use a restrained wood or metal colour when visible settlement activity is sustained, but it does not turn individual work events into a beat and introduces no fictional machinery.

## Components and boundaries

All new state remains in `progressus-client`:

- `score` owns deterministic span/section/harmony planning and recent musical history;
- `motif` owns candidate seeds, validation and permitted transformations;
- `instruments` renders bounded procedural voices;
- `mix` owns shared effects, headroom and rendered score segments;
- the existing playback layer prepares future material off the frame path and retires old assets after transitions.

The score uses a presentation seed and wall-clock musical playback. It does not consume authoritative RNG, affect simulation, enter save data or require Bevy in headless tests. Game activity reaches music only through disposable client observations already allowed for audio presentation.

Generation failure keeps the current compatible room/environment tail and retries preparation without blocking a frame. It does not replay a foreground phrase or insert a queued burst later. Memory remains bounded to current, outgoing and prepared-next musical material plus the fixed effect bank.

## Review stages and acceptance

Implementation is divided by listening gates because signal-level tests cannot establish musical quality.

### Stage 1: motif audition

- Export 8 to 12 neutral motif candidates in isolated and contextual versions.
- Verify deterministic generation, uniqueness constraints, valid WAV data, finite PCM and headroom.
- Do not replace runtime music until the owner selects four to six seeds.

### Stage 2: palette and form audition

- Export three 90-second examples using the selected motifs: sparse daylight, gently active settlement and reflective evening.
- The labels describe musical density only and do not introduce authoritative time-of-day or weather.
- Each example must contain at least two coherent harmonic changes, one phrase answer, one genuine instrumental rest and no continuous low oscillator.
- Record raw generation time, peak, RMS by frequency band and longest near-constant low-frequency run. These measurements detect defects but do not substitute for listening approval.

### Stage 3: continuous score and integration

- Export one continuous five-to-seven-minute span plus its transition into a different span.
- Verify phrase/harmony history, musical transition continuity, bounded generation work, voices and assets.
- Replace the current runtime generator only after owner approval of the audition material.
- Re-run client tests, strict Clippy, dependency-boundary checks, authoritative save-equivalence checks and native playback across a span transition.
- Confirm Music/Effects controls, pause/load behavior and graceful no-device startup in the native client.

The current ambient remains a historical implementation until Stage 3 is accepted. Stage B sleep and shelter remains the next authoritative gameplay milestone and is unaffected by this presentation-only revision.

## Stage 1 validation record — 2026-09-10

- The isolated generator deterministically fills a 48-candidate pool with exactly 16 Open, 16 Resting and 16 Resolving motifs, then selects ten structurally separated auditions. Every motif has three to seven notes, a maximum sixth range, at most one three-step leap, a nonuniform relative pulse and a role-compatible final motion.
- Ten 12-second isolated plucked-wood auditions, ten 18-second harmonic-context auditions and two samplers were exported under `target/neutral-motif-audition-20260909/`. All 22 WAV files are 24 kHz stereo PCM16 with nonzero RMS, fixed headroom and no clipped samples. Peaks range from 0.071413 to 0.108585.
- Six standalone generator tests pass. They cover deterministic candidate generation, structural validity, transposition-equivalent uniqueness, per-role capacity across the fixed audition seed and a former sparse-role regression seed, shortlist diversity, role-directed final motion, deterministic WAV output, dynamic rests and restrained low-frequency energy.
- Strict client Clippy, all 66 client library tests and both core dependency-boundary checks pass. The new source module is included only by the standalone preview example; `lib.rs`, runtime playback, authoritative simulation and saves are unchanged.
- Independent review found sparse resolving-role capacity and false repeated-tonic resolutions. Both were reproduced with failing tests and fixed before publication. Subjective musical and timbral acceptance depends on the owner's review of the exported samplers.
