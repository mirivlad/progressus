//! Deterministic, presentation-only motif generation for listening auditions.

use std::collections::HashSet;

pub const SAMPLE_RATE: u32 = 24_000;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EndingRole {
    Open,
    Resting,
    Resolving,
}

impl EndingRole {
    pub const ALL: [Self; 3] = [Self::Open, Self::Resting, Self::Resolving];
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Motif {
    pub degrees: Vec<i8>,
    pub durations: Vec<u8>,
    pub role: EndingRole,
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

    fn choose<T: Copy>(&mut self, values: &[T]) -> T {
        values[self.next() as usize % values.len()]
    }

    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / 16_777_216.0
    }
}

fn normalized_signature(motif: &Motif) -> (Vec<i8>, Vec<u8>) {
    let root = motif.degrees[0];
    (
        motif.degrees.iter().map(|degree| degree - root).collect(),
        motif.durations.clone(),
    )
}

fn is_valid(motif: &Motif) -> bool {
    if !(3..=7).contains(&motif.degrees.len()) || motif.degrees.len() != motif.durations.len() {
        return false;
    }
    let range = motif.degrees.iter().max().unwrap() - motif.degrees.iter().min().unwrap();
    let large_leaps = motif
        .degrees
        .windows(2)
        .filter(|pair| (pair[1] - pair[0]).abs() > 2)
        .count();
    let leaps_are_bounded = motif
        .degrees
        .windows(2)
        .all(|pair| (pair[1] - pair[0]).abs() <= 3);
    let repeats_phrase = motif
        .degrees
        .windows(6)
        .any(|notes| notes[..3] == notes[3..]);
    let repeats_note = motif
        .degrees
        .windows(3)
        .any(|notes| notes[0] == notes[1] && notes[1] == notes[2]);
    let fixed_pulse = motif
        .durations
        .windows(2)
        .all(|durations| durations[0] == durations[1]);
    let final_degree = motif.degrees[motif.degrees.len() - 1];
    let previous_degree = motif.degrees[motif.degrees.len() - 2];
    let ending_valid = match motif.role {
        EndingRole::Resolving => final_degree == 0 && previous_degree > final_degree,
        EndingRole::Resting => matches!(final_degree, 2 | 4) && previous_degree != final_degree,
        EndingRole::Open => {
            matches!(final_degree, 1 | 3 | 5 | 6) && previous_degree != final_degree
        }
    };
    range <= 5
        && large_leaps <= 1
        && leaps_are_bounded
        && !repeats_phrase
        && !repeats_note
        && !fixed_pulse
        && ending_valid
}

pub fn candidate_pool(seed: u64) -> Vec<Motif> {
    const STEPS: [i8; 9] = [-3, -2, -1, -1, 0, 1, 1, 2, 3];
    const DURATIONS: [u8; 4] = [2, 3, 4, 6];
    const ENDINGS: [&[i8]; 3] = [&[1, 3, 5, 6], &[2, 4], &[0]];

    let mut rng = Rng(seed);
    let mut result = Vec::with_capacity(48);
    let mut signatures = HashSet::with_capacity(48);
    let mut role_counts = [0_usize; EndingRole::ALL.len()];
    for attempt in 0..20_000 {
        if result.len() == 48 {
            break;
        }
        let role_index = attempt % EndingRole::ALL.len();
        if role_counts[role_index] == 16 {
            continue;
        }
        let role = EndingRole::ALL[role_index];
        let note_count = 3 + rng.next() as usize % 5;
        let mut degrees = vec![(rng.next() % 7) as i8];
        while degrees.len() + 1 < note_count {
            let previous = *degrees.last().unwrap();
            degrees.push((previous + rng.choose(&STEPS)).clamp(0, 6));
        }
        let endings = ENDINGS[role_index];
        degrees.push(rng.choose(endings));
        let durations = (0..note_count)
            .map(|_| rng.choose(&DURATIONS))
            .collect::<Vec<_>>();
        let motif = Motif {
            degrees,
            durations,
            role,
        };
        let signature = normalized_signature(&motif);
        if is_valid(&motif) && signatures.insert(signature) {
            role_counts[role_index] += 1;
            result.push(motif);
        }
    }
    assert_eq!(
        result.len(),
        48,
        "neutral grammar must produce its bounded candidate pool"
    );
    result
}

fn motif_distance(left: &Motif, right: &Motif) -> usize {
    let left = normalized_signature(left);
    let right = normalized_signature(right);
    let shared = left.0.len().min(right.0.len());
    let length_distance = left.0.len().abs_diff(right.0.len()) * 6;
    let pitch_distance = (0..shared)
        .map(|index| left.0[index].abs_diff(right.0[index]) as usize)
        .sum::<usize>();
    let rhythm_distance = (0..shared)
        .filter(|index| left.1[*index] != right.1[*index])
        .count()
        * 2;
    length_distance + pitch_distance + rhythm_distance
}

pub fn shortlist(seed: u64, count: usize) -> Vec<Motif> {
    let pool = candidate_pool(seed);
    let mut selected = Vec::with_capacity(count.min(pool.len()));
    let mut used = vec![false; pool.len()];
    while selected.len() < count.min(pool.len()) {
        let desired_role = EndingRole::ALL[selected.len() % EndingRole::ALL.len()];
        let mut best: Option<(usize, usize)> = None;
        for (index, motif) in pool.iter().enumerate() {
            if used[index] || motif.role != desired_role {
                continue;
            }
            let distance = selected
                .iter()
                .map(|chosen| motif_distance(motif, chosen))
                .min()
                .unwrap_or(usize::MAX);
            if best.is_none_or(|(_, best_distance)| distance > best_distance) {
                best = Some((index, distance));
            }
        }
        let index = best
            .map(|(index, _)| index)
            .expect("candidate pool must cover every ending role");
        used[index] = true;
        selected.push(pool[index].clone());
    }
    selected
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditionKind {
    Isolated,
    Contextual,
}

impl AuditionKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Isolated => "isolated",
            Self::Contextual => "context",
        }
    }

    const fn seconds(self) -> f32 {
        match self {
            Self::Isolated => 12.0,
            Self::Contextual => 18.0,
        }
    }
}

fn midi_hz(midi: f32) -> f32 {
    440.0 * 2.0_f32.powf((midi - 69.0) / 12.0)
}

fn pan_gains(pan: f32) -> (f32, f32) {
    (((1.0 - pan) * 0.5).sqrt(), ((1.0 + pan) * 0.5).sqrt())
}

fn add_plucked_wood(
    pcm: &mut [f32],
    hz: f32,
    start: f32,
    duration: f32,
    gain: f32,
    pan: f32,
    seed: u64,
) {
    let first = (start * SAMPLE_RATE as f32) as usize;
    if first >= pcm.len() / 2 {
        return;
    }
    let count = (duration * SAMPLE_RATE as f32) as usize;
    let delay_len = (SAMPLE_RATE as f32 / hz).round().max(2.0) as usize;
    let mut rng = Rng(seed);
    let mut delay = (0..delay_len)
        .map(|_| rng.unit() * 2.0 - 1.0)
        .collect::<Vec<_>>();
    for index in 1..delay.len() {
        delay[index] = delay[index] * 0.72 + delay[index - 1] * 0.28;
    }
    let (left, right) = pan_gains(pan);
    let mut cursor = 0;
    for offset in 0..count.min(pcm.len() / 2 - first) {
        let t = offset as f32 / SAMPLE_RATE as f32;
        let current = delay[cursor];
        let following = delay[(cursor + 1) % delay_len];
        delay[cursor] = (current + following) * 0.5 * 0.9965;
        cursor = (cursor + 1) % delay_len;
        let attack = (t / 0.012).min(1.0);
        let release = ((duration - t) / 0.5).clamp(0.0, 1.0);
        let wooden_body = (std::f32::consts::TAU * hz * 2.73 * t).sin() * (-t * 7.2).exp() * 0.13
            + (std::f32::consts::TAU * hz * 4.18 * t + 0.7).sin() * (-t * 10.0).exp() * 0.06;
        let value = (current * 0.78 + wooden_body) * attack * release * gain;
        pcm[(first + offset) * 2] += value * left;
        pcm[(first + offset) * 2 + 1] += value * right;
    }
}

fn add_air_voice(
    pcm: &mut [f32],
    hz: f32,
    start: f32,
    duration: f32,
    gain: f32,
    pan: f32,
    seed: u64,
) {
    let first = (start * SAMPLE_RATE as f32) as usize;
    let count = (duration * SAMPLE_RATE as f32) as usize;
    let (left, right) = pan_gains(pan);
    let mut rng = Rng(seed);
    let mut breath = 0.0_f32;
    let breath_alpha = 1.0 - (-std::f32::consts::TAU * 1_600.0 / SAMPLE_RATE as f32).exp();
    for offset in 0..count.min(pcm.len() / 2 - first.min(pcm.len() / 2)) {
        let t = offset as f32 / SAMPLE_RATE as f32;
        let phase =
            std::f32::consts::TAU * hz * t + 0.003 * (std::f32::consts::TAU * t / 3.7).sin();
        breath += breath_alpha * (rng.unit() * 2.0 - 1.0 - breath);
        let attack = (t / 0.7).min(1.0);
        let decay = (-t * 0.32).exp();
        let release = ((duration - t) / 1.2).clamp(0.0, 1.0);
        let tone = phase.sin() + 0.17 * (phase * 2.01 + 0.3).sin();
        let value = (tone * 0.82 + breath * 0.05) * attack * decay * release * gain;
        pcm[(first + offset) * 2] += value * left;
        pcm[(first + offset) * 2 + 1] += value * right;
    }
}

fn add_room(pcm: &mut [f32], wet_gain: f32) {
    const DELAYS: [usize; 4] = [1_499, 2_257, 3_307, 4_093];
    const FEEDBACK: [f32; 4] = [0.42, 0.39, 0.36, 0.33];
    let mut combs = DELAYS.map(|delay| vec![0.0_f32; delay]);
    let mut cursors = [0_usize; 4];
    let alpha = 1.0 - (-std::f32::consts::TAU * 120.0 / SAMPLE_RATE as f32).exp();
    let mut wet_low_left = 0.0_f32;
    let mut wet_low_right = 0.0_f32;
    for frame in 0..pcm.len() / 2 {
        let input = (pcm[frame * 2] + pcm[frame * 2 + 1]) * 0.5;
        let mut taps = [0.0_f32; 4];
        for voice in 0..combs.len() {
            taps[voice] = combs[voice][cursors[voice]];
            combs[voice][cursors[voice]] = input + taps[voice] * FEEDBACK[voice];
            cursors[voice] = (cursors[voice] + 1) % combs[voice].len();
        }
        let wet_left = taps[0] + taps[2] * 0.7 + taps[3] * 0.35;
        let wet_right = taps[1] + taps[3] * 0.7 + taps[2] * 0.35;
        wet_low_left += alpha * (wet_left - wet_low_left);
        wet_low_right += alpha * (wet_right - wet_low_right);
        pcm[frame * 2] += (wet_left - wet_low_left) * wet_gain;
        pcm[frame * 2 + 1] += (wet_right - wet_low_right) * wet_gain;
    }
}

fn render_motif(pcm: &mut [f32], motif: &Motif, index: u64, start: f32, gain: f32) -> f32 {
    const SCALE: [f32; 7] = [62.0, 64.0, 65.0, 67.0, 69.0, 71.0, 72.0];
    const UNIT_SECONDS: f32 = 0.16;
    let mut cursor = start;
    for (position, (&degree, &units)) in motif.degrees.iter().zip(&motif.durations).enumerate() {
        let pan = ((position as f32 * 1.7 + index as f32 * 0.9).sin() * 0.34).clamp(-0.4, 0.4);
        add_plucked_wood(
            pcm,
            midi_hz(SCALE[degree as usize]),
            cursor,
            3.8,
            gain,
            pan,
            index ^ (position as u64).wrapping_mul(0x9e37_79b9),
        );
        cursor += f32::from(units) * UNIT_SECONDS;
    }
    cursor
}

fn fixed_headroom_wav(samples: &[f32]) -> Vec<u8> {
    let data_bytes = u32::try_from(samples.len() * 2).expect("audition PCM fits WAV");
    let mut wav = Vec::with_capacity(data_bytes as usize + 44);
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
    for sample in samples {
        let encoded = (sample.clamp(-1.0, 1.0) * 32_767.0).round() as i16;
        wav.extend_from_slice(&encoded.to_le_bytes());
    }
    wav
}

pub fn audition_wav(motif: &Motif, index: u64, kind: AuditionKind) -> Vec<u8> {
    let frames = (kind.seconds() * SAMPLE_RATE as f32) as usize;
    let mut pcm = vec![0.0_f32; frames * 2];
    match kind {
        AuditionKind::Isolated => {
            render_motif(&mut pcm, motif, index, 1.0, 0.19);
            add_room(&mut pcm, 0.09);
        }
        AuditionKind::Contextual => {
            add_air_voice(
                &mut pcm,
                midi_hz(62.0),
                0.45,
                6.4,
                0.026,
                -0.22,
                index ^ 0x6169_722d_6f70_656e,
            );
            add_air_voice(
                &mut pcm,
                midi_hz(69.0),
                0.8,
                5.3,
                0.018,
                0.26,
                index ^ 0x6669_6674_6821,
            );
            let phrase_end = render_motif(&mut pcm, motif, index, 2.2, 0.16);
            let answer_start = (phrase_end + 1.0).min(11.8);
            let answer_midi = match motif.role {
                EndingRole::Open => 67.0,
                EndingRole::Resting => 65.0,
                EndingRole::Resolving => 62.0,
            };
            add_air_voice(
                &mut pcm,
                midi_hz(answer_midi + 12.0),
                answer_start,
                4.8,
                0.021,
                0.18,
                index ^ 0x616e_7377_6572_2121,
            );
            add_plucked_wood(
                &mut pcm,
                midi_hz(answer_midi + 7.0),
                answer_start + 0.65,
                3.2,
                0.07,
                -0.16,
                index ^ 0x776f_6f64_656e_2121,
            );
            add_room(&mut pcm, 0.16);
        }
    }
    for frame in 0..frames {
        let t = frame as f32 / SAMPLE_RATE as f32;
        let fade = (t / 0.18).min(1.0) * ((kind.seconds() - t) / 0.7).clamp(0.0, 1.0);
        pcm[frame * 2] *= fade;
        pcm[frame * 2 + 1] *= fade;
    }
    debug_assert!(pcm.iter().all(|sample| sample.is_finite()));
    debug_assert!(pcm.iter().all(|sample| sample.abs() < 0.82));
    fixed_headroom_wav(&pcm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const NEUTRAL_SEED: u64 = 0x5052_4f47_5245_5353;

    fn valid_motif(motif: &Motif) -> bool {
        let note_count_valid = (3..=7).contains(&motif.degrees.len());
        let duration_count_valid = motif.degrees.len() == motif.durations.len();
        let range = motif.degrees.iter().max().unwrap() - motif.degrees.iter().min().unwrap();
        let large_leaps = motif
            .degrees
            .windows(2)
            .filter(|pair| (pair[1] - pair[0]).abs() > 2)
            .count();
        let leaps_are_bounded = motif
            .degrees
            .windows(2)
            .all(|pair| (pair[1] - pair[0]).abs() <= 3);
        let repeated_triplet = motif
            .degrees
            .windows(6)
            .any(|notes| notes[..3] == notes[3..]);
        let repeated_note = motif
            .degrees
            .windows(3)
            .any(|notes| notes[0] == notes[1] && notes[1] == notes[2]);
        let fixed_pulse = motif
            .durations
            .windows(2)
            .all(|durations| durations[0] == durations[1]);
        let ending_valid = match motif.role {
            EndingRole::Resolving => motif.degrees.last() == Some(&0),
            EndingRole::Resting => matches!(motif.degrees.last(), Some(2 | 4)),
            EndingRole::Open => matches!(motif.degrees.last(), Some(1 | 3 | 5 | 6)),
        };
        note_count_valid
            && duration_count_valid
            && range <= 5
            && large_leaps <= 1
            && leaps_are_bounded
            && !repeated_triplet
            && !repeated_note
            && !fixed_pulse
            && ending_valid
    }

    fn wav_samples(wav: &[u8]) -> Vec<f32> {
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 2);
        assert_eq!(
            u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
            SAMPLE_RATE
        );
        assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 16);
        wav[44..]
            .chunks_exact(2)
            .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]) as f32 / 32_767.0)
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn neutral_pool_is_reproducible_and_structurally_valid() {
        let pool = candidate_pool(NEUTRAL_SEED);
        assert_eq!(pool.len(), 48);
        assert_eq!(pool, candidate_pool(NEUTRAL_SEED));
        assert!(pool.iter().all(valid_motif));
        let signatures = pool
            .iter()
            .map(normalized_signature)
            .collect::<HashSet<_>>();
        assert_eq!(signatures.len(), pool.len());
    }

    #[test]
    fn shortlist_is_diverse_and_covers_ending_roles() {
        let selected = shortlist(NEUTRAL_SEED, 10);
        assert_eq!(selected.len(), 10);
        assert!(
            EndingRole::ALL
                .into_iter()
                .all(|role| selected.iter().any(|motif| motif.role == role))
        );
        let signatures = selected
            .iter()
            .map(normalized_signature)
            .collect::<HashSet<_>>();
        assert_eq!(signatures.len(), selected.len());
    }

    #[test]
    fn every_pool_has_equal_capacity_for_each_ending_role() {
        for seed in [NEUTRAL_SEED, 5_737] {
            let pool = candidate_pool(seed);
            let counts =
                EndingRole::ALL.map(|role| pool.iter().filter(|motif| motif.role == role).count());
            assert_eq!(counts, [16, 16, 16], "unbalanced pool for seed {seed}");
            assert_eq!(shortlist(seed, 48).len(), 48);
        }
    }

    #[test]
    fn ending_roles_require_a_final_motion() {
        for motif in candidate_pool(NEUTRAL_SEED) {
            let last = motif.degrees[motif.degrees.len() - 1];
            let previous = motif.degrees[motif.degrees.len() - 2];
            assert_ne!(previous, last);
            if motif.role == EndingRole::Resolving {
                assert!(previous > last, "resolution must move down into the tonic");
            }
        }
    }

    #[test]
    fn audition_wavs_are_deterministic_distinct_and_bounded() {
        let motif = &shortlist(NEUTRAL_SEED, 10)[0];
        let isolated = audition_wav(motif, 0, AuditionKind::Isolated);
        let contextual = audition_wav(motif, 0, AuditionKind::Contextual);
        assert_eq!(isolated, audition_wav(motif, 0, AuditionKind::Isolated));
        assert_ne!(isolated, contextual);
        for wav in [&isolated, &contextual] {
            let samples = wav_samples(wav);
            assert!(rms(&samples) > 0.001);
            assert!(samples.iter().all(|sample| sample.is_finite()));
            assert!(
                samples.iter().all(|sample| sample.abs() < 0.82),
                "audition must retain fixed headroom"
            );
        }
    }

    #[test]
    fn contextual_audition_breathes_without_a_stationary_low_bed() {
        let motif = &shortlist(NEUTRAL_SEED, 10)[0];
        let samples = wav_samples(&audition_wav(motif, 0, AuditionKind::Contextual));
        let one_second = SAMPLE_RATE as usize * 2;
        let windows = samples
            .chunks_exact(one_second)
            .map(rms)
            .collect::<Vec<_>>();
        let quiet = windows.iter().copied().fold(f32::INFINITY, f32::min);
        let loud = windows.iter().copied().fold(0.0_f32, f32::max);
        assert!(
            loud > quiet * 2.0,
            "context must contain real dynamic rests"
        );

        let alpha = 1.0 - (-std::f32::consts::TAU * 130.0 / SAMPLE_RATE as f32).exp();
        let mut low = 0.0_f32;
        let mut low_energy = 0.0_f64;
        let mut upper_energy = 0.0_f64;
        for frame in samples.chunks_exact(2) {
            let mono = (frame[0] + frame[1]) * 0.5;
            low += alpha * (mono - low);
            low_energy += f64::from(low * low);
            upper_energy += f64::from((mono - low) * (mono - low));
        }
        assert!(
            low_energy < upper_energy,
            "context must not be dominated by a low appliance-like bed"
        );
    }
}
