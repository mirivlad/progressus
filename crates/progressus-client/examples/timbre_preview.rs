//! Export a fair A/B/C timbre comparison without registering it in the game client.
#[path = "../src/score_audition.rs"]
mod score_audition;

use score_audition::{
    COMPARISON_SECONDS, ForegroundTimbre, SAMPLE_RATE, comparison_wav, phrase_events,
};
use std::{error::Error, fs, path::PathBuf, time::Instant};

const COMPARISON_SEED: u64 = 0x5052_4f47_5245_5353;

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

    let variants = [
        ForegroundTimbre::FeltPiano,
        ForegroundTimbre::BowedStrings,
        ForegroundTimbre::Alternating,
    ];
    let mut log = String::new();
    let mut verification = format!(
        "{{\n  \"sample_rate\": {SAMPLE_RATE},\n  \"channels\": 2,\n  \"bits_per_sample\": 16,\n  \"duration_seconds\": {COMPARISON_SECONDS},\n  \"files\": [\n"
    );
    for (index, variant) in variants.iter().copied().enumerate() {
        let timer = Instant::now();
        let wav = comparison_wav(COMPARISON_SEED, variant);
        let elapsed_ms = timer.elapsed().as_secs_f64() * 1_000.0;
        let stats = wav_stats(&wav);
        fs::write(output.join(variant.filename()), &wav)?;
        log.push_str(&format!(
            "file={} generation_ms={elapsed_ms:.3} bytes={}\n",
            variant.filename(),
            wav.len()
        ));
        verification.push_str(&format!(
            "    {{\"file\": \"{}\", \"peak\": {:.6}, \"rms\": {:.6}, \"clipped_samples\": {}}}{}\n",
            variant.filename(),
            stats.peak,
            stats.rms,
            stats.clipped_samples,
            if index + 1 == variants.len() { "" } else { "," }
        ));
    }
    verification.push_str("  ]\n}\n");

    let events = phrase_events(COMPARISON_SEED);
    let mut manifest = format!(
        "Progressus neutral timbre comparison\nseed=0x{COMPARISON_SEED:016x}\nduration_seconds={COMPARISON_SECONDS}\nsample_rate={SAMPLE_RATE}\nchannels=2\n\n"
    );
    manifest.push_str("All versions use the same score and continuous background.\n");
    for variant in variants {
        manifest.push_str(&format!(
            "{}: foreground={}\n",
            variant.filename(),
            variant.label()
        ));
    }
    manifest.push_str("\nScore events (seconds, MIDI, duration):\n");
    for event in events {
        manifest.push_str(&format!(
            "{:.3} {:.0} {:.2}\n",
            event.start, event.midi, event.duration
        ));
    }
    fs::write(output.join("manifest.txt"), manifest)?;
    fs::write(output.join("generation.log"), &log)?;
    fs::write(output.join("verification.json"), verification)?;
    print!("{log}");
    println!("output={}", output.display());
    Ok(())
}
