//! Client-owned procedural audio. No engine, world state or external assets.

use std::f32::consts::TAU;

pub const SAMPLE_RATE: u32 = 24_000;

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
}
