//! Export the continuous neutral ambient audition without runtime registration.
#[allow(dead_code)]
#[path = "../src/score_audition.rs"]
mod score_audition;

use score_audition::{
    SAMPLE_RATE,
    ambient_timeline::{PhraseEffect, collect_plan},
    continuous_wav,
};
use std::{error::Error, fs, path::PathBuf, time::Instant};

const PREVIEW_SEED: u64 = 0x5052_4f47_5245_5353;
const PREVIEW_SECONDS: usize = 180;

struct WavStats {
    peak: f32,
    rms: f32,
    clipped_samples: usize,
}

fn wav_stats(wav: &[u8]) -> WavStats {
    let samples = wav[44..]
        .chunks_exact(2)
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]) as f32 / 32768.0)
        .collect::<Vec<_>>();
    let peak = samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0, f32::max);
    let rms =
        (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt();
    let clipped_samples = samples
        .iter()
        .filter(|sample| sample.abs() >= 0.999_969)
        .count();
    WavStats {
        peak,
        rms,
        clipped_samples,
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let output = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("provide an output directory")?,
    );
    fs::create_dir_all(&output)?;

    let timer = Instant::now();
    let wav = continuous_wav(PREVIEW_SEED, PREVIEW_SECONDS);
    let elapsed_ms = timer.elapsed().as_secs_f64() * 1_000.0;
    let stats = wav_stats(&wav);
    let filename = "continuous-alternating-180s.wav";
    fs::write(output.join(filename), &wav)?;

    let plan = collect_plan(PREVIEW_SEED, PREVIEW_SECONDS as f32);
    let dry = plan
        .phrases
        .iter()
        .filter(|phrase| matches!(phrase.effect, PhraseEffect::Dry))
        .count();
    let reverb = plan
        .phrases
        .iter()
        .filter(|phrase| matches!(phrase.effect, PhraseEffect::Reverb { .. }))
        .count();
    let delay = plan
        .phrases
        .iter()
        .filter(|phrase| matches!(phrase.effect, PhraseEffect::Delay { .. }))
        .count();

    let manifest = format!(
        "Progressus continuous neutral ambient audition\nseed=0x{PREVIEW_SEED:016x}\nduration_seconds={PREVIEW_SECONDS}\nsample_rate={SAMPLE_RATE}\nchannels=2\nbackground_gain=0.8\nducking=8-12%\nphrases={}\ndry_phrases={dry}\nreverb_phrases={reverb}\ndelay_phrases={delay}\nforeground=alternating felt piano and bowed strings\n",
        plan.phrases.len()
    );
    let log = format!(
        "file={filename} generation_ms={elapsed_ms:.3} bytes={}\n",
        wav.len()
    );
    let verification = format!(
        "{{\n  \"file\": \"{filename}\",\n  \"sample_rate\": {SAMPLE_RATE},\n  \"channels\": 2,\n  \"bits_per_sample\": 16,\n  \"duration_seconds\": {PREVIEW_SECONDS},\n  \"peak\": {:.6},\n  \"rms\": {:.6},\n  \"clipped_samples\": {}\n}}\n",
        stats.peak, stats.rms, stats.clipped_samples
    );
    fs::write(output.join("manifest.txt"), manifest)?;
    fs::write(output.join("generation.log"), &log)?;
    fs::write(output.join("verification.json"), verification)?;
    print!("{log}");
    println!("output={}", output.display());
    Ok(())
}
