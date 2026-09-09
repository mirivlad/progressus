//! Deterministic, presentation-only score generation for timbre auditions.

use std::f32::consts::TAU;

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

fn felt_string_frequencies(midi: f32) -> [f32; 3] {
    const CENTS: [f32; 3] = [-3.0, 0.0, 3.5];
    let base = midi_hz(midi);
    CENTS.map(|cents| base * 2.0_f32.powf(cents / 1_200.0))
}

fn pan_gains(pan: f32) -> (f32, f32) {
    (((1.0 - pan) * 0.5).sqrt(), ((1.0 + pan) * 0.5).sqrt())
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
    let first = (start.max(0.0) * SAMPLE_RATE as f32) as usize;
    let last = (((start + duration) * SAMPLE_RATE as f32) as usize).min(pcm.len() / 2);
    let hz = midi_hz(midi);
    let (left, right) = pan_gains(pan);
    let mut rng = Rng(seed);
    let phase = rng.unit() * TAU;
    let slow_phase = rng.unit() * TAU;
    let slow_period = 13.0 + rng.unit() * 18.0;
    let mut breath = 0.0;
    for frame in first..last {
        let time = frame as f32 / SAMPLE_RATE as f32 - start;
        let attack = (time / 4.5).clamp(0.0, 1.0);
        let release = ((duration - time) / 5.5).clamp(0.0, 1.0);
        let envelope = attack * attack * release * release;
        let movement = 0.78 + 0.22 * (TAU * time / slow_period + slow_phase).sin();
        let fundamental = (TAU * hz * time + phase).sin();
        let second = (TAU * hz * 2.003 * time + phase * 0.61).sin();
        let third = (TAU * hz * 3.997 * time + phase * 1.17).sin();
        let white = rng.unit() * 2.0 - 1.0;
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
    let first = (event.start * SAMPLE_RATE as f32) as usize;
    let tail = event.duration + 4.2;
    let last = (((event.start + tail) * SAMPLE_RATE as f32) as usize).min(pcm.len() / 2);
    let frequencies = felt_string_frequencies(event.midi);
    let (left, right) = pan_gains(pan);
    let mut rng = Rng(seed);
    let phases = [rng.unit() * TAU, rng.unit() * TAU, rng.unit() * TAU];
    for frame in first..last {
        let time = frame as f32 / SAMPLE_RATE as f32 - event.start;
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
        let hammer = (rng.unit() * 2.0 - 1.0) * (-time / 0.06).exp() * 0.025;
        let sample = (body + soundboard + hammer) * attack * 0.15;
        pcm[frame * 2] += sample * left;
        pcm[frame * 2 + 1] += sample * right;
    }
}

fn add_bowed_strings(pcm: &mut [f32], event: NoteEvent, pan: f32, seed: u64) {
    let first = (event.start * SAMPLE_RATE as f32) as usize;
    let tail = event.duration + 3.2;
    let last = (((event.start + tail) * SAMPLE_RATE as f32) as usize).min(pcm.len() / 2);
    let base_hz = midi_hz(event.midi);
    let (left, right) = pan_gains(pan);
    let mut rng = Rng(seed);
    let phases = [rng.unit() * TAU, rng.unit() * TAU, rng.unit() * TAU];
    let cents = [-4.0_f32, 0.5, 3.5];
    let vibrato_rate = 4.6 + rng.unit() * 0.5;
    let mut bow_noise = 0.0;
    for frame in first..last {
        let time = frame as f32 / SAMPLE_RATE as f32 - event.start;
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
        let white = rng.unit() * 2.0 - 1.0;
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
        for frame in 0..COMPARISON_SECONDS * SAMPLE_RATE as usize {
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
}
