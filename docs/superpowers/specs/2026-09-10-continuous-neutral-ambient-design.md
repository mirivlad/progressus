# Continuous Neutral Ambient Audition Design

**Status:** Approved direction; written specification awaiting owner review  
**Date:** 2026-09-10  
**Scope:** Standalone presentation-only generator and a three-minute listening export

## Purpose

Extend the accepted alternating piano-and-strings audition into an effectively nonrepeating ambient stream. The background must stay behind the foreground, spatial effects must appear only on selected phrases, and the result must remain suitable for later client integration without affecting authoritative simulation.

This stage produces a review recording only. It does not register the generator in `progressus-client` runtime audio.

## Accepted listening baseline

The accepted baseline is `03-alternating.wav` from the neutral timbre comparison:

- complete phrases alternate between felt piano and bowed strings;
- the shared mid-register background remains continuous between phrases;
- the Western diatonic melodic language and restrained phrase density are preserved;
- no constant low oscillator is introduced.

The previous 75-second file is a reference, not a loop or a source asset for the new output.

## Level balance

The dry background bus is multiplied by `0.8` relative to the accepted comparison. This is the concrete interpretation of lowering the background by `0.2`.

While a foreground phrase is active, a smooth gain envelope reduces the background by a further 8-12 percent. The envelope begins before the first note, releases after the foreground tail, and has no abrupt gain steps. Ducking exists only to keep piano and strings in front; it must not create audible pumping.

Foreground gain remains at the accepted level unless final peak protection requires one shared reduction across the complete mix. No compressor or limiter may change the relative A/B timbre balance.

## Unbounded musical timeline

An `AmbientTimeline` owns presentation-only state:

- a master seed;
- absolute musical time;
- the current harmony and energy phase;
- the next foreground timbre;
- recent motif fingerprints;
- independent deterministic counters for harmony, phrase, background and effect decisions.

The timeline can extend to any requested absolute time. It generates finite look-ahead plans, so neither memory use nor generation work grows with the age of the session. The three-minute export uses the same extension API that later client playback can call repeatedly.

Randomness is deterministic for a given presentation seed and event index. It never consumes or changes authoritative simulation RNG.

## Harmony and background

Harmony advances in overlapping regions lasting approximately 13-19 seconds. The next chord is chosen from reversible C-major/A-minor functional transitions, with these constraints:

- no immediate repetition of the same voicing;
- close-position voices remain at or above A3;
- strong tonic cadences are separated by at least three regions;
- adjacent regions share or move voices mostly by step;
- density follows slow energy arcs whose lengths do not align with phrase spacing.

Each region is rendered from the accepted air/ensemble background family. Independent envelope and movement periods avoid a stationary texture. Regions overlap in absolute time so the background has no silent one-second window or hard boundary.

## Foreground phrase grammar

Foreground phrases occur after variable gaps of approximately 10-22 seconds. Each phrase contains four to six notes and follows the established neutral grammar:

- pitches belong to the active C-major/A-minor collection;
- chord tones anchor phrase starts and endings;
- passing tones move mainly by step;
- leaps are no wider than a fourth, with at most one fourth per phrase;
- occasional leading-tone resolution is retained;
- complete phrases alternate felt piano, bowed strings, felt piano, bowed strings, and so on.

A motif fingerprint records normalized intervals, rhythm proportions, ending role and timbre. A candidate is rejected when it exactly matches any of the previous 12 fingerprints. Near matches are allowed only when both rhythm and ending role change. This provides practical nonrepetition without claiming a mathematically infinite sequence that can never repeat.

## Spatial effects

The existing restrained shared room remains the base acoustic space. Additional effects use per-phrase send levels and never process the background bus:

- some phrases remain dry apart from the shared room;
- selected phrases receive a small extra reverb send with a 2.0-4.5 second tail;
- more rarely, a phrase receives a stereo delay of 280-620 ms with bounded feedback and two to four audible repeats;
- effect selection, send level, delay time and stereo position derive from the phrase event seed;
- two consecutive phrases may not receive the same additional effect treatment.

Effect tails overlap subsequent background naturally. Their wet peak must remain below the foreground dry peak, keeping the instrument intelligible and in front.

## Rendering boundaries

The score uses absolute event times. The renderer accepts a finite time window plus an overlap margin large enough for attacks and effect tails. Adjacent windows use deterministic overlap-add so a later streaming client can request bounded chunks without restarting oscillators, random sources or reverb tails.

The standalone exporter renders a 180-second demonstration of the alternating stream. It writes the WAV, manifest, generation log and verification data outside Cargo `target`, under a new dated review directory.

After the replacement passes verification, the previous neutral-timbre review directory and temporary build products are deleted. The accepted source generator remains versioned until the owner decides whether to integrate it into runtime audio.

## Verification

Focused tests must prove:

- identical seed and duration produce identical score plans and WAV bytes;
- different seeds produce different plans;
- the timeline extends beyond 75 and 180 seconds without cycling its first section;
- recent motif fingerprints satisfy the 12-phrase exclusion rule;
- piano and strings alternate by complete phrase;
- effect decisions include dry, extra-reverb and delay treatments in the three-minute fixture, with no identical treatment on consecutive phrases;
- the un-ducked background samples equal the accepted background model multiplied by `0.8`;
- ducking envelopes are smooth and remain within the additional 8-12 percent reduction;
- no one-second output window after startup is silent;
- matched low/high filters show that energy below 130 Hz does not dominate;
- adjacent render windows join without a discontinuity;
- decoded stereo PCM16 lasts exactly 180 seconds, contains only finite samples, has no clipped samples and peaks below `0.82`.

Project checks remain `cargo fmt --all --check`, strict client `clippy`, the client library tests and the core dependency-boundary script. A source diff must confirm that runtime audio, simulation, saves and `lib.rs` remain unchanged.

## Listening acceptance

Technical checks establish continuity, balance bounds and file safety. Final acceptance remains a listening decision by the owner:

- the background stays behind both instruments throughout the recording;
- the ducking is not heard as pumping;
- reverb and delay add occasional depth without washing out phrases;
- the three-minute progression does not sound like a repeated 75-second block;
- the musical language retains the accepted neutral character.
