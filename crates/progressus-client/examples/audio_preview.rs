//! Standalone preview generator: rustc -O --edition=2024 .../audio_preview.rs.
#[path = "../src/audio_synthesis.rs"]
mod audio_synthesis;

use audio_synthesis::{EffectKind, MUSIC_GAP_SECONDS, SAMPLE_RATE, effect_wav, music_wav, wav};
use std::{error::Error, fs, path::PathBuf, time::Instant};

fn samples(bytes: &[u8]) -> Vec<f32> {
    bytes[44..]
        .chunks_exact(2)
        .map(|s| i16::from_le_bytes([s[0], s[1]]) as f32 / 32767.0)
        .collect()
}
fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("provide an output directory")?,
    );
    fs::create_dir_all(&root)?;
    let mut sequence = Vec::new();
    for (index, name) in ["01-pastoral", "02-harp-waltz", "03-modal-rest"]
        .iter()
        .enumerate()
    {
        let started = Instant::now();
        let bytes = music_wav(index as u64);
        println!(
            "{name}: {} bytes, generation_ms={:.3}",
            bytes.len(),
            started.elapsed().as_secs_f64() * 1000.0
        );
        fs::write(root.join(format!("{name}.wav")), &bytes)?;
        let pcm = samples(&bytes);
        let mut excerpt =
            pcm[24 * SAMPLE_RATE as usize * 2..54 * SAMPLE_RATE as usize * 2].to_vec();
        let frames = excerpt.len() / 2;
        for (frame, pair) in excerpt.chunks_exact_mut(2).enumerate() {
            let fade = (frame as f32 / SAMPLE_RATE as f32).min(1.0)
                * ((frames - 1 - frame) as f32 / SAMPLE_RATE as f32).min(1.0);
            pair[0] *= fade;
            pair[1] *= fade;
        }
        fs::write(root.join(format!("{name}-excerpt.wav")), wav(&excerpt))?;
        if index < 2 {
            if index == 1 {
                sequence.resize(
                    sequence.len() + MUSIC_GAP_SECONDS as usize * SAMPLE_RATE as usize * 2,
                    0.0,
                );
            }
            sequence.extend(pcm);
        }
    }
    fs::write(root.join("music-120s-gap20s-new120s.wav"), wav(&sequence))?;
    let mut effects = Vec::new();
    for kind in EffectKind::ALL {
        for variant in 0..3 {
            let bytes = effect_wav(kind, variant);
            fs::write(root.join(format!("{}-{variant}.wav", kind.name())), &bytes)?;
            effects.extend(samples(&bytes));
            effects.resize(effects.len() + SAMPLE_RATE as usize, 0.0);
        }
        effects.resize(effects.len() + SAMPLE_RATE as usize * 2, 0.0);
    }
    fs::write(root.join("effects-sampler.wav"), wav(&effects))?;
    Ok(())
}
