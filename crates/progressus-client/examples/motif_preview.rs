//! Export neutral motif auditions without registering them in the game client.
#[path = "../src/motif_generation.rs"]
mod motif_generation;

use motif_generation::{AuditionKind, EndingRole, Motif, SAMPLE_RATE, audition_wav, shortlist};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

const NEUTRAL_SEED: u64 = 0x5052_4f47_5245_5353;
const MOTIF_COUNT: usize = 10;

fn role_name(role: EndingRole) -> &'static str {
    match role {
        EndingRole::Open => "open",
        EndingRole::Resting => "resting",
        EndingRole::Resolving => "resolving",
    }
}

fn relative_degrees(motif: &Motif) -> Vec<i8> {
    let root = motif.degrees[0];
    motif.degrees.iter().map(|degree| degree - root).collect()
}

fn wav_from_pcm16(pcm: &[u8]) -> Vec<u8> {
    let data_bytes = u32::try_from(pcm.len()).expect("sampler PCM fits WAV");
    let mut wav = Vec::with_capacity(pcm.len() + 44);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(data_bytes + 36).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 4).to_le_bytes());
    wav.extend_from_slice(&4_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());
    wav.extend_from_slice(pcm);
    wav
}

fn export_kind(
    root: &Path,
    motifs: &[Motif],
    kind: AuditionKind,
    log: &mut String,
) -> Result<(), Box<dyn Error>> {
    let mut sampler_pcm = Vec::new();
    for (index, motif) in motifs.iter().enumerate() {
        let timer = Instant::now();
        let wav = audition_wav(motif, index as u64, kind);
        let elapsed_ms = timer.elapsed().as_secs_f64() * 1_000.0;
        let filename = format!("motif-{:02}-{}.wav", index + 1, kind.name());
        fs::write(root.join(&filename), &wav)?;
        sampler_pcm.extend_from_slice(&wav[44..]);
        if index + 1 < motifs.len() {
            sampler_pcm.resize(sampler_pcm.len() + SAMPLE_RATE as usize * 2 * 2 * 2, 0);
        }
        let line = format!(
            "file={filename} generation_ms={elapsed_ms:.3} bytes={}\n",
            wav.len()
        );
        print!("{line}");
        log.push_str(&line);
    }
    fs::write(
        root.join(format!("neutral-motifs-{}-sampler.wav", kind.name())),
        wav_from_pcm16(&sampler_pcm),
    )?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("provide an output directory")?,
    );
    fs::create_dir_all(&root)?;
    let motifs = shortlist(NEUTRAL_SEED, MOTIF_COUNT);
    let mut manifest = format!(
        "Progressus neutral motif audition\nseed=0x{NEUTRAL_SEED:016x}\ncandidates=48\nshortlist={}\n\n",
        motifs.len()
    );
    for (index, motif) in motifs.iter().enumerate() {
        manifest.push_str(&format!(
            "{:02}: role={} degrees={:?} relative={:?} durations={:?}\n",
            index + 1,
            role_name(motif.role),
            motif.degrees,
            relative_degrees(motif),
            motif.durations
        ));
    }
    fs::write(root.join("manifest.txt"), manifest)?;

    let mut log = String::new();
    export_kind(&root, &motifs, AuditionKind::Isolated, &mut log)?;
    export_kind(&root, &motifs, AuditionKind::Contextual, &mut log)?;
    fs::write(root.join("generation.log"), log)?;
    println!("output={}", root.display());
    Ok(())
}
