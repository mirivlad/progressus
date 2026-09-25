//! Standalone, presentation-only ambient texture auditions.
//! No score, melody, simulation input, or runtime registration.

use std::{error::Error, f32::consts::TAU, fs, path::PathBuf};

#[allow(dead_code)]
#[path = "../src/ambient_timeline.rs"]
mod ambient_timeline;

const RATE: usize = 24_000;
const SECONDS: usize = 90;

#[derive(Clone, Copy)]
enum Texture {
    WarmAir,
    OpenSpace,
    LivingAir,
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut x = self.0;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        x ^ (x >> 31)
    }

    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / 16_777_216.0
    }
}

fn value(seed: u64, index: u64) -> f32 {
    let mut rng = Rng(seed ^ index.wrapping_mul(0xd134_2543_de82_ef95));
    rng.unit()
}

fn drift(seed: u64, seconds: f32, interval: f32) -> f32 {
    let cell = (seconds / interval).floor() as u64;
    let fraction = seconds / interval - cell as f32;
    let smooth = fraction * fraction * (3.0 - 2.0 * fraction);
    value(seed, cell) * (1.0 - smooth) + value(seed, cell + 1) * smooth
}

fn alpha(hz: f32) -> f32 {
    1.0 - (-TAU * hz / RATE as f32).exp()
}

fn midi_hz(midi: f32) -> f32 {
    440.0 * 2.0_f32.powf((midi - 69.0) / 12.0)
}

fn add_harmonic_wash(pcm: &mut [f32], seed: u64, gain: f32, duration: f32) {
    let plan = ambient_timeline::collect_plan(seed, SECONDS as f32);
    for region in plan.regions {
        let start = region.start + 1.0;
        let first = (start * RATE as f32) as usize;
        let count = (duration * RATE as f32) as usize;
        for (voice, midi) in region.chord.into_iter().enumerate() {
            let base = midi_hz(midi);
            let pan = (voice as f32 - 1.0) * 0.27;
            for offset in 0..count.min(SECONDS * RATE - first.min(SECONDS * RATE)) {
                let t = offset as f32 / RATE as f32;
                let rise = (t / 1.1).min(1.0);
                let fall = ((duration - t) / 1.5).clamp(0.0, 1.0);
                let envelope = rise * rise * fall * fall;
                let phase = TAU * base * t;
                let tone = (phase * 0.997).sin() * 0.33
                    + phase.sin() * 0.38
                    + (phase * 1.004).sin() * 0.25
                    + (phase * 2.001).sin() * 0.08;
                let sample = tone * envelope * gain * region.gain;
                let frame = first + offset;
                pcm[frame * 2] += sample * (1.0 - pan);
                pcm[frame * 2 + 1] += sample * (1.0 + pan);
            }
        }
    }
}

struct Filters {
    low: f32,
    body_low: f32,
    body_high: f32,
    air_high: f32,
}

impl Filters {
    fn sample(&mut self, white: f32) -> [f32; 3] {
        self.low += alpha(95.0) * (white - self.low);
        self.body_low += alpha(240.0) * (white - self.body_low);
        self.body_high += alpha(1_100.0) * (white - self.body_high);
        self.air_high += alpha(3_100.0) * (white - self.air_high);
        [
            self.low,
            self.body_high - self.body_low,
            self.air_high - self.body_high,
        ]
    }
}

fn render(seed: u64, texture: Texture) -> Vec<f32> {
    let mut pcm = vec![0.0; SECONDS * RATE * 2];
    let mut noise = [Rng(seed ^ 0x4c45_4654), Rng(seed ^ 0x5249_4748)];
    let mut filters = [
        Filters {
            low: 0.0,
            body_low: 0.0,
            body_high: 0.0,
            air_high: 0.0,
        },
        Filters {
            low: 0.0,
            body_low: 0.0,
            body_high: 0.0,
            air_high: 0.0,
        },
    ];
    let gains = match texture {
        Texture::WarmAir => [0.10, 0.065, 0.012],
        Texture::OpenSpace => [0.045, 0.070, 0.055],
        Texture::LivingAir => [0.075, 0.070, 0.042],
    };
    for frame in 0..SECONDS * RATE {
        let t = frame as f32 / RATE as f32;
        let low_motion = 0.25 + 0.75 * drift(seed ^ 1, t, 11.0);
        let body_motion = 0.20 + 0.80 * drift(seed ^ 2, t, 7.0);
        let high_motion = 0.10 + 0.90 * drift(seed ^ 3, t, 4.5);
        let stereo = (drift(seed ^ 4, t, 13.0) - 0.5) * 0.45;
        let fade = (t / 3.0).min(1.0) * ((SECONDS as f32 - t) / 3.0).min(1.0);
        for channel in 0..2 {
            let white = noise[channel].unit() * 2.0 - 1.0;
            let [low, body, air] = filters[channel].sample(white);
            let side = if channel == 0 {
                1.0 + stereo
            } else {
                1.0 - stereo
            };
            pcm[frame * 2 + channel] = (low * gains[0] * low_motion
                + body * gains[1] * body_motion
                + air * gains[2] * high_motion)
                * side
                * fade;
        }
    }
    match texture {
        Texture::WarmAir => add_harmonic_wash(&mut pcm, seed, 0.019, 5.0),
        Texture::OpenSpace => add_harmonic_wash(&mut pcm, seed ^ 0x0053_5041_4345, 0.012, 3.8),
        Texture::LivingAir => {}
    }
    if matches!(texture, Texture::LivingAir) {
        let mut events = Rng(seed ^ 0x5255_5354_4c45);
        let mut cursor = 4.0;
        while cursor < SECONDS as f32 - 4.0 {
            let duration = 0.6 + events.unit() * 1.4;
            let first = (cursor * RATE as f32) as usize;
            let count = (duration * RATE as f32) as usize;
            let pan = events.unit() * 0.7 - 0.35;
            let mut filtered = 0.0;
            for offset in 0..count.min(SECONDS * RATE - first) {
                let progress = offset as f32 / count as f32;
                let envelope = (progress * std::f32::consts::PI).sin().powi(2);
                let white = events.unit() * 2.0 - 1.0;
                filtered += alpha(1_400.0) * (white - filtered);
                let sample = (white - filtered) * envelope * 0.014;
                pcm[(first + offset) * 2] += sample * (1.0 - pan);
                pcm[(first + offset) * 2 + 1] += sample * (1.0 + pan);
            }
            cursor += 5.0 + events.unit() * 9.0;
        }
    }
    pcm
}

fn wav(pcm: &[f32]) -> Vec<u8> {
    const PREVIEW_GAIN: f32 = 1.8;
    let bytes = u32::try_from(pcm.len() * 2).expect("audition fits WAV");
    let mut out = Vec::with_capacity(bytes as usize + 44);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(bytes + 36).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&2_u16.to_le_bytes());
    out.extend_from_slice(&(RATE as u32).to_le_bytes());
    out.extend_from_slice(&((RATE * 4) as u32).to_le_bytes());
    out.extend_from_slice(&4_u16.to_le_bytes());
    out.extend_from_slice(&16_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&bytes.to_le_bytes());
    for sample in pcm {
        let sample = sample * PREVIEW_GAIN;
        assert!(sample.is_finite() && sample.abs() < 1.0);
        out.extend_from_slice(&((sample * 32767.0).round() as i16).to_le_bytes());
    }
    out
}

fn main() -> Result<(), Box<dyn Error>> {
    let output = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("provide an output directory")?,
    );
    fs::create_dir_all(&output)?;
    for (name, texture) in [
        ("01-warm-air.wav", Texture::WarmAir),
        ("02-open-space.wav", Texture::OpenSpace),
        ("03-living-air.wav", Texture::LivingAir),
    ] {
        fs::write(
            output.join(name),
            wav(&render(0x5052_4f47_5245_5353, texture)),
        )?;
    }
    Ok(())
}
