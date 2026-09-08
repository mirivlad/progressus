//! Client-owned procedural audio. No engine, world state or external assets.

use std::f32::consts::TAU;

pub const SAMPLE_RATE: u32 = 24_000;
pub const MUSIC_SECONDS: f32 = 120.0;
pub const MUSIC_GAP_SECONDS: f32 = 20.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum EffectKind {
    Harvest,
    Construct,
    Craft,
    Pickup,
    Drop,
}
impl EffectKind {
    pub const ALL: [Self; 5] = [
        Self::Harvest,
        Self::Construct,
        Self::Craft,
        Self::Pickup,
        Self::Drop,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            Self::Harvest => "harvest",
            Self::Construct => "construction",
            Self::Craft => "craft",
            Self::Pickup => "pickup",
            Self::Drop => "drop",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Instrument {
    Pluck,
    Harp,
    Air,
    Bow,
    Wood,
}
#[derive(Clone, Debug, PartialEq)]
struct Note {
    start: f32,
    duration: f32,
    midi: f32,
    gain: f32,
    pan: f32,
    instrument: Instrument,
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut x = self.0;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
        x ^ (x >> 31)
    }
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / 16_777_216.0
    }
}

fn score(index: u64) -> Vec<Note> {
    let family = index % 3;
    let mut rng = Rng(index ^ 0x70726f6772657373);
    let transposition = ((index / 3) % 5) as f32 - 2.0;
    let (root, beat, bar_beats, progression, lead) = match family {
        0 => (
            50.0,
            60.0 / 66.0,
            4,
            [
                (0., 4., 7.),
                (7., 4., 7.),
                (9., 3., 7.),
                (5., 4., 7.),
                (2., 3., 7.),
                (7., 4., 7.),
            ],
            Instrument::Pluck,
        ),
        1 => (
            57.0,
            60.0 / 78.0,
            3,
            [
                (0., 3., 7.),
                (5., 4., 7.),
                (10., 4., 7.),
                (3., 4., 7.),
                (7., 3., 7.),
                (0., 3., 7.),
            ],
            Instrument::Harp,
        ),
        _ => (
            48.0,
            60.0 / 54.0,
            4,
            [
                (0., 5., 7.),
                (10., 5., 7.),
                (5., 2., 7.),
                (0., 5., 9.),
                (7., 5., 7.),
                (5., 2., 7.),
            ],
            Instrument::Bow,
        ),
    };
    let root = root + transposition;
    let bar = beat * bar_beats as f32;
    let chord_span = if family == 2 { bar * 3.0 } else { bar * 2.0 };
    let mut notes = Vec::new();
    let mut chord_number = 0;
    let mut start = 0.0;
    let mut previous_tone = (rng.next() % 3) as usize;
    while start < MUSIC_SECONDS - 5.0 {
        let (offset, third, fifth) = progression[chord_number % progression.len()];
        let chord = [root + offset, root + offset + third, root + offset + fifth];
        // A low, spacious harmonic bed leaves room for the melody and game sounds.
        notes.push(Note {
            start,
            duration: chord_span.min(10.0),
            midi: chord[0] - 12.0,
            gain: 0.095,
            pan: -0.15,
            instrument: if family == 2 {
                Instrument::Bow
            } else {
                Instrument::Air
            },
        });
        for (tone, midi) in chord.iter().enumerate() {
            notes.push(Note {
                start: start + tone as f32 * 0.16,
                duration: chord_span * 0.85,
                midi: *midi,
                gain: 0.065,
                pan: (tone as f32 - 1.0) * 0.48,
                instrument: Instrument::Air,
            });
        }
        // Distinct metres and densities, with an opening, active middle and thinning ending.
        let density = match family {
            0 => 7,
            1 => 11,
            _ => 4,
        };
        let count = if !(22.0..98.0).contains(&start) {
            density / 2
        } else {
            density
        };
        for step in 0..count {
            previous_tone = (previous_tone + 1 + (rng.next() % 2) as usize) % 3;
            let octave = if family == 1 || (step % 4 == 2) {
                12.0
            } else {
                0.0
            };
            let time = start + 0.45 + step as f32 * chord_span / density as f32 + rng.unit() * 0.08;
            if time >= MUSIC_SECONDS - 4.0 {
                continue;
            }
            notes.push(Note {
                start: time,
                duration: if family == 2 { 3.8 } else { 2.6 },
                midi: chord[previous_tone] + octave,
                gain: 0.18 + rng.unit() * 0.09,
                pan: rng.unit() * 1.1 - 0.55,
                instrument: lead,
            });
            if family == 0 && step % 5 == 3 {
                notes.push(Note {
                    start: time + beat * 0.5,
                    duration: 2.0,
                    midi: chord[(previous_tone + 1) % 3] + 12.0,
                    gain: 0.09,
                    pan: 0.3,
                    instrument: Instrument::Air,
                });
            }
        }
        if family == 1 && (30.0..90.0).contains(&start) {
            notes.push(Note {
                start: start + bar,
                duration: 0.35,
                midi: 57.0,
                gain: 0.055,
                pan: -0.4,
                instrument: Instrument::Wood,
            });
        }
        chord_number += 1;
        start += chord_span;
    }
    notes
}

fn render_note(samples: &mut [f32], note: &Note) {
    let start = (note.start * SAMPLE_RATE as f32) as usize;
    let count = (note.duration * SAMPLE_RATE as f32) as usize;
    let hz = 440.0 * 2.0_f32.powf((note.midi - 69.0) / 12.0);
    let left = ((1.0 - note.pan) * 0.5).sqrt();
    let right = ((1.0 + note.pan) * 0.5).sqrt();
    for n in 0..count.min(samples.len() / 2 - start.min(samples.len() / 2)) {
        let t = n as f32 / SAMPLE_RATE as f32;
        let phase = TAU * hz * t;
        let end = ((note.duration - t) / 0.3).clamp(0.0, 1.0);
        let value = match note.instrument {
            Instrument::Pluck | Instrument::Harp => {
                let harp = note.instrument == Instrument::Harp;
                let decay = if harp { 1.1 } else { 1.9 };
                let body = phase.sin()
                    + 0.40 * (phase * 2.002).sin() * (-t * 2.0).exp()
                    + 0.20 * (phase * 3.008).sin() * (-t * 3.8).exp()
                    + 0.09 * (phase * 4.015).sin() * (-t * 5.0).exp();
                body * (1.0 - (-t * 130.0).exp()) * (-t * decay).exp()
            }
            Instrument::Air => {
                let env = (t / 0.65).min(1.0) * ((note.duration - t) / 1.5).clamp(0.0, 1.0);
                (phase.sin() + 0.12 * (phase * 2.0 + 0.006 * (TAU * 4.5 * t).sin()).sin())
                    * env
                    * 0.65
            }
            Instrument::Bow => {
                let env = (t / 0.9).min(1.0) * ((note.duration - t) / 1.1).clamp(0.0, 1.0);
                let p = phase + 0.025 * (TAU * 4.1 * t).sin();
                (p.sin() + 0.23 * (p * 2.0).sin() + 0.11 * (p * 3.0).sin()) * env * 0.65
            }
            Instrument::Wood => {
                (phase.sin() + 0.45 * (phase * 2.71).sin())
                    * (1.0 - (-t * 500.0).exp())
                    * (-t * 28.0).exp()
            }
        } * note.gain
            * end;
        samples[(start + n) * 2] += value * left;
        samples[(start + n) * 2 + 1] += value * right;
    }
}

pub fn music_wav(index: u64) -> Vec<u8> {
    let mut samples = vec![0.0; SAMPLE_RATE as usize * MUSIC_SECONDS as usize * 2];
    for note in score(index) {
        render_note(&mut samples, &note);
    }
    // A bounded cross-channel room tail, applied once to the completed buffer.
    for frame in 3001..samples.len() / 2 {
        samples[frame * 2] += samples[(frame - 1901) * 2 + 1] * 0.13;
        samples[frame * 2 + 1] += samples[(frame - 3001) * 2] * 0.11;
    }
    for frame in 0..samples.len() / 2 {
        let t = frame as f32 / SAMPLE_RATE as f32;
        let fade = (t / 3.0).min(1.0)
            * ((MUSIC_SECONDS - 1.0 / SAMPLE_RATE as f32 - t) / 5.0).clamp(0.0, 1.0);
        samples[frame * 2] *= fade;
        samples[frame * 2 + 1] *= fade;
    }
    wav(&samples)
}

pub fn effect_wav(kind: EffectKind, variant: u64) -> Vec<u8> {
    let duration = match kind {
        EffectKind::Harvest => 0.55,
        EffectKind::Construct => 0.48,
        EffectKind::Craft => 0.35,
        EffectKind::Pickup => 0.22,
        EffectKind::Drop => 0.38,
    };
    let mut samples = vec![0.0; (duration * SAMPLE_RATE as f32) as usize * 2];
    let mut rng = Rng(variant ^ (kind as u64).wrapping_mul(0x9e3779b97f4a7c15));
    let pitch = 0.90 + rng.unit() * 0.2;
    let mut filtered = 0.0;
    let frames = samples.len() / 2;
    for frame in 0..frames {
        let t = frame as f32 / SAMPLE_RATE as f32;
        filtered = filtered * 0.65 + (rng.unit() * 2.0 - 1.0) * 0.35;
        let (hz, noise, decay) = match kind {
            EffectKind::Harvest => (170., 0.85, 10.),
            EffectKind::Construct => (440., 0.25, 16.),
            EffectKind::Craft => (760., 0.14, 23.),
            EffectKind::Pickup => (220., 0.55, 24.),
            EffectKind::Drop => (110., 0.5, 17.),
        };
        let phase = TAU * hz * pitch * t;
        let envelope = (t / 0.004).min(1.0)
            * (-t * decay).exp()
            * ((frames - 1 - frame) as f32 / 240.).min(1.0);
        let value = (filtered * noise + (phase.sin() + 0.3 * (phase * 2.37).sin()) * (1.0 - noise))
            * envelope;
        samples[frame * 2] = value;
        samples[frame * 2 + 1] = value;
    }
    wav(&samples)
}

pub fn wav(samples: &[f32]) -> Vec<u8> {
    let peak = samples.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
    let gain = if peak > 0.0 {
        0.72 / peak.max(0.25)
    } else {
        0.0
    };
    let bytes = u32::try_from(samples.len() * 2).expect("bounded audio PCM fits WAV");
    let mut out = Vec::with_capacity(bytes as usize + 44);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(bytes + 36).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&2_u16.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 4).to_le_bytes());
    out.extend_from_slice(&4_u16.to_le_bytes());
    out.extend_from_slice(&16_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&bytes.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&((sample * gain * 32767.0).round() as i16).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pcm(bytes: &[u8]) -> Vec<i16> {
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(
            u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize,
            bytes.len() - 44
        );
        bytes[44..]
            .chunks_exact(2)
            .map(|s| i16::from_le_bytes([s[0], s[1]]))
            .collect()
    }

    #[test]
    fn effects_are_finite_audible_bounded_and_reproducible() {
        for kind in EffectKind::ALL {
            let wav = effect_wav(kind, 0);
            assert_eq!(wav, effect_wav(kind, 0));
            assert_ne!(wav, effect_wav(kind, 1));
            let samples = pcm(&wav);
            assert!(samples.len() < SAMPLE_RATE as usize * 2 * 2);
            assert!(samples.iter().any(|s| s.unsigned_abs() > 1000));
            assert!(samples.iter().all(|s| s.unsigned_abs() < 30000));
            assert_eq!(samples[0], 0);
            assert_eq!(*samples.last().unwrap(), 0);
        }
    }

    #[test]
    fn compositions_change_structure_not_only_seed() {
        let a = score(0);
        let b = score(1);
        let c = score(2);
        assert_eq!(a, score(0));
        assert_ne!(a, score(3));
        assert_ne!(a.len(), b.len());
        assert_ne!(b.len(), c.len());
        assert_ne!(a[0].midi, b[0].midi);
        assert_ne!(a[0].instrument, c[0].instrument);
        for notes in [a, b, c] {
            assert!(
                notes
                    .iter()
                    .all(|n| n.start >= 0.0 && n.start < MUSIC_SECONDS)
            );
        }
    }

    #[test]
    fn full_music_has_exact_duration_headroom_and_fades() {
        let samples = pcm(&music_wav(0));
        assert_eq!(
            samples.len(),
            SAMPLE_RATE as usize * MUSIC_SECONDS as usize * 2
        );
        assert!(samples.iter().any(|s| s.unsigned_abs() > 5000));
        assert!(samples.iter().all(|s| s.unsigned_abs() < 30000));
        assert_eq!(samples[0], 0);
        assert_eq!(*samples.last().unwrap(), 0);
    }
}
