//! Presentation-only planning for an unbounded neutral ambient timeline.

use std::collections::VecDeque;

const VOICINGS: [[f32; 3]; 6] = [
    [60.0, 64.0, 67.0],
    [60.0, 64.0, 69.0],
    [60.0, 65.0, 69.0],
    [62.0, 67.0, 71.0],
    [62.0, 65.0, 69.0],
    [59.0, 64.0, 67.0],
];
const TRANSITIONS: [&[usize]; 6] = [
    &[1, 2, 3, 4, 5],
    &[0, 2, 4, 5],
    &[0, 3, 4],
    &[0, 1, 5],
    &[1, 2, 3],
    &[1, 2, 3],
];
const SCALE: [f32; 10] = [60.0, 62.0, 64.0, 65.0, 67.0, 69.0, 71.0, 72.0, 74.0, 76.0];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhraseTimbre {
    FeltPiano,
    BowedStrings,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PhraseEffect {
    Dry,
    Reverb {
        send: f32,
        tail: f32,
    },
    Delay {
        send: f32,
        seconds: f32,
        feedback: f32,
        repeats: u8,
        pan: f32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MotifFingerprint {
    intervals: Vec<i8>,
    rhythm: Vec<u8>,
    ending_role: u8,
    timbre: PhraseTimbre,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HarmonyRegion {
    pub index: u64,
    pub start: f32,
    pub duration: f32,
    pub chord: [f32; 3],
    pub gain: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlannedNote {
    pub start: f32,
    pub midi: f32,
    pub duration: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhrasePlan {
    pub index: u64,
    pub notes: Vec<PlannedNote>,
    pub timbre: PhraseTimbre,
    pub effect: PhraseEffect,
    pub fingerprint: MotifFingerprint,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AmbientPlan {
    pub regions: Vec<HarmonyRegion>,
    pub phrases: Vec<PhrasePlan>,
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

fn event_rng(seed: u64, domain: u64, index: u64, attempt: u64) -> Rng {
    Rng(seed ^ domain ^ index.wrapping_mul(0xd134_2543_de82_ef95) ^ attempt.rotate_left(29))
}

fn effect_kind(effect: PhraseEffect) -> u8 {
    match effect {
        PhraseEffect::Dry => 0,
        PhraseEffect::Reverb { .. } => 1,
        PhraseEffect::Delay { .. } => 2,
    }
}

pub struct AmbientTimeline {
    seed: u64,
    region_index: u64,
    region_start: f32,
    chord_index: usize,
    last_cadence: i64,
    phrase_index: u64,
    phrase_start: f32,
    recent_fingerprints: VecDeque<MotifFingerprint>,
    previous_effect_kind: Option<u8>,
}

impl AmbientTimeline {
    pub fn new(seed: u64) -> Self {
        let mut rng = event_rng(seed, 0x5048_5241_5345_3030, 0, 0);
        Self {
            seed,
            region_index: 0,
            region_start: 0.0,
            chord_index: 0,
            last_cadence: -4,
            phrase_index: 0,
            phrase_start: 6.0 + rng.unit() * 2.0,
            recent_fingerprints: VecDeque::with_capacity(12),
            previous_effect_kind: None,
        }
    }

    pub fn next_region(&mut self) -> HarmonyRegion {
        let index = self.region_index;
        let mut rng = event_rng(self.seed, 0x4841_524d_4f4e_5931, index, 0);
        if index > 0 {
            let choices = TRANSITIONS[self.chord_index];
            let mut next = choices[rng.next() as usize % choices.len()];
            if self.chord_index == 3 && next == 0 && index as i64 - self.last_cadence < 3 {
                next = choices
                    .iter()
                    .copied()
                    .find(|candidate| *candidate != 0)
                    .unwrap();
            }
            if self.chord_index == 3 && next == 0 {
                self.last_cadence = index as i64;
            }
            self.chord_index = next;
        }
        let duration = 13.0 + rng.unit() * 6.0;
        let gain = (0.68
            + (index as f32 * 0.73 + self.seed as f32 * 0.000_000_1).sin() * 0.16
            + (rng.unit() - 0.5) * 0.12)
            .clamp(0.42, 0.9);
        let region = HarmonyRegion {
            index,
            start: self.region_start,
            duration,
            chord: VOICINGS[self.chord_index],
            gain,
        };
        let overlap = 3.5 + rng.unit() * 2.0;
        self.region_start += duration - overlap;
        self.region_index += 1;
        region
    }

    pub fn next_phrase(&mut self) -> PhrasePlan {
        let index = self.phrase_index;
        let timbre = if index.is_multiple_of(2) {
            PhraseTimbre::FeltPiano
        } else {
            PhraseTimbre::BowedStrings
        };
        let chord = chord_at_time(self.seed, self.phrase_start);
        let chord_classes = chord.map(|midi| midi.round() as i32 % 12);
        let mut selected = None;
        for attempt in 0..128 {
            let mut rng = event_rng(self.seed, 0x4d4f_5449_465f_3031, index, attempt);
            let note_count = 4 + rng.next() as usize % 3;
            let chord_tones = SCALE
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, midi)| chord_classes.contains(&(midi.round() as i32 % 12)))
                .collect::<Vec<_>>();
            let (mut scale_index, _) = chord_tones[rng.next() as usize % chord_tones.len()];
            let mut pitches = vec![SCALE[scale_index]];
            let mut used_fourth = false;
            while pitches.len() + 1 < note_count {
                const MOVES: [i32; 8] = [-2, -1, -1, 1, 1, 2, 2, 3];
                let mut movement = MOVES[rng.next() as usize % MOVES.len()];
                let candidate = (scale_index as i32 + movement).clamp(0, SCALE.len() as i32 - 1);
                let interval = (SCALE[candidate as usize] - SCALE[scale_index]).abs();
                if interval > 5.0 || (interval > 4.0 && used_fourth) {
                    movement = if movement.is_negative() { -1 } else { 1 };
                }
                let candidate =
                    (scale_index as i32 + movement).clamp(0, SCALE.len() as i32 - 1) as usize;
                let interval = (SCALE[candidate] - SCALE[scale_index]).abs();
                used_fourth |= interval > 4.0;
                scale_index = candidate;
                pitches.push(SCALE[scale_index]);
            }
            let ending = chord_tones
                .iter()
                .min_by(|left, right| {
                    (left.1 - pitches[pitches.len() - 1])
                        .abs()
                        .total_cmp(&(right.1 - pitches[pitches.len() - 1]).abs())
                })
                .unwrap();
            pitches.push(ending.1);
            let durations = (0..note_count)
                .map(|note| [0.85, 1.1, 1.35, 1.6][(rng.next() as usize + note) % 4])
                .collect::<Vec<f32>>();
            let intervals = pitches
                .windows(2)
                .map(|pair| (pair[1] - pair[0]).round() as i8)
                .collect::<Vec<_>>();
            let rhythm = durations
                .iter()
                .map(|duration| (duration * 20.0).round() as u8)
                .collect::<Vec<_>>();
            let fingerprint = MotifFingerprint {
                intervals,
                rhythm,
                ending_role: (ending.1.round() as i32 % 12) as u8,
                timbre,
            };
            let conflicts = self.recent_fingerprints.iter().any(|recent| {
                recent == &fingerprint
                    || (recent.intervals == fingerprint.intervals
                        && recent.rhythm == fingerprint.rhythm
                        && recent.ending_role == fingerprint.ending_role)
            });
            if !conflicts {
                selected = Some((pitches, durations, fingerprint, rng));
                break;
            }
        }
        let (pitches, durations, fingerprint, mut rng) =
            selected.expect("neutral motif search must find a fresh candidate");
        let mut cursor = self.phrase_start;
        let notes = pitches
            .into_iter()
            .zip(durations)
            .map(|(midi, duration)| {
                let note = PlannedNote {
                    start: cursor,
                    midi,
                    duration,
                };
                cursor += duration * 0.72 + 0.2 + rng.unit() * 0.12;
                note
            })
            .collect::<Vec<_>>();

        let mut effect = effect_for_phrase(self.seed, index);
        if self.previous_effect_kind == Some(effect_kind(effect)) {
            effect = if matches!(effect, PhraseEffect::Dry) {
                reverb_for_phrase(self.seed, index)
            } else {
                PhraseEffect::Dry
            };
        }
        self.previous_effect_kind = Some(effect_kind(effect));
        self.recent_fingerprints.push_back(fingerprint.clone());
        if self.recent_fingerprints.len() > 12 {
            self.recent_fingerprints.pop_front();
        }
        self.phrase_start = cursor + 10.0 + rng.unit() * 12.0;
        self.phrase_index += 1;
        PhrasePlan {
            index,
            notes,
            timbre,
            effect,
            fingerprint,
        }
    }

    #[cfg(test)]
    fn recent_fingerprint_count(&self) -> usize {
        self.recent_fingerprints.len()
    }
}

fn reverb_for_phrase(seed: u64, index: u64) -> PhraseEffect {
    let mut rng = event_rng(seed, 0x5245_5645_5242_3031, index, 0);
    PhraseEffect::Reverb {
        send: 0.08 + rng.unit() * 0.14,
        tail: 2.0 + rng.unit() * 2.5,
    }
}

fn delay_for_phrase(seed: u64, index: u64) -> PhraseEffect {
    let mut rng = event_rng(seed, 0x4445_4c41_595f_3031, index, 0);
    PhraseEffect::Delay {
        send: 0.1 + rng.unit() * 0.12,
        seconds: 0.28 + rng.unit() * 0.34,
        feedback: 0.16 + rng.unit() * 0.12,
        repeats: 2 + (rng.next() % 3) as u8,
        pan: -0.55 + rng.unit() * 1.1,
    }
}

fn effect_for_phrase(seed: u64, index: u64) -> PhraseEffect {
    match index {
        0 => PhraseEffect::Dry,
        1 => reverb_for_phrase(seed, index),
        2 => delay_for_phrase(seed, index),
        _ => {
            let mut rng = event_rng(seed, 0x4546_4645_4354_3031, index, 0);
            match rng.next() % 100 {
                0..=49 => PhraseEffect::Dry,
                50..=84 => reverb_for_phrase(seed, index),
                _ => delay_for_phrase(seed, index),
            }
        }
    }
}

fn chord_at_time(seed: u64, time: f32) -> [f32; 3] {
    let mut timeline = AmbientTimeline::new(seed);
    let mut chord = VOICINGS[0];
    loop {
        let region = timeline.next_region();
        if region.start > time {
            break;
        }
        chord = region.chord;
        if region.start + region.duration >= time && region.start > time - 5.5 {
            break;
        }
    }
    chord
}

pub fn collect_plan(seed: u64, seconds: f32) -> AmbientPlan {
    let mut timeline = AmbientTimeline::new(seed);
    let mut regions = Vec::new();
    loop {
        let region = timeline.next_region();
        if region.start >= seconds {
            break;
        }
        regions.push(region);
    }
    let mut phrases = Vec::new();
    loop {
        let phrase = timeline.next_phrase();
        if phrase.notes[0].start >= seconds {
            break;
        }
        phrases.push(phrase);
    }
    AmbientPlan { regions, phrases }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SEED: u64 = 0x5052_4f47_5245_5353;

    #[test]
    fn timeline_extends_deterministically_with_bounded_state() {
        let first = collect_plan(TEST_SEED, 240.0);
        assert_eq!(first, collect_plan(TEST_SEED, 240.0));
        assert_ne!(first, collect_plan(TEST_SEED ^ 1, 240.0));
        assert!(
            first
                .regions
                .windows(2)
                .all(|pair| pair[0].start < pair[1].start)
        );
        assert!(
            first
                .phrases
                .windows(2)
                .all(|pair| { pair[0].notes[0].start < pair[1].notes[0].start })
        );
        assert!(first.phrases.iter().enumerate().all(|(index, phrase)| {
            phrase.timbre
                == if index % 2 == 0 {
                    PhraseTimbre::FeltPiano
                } else {
                    PhraseTimbre::BowedStrings
                }
        }));

        let mut timeline = AmbientTimeline::new(TEST_SEED);
        for _ in 0..100 {
            timeline.next_phrase();
        }
        assert_eq!(timeline.recent_fingerprint_count(), 12);
    }

    #[test]
    fn phrases_obey_neutral_grammar_and_recent_history_exclusion() {
        let plan = collect_plan(TEST_SEED, 480.0);
        assert!(plan.phrases.len() > 20);
        for phrase in &plan.phrases {
            assert!((4..=6).contains(&phrase.notes.len()));
            assert!(phrase.notes.iter().all(|note| {
                note.midi >= 57.0
                    && matches!(
                        (note.midi.round() as i32).rem_euclid(12),
                        0 | 2 | 4 | 5 | 7 | 9 | 11
                    )
            }));
            let wide_leaps = phrase
                .notes
                .windows(2)
                .map(|pair| (pair[1].midi - pair[0].midi).abs())
                .filter(|interval| *interval > 4.0)
                .collect::<Vec<_>>();
            assert!(wide_leaps.len() <= 1);
            assert!(wide_leaps.iter().all(|interval| *interval <= 5.0));
        }
        for (index, phrase) in plan.phrases.iter().enumerate() {
            let recent_start = index.saturating_sub(12);
            assert!(
                !plan.phrases[recent_start..index]
                    .iter()
                    .any(|recent| recent.fingerprint == phrase.fingerprint)
            );
        }
        assert_ne!(
            plan.phrases[..5]
                .iter()
                .map(|phrase| &phrase.fingerprint)
                .collect::<Vec<_>>(),
            plan.phrases[5..10]
                .iter()
                .map(|phrase| &phrase.fingerprint)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn three_minute_fixture_contains_sparse_nonrepeating_effect_treatments() {
        let plan = collect_plan(TEST_SEED, 180.0);
        assert!(
            plan.phrases
                .iter()
                .any(|phrase| matches!(phrase.effect, PhraseEffect::Dry))
        );
        assert!(
            plan.phrases
                .iter()
                .any(|phrase| matches!(phrase.effect, PhraseEffect::Reverb { .. }))
        );
        assert!(
            plan.phrases
                .iter()
                .any(|phrase| matches!(phrase.effect, PhraseEffect::Delay { .. }))
        );
        assert!(plan.phrases.windows(2).all(|pair| {
            std::mem::discriminant(&pair[0].effect) != std::mem::discriminant(&pair[1].effect)
        }));
        assert!(
            plan.phrases
                .iter()
                .filter(|phrase| !matches!(phrase.effect, PhraseEffect::Dry))
                .count()
                < plan.phrases.len()
        );
    }
}
