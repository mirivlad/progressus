//! Standalone continuous-ambient preview exporter.
#[path = "../src/ambient_synthesis.rs"]
mod ambient_synthesis;
#[path = "../src/audio_synthesis.rs"]
mod audio_synthesis;

use ambient_synthesis::{AmbientLayer, SAMPLE_RATE, WORK_VOICE_GAIN, layer_wav};
use audio_synthesis::{EffectKind, effect_wav};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

const SECONDS: usize = 240;
const FADE: f64 = 6.0;
const MUSIC_GAIN: f32 = 0.25;
const EFFECT_GAIN: f32 = 0.50;

fn samples(wav: &[u8]) -> Vec<f32> {
    assert_eq!(&wav[..4], b"RIFF");
    wav[44..]
        .chunks_exact(2)
        .map(|p| i16::from_le_bytes([p[0], p[1]]) as f32 / 32767.0)
        .collect()
}

fn fixed_wav(samples: &[f32]) -> Vec<u8> {
    let bytes = u32::try_from(samples.len() * 2).expect("preview PCM fits WAV");
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
        out.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    out
}

fn name(layer: AmbientLayer) -> &'static str {
    match layer {
        AmbientLayer::Foundation => "foundation",
        AmbientLayer::Melody => "melody",
        AmbientLayer::Nature => "nature",
    }
}

fn add_layer(mix: &mut [f32], layer: AmbientLayer, index: u64, start: f64) {
    let timer = Instant::now();
    let pcm = samples(&layer_wav(layer, index));
    println!(
        "layer={} index={index} generation_ms={:.3}",
        name(layer),
        timer.elapsed().as_secs_f64() * 1000.0
    );
    let first = (start * SAMPLE_RATE as f64) as usize;
    for frame in 0..pcm.len() / 2 {
        if first + frame >= mix.len() / 2 {
            break;
        }
        let local = frame as f64 / SAMPLE_RATE as f64;
        let fade_in = (local / FADE).min(1.0) as f32;
        let fade_out = if layer.looping() {
            ((layer.seconds() - local) / FADE).clamp(0.0, 1.0) as f32
        } else {
            1.0
        };
        let gain = fade_in * fade_out * MUSIC_GAIN;
        mix[(first + frame) * 2] += pcm[frame * 2] * gain;
        mix[(first + frame) * 2 + 1] += pcm[frame * 2 + 1] * gain;
    }
}

fn ambient_mix() -> Vec<f32> {
    let mut mix = vec![0.0; SECONDS * SAMPLE_RATE as usize * 2];
    for layer in AmbientLayer::ALL {
        let stride = if layer.looping() {
            layer.seconds() - FADE
        } else {
            layer.seconds()
        };
        let (mut index, mut start) = (0, 0.0);
        while start < SECONDS as f64 {
            add_layer(&mut mix, layer, index, start);
            index += 1;
            start += stride;
        }
    }
    mix
}

fn add_effect(mix: &mut [f32], kind: EffectKind, variant: u64, at: f64, pan: f32) {
    let timer = Instant::now();
    let pcm = samples(&effect_wav(kind, variant));
    println!(
        "scripted_demo_effect={} variant={variant} at={at:.1}s generation_ms={:.3}",
        kind.name(),
        timer.elapsed().as_secs_f64() * 1000.0
    );
    let first = (at * SAMPLE_RATE as f64) as usize;
    let (left, right) = (((1.0 - pan) * 0.5).sqrt(), ((1.0 + pan) * 0.5).sqrt());
    for frame in 0..pcm.len() / 2 {
        if first + frame >= mix.len() / 2 {
            break;
        }
        let gain = EFFECT_GAIN * WORK_VOICE_GAIN;
        mix[(first + frame) * 2] += pcm[frame * 2] * gain * left;
        mix[(first + frame) * 2 + 1] += pcm[frame * 2 + 1] * gain * right;
    }
}

fn excerpt(pcm: &[f32], start: usize) -> Vec<f32> {
    let first = start * SAMPLE_RATE as usize * 2;
    let mut out = pcm[first..first + 45 * SAMPLE_RATE as usize * 2].to_vec();
    let frames = out.len() / 2;
    for (frame, pair) in out.chunks_exact_mut(2).enumerate() {
        let fade = (frame as f32 / SAMPLE_RATE as f32).min(1.0)
            * ((frames - 1 - frame) as f32 / SAMPLE_RATE as f32).min(1.0);
        pair[0] *= fade;
        pair[1] *= fade;
    }
    out
}

fn write(path: &Path, pcm: &[f32]) -> Result<(), Box<dyn Error>> {
    let peak = pcm.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
    if peak >= 1.0 {
        return Err(format!("{} would clip: peak={peak}", path.display()).into());
    }
    println!("write={} peak={peak:.4}", path.display());
    fs::write(path, fixed_wav(pcm))?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("provide an output directory")?,
    );
    fs::create_dir_all(&root)?;
    for layer in AmbientLayer::ALL {
        let timer = Instant::now();
        let wav = layer_wav(layer, 0);
        println!(
            "isolated_layer={} generation_ms={:.3}",
            name(layer),
            timer.elapsed().as_secs_f64() * 1000.0
        );
        fs::write(root.join(format!("isolated-{}.wav", name(layer))), wav)?;
    }
    let quiet = ambient_mix();
    write(&root.join("quiet-continuous-4min.wav"), &quiet)?;
    write(
        &root.join("quiet-continuous-excerpt-45s.wav"),
        &excerpt(&quiet, 52),
    )?;
    // Explicitly scripted listening demonstration, not captured gameplay.
    let mut work = quiet.clone();
    for (at, kind, variant, pan) in [
        (18.0, EffectKind::Harvest, 0, -0.35),
        (18.8, EffectKind::Harvest, 1, -0.25),
        (47.0, EffectKind::Pickup, 2, 0.2),
        (70.0, EffectKind::Construct, 0, 0.3),
        (71.1, EffectKind::Construct, 2, 0.38),
        (96.0, EffectKind::Craft, 1, -0.15),
        (96.7, EffectKind::Craft, 2, -0.05),
        (126.0, EffectKind::Drop, 0, 0.35),
        (158.0, EffectKind::Construct, 1, -0.3),
        (159.0, EffectKind::Construct, 0, -0.2),
        (202.0, EffectKind::Pickup, 1, 0.12),
        (218.0, EffectKind::Craft, 0, 0.28),
    ] {
        add_effect(&mut work, kind, variant, at, pan);
    }
    write(&root.join("scripted-work-demo-4min.wav"), &work)?;
    write(
        &root.join("scripted-work-demo-excerpt-45s.wav"),
        &excerpt(&work, 60),
    )?;
    let mut sampler = Vec::new();
    for kind in EffectKind::ALL {
        for variant in 0..3 {
            let wav = effect_wav(kind, variant);
            fs::write(
                root.join(format!("effect-{}-{variant}.wav", kind.name())),
                &wav,
            )?;
            sampler.extend(samples(&wav));
            sampler.resize(sampler.len() + SAMPLE_RATE as usize, 0.0);
        }
    }
    fs::write(root.join("effects-sampler.wav"), fixed_wav(&sampler))?;
    println!("scripted-work-demo is synthetic demonstration activity, not a gameplay capture");
    Ok(())
}
