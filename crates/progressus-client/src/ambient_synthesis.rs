//! Pure, client-owned synthesis for independently scheduled ambient layers.

use std::f32::consts::TAU;

pub const SAMPLE_RATE: u32 = 24_000;
/// Per-voice attenuation applied after the user effects level.
pub const WORK_VOICE_GAIN: f32 = 0.08;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AmbientLayer {
    Foundation,
    Melody,
    Nature,
}

impl AmbientLayer {
    pub const ALL: [Self; 3] = [Self::Foundation, Self::Melody, Self::Nature];

    pub const fn seconds(self) -> f64 {
        match self {
            Self::Foundation => 64.0,
            Self::Melody => 64.0,
            Self::Nature => 43.0,
        }
    }

    pub const fn looping(self) -> bool {
        !matches!(self, Self::Melody)
    }
}

pub fn layer_wav(_layer: AmbientLayer, _index: u64) -> Vec<u8> {
    let layer = _layer;
    let index = _index;
    fixed_headroom_wav(&layer_pcm(layer, index))
}

fn layer_pcm(layer: AmbientLayer, index: u64) -> Vec<f32> {
    let frames = (layer.seconds() * SAMPLE_RATE as f64) as usize;
    let overlap = if layer.looping() {
        SAMPLE_RATE as usize
    } else {
        0
    };
    let mut pcm = vec![0.0_f32; (frames + overlap) * 2];
    match layer {
        AmbientLayer::Foundation => render_foundation(&mut pcm, index),
        AmbientLayer::Melody => render_melody(&mut pcm, index),
        AmbientLayer::Nature => render_nature(&mut pcm, index),
    }
    if layer.looping() {
        pcm = overlap_loop(pcm, frames, overlap);
    }
    pcm
}

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

fn render_foundation(pcm: &mut [f32], index: u64) {
    // Every voicing draws from D Dorian. Common D/A tones keep independently
    // selected layers compatible while register and colour rotate by index.
    const VOICINGS: [[f32; 4]; 4] = [
        [38.0, 45.0, 52.0, 55.0],
        [38.0, 45.0, 50.0, 57.0],
        [38.0, 45.0, 53.0, 59.0],
        [38.0, 45.0, 55.0, 60.0],
    ];
    let voicing = VOICINGS[index as usize % VOICINGS.len()];
    let brightness = 0.75 + (index.wrapping_mul(17) % 7) as f32 * 0.045;
    for frame in 0..pcm.len() / 2 {
        let t = frame as f32 / SAMPLE_RATE as f32;
        let motion = 0.78
            + 0.08 * (TAU * t / 17.0 + index as f32 * 0.31).sin()
            + 0.06 * (TAU * t / 23.0 + index as f32 * 0.17).sin()
            + 0.04 * (TAU * t / 29.0 + index as f32 * 0.11).sin();
        let mut left = 0.0;
        let mut right = 0.0;
        for (voice, midi) in voicing.into_iter().enumerate() {
            let hz = midi_hz(midi);
            let phase = TAU * hz * t + voice as f32 * 1.7;
            let drift = 0.012 * (TAU * t / (17.0 + voice as f32 * 3.0)).sin();
            let tone = (phase + drift).sin()
                + brightness * 0.16 * (phase * 2.0 + 0.4).sin()
                + (1.0 - brightness * 0.4) * 0.07 * (phase * 3.0 + 1.1).sin();
            let (l, r) = pan_gains((voice as f32 - 1.5) * 0.22);
            let gain = if voice < 2 { 0.026 } else { 0.019 };
            left += tone * gain * l;
            right += tone * gain * r;
        }
        pcm[frame * 2] = left * motion;
        pcm[frame * 2 + 1] = right * motion;
    }
}

#[derive(Clone, Debug)]
struct Phrase {
    start: f32,
    notes: Vec<i8>,
    note_seconds: f32,
    timbre: u8,
}

impl Phrase {
    fn tail_seconds(&self) -> f32 {
        (self.notes.len() - 1) as f32 * self.note_seconds
            + self.note_seconds * 2.4
            + 1.4
            + (5 + self.timbre) as f32 / 12.0
    }
}

fn phrases(index: u64) -> Vec<Phrase> {
    const CONTOURS: [&[i8]; 6] = [
        &[0, 2, 3, 2],
        &[4, 3, 1, 0],
        &[0, 2, 5, 3, 2],
        &[3, 5, 7, 5, 4, 2],
        &[7, 4, 5, 3],
        &[2, 0, 3, 2, 0],
    ];
    let mut rng = Rng(index ^ 0x616d_6269_656e_7421);
    let mut result = Vec::new();
    let pool = if index.is_multiple_of(2) {
        [0, 2, 4]
    } else {
        [1, 3, 5]
    };
    let mut previous_notes = Vec::new();
    for position in 0..3 {
        let mut choice = rng.next() as usize % pool.len();
        let mut contour = pool[choice];
        let count = 3 + rng.next() as usize % (CONTOURS[contour].len() - 2).max(1);
        let mut notes = CONTOURS[contour][..count.min(CONTOURS[contour].len())].to_vec();
        if notes == previous_notes {
            choice = (choice + 1) % pool.len();
            contour = pool[choice];
            notes = CONTOURS[contour].to_vec();
        }
        let note_seconds = 0.72 + rng.unit() * 0.24;
        result.push(Phrase {
            start: 0.0,
            notes: notes.clone(),
            note_seconds,
            timbre: (index as u8).wrapping_add(position) % 3,
        });
        previous_notes = notes;
    }
    // Reserve a final tail near the buffer end so the next buffer's opening
    // rest does not combine with a long accidental trailing silence.
    let first_start = 10.0 + rng.unit() * 2.0;
    let last_end = 61.0 + rng.unit();
    let free = last_end - first_start - result.iter().map(Phrase::tail_seconds).sum::<f32>();
    let minimum = 10.0_f32.max(free - 25.0);
    let maximum = 25.0_f32.min(free - 10.0);
    let first_rest = minimum + (maximum - minimum) * rng.unit();
    result[0].start = first_start;
    result[1].start = result[0].start + result[0].tail_seconds() + first_rest;
    result[2].start = result[1].start + result[1].tail_seconds() + free - first_rest;
    result
}

fn render_melody(pcm: &mut [f32], index: u64) {
    // Scale degrees of D Dorian, shifted between restrained acoustic registers.
    const SCALE: [f32; 8] = [62.0, 64.0, 65.0, 67.0, 69.0, 71.0, 72.0, 74.0];
    let register = if index % 3 == 1 { 12.0 } else { 0.0 };
    for phrase in phrases(index) {
        for (position, degree) in phrase.notes.iter().enumerate() {
            let start = phrase.start + position as f32 * phrase.note_seconds;
            let duration = phrase.note_seconds * 2.4 + 1.4;
            let first = (start * SAMPLE_RATE as f32) as usize;
            let count = (duration * SAMPLE_RATE as f32) as usize;
            let hz = midi_hz(SCALE[*degree as usize] + register);
            let pan = ((position as f32 * 1.9 + index as f32).sin() * 0.32).clamp(-0.4, 0.4);
            let (l, r) = pan_gains(pan);
            for offset in 0..count.min(pcm.len() / 2 - first.min(pcm.len() / 2)) {
                let t = offset as f32 / SAMPLE_RATE as f32;
                let phase = TAU * hz * t;
                let attack = (t / 0.32).min(1.0);
                let tail = ((duration - t) / 1.35).clamp(0.0, 1.0);
                let decay = match phrase.timbre {
                    0 => (-t * 1.15).exp(),
                    1 => (-t * 0.72).exp(),
                    _ => (-t * 0.38).exp(),
                };
                let colour = match phrase.timbre {
                    0 => phase.sin() + 0.30 * (phase * 2.01).sin() + 0.10 * (phase * 3.02).sin(),
                    1 => phase.sin() + 0.16 * (phase * 2.0 + 0.2).sin(),
                    _ => phase.sin() + 0.08 * (phase * 3.0 + 0.7).sin(),
                };
                let value = colour * attack * tail * decay * 0.105;
                pcm[(first + offset) * 2] += value * l;
                pcm[(first + offset) * 2 + 1] += value * r;
                // A short, restrained cross-channel echo.
                let echo =
                    first + offset + (SAMPLE_RATE as usize * (5 + phrase.timbre as usize) / 12);
                if echo < pcm.len() / 2 {
                    pcm[echo * 2] += value * r * 0.12;
                    pcm[echo * 2 + 1] += value * l * 0.12;
                }
            }
        }
    }
    let frames = pcm.len() / 2;
    for frame in frames.saturating_sub(SAMPLE_RATE as usize * 4)..frames {
        let fade = (frames - 1 - frame) as f32 / (SAMPLE_RATE as f32 * 4.0);
        pcm[frame * 2] *= fade;
        pcm[frame * 2 + 1] *= fade;
    }
}

fn render_nature(pcm: &mut [f32], index: u64) {
    let seed = index ^ 0x6c65_6176_6573_2121;
    let mut rng = Rng(seed);
    let alpha_100 = 1.0 - (-TAU * 100.0 / SAMPLE_RATE as f32).exp();
    let alpha_700 = 1.0 - (-TAU * 700.0 / SAMPLE_RATE as f32).exp();
    let alpha_1k8 = 1.0 - (-TAU * 1_800.0 / SAMPLE_RATE as f32).exp();
    let (mut low, mut wind, mut leaves) = (0.0_f32, 0.0_f32, 0.0_f32);
    for frame in 0..pcm.len() / 2 {
        let t = frame as f32 / SAMPLE_RATE as f32;
        let white = rng.unit() * 2.0 - 1.0;
        low += alpha_100 * (white - low);
        wind += alpha_700 * (white - wind);
        leaves += alpha_1k8 * (white - leaves);
        let wind_band = wind - low;
        let leaf_band = leaves - wind;
        let breath = 0.58
            + 0.17 * (TAU * t / 17.0 + index as f32).sin()
            + 0.12 * (TAU * t / 23.0 + 0.4).sin()
            + 0.08 * (TAU * t / 29.0 + 1.2).sin();
        let leaf_gate = (0.35 + 0.65 * (TAU * t / 3.7 + index as f32).sin().max(0.0)).powi(3);
        let pan_motion = (TAU * t / 29.0 + index as f32 * 0.7).sin() * 0.18;
        let (l, r) = pan_gains(pan_motion);
        let value = wind_band * breath * 0.085 + leaf_band * leaf_gate * 0.055;
        pcm[frame * 2] = value * l;
        pcm[frame * 2 + 1] = value * r;
    }
}

fn overlap_loop(mut pcm: Vec<f32>, frames: usize, overlap: usize) -> Vec<f32> {
    for offset in 0..overlap {
        let x = offset as f32 / overlap as f32;
        let fade_in = (x * std::f32::consts::FRAC_PI_2).sin();
        let fade_out = (x * std::f32::consts::FRAC_PI_2).cos();
        for channel in 0..2 {
            pcm[offset * 2 + channel] = pcm[offset * 2 + channel] * fade_in
                + pcm[(frames + offset) * 2 + channel] * fade_out;
        }
    }
    pcm.truncate(frames * 2);
    pcm
}

fn fixed_headroom_wav(samples: &[f32]) -> Vec<u8> {
    assert!(
        samples
            .iter()
            .all(|sample| sample.is_finite() && sample.abs() <= 1.0),
        "ambient synthesis must produce finite PCM with fixed headroom"
    );
    let data_bytes = u32::try_from(samples.len() * 2).expect("bounded layer PCM fits WAV");
    let mut output = Vec::with_capacity(data_bytes as usize + 44);
    output.extend_from_slice(b"RIFF");
    output.extend_from_slice(&(data_bytes + 36).to_le_bytes());
    output.extend_from_slice(b"WAVEfmt ");
    output.extend_from_slice(&16_u32.to_le_bytes());
    output.extend_from_slice(&1_u16.to_le_bytes());
    output.extend_from_slice(&2_u16.to_le_bytes());
    output.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    output.extend_from_slice(&(SAMPLE_RATE * 4).to_le_bytes());
    output.extend_from_slice(&4_u16.to_le_bytes());
    output.extend_from_slice(&16_u16.to_le_bytes());
    output.extend_from_slice(b"data");
    output.extend_from_slice(&data_bytes.to_le_bytes());
    for sample in samples {
        output
            .extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pcm(bytes: &[u8]) -> Vec<i16> {
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[36..40], b"data");
        bytes[44..]
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect()
    }

    fn boundary_delta(samples: &[i16]) -> u16 {
        let last = samples.len() - 2;
        samples[0]
            .abs_diff(samples[last])
            .max(samples[1].abs_diff(samples[last + 1]))
    }

    #[test]
    fn layer_contract_has_fixed_durations_and_loop_policy() {
        assert_eq!(AmbientLayer::Foundation.seconds(), 64.0);
        assert_eq!(AmbientLayer::Nature.seconds(), 43.0);
        assert_eq!(AmbientLayer::Melody.seconds(), 64.0);
        assert!(AmbientLayer::Foundation.looping());
        assert!(AmbientLayer::Nature.looping());
        assert!(!AmbientLayer::Melody.looping());
    }

    #[test]
    fn buffers_are_deterministic_changed_finite_and_have_fixed_headroom() {
        for layer in AmbientLayer::ALL {
            let first = layer_wav(layer, 0);
            assert_eq!(first, layer_wav(layer, 0));
            assert_ne!(first, layer_wav(layer, 1));
            let samples = pcm(&first);
            assert_eq!(
                samples.len(),
                (layer.seconds() * SAMPLE_RATE as f64) as usize * 2
            );
            assert!(samples.iter().any(|sample| sample.unsigned_abs() > 300));
            assert!(samples.iter().all(|sample| sample.unsigned_abs() <= 12_000));
        }
    }

    #[test]
    fn continuous_layers_join_without_a_click() {
        for layer in [AmbientLayer::Foundation, AmbientLayer::Nature] {
            let samples = pcm(&layer_wav(layer, 7));
            assert!(
                boundary_delta(&samples) < 96,
                "{layer:?} boundary delta was {}",
                boundary_delta(&samples)
            );
        }
    }

    #[test]
    fn independently_indexed_layers_leave_combined_headroom() {
        let layers = AmbientLayer::ALL.map(|layer| pcm(&layer_wav(layer, 19 + layer as u64)));
        let frames = layers.iter().map(Vec::len).min().unwrap();
        let peak = (0..frames)
            .map(|at| {
                layers
                    .iter()
                    .map(|layer| i32::from(layer[at]))
                    .sum::<i32>()
                    .unsigned_abs()
            })
            .max()
            .unwrap();
        assert!(peak < 28_000, "combined PCM peak was {peak}");
    }

    #[test]
    fn arrangement_phrases_have_connected_contours_and_long_rests() {
        for index in 0..12 {
            let score = phrases(index);
            assert!(score.len() >= 2);
            for phrase in &score {
                assert!((3..=6).contains(&phrase.notes.len()));
                assert!(
                    phrase
                        .notes
                        .windows(2)
                        .all(|pair| (pair[1] - pair[0]).abs() <= 3)
                );
            }
            for pair in score.windows(2) {
                assert_ne!(pair[0].notes, pair[1].notes);
                let end = pair[0].start + pair[0].tail_seconds();
                let rest = pair[1].start - end;
                assert!((10.0..=25.0).contains(&rest), "rest was {rest}");
            }
        }
    }

    #[test]
    fn phrase_rests_and_contours_remain_valid_across_buffer_boundaries() {
        for index in 0..100 {
            let score = phrases(index);
            let next = phrases(index + 1);
            let last = score.last().unwrap();
            let end = last.start
                + (last.notes.len() - 1) as f32 * last.note_seconds
                + last.note_seconds * 2.4
                + 1.4
                + (5 + last.timbre) as f32 / 12.0;
            let rest = 64.0 + next[0].start - end;
            assert!(
                (10.0..=25.0).contains(&rest),
                "boundary {index}: {rest}s rest"
            );
            assert_ne!(last.notes, next[0].notes);
        }
    }

    #[test]
    fn ambient_and_eight_work_voices_have_worst_case_headroom() {
        let layer_peak_sum = AmbientLayer::ALL
            .into_iter()
            .map(|layer| {
                pcm(&layer_wav(layer, 31 + layer as u64))
                    .into_iter()
                    .map(i16::unsigned_abs)
                    .max()
                    .unwrap() as f32
                    / 32767.0
            })
            .sum::<f32>();
        assert!(
            layer_peak_sum <= 0.4,
            "ambient peak bound was {layer_peak_sum}"
        );
        assert!(layer_peak_sum + 8.0 * 0.72 * WORK_VOICE_GAIN < 1.0);
    }

    #[test]
    fn raw_float_pcm_is_finite_and_bounded_before_encoding() {
        for layer in AmbientLayer::ALL {
            let pcm = layer_pcm(layer, 5);
            assert!(pcm.iter().all(|sample| sample.is_finite()));
            let peak = pcm
                .iter()
                .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
            assert!(peak < 0.4, "{layer:?} raw peak was {peak}");
        }
    }
}
