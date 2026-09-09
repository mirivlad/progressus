//! Deterministic, presentation-only score generation for timbre auditions.

#[path = "ambient_timeline.rs"]
pub mod ambient_timeline;

use ambient_timeline::{AmbientPlan, PhraseEffect, PhrasePlan, PhraseTimbre, collect_plan};
use std::f32::consts::{PI, TAU};

pub const SAMPLE_RATE: u32 = 24_000;
pub const COMPARISON_SECONDS: usize = 75;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteEvent {
    pub start: f32,
    pub midi: f32,
    pub duration: f32,
}

#[derive(Clone, Copy)]
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / 16_777_216.0
    }
}

fn midi_hz(midi: f32) -> f32 {
    440.0 * 2.0_f32.powf((midi - 69.0) / 12.0)
}

fn stateless_unit(seed: u64, sample_index: u64) -> f32 {
    let mut value = seed ^ sample_index.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((value ^ (value >> 31)) >> 40) as f32 / 16_777_216.0
}

fn felt_string_frequencies(midi: f32) -> [f32; 3] {
    const CENTS: [f32; 3] = [-3.0, 0.0, 3.5];
    let base = midi_hz(midi);
    CENTS.map(|cents| base * 2.0_f32.powf(cents / 1_200.0))
}

fn pan_gains(pan: f32) -> (f32, f32) {
    (((1.0 - pan) * 0.5).sqrt(), ((1.0 + pan) * 0.5).sqrt())
}

fn sample_bounds(start: f32, duration: f32, total_frames: usize) -> (usize, usize, i64) {
    let start_sample = (start * SAMPLE_RATE as f32).round() as i64;
    let duration_samples = (duration * SAMPLE_RATE as f32).round() as i64;
    let first = start_sample.max(0).min(total_frames as i64) as usize;
    let last = (start_sample + duration_samples)
        .max(0)
        .min(total_frames as i64) as usize;
    (first, last, start_sample)
}

pub fn phrase_events(seed: u64) -> Vec<NoteEvent> {
    const PHRASES: [&[f32]; 5] = [
        &[64.0, 65.0, 67.0, 64.0, 60.0],
        &[69.0, 67.0, 65.0, 64.0, 69.0],
        &[65.0, 67.0, 69.0, 67.0, 65.0],
        &[67.0, 69.0, 71.0, 74.0, 71.0],
        &[67.0, 69.0, 71.0, 72.0],
    ];
    const STARTS: [f32; 5] = [6.5, 20.5, 35.0, 49.5, 64.0];
    const DURATIONS: [f32; 4] = [0.85, 1.1, 1.35, 1.6];

    let mut rng = Rng(seed ^ 0x5343_4f52_455f_3031);
    let mut events = Vec::with_capacity(24);
    for (phrase_index, pitches) in PHRASES.iter().enumerate() {
        let mut cursor = STARTS[phrase_index] + (rng.unit() - 0.5) * 0.3;
        for (note_index, midi) in pitches.iter().enumerate() {
            let duration = DURATIONS[(rng.next() as usize + note_index) % DURATIONS.len()];
            events.push(NoteEvent {
                start: cursor,
                midi: *midi,
                duration,
            });
            cursor += duration * 0.72 + 0.22 + rng.unit() * 0.12;
        }
    }
    events
}

fn add_background_voice(
    pcm: &mut [f32],
    start: f32,
    duration: f32,
    midi: f32,
    gain: f32,
    pan: f32,
    seed: u64,
) {
    let (first, last, start_sample) = sample_bounds(start, duration, pcm.len() / 2);
    let hz = midi_hz(midi);
    let (left, right) = pan_gains(pan);
    let mut rng = Rng(seed);
    let phase = rng.unit() * TAU;
    let slow_phase = rng.unit() * TAU;
    let slow_period = 13.0 + rng.unit() * 18.0;
    let mut breath = 0.0;
    for frame in first..last {
        let time = (frame as i64 - start_sample) as f32 / SAMPLE_RATE as f32;
        let attack = (time / 4.5).clamp(0.0, 1.0);
        let release = ((duration - time) / 5.5).clamp(0.0, 1.0);
        let envelope = attack * attack * release * release;
        let movement = 0.78 + 0.22 * (TAU * time / slow_period + slow_phase).sin();
        let fundamental = (TAU * hz * time + phase).sin();
        let second = (TAU * hz * 2.003 * time + phase * 0.61).sin();
        let third = (TAU * hz * 3.997 * time + phase * 1.17).sin();
        let sample_index = (time * SAMPLE_RATE as f32).max(0.0) as u64;
        let white = stateless_unit(seed ^ 0x4252_4541_5448, sample_index) * 2.0 - 1.0;
        breath += (white - breath) * 0.075;
        let sample = (fundamental * 0.72 + second * 0.18 + third * 0.06 + breath * 0.04)
            * envelope
            * movement
            * gain;
        pcm[frame * 2] += sample * left;
        pcm[frame * 2 + 1] += sample * right;
    }
}

pub fn background_pcm(seed: u64) -> Vec<f32> {
    const CHORDS: [[f32; 3]; 5] = [
        [60.0, 64.0, 67.0],
        [60.0, 64.0, 69.0],
        [60.0, 65.0, 69.0],
        [62.0, 67.0, 71.0],
        [60.0, 64.0, 67.0],
    ];
    const GAINS: [f32; 5] = [0.42, 0.72, 0.5, 0.82, 0.46];
    const PANS: [f32; 3] = [-0.42, 0.08, 0.48];

    let mut pcm = vec![0.0; COMPARISON_SECONDS * SAMPLE_RATE as usize * 2];
    for (region, chord) in CHORDS.iter().enumerate() {
        let start = region as f32 * 15.0 - if region == 0 { 0.0 } else { 4.5 };
        for (voice, midi) in chord.iter().enumerate() {
            add_background_voice(
                &mut pcm,
                start,
                21.0,
                *midi,
                GAINS[region] * 0.075,
                PANS[voice],
                seed ^ ((region * 7 + voice) as u64).wrapping_mul(0x8f3d_9a75),
            );
        }
    }

    // Remove accumulated DC without introducing a stationary bass layer.
    for channel in 0..2 {
        let mut previous_input = 0.0;
        let mut previous_output = 0.0;
        for frame in 0..COMPARISON_SECONDS * SAMPLE_RATE as usize {
            let index = frame * 2 + channel;
            let input = pcm[index];
            let output = input - previous_input + 0.995 * previous_output;
            pcm[index] = output;
            previous_input = input;
            previous_output = output;
        }
    }
    pcm
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForegroundTimbre {
    FeltPiano,
    BowedStrings,
    Alternating,
}

impl ForegroundTimbre {
    pub const fn filename(self) -> &'static str {
        match self {
            Self::FeltPiano => "01-felt-piano.wav",
            Self::BowedStrings => "02-bowed-strings.wav",
            Self::Alternating => "03-alternating.wav",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::FeltPiano => "felt piano",
            Self::BowedStrings => "bowed strings",
            Self::Alternating => "alternating phrases",
        }
    }
}

fn add_felt_piano(pcm: &mut [f32], event: NoteEvent, pan: f32, seed: u64) {
    let tail = event.duration + 4.2;
    let (first, last, start_sample) = sample_bounds(event.start, tail, pcm.len() / 2);
    let frequencies = felt_string_frequencies(event.midi);
    let (left, right) = pan_gains(pan);
    let mut rng = Rng(seed);
    let phases = [rng.unit() * TAU, rng.unit() * TAU, rng.unit() * TAU];
    for frame in first..last {
        let time = (frame as i64 - start_sample) as f32 / SAMPLE_RATE as f32;
        let attack = (time / 0.055).clamp(0.0, 1.0).powi(2);
        let fundamental_decay = (-time / 3.5).exp();
        let upper_decay = (-time / 1.15).exp();
        let mut body = 0.0;
        for string in 0..3 {
            let phase = TAU * frequencies[string] * time + phases[string];
            body += phase.sin() * 0.72 * fundamental_decay
                + (phase * 2.002).sin() * 0.18 * upper_decay
                + (phase * 3.006).sin() * 0.07 * upper_decay;
        }
        body /= 3.0;
        let soundboard = (TAU * frequencies[1] * 0.501 * time + phases[1] * 0.3).sin()
            * 0.03
            * fundamental_decay;
        let sample_index = (time * SAMPLE_RATE as f32).max(0.0) as u64;
        let hammer = (stateless_unit(seed ^ 0x4841_4d4d_4552, sample_index) * 2.0 - 1.0)
            * (-time / 0.06).exp()
            * 0.025;
        let sample = (body + soundboard + hammer) * attack * 0.15;
        pcm[frame * 2] += sample * left;
        pcm[frame * 2 + 1] += sample * right;
    }
}

fn add_bowed_strings(pcm: &mut [f32], event: NoteEvent, pan: f32, seed: u64) {
    let tail = event.duration + 3.2;
    let (first, last, start_sample) = sample_bounds(event.start, tail, pcm.len() / 2);
    let base_hz = midi_hz(event.midi);
    let (left, right) = pan_gains(pan);
    let mut rng = Rng(seed);
    let phases = [rng.unit() * TAU, rng.unit() * TAU, rng.unit() * TAU];
    let cents = [-4.0_f32, 0.5, 3.5];
    let vibrato_rate = 4.6 + rng.unit() * 0.5;
    let mut bow_noise = 0.0;
    for frame in first..last {
        let time = (frame as i64 - start_sample) as f32 / SAMPLE_RATE as f32;
        let attack = (time / 0.48).clamp(0.0, 1.0);
        let release = ((tail - time) / 2.7).clamp(0.0, 1.0).powi(2);
        let vibrato = 0.42 * (TAU * vibrato_rate * time).sin();
        let mut ensemble = 0.0;
        for voice in 0..3 {
            let hz = base_hz * 2.0_f32.powf(cents[voice] / 1_200.0);
            let phase = TAU * hz * time + vibrato + phases[voice];
            ensemble += phase.sin() * 0.68
                + (phase * 2.0).sin() * 0.2
                + (phase * 3.0).sin() * 0.08
                + (phase * 4.0).sin() * 0.04;
        }
        let sample_index = (time * SAMPLE_RATE as f32).max(0.0) as u64;
        let white = stateless_unit(seed ^ 0x424f_575f_4e4f_4953, sample_index) * 2.0 - 1.0;
        bow_noise += (white - bow_noise) * 0.16;
        let sample = (ensemble / 3.0 + bow_noise * 0.025) * attack * release * 0.115;
        pcm[frame * 2] += sample * left;
        pcm[frame * 2 + 1] += sample * right;
    }
}

fn foreground_pcm(seed: u64, timbre: ForegroundTimbre) -> Vec<f32> {
    let events = phrase_events(seed);
    let mut pcm = vec![0.0; COMPARISON_SECONDS * SAMPLE_RATE as usize * 2];
    let mut phrase_index = 0;
    for (event_index, event) in events.iter().copied().enumerate() {
        if event_index > 0 {
            let previous = events[event_index - 1];
            if event.start - (previous.start + previous.duration) > 2.0 {
                phrase_index += 1;
            }
        }
        let selected = match timbre {
            ForegroundTimbre::FeltPiano => ForegroundTimbre::FeltPiano,
            ForegroundTimbre::BowedStrings => ForegroundTimbre::BowedStrings,
            ForegroundTimbre::Alternating if phrase_index % 2 == 0 => ForegroundTimbre::FeltPiano,
            ForegroundTimbre::Alternating => ForegroundTimbre::BowedStrings,
        };
        let pan = -0.22 + (event_index % 4) as f32 * 0.14;
        let event_seed = seed ^ (event_index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95);
        match selected {
            ForegroundTimbre::FeltPiano => add_felt_piano(&mut pcm, event, pan, event_seed),
            ForegroundTimbre::BowedStrings => add_bowed_strings(&mut pcm, event, pan, event_seed),
            ForegroundTimbre::Alternating => unreachable!(),
        }
    }
    pcm
}

fn comparison_layers(seed: u64, timbre: ForegroundTimbre) -> (Vec<f32>, Vec<f32>) {
    (background_pcm(seed), foreground_pcm(seed, timbre))
}

fn apply_room(pcm: &mut [f32]) {
    let first_delay = (SAMPLE_RATE as f32 * 0.23) as usize * 2;
    let second_delay = (SAMPLE_RATE as f32 * 0.41) as usize * 2;
    for index in first_delay..pcm.len() {
        pcm[index] += pcm[index - first_delay] * 0.13;
        if index >= second_delay {
            pcm[index] += pcm[index - second_delay] * 0.07;
        }
    }
    for channel in 0..2 {
        let mut previous_input = 0.0;
        let mut previous_output = 0.0;
        for frame in 0..pcm.len() / 2 {
            let index = frame * 2 + channel;
            let input = pcm[index];
            let output = input - previous_input + 0.997 * previous_output;
            pcm[index] = output;
            previous_input = input;
            previous_output = output;
        }
    }
}

fn encode_wav(pcm: &[f32]) -> Vec<u8> {
    let data_bytes = u32::try_from(pcm.len() * 2).expect("comparison PCM fits WAV");
    let mut wav = Vec::with_capacity(pcm.len() * 2 + 44);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(data_bytes + 36).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 4).to_le_bytes());
    wav.extend_from_slice(&4_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());
    for sample in pcm {
        let encoded = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        wav.extend_from_slice(&encoded.to_le_bytes());
    }
    wav
}

pub fn comparison_wav(seed: u64, timbre: ForegroundTimbre) -> Vec<u8> {
    let (background, foreground) = comparison_layers(seed, timbre);
    let mut mixed = background
        .iter()
        .zip(foreground)
        .map(|(background, foreground)| background + foreground)
        .collect::<Vec<_>>();
    apply_room(&mut mixed);
    encode_wav(&mixed)
}

pub struct RenderedLayers {
    pub unscaled_background: Vec<f32>,
    pub background: Vec<f32>,
    pub foreground: Vec<f32>,
    pub wet: Vec<f32>,
}

fn continuous_background(plan: &AmbientPlan, seconds: usize) -> Vec<f32> {
    let mut pcm = vec![0.0; seconds * SAMPLE_RATE as usize * 2];
    for region in &plan.regions {
        const PANS: [f32; 3] = [-0.42, 0.08, 0.48];
        for (voice, midi) in region.chord.iter().enumerate() {
            add_background_voice(
                &mut pcm,
                region.start,
                region.duration,
                *midi,
                region.gain * 0.075,
                PANS[voice],
                0x4241_434b_4752_4f55 ^ region.index.wrapping_mul(7).wrapping_add(voice as u64),
            );
        }
    }
    for channel in 0..2 {
        let mut previous_input = 0.0;
        let mut previous_output = 0.0;
        for frame in 0..seconds * SAMPLE_RATE as usize {
            let index = frame * 2 + channel;
            let input = pcm[index];
            let output = input - previous_input + 0.995 * previous_output;
            pcm[index] = output;
            previous_input = input;
            previous_output = output;
        }
    }
    pcm
}

fn smoothstep_cosine(value: f32) -> f32 {
    0.5 - 0.5 * (PI * value.clamp(0.0, 1.0)).cos()
}

fn background_duck_gain(time: f32, phrases: &[PhrasePlan]) -> f32 {
    let mut gain: f32 = 1.0;
    for phrase in phrases {
        let first = phrase.notes[0].start;
        let last = phrase.notes.last().unwrap();
        let end = last.start + last.duration + 2.8;
        let depth = 0.08 + (phrase.index % 5) as f32 * 0.01;
        let phrase_gain = if (first - 0.8..first).contains(&time) {
            1.0 - depth * smoothstep_cosine((time - first + 0.8) / 0.8)
        } else if (first..=end).contains(&time) {
            1.0 - depth
        } else if (end..end + 1.2).contains(&time) {
            1.0 - depth * (1.0 - smoothstep_cosine((time - end) / 1.2))
        } else {
            1.0
        };
        gain = gain.min(phrase_gain);
    }
    gain
}

fn background_duck_envelope(frames: usize, phrases: &[PhrasePlan]) -> Vec<f32> {
    let mut envelope = vec![1.0_f32; frames];
    for phrase in phrases {
        let first = phrase.notes[0].start;
        let last = phrase.notes.last().unwrap();
        let end = last.start + last.duration + 2.8;
        let first_frame = ((first - 0.8).max(0.0) * SAMPLE_RATE as f32) as usize;
        let last_frame = (((end + 1.2) * SAMPLE_RATE as f32).ceil() as usize).min(frames);
        for (frame, gain) in envelope
            .iter_mut()
            .enumerate()
            .take(last_frame)
            .skip(first_frame)
        {
            let time = frame as f32 / SAMPLE_RATE as f32;
            *gain = gain.min(background_duck_gain(time, std::slice::from_ref(phrase)));
        }
    }
    envelope
}

fn render_phrase_dry(pcm: &mut [f32], phrase: &PhrasePlan, seed: u64) {
    for (note_index, note) in phrase.notes.iter().enumerate() {
        let event = NoteEvent {
            start: note.start,
            midi: note.midi,
            duration: note.duration,
        };
        let pan = -0.22 + (note_index % 4) as f32 * 0.14;
        let note_seed = seed ^ phrase.index.wrapping_mul(0xd134_2543_de82_ef95) ^ note_index as u64;
        match phrase.timbre {
            PhraseTimbre::FeltPiano => add_felt_piano(pcm, event, pan, note_seed),
            PhraseTimbre::BowedStrings => add_bowed_strings(pcm, event, pan, note_seed),
        }
    }
}

fn add_reverb_send(wet: &mut [f32], dry: &[f32], phrase: &PhrasePlan, send: f32, tail: f32) {
    let first_frame = (phrase.notes[0].start * SAMPLE_RATE as f32) as usize;
    let last = phrase.notes.last().unwrap();
    let source_end =
        (((last.start + last.duration) * SAMPLE_RATE as f32) as usize).min(dry.len() / 2);
    let repeats = (tail / 0.31).ceil() as usize;
    for repeat in 1..=repeats {
        let left_delay = ((0.17 + repeat as f32 * 0.31) * SAMPLE_RATE as f32) as usize;
        let right_delay = ((0.21 + repeat as f32 * 0.293) * SAMPLE_RATE as f32) as usize;
        let gain = send * 0.58_f32.powi(repeat as i32);
        for frame in first_frame..source_end {
            let left_destination = frame + left_delay;
            if left_destination < wet.len() / 2 {
                wet[left_destination * 2] += dry[frame * 2] * gain;
            }
            let right_destination = frame + right_delay;
            if right_destination < wet.len() / 2 {
                wet[right_destination * 2 + 1] += dry[frame * 2 + 1] * gain;
            }
        }
    }
}

fn add_delay_send(wet: &mut [f32], dry: &[f32], phrase: &PhrasePlan, effect: PhraseEffect) {
    let PhraseEffect::Delay {
        send,
        seconds,
        feedback,
        repeats,
        pan,
    } = effect
    else {
        unreachable!("delay renderer requires a delay plan");
    };
    let first_frame = (phrase.notes[0].start * SAMPLE_RATE as f32) as usize;
    let last = phrase.notes.last().unwrap();
    let source_end =
        (((last.start + last.duration) * SAMPLE_RATE as f32) as usize).min(dry.len() / 2);
    let delay_frames = (seconds * SAMPLE_RATE as f32) as usize;
    let (left, right) = pan_gains(pan);
    for repeat in 1..=repeats as usize {
        let gain = send * feedback.powi((repeat - 1) as i32);
        for frame in first_frame..source_end {
            let destination = frame + delay_frames * repeat;
            if destination >= wet.len() / 2 {
                break;
            }
            if repeat % 2 == 1 {
                wet[destination * 2] += dry[frame * 2 + 1] * gain * left;
                wet[destination * 2 + 1] += dry[frame * 2] * gain * right;
            } else {
                wet[destination * 2] += dry[frame * 2] * gain * right;
                wet[destination * 2 + 1] += dry[frame * 2 + 1] * gain * left;
            }
        }
    }
}

fn continuous_layers_from_plan(seed: u64, plan: &AmbientPlan, seconds: usize) -> RenderedLayers {
    let unscaled_background = continuous_background(plan, seconds);
    let duck_envelope = background_duck_envelope(unscaled_background.len() / 2, &plan.phrases);
    let background = unscaled_background
        .chunks_exact(2)
        .enumerate()
        .flat_map(|(frame, samples)| {
            let gain = 0.8 * duck_envelope[frame];
            [samples[0] * gain, samples[1] * gain]
        })
        .collect::<Vec<_>>();
    let mut foreground = vec![0.0; unscaled_background.len()];
    let mut wet = vec![0.0; unscaled_background.len()];
    let mut phrase_dry = vec![0.0; unscaled_background.len()];
    let mut previous_range = 0..0;
    for phrase in &plan.phrases {
        phrase_dry[previous_range.clone()].fill(0.0);
        render_phrase_dry(&mut phrase_dry, phrase, seed);
        let first_frame = (phrase.notes[0].start * SAMPLE_RATE as f32).floor() as usize;
        let last = phrase.notes.last().unwrap();
        let last_frame = (((last.start + last.duration + 4.3) * SAMPLE_RATE as f32).ceil()
            as usize)
            .min(unscaled_background.len() / 2);
        let phrase_range = first_frame * 2..last_frame * 2;
        for index in phrase_range.clone() {
            foreground[index] += phrase_dry[index];
        }
        match phrase.effect {
            PhraseEffect::Dry => {}
            PhraseEffect::Reverb { send, tail } => {
                add_reverb_send(&mut wet, &phrase_dry, phrase, send, tail)
            }
            effect @ PhraseEffect::Delay { .. } => {
                add_delay_send(&mut wet, &phrase_dry, phrase, effect)
            }
        }
        previous_range = phrase_range;
    }
    RenderedLayers {
        unscaled_background,
        background,
        foreground,
        wet,
    }
}

pub fn continuous_layers(seed: u64, seconds: usize) -> RenderedLayers {
    let plan = collect_plan(seed, seconds as f32);
    continuous_layers_from_plan(seed, &plan, seconds)
}

fn continuous_pcm_from_plan(seed: u64, plan: &AmbientPlan, seconds: usize) -> Vec<f32> {
    let layers = continuous_layers_from_plan(seed, plan, seconds);
    let mut mixed = layers
        .background
        .into_iter()
        .zip(layers.foreground)
        .zip(layers.wet)
        .map(|((background, foreground), wet)| background + foreground + wet)
        .collect::<Vec<_>>();
    apply_room(&mut mixed);
    mixed
}

fn continuous_pcm(seed: u64, seconds: usize) -> Vec<f32> {
    let plan = collect_plan(seed, seconds as f32);
    continuous_pcm_from_plan(seed, &plan, seconds)
}

#[cfg(test)]
fn render_overlapping_windows(seed: u64, window_seconds: usize, total_seconds: usize) -> Vec<f32> {
    const MARGIN_SECONDS: usize = 6;
    let complete_plan = collect_plan(seed, total_seconds as f32);
    let mut output = Vec::with_capacity(total_seconds * SAMPLE_RATE as usize * 2);
    for requested_start in (0..total_seconds).step_by(window_seconds) {
        let requested_end = (requested_start + window_seconds).min(total_seconds);
        let render_start = requested_start.saturating_sub(MARGIN_SECONDS);
        let render_end = (requested_end + MARGIN_SECONDS).min(total_seconds);
        let mut local_plan = complete_plan.clone();
        local_plan.regions.retain(|region| {
            region.start < render_end as f32 && region.start + region.duration > render_start as f32
        });
        for region in &mut local_plan.regions {
            region.start -= render_start as f32;
        }
        local_plan.phrases.retain(|phrase| {
            let first = phrase.notes[0].start;
            let last = phrase.notes.last().unwrap();
            first < render_end as f32 && last.start + last.duration + 5.0 > render_start as f32
        });
        for phrase in &mut local_plan.phrases {
            for note in &mut phrase.notes {
                note.start -= render_start as f32;
            }
        }
        let local = continuous_pcm_from_plan(seed, &local_plan, render_end - render_start);
        let first = (requested_start - render_start) * SAMPLE_RATE as usize * 2;
        let count = (requested_end - requested_start) * SAMPLE_RATE as usize * 2;
        output.extend_from_slice(&local[first..first + count]);
    }
    output
}

pub fn continuous_wav(seed: u64, seconds: usize) -> Vec<u8> {
    encode_wav(&continuous_pcm(seed, seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPARISON_SEED: u64 = 0x5052_4f47_5245_5353;

    fn phrase_groups(events: &[NoteEvent]) -> Vec<&[NoteEvent]> {
        let mut groups = Vec::new();
        let mut start = 0;
        for index in 1..events.len() {
            let previous_end = events[index - 1].start + events[index - 1].duration;
            if events[index].start - previous_end > 2.0 {
                groups.push(&events[start..index]);
                start = index;
            }
        }
        groups.push(&events[start..]);
        groups
    }

    fn rms_windows(pcm: &[f32]) -> Vec<f32> {
        pcm.chunks_exact(SAMPLE_RATE as usize * 2)
            .map(|window| {
                (window.iter().map(|sample| sample * sample).sum::<f32>() / window.len() as f32)
                    .sqrt()
            })
            .collect()
    }

    fn split_band_energy(pcm: &[f32], cutoff_hz: f32) -> (f32, f32) {
        let omega = TAU * cutoff_hz / SAMPLE_RATE as f32;
        let cosine = omega.cos();
        let alpha = omega.sin() / 2.0_f32.sqrt();
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cosine / a0;
        let a2 = (1.0 - alpha) / a0;
        let low_coefficients = (
            (1.0 - cosine) * 0.5 / a0,
            (1.0 - cosine) / a0,
            (1.0 - cosine) * 0.5 / a0,
        );
        let upper_coefficients = (
            (1.0 + cosine) * 0.5 / a0,
            -(1.0 + cosine) / a0,
            (1.0 + cosine) * 0.5 / a0,
        );
        let mut low_state = [[0.0_f32; 4]; 2];
        let mut upper_state = [[0.0_f32; 4]; 2];
        let mut low_energy = 0.0;
        let mut upper_energy = 0.0;
        for (frame_index, frame) in pcm.chunks_exact(2).enumerate() {
            for channel in 0..2 {
                let [low_x1, low_x2, low_y1, low_y2] = low_state[channel];
                let low = low_coefficients.0 * frame[channel]
                    + low_coefficients.1 * low_x1
                    + low_coefficients.2 * low_x2
                    - a1 * low_y1
                    - a2 * low_y2;
                low_state[channel] = [frame[channel], low_x1, low, low_y1];

                let [upper_x1, upper_x2, upper_y1, upper_y2] = upper_state[channel];
                let upper = upper_coefficients.0 * frame[channel]
                    + upper_coefficients.1 * upper_x1
                    + upper_coefficients.2 * upper_x2
                    - a1 * upper_y1
                    - a2 * upper_y2;
                upper_state[channel] = [frame[channel], upper_x1, upper, upper_y1];
                if frame_index >= SAMPLE_RATE as usize {
                    low_energy += low * low;
                    upper_energy += upper * upper;
                }
            }
        }
        (low_energy, upper_energy)
    }

    fn decode_wav(wav: &[u8]) -> Vec<f32> {
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 2);
        assert_eq!(
            u32::from_le_bytes(wav[24..28].try_into().unwrap()),
            SAMPLE_RATE
        );
        assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 16);
        wav[44..]
            .chunks_exact(2)
            .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]) as f32 / 32768.0)
            .collect()
    }

    #[test]
    fn score_is_diatonic_harmony_aware_and_reproducible() {
        let events = phrase_events(COMPARISON_SEED);
        assert_eq!(events, phrase_events(COMPARISON_SEED));
        let groups = phrase_groups(&events);
        assert_eq!(groups.len(), 5);
        assert!(groups.iter().all(|group| (4..=6).contains(&group.len())));
        assert!(events.iter().all(|event| event.midi >= 57.0));
        assert!(events.iter().all(|event| {
            matches!(
                (event.midi.round() as i32).rem_euclid(12),
                0 | 2 | 4 | 5 | 7 | 9 | 11
            )
        }));
        let wide_leaps = events
            .windows(2)
            .filter(|pair| pair[1].start - (pair[0].start + pair[0].duration) <= 2.0)
            .map(|pair| (pair[1].midi - pair[0].midi).abs())
            .filter(|interval| *interval > 4.0)
            .collect::<Vec<_>>();
        assert!(wide_leaps.len() <= 1);
        assert!(wide_leaps.iter().all(|interval| *interval <= 5.0));
        assert!(events.windows(2).any(|pair| {
            pair[0].midi.round() as i32 % 12 == 11
                && pair[1].midi.round() as i32 % 12 == 0
                && (pair[1].midi - pair[0].midi - 1.0).abs() < f32::EPSILON
        }));

        let ending_chords = [[0, 4, 7], [9, 0, 4], [5, 9, 0], [7, 11, 2], [0, 4, 7]];
        for (group, chord) in groups.iter().zip(ending_chords) {
            let pitch_class = (group.last().unwrap().midi.round() as i32).rem_euclid(12);
            assert!(chord.contains(&pitch_class));
        }
    }

    #[test]
    fn felt_piano_uses_three_close_but_distinct_strings() {
        let frequencies = felt_string_frequencies(60.0);
        assert!(frequencies.windows(2).all(|pair| pair[0] != pair[1]));
        let spread_cents = 1_200.0 * (frequencies[2] / frequencies[0]).log2();
        assert!(spread_cents > 2.0 && spread_cents < 12.0);
    }

    #[test]
    fn band_measurement_separates_tones_around_130_hz() {
        let tone = |hz: f32| {
            let mut pcm = Vec::with_capacity(SAMPLE_RATE as usize * 4 * 2);
            for frame in 0..SAMPLE_RATE as usize * 4 {
                let sample = (TAU * hz * frame as f32 / SAMPLE_RATE as f32).sin() * 0.1;
                pcm.extend_from_slice(&[sample, sample]);
            }
            pcm
        };
        for hz in [80.0, 100.0, 120.0] {
            let (low, upper) = split_band_energy(&tone(hz), 130.0);
            assert!(low > upper, "{hz} Hz must belong to the low band");
        }
        let (low, upper) = split_band_energy(&tone(260.0), 130.0);
        assert!(upper > low);
    }

    #[test]
    fn background_evolves_for_the_full_comparison_without_low_hum() {
        let pcm = background_pcm(COMPARISON_SEED);
        assert_eq!(pcm.len(), COMPARISON_SECONDS * SAMPLE_RATE as usize * 2);
        assert!(pcm.iter().all(|sample| sample.is_finite()));
        let rms = rms_windows(&pcm);
        assert!(rms[1..].iter().all(|value| *value > 0.0002));
        let minimum = rms[1..].iter().copied().fold(f32::INFINITY, f32::min);
        let maximum = rms[1..].iter().copied().fold(0.0, f32::max);
        assert!(maximum / minimum > 1.5);

        let (low_energy, upper_energy) = split_band_energy(&pcm, 130.0);
        assert!(low_energy < upper_energy);
    }

    #[test]
    fn comparison_variants_share_score_and_background_but_sound_distinct() {
        let (felt_background, felt_foreground) =
            comparison_layers(COMPARISON_SEED, ForegroundTimbre::FeltPiano);
        let (bowed_background, bowed_foreground) =
            comparison_layers(COMPARISON_SEED, ForegroundTimbre::BowedStrings);
        let (alternating_background, alternating_foreground) =
            comparison_layers(COMPARISON_SEED, ForegroundTimbre::Alternating);

        assert_eq!(felt_background, bowed_background);
        assert_eq!(felt_background, alternating_background);
        assert_ne!(felt_foreground, bowed_foreground);
        assert_ne!(felt_foreground, alternating_foreground);
        assert_ne!(bowed_foreground, alternating_foreground);

        for event in phrase_events(COMPARISON_SEED) {
            let first = (event.start * SAMPLE_RATE as f32) as usize * 2;
            let last = first + SAMPLE_RATE as usize * 2;
            let energy = felt_foreground[first..last]
                .iter()
                .map(|sample| sample * sample)
                .sum::<f32>()
                / (last - first) as f32;
            assert!(energy > 0.00001);
        }
    }

    #[test]
    fn comparison_wavs_are_deterministic_safe_and_continuous() {
        let variants = [
            ForegroundTimbre::FeltPiano,
            ForegroundTimbre::BowedStrings,
            ForegroundTimbre::Alternating,
        ];
        let mut rendered = Vec::new();
        for variant in variants {
            let wav = comparison_wav(COMPARISON_SEED, variant);
            assert_eq!(wav, comparison_wav(COMPARISON_SEED, variant));
            assert_eq!(
                wav.len(),
                44 + COMPARISON_SECONDS * SAMPLE_RATE as usize * 4
            );
            let pcm = decode_wav(&wav);
            assert!(pcm.iter().all(|sample| sample.is_finite()));
            assert!(pcm.iter().all(|sample| sample.abs() < 0.82));
            assert!(rms_windows(&pcm)[1..].iter().all(|rms| *rms > 0.0002));
            let (low_energy, upper_energy) = split_band_energy(&pcm, 130.0);
            assert!(low_energy < upper_energy);
            rendered.push(wav);
        }
        assert_ne!(rendered[0], rendered[1]);
        assert_ne!(rendered[0], rendered[2]);
        assert_ne!(rendered[1], rendered[2]);
    }

    #[test]
    fn continuous_mix_reduces_and_smoothly_ducks_the_background() {
        let plan = ambient_timeline::collect_plan(COMPARISON_SEED, 180.0);
        let layers = continuous_layers(COMPARISON_SEED, 180);
        assert_eq!(layers.background.len(), 180 * SAMPLE_RATE as usize * 2);
        for frame in (0..180 * SAMPLE_RATE as usize).step_by(211) {
            let time = frame as f32 / SAMPLE_RATE as f32;
            let gain = background_duck_gain(time, &plan.phrases);
            assert!((0.88..=1.0).contains(&gain));
            if layers.unscaled_background[frame * 2].abs() > 0.000_001 {
                let expected = layers.unscaled_background[frame * 2] * 0.8 * gain;
                assert!((layers.background[frame * 2] - expected).abs() < 0.000_001);
            }
        }
        for frame in 1..180 * SAMPLE_RATE as usize {
            let now = background_duck_gain(frame as f32 / SAMPLE_RATE as f32, &plan.phrases);
            let previous =
                background_duck_gain((frame - 1) as f32 / SAMPLE_RATE as f32, &plan.phrases);
            assert!((now - previous).abs() < 0.000_02);
        }
        for phrase in &plan.phrases {
            let first = ((phrase.notes[0].start + 0.5) * SAMPLE_RATE as f32) as usize;
            let last_note = phrase.notes.last().unwrap();
            let last = ((last_note.start + last_note.duration) * SAMPLE_RATE as f32) as usize;
            let foreground_energy = layers.foreground[first * 2..last * 2]
                .iter()
                .map(|sample| sample * sample)
                .sum::<f32>();
            let background_energy = layers.background[first * 2..last * 2]
                .iter()
                .map(|sample| sample * sample)
                .sum::<f32>();
            assert!(
                foreground_energy > background_energy,
                "phrase {} must remain in front: foreground={foreground_energy} background={background_energy}",
                phrase.index
            );
        }
    }

    #[test]
    fn spatial_bus_is_bounded_and_does_not_change_background_generation() {
        let plan = ambient_timeline::collect_plan(COMPARISON_SEED, 180.0);
        let layers = continuous_layers(COMPARISON_SEED, 180);
        assert_eq!(
            layers.unscaled_background,
            continuous_background(&plan, 180)
        );
        let wet_peak = layers
            .wet
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0, f32::max);
        let foreground_peak = layers
            .foreground
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0, f32::max);
        assert!(wet_peak > 0.001);
        assert!(wet_peak < foreground_peak);
    }

    #[test]
    fn continuous_wav_is_deterministic_safe_and_not_a_repeated_block() {
        let wav = continuous_wav(COMPARISON_SEED, 180);
        assert_eq!(wav, continuous_wav(COMPARISON_SEED, 180));
        assert_eq!(wav.len(), 44 + 180 * SAMPLE_RATE as usize * 4);
        let pcm = decode_wav(&wav);
        assert!(pcm.iter().all(|sample| sample.abs() < 0.82));
        assert!(rms_windows(&pcm)[1..].iter().all(|rms| *rms > 0.0002));
        let block_frames = COMPARISON_SECONDS * SAMPLE_RATE as usize;
        assert_ne!(
            &pcm[..block_frames * 2],
            &pcm[block_frames * 2..block_frames * 4]
        );
        let (low_energy, upper_energy) = split_band_energy(&pcm, 130.0);
        assert!(low_energy < upper_energy);
    }

    #[test]
    fn bounded_overlapping_windows_match_the_one_shot_mix() {
        let whole = continuous_pcm(COMPARISON_SEED, 90);
        let joined = render_overlapping_windows(COMPARISON_SEED, 30, 90);
        assert_eq!(whole.len(), joined.len());
        let (maximum_index, maximum_difference) = whole
            .iter()
            .zip(joined)
            .enumerate()
            .map(|(index, (one_shot, windowed))| (index, (one_shot - windowed).abs()))
            .max_by(|left, right| left.1.total_cmp(&right.1))
            .unwrap();
        assert!(
            maximum_difference < 0.000_01,
            "difference={maximum_difference} index={maximum_index}"
        );
    }
}
