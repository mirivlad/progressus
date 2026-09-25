//! Listening-only arrangements of one deterministic timeline.
#[allow(dead_code)]
#[path = "../src/score_audition.rs"]
mod score_audition;

use score_audition::{AuditionArrangement, SAMPLE_RATE, audition_wav};
use std::{error::Error, fs, path::PathBuf};

const SEED: u64 = 0x5052_4f47_5245_5353;
const SECONDS: usize = 90;

fn main() -> Result<(), Box<dyn Error>> {
    let output = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("provide an output directory")?,
    );
    fs::create_dir_all(&output)?;
    for (name, arrangement) in [
        ("01-sparse-piano.wav", AuditionArrangement::SparsePiano),
        (
            "02-sustained-strings.wav",
            AuditionArrangement::SustainedStrings,
        ),
        (
            "03-gentle-alternation.wav",
            AuditionArrangement::GentleAlternation,
        ),
    ] {
        let wav = audition_wav(SEED, SECONDS, arrangement);
        assert_eq!(wav.len(), 44 + SECONDS * SAMPLE_RATE as usize * 4);
        fs::write(output.join(name), wav)?;
    }
    Ok(())
}
