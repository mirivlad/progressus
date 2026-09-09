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

    fn first_difference_energy(pcm: &[f32]) -> f32 {
        let mut sum = 0.0;
        let mut previous = 0.0;
        for frame in pcm.chunks_exact(2) {
            let mono = (frame[0] + frame[1]) * 0.5;
            let difference = mono - previous;
            sum += difference * difference;
            previous = mono;
        }
        sum / (pcm.len() / 2) as f32
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
    fn background_evolves_for_the_full_comparison_without_low_hum() {
        let pcm = background_pcm(COMPARISON_SEED);
        assert_eq!(pcm.len(), COMPARISON_SECONDS * SAMPLE_RATE as usize * 2);
        assert!(pcm.iter().all(|sample| sample.is_finite()));
        let rms = rms_windows(&pcm);
        assert!(rms[1..].iter().all(|value| *value > 0.0002));
        let minimum = rms[1..].iter().copied().fold(f32::INFINITY, f32::min);
        let maximum = rms[1..].iter().copied().fold(0.0, f32::max);
        assert!(maximum / minimum > 1.5);

        let signal_energy =
            pcm.iter().map(|sample| sample * sample).sum::<f32>() / pcm.len() as f32;
        assert!(first_difference_energy(&pcm) > signal_energy * 0.0003);
    }
}
