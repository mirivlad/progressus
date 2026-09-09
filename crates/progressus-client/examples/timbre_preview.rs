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
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    duration_seconds: f64,
    peak: f32,
    rms: f32,
    clipped_samples: usize,
}

fn wav_stats(wav: &[u8]) -> Result<WavStats, Box<dyn Error>> {
    if wav.len() < 44
        || &wav[0..4] != b"RIFF"
        || &wav[8..12] != b"WAVE"
        || &wav[12..16] != b"fmt "
        || u32::from_le_bytes(wav[4..8].try_into()?) as usize != wav.len() - 8
        || u32::from_le_bytes(wav[16..20].try_into()?) != 16
        || u16::from_le_bytes(wav[20..22].try_into()?) != 1
    {
        return Err("generated output is not a canonical WAV file".into());
    }
    let channels = u16::from_le_bytes(wav[22..24].try_into()?);
    let sample_rate = u32::from_le_bytes(wav[24..28].try_into()?);
    let bits_per_sample = u16::from_le_bytes(wav[34..36].try_into()?);
    let data_bytes = u32::from_le_bytes(wav[40..44].try_into()?) as usize;
    let frame_bytes = channels as usize * usize::from(bits_per_sample / 8);
    if &wav[36..40] != b"data"
        || channels != 2
        || sample_rate != SAMPLE_RATE
        || bits_per_sample != 16
        || frame_bytes == 0
        || !data_bytes.is_multiple_of(frame_bytes)
        || wav.len() != 44 + data_bytes
    {
        return Err("generated WAV header does not match its PCM payload".into());
    }
    let samples = wav[44..44 + data_bytes]
        .chunks_exact(2)
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]) as f32 / 32768.0)
        .collect::<Vec<_>>();
    let peak = samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0, f32::max);
    let rms = (samples
        .iter()
        .map(|sample| f64::from(*sample) * f64::from(*sample))
        .sum::<f64>()
        / samples.len() as f64)
        .sqrt() as f32;
    let clipped_samples = samples
        .iter()
        .filter(|sample| sample.abs() >= 0.999_969)
        .count();
    let frames = samples.len() / channels as usize;
    Ok(WavStats {
        sample_rate,
        channels,
        bits_per_sample,
        duration_seconds: frames as f64 / sample_rate as f64,
        peak,
        rms,
        clipped_samples,
    })
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
    let stats = wav_stats(&wav)?;
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
        "{{\n  \"file\": \"{filename}\",\n  \"sample_rate\": {},\n  \"channels\": {},\n  \"bits_per_sample\": {},\n  \"duration_seconds\": {:.3},\n  \"peak\": {:.6},\n  \"rms\": {:.6},\n  \"clipped_samples\": {}\n}}\n",
        stats.sample_rate,
        stats.channels,
        stats.bits_per_sample,
        stats.duration_seconds,
        stats.peak,
        stats.rms,
        stats.clipped_samples
    );
    fs::write(output.join("manifest.txt"), manifest)?;
    fs::write(output.join("generation.log"), &log)?;
    fs::write(output.join("verification.json"), verification)?;
    print!("{log}");
    println!("output={}", output.display());
    Ok(())
}
