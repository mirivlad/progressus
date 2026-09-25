//! Bounded, presentation-only rendering of an unbounded ambient score.
//! Each window includes four seconds of history so the effects cross chunk edges.

use std::f32::consts::TAU;

use crate::ambient_score::{Chord, Note, PHRASES, RINGS};

pub(crate) const SAMPLE_RATE: u32 = 24_000;
const RATE: usize = SAMPLE_RATE as usize;
const BEAT: f32 = 60.0 / 76.0;
const BAR: f32 = BEAT * 4.0;
const SECTION_BARS: usize = 28;
const HISTORY: usize = 4;

fn hash(seed: u64, value: u64) -> u64 {
    let mut x = seed ^ value.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn hz(pitch: i32) -> f32 {
    440.0 * 2.0_f32.powf((pitch as f32 - 69.0) / 12.0)
}

fn section(bar: usize) -> (usize, usize, usize) {
    let global = bar / SECTION_BARS;
    (global % 3, bar % SECTION_BARS, global / 3)
}

fn chord(bar: usize) -> Chord {
    let (kind, local, _) = section(bar);
    RINGS[kind][local % 8]
}

fn voicing(chord: Chord) -> Vec<i32> {
    let mut notes = Vec::with_capacity(chord.notes.len());
    for &pitch in &chord.notes[1..] {
        let mut pitch = pitch;
        while pitch < 55 {
            pitch += 12;
        }
        if !notes.contains(&pitch) {
            notes.push(pitch);
        }
    }
    notes
}

fn add_note(
    buffer: &mut [f32],
    origin: f32,
    start: f32,
    end: f32,
    pitch: i32,
    level: f32,
    kind: u8,
) {
    let first = ((start - origin).max(0.0) * SAMPLE_RATE as f32) as usize;
    let last =
        (((end + 2.7 - origin).max(0.0) * SAMPLE_RATE as f32) as usize).min(buffer.len() / 2);
    if first >= last {
        return;
    }
    let freq = hz(pitch);
    for frame in first..last {
        let t = origin + frame as f32 / SAMPLE_RATE as f32 - start;
        if t < 0.0 {
            continue;
        }
        let release = (t - (end - start)).max(0.0);
        let envelope = match kind {
            0 => {
                ((t / 0.48).min(1.0) * std::f32::consts::FRAC_PI_2)
                    .sin()
                    .powi(2)
                    * (-release / 0.75).exp()
            }
            1 => (t / 0.012).min(1.0) * (-t / 0.85).exp() * (-release / 0.4).exp(),
            _ => (t / 0.012).min(1.0) * (-t / 0.85).exp() * (-release / 0.35).exp(),
        };
        if envelope < 0.00001 {
            continue;
        }
        for side in 0..2 {
            let detune = if side == 0 { 0.9975 } else { 1.0027 };
            let phase = TAU * freq * detune * t;
            let tone = match kind {
                0 => phase.sin() + 0.23 * (2.0 * phase + 0.2).sin() + 0.08 * (3.0 * phase).sin(),
                1 => phase.sin() + 0.28 * (2.0 * phase).sin() + 0.11 * (3.0 * phase).sin(),
                _ => phase.sin() + 0.22 * (2.0 * phase).sin() + 0.07 * (3.0 * phase).sin(),
            };
            buffer[frame * 2 + side] += level * envelope * tone;
        }
    }
}

fn add_lead(buffer: &mut [f32], origin: f32, bar: usize, notes: &[Note], seed: u64) {
    if notes.is_empty() {
        return;
    }
    let bar_start = bar as f32 * BAR;
    let first_note = bar_start + notes[0].beat * BEAT;
    let last_note = notes.last().unwrap();
    let end = bar_start + (last_note.beat + last_note.length) * BEAT;
    let first = ((first_note - origin).max(0.0) * SAMPLE_RATE as f32) as usize;
    let last =
        (((end + 2.5 - origin).max(0.0) * SAMPLE_RATE as f32) as usize).min(buffer.len() / 2);
    if first >= last {
        return;
    }
    let mut phase = [0.0_f32; 2];
    // Integrate from the actual note onset even when the requested window starts later.
    let skipped = ((origin - first_note).max(0.0) * SAMPLE_RATE as f32) as usize;
    for step in 0..(last - first + skipped) {
        let t = first_note + step as f32 / SAMPLE_RATE as f32;
        let mut note_index = 0;
        for (index, note) in notes.iter().enumerate().skip(1) {
            if t >= bar_start + note.beat * BEAT {
                note_index = index;
            } else {
                break;
            }
        }
        let mut freq = hz(notes[note_index].pitch);
        for index in 1..notes.len() {
            let onset = bar_start + notes[index].beat * BEAT;
            if t >= onset - 0.08 && t < onset + 0.15 {
                let p = ((t - onset + 0.08) / 0.23).clamp(0.0, 1.0);
                let smooth = p * p * (3.0 - 2.0 * p);
                let previous = hz(notes[index - 1].pitch);
                freq = previous * (hz(notes[index].pitch) / previous).powf(smooth);
                break;
            }
        }
        for (side, oscillator) in phase.iter_mut().enumerate() {
            *oscillator +=
                TAU * freq * if side == 0 { 0.9968 } else { 1.0034 } / SAMPLE_RATE as f32;
        }
        if step < skipped {
            continue;
        }
        let frame = first + step - skipped;
        if frame >= last {
            break;
        }
        let elapsed = t - first_note;
        let envelope = (elapsed / 0.07).clamp(0.0, 1.0) * (-(t - end).max(0.0) / 0.62).exp();
        let strength = 0.023 * (0.93 + (hash(seed, bar as u64) % 15) as f32 / 100.0);
        let color = if (bar / SECTION_BARS) % 5 == 2 {
            0.045
        } else {
            0.0
        };
        for side in 0..2 {
            let p = phase[side] + 0.0017 * (TAU * 4.4 * t + side as f32 * 1.7).sin();
            let tone = p.sin()
                + 0.46 * (2.0 * p).sin()
                + 0.24 * (3.0 * p).sin()
                + 0.12 * (4.0 * p).sin()
                + 0.13 * (0.5 * p).sin();
            let clean = strength * envelope * tone;
            buffer[frame * 2 + side] += clean + color * (clean * 18.0).tanh() * envelope;
        }
    }
}

fn add_pad(buffer: &mut [f32], origin: f32, end_time: f32) {
    let first_bar = ((origin / BAR).floor() as isize - 16).max(0) as usize;
    let last_bar = (end_time / BAR).ceil() as usize + 1;
    let mut active: Vec<(i32, usize)> = Vec::new();
    for bar in first_bar..=last_bar {
        let current = voicing(chord(bar));
        let mut index = 0;
        while index < active.len() {
            if !current.contains(&active[index].0) {
                let (pitch, start_bar) = active.swap_remove(index);
                add_note(
                    buffer,
                    origin,
                    start_bar as f32 * BAR,
                    bar as f32 * BAR,
                    pitch,
                    0.0105,
                    0,
                );
            } else {
                index += 1;
            }
        }
        for pitch in current {
            if !active.iter().any(|(held, _)| *held == pitch) {
                let mut start = bar;
                if bar == first_bar {
                    while start > 0 && voicing(chord(start - 1)).contains(&pitch) {
                        start -= 1;
                    }
                }
                active.push((pitch, start));
            }
        }
    }
    for (pitch, start_bar) in active {
        add_note(
            buffer,
            origin,
            start_bar as f32 * BAR,
            (last_bar + 1) as f32 * BAR,
            pitch,
            0.0105,
            0,
        );
    }
}

fn effects(lead: &[f32], pad: &[f32], bass: &[f32], answer: &[f32], origin: f32) -> Vec<f32> {
    let frames = lead.len() / 2;
    let mut output = vec![0.0; lead.len()];
    let taps = [
        (0.59, 0.23),
        (1.18, 0.065),
        (0.029, 0.12),
        (0.057, 0.09),
        (0.113, 0.07),
        (0.37, 0.08),
        (0.91, 0.07),
        (1.67, 0.05),
        (2.55, 0.04),
    ];
    for frame in 0..frames {
        for side in 0..2 {
            let i = frame * 2 + side;
            let mut sample = lead[i] + 1.20 * pad[i] + 0.86 * bass[i] + 0.56 * answer[i];
            let chorus = (0.018
                + 0.005
                    * (TAU * 0.33 * (origin + frame as f32 / SAMPLE_RATE as f32)
                        + side as f32 * std::f32::consts::PI)
                        .sin())
                * SAMPLE_RATE as f32;
            let at = frame.saturating_sub(chorus as usize) * 2 + side;
            sample += 0.29 * pad[at];
            for &(seconds, gain) in &taps {
                let delay = (seconds * SAMPLE_RATE as f32) as usize;
                if frame >= delay {
                    let other = if seconds == 0.59 || seconds == 1.18 {
                        1 - side
                    } else {
                        side
                    };
                    let j = (frame - delay) * 2 + other;
                    sample += gain * (lead[j] + 1.4 * pad[j] + 0.55 * answer[j]);
                }
            }
            output[i] = (sample * 3.0).tanh() * 0.9;
        }
    }
    output
}

pub(crate) fn continuous_pcm_window(seed: u64, start_seconds: usize, seconds: usize) -> Vec<f32> {
    let history = HISTORY.min(start_seconds);
    let origin = (start_seconds - history) as f32;
    let frames = (seconds + history) * RATE;
    let mut lead = vec![0.0; frames * 2];
    let mut pad = vec![0.0; frames * 2];
    let mut bass = vec![0.0; frames * 2];
    let mut answer = vec![0.0; frames * 2];
    let end_time = (start_seconds + seconds) as f32;
    add_pad(&mut pad, origin, end_time);
    let first_bar = ((origin / BAR).floor() as isize - 1).max(0) as usize;
    let last_bar = (end_time / BAR).ceil() as usize;
    for bar in first_bar..=last_bar {
        let (kind, local, pass) = section(bar);
        let chord = chord(bar);
        let time = bar as f32 * BAR;
        add_note(
            &mut bass,
            origin,
            time,
            time + 2.25 * BEAT,
            chord.root,
            0.052,
            2,
        );
        if kind != 1 && local % 2 == 0 {
            add_note(
                &mut bass,
                origin,
                time + 2.7 * BEAT,
                time + 3.53 * BEAT,
                chord.root + 12,
                0.018,
                2,
            );
        }
        // A quiet piano transient retains the acoustic trace of the approved mix.
        for (index, &pitch) in chord.notes[1..].iter().enumerate() {
            add_note(
                &mut pad,
                origin,
                time + index as f32 * 0.05,
                time + 0.9 * BAR,
                pitch.max(55),
                0.0019,
                1,
            );
        }
        let cycle = (local / 8 + pass) % 3;
        let mut line = PHRASES[kind][cycle][local % 8].to_vec();
        if kind == 1 && cycle != 0 && local % 2 == 1 {
            line.pop();
        }
        // Later sections vary the sentence order and sparse notes without leaving the harmony.
        if pass > 0 && hash(seed, bar as u64).is_multiple_of(7) && line.len() > 2 {
            line.remove(1);
        }
        add_lead(&mut lead, origin, bar, &line, seed);
        if (kind == 0 && local % 8 == 7)
            || (kind == 1 && local % 8 == 4)
            || (kind == 2 && local % 8 == 3)
        {
            add_note(
                &mut answer,
                origin,
                time + 2.8 * BEAT,
                time + 3.55 * BEAT,
                chord.notes[chord.notes.len() - 1] + 12,
                0.004,
                2,
            );
        }
    }
    let mixed = effects(&lead, &pad, &bass, &answer, origin);
    mixed[history * RATE * 2..].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_changes_tonal_center_and_continues_past_first_cycle() {
        assert_ne!(chord(0).notes, chord(SECTION_BARS).notes);
        assert_ne!(chord(SECTION_BARS).notes, chord(SECTION_BARS * 2).notes);
        assert_eq!(section(SECTION_BARS * 3), (0, 0, 1));
        assert_ne!(PHRASES[0][0][0][0].pitch, PHRASES[0][1][0][0].pitch);
    }

    #[test]
    fn windows_are_finite_and_have_continuous_edges() {
        let a = continuous_pcm_window(7, 0, 12);
        let b = continuous_pcm_window(7, 12, 12);
        assert_eq!(a.len(), 12 * RATE * 2);
        assert!(
            a.iter()
                .chain(&b)
                .all(|sample| sample.is_finite() && sample.abs() <= 1.0)
        );
        assert!((a[a.len() - 2] - b[0]).abs() < 0.12);
        let together = continuous_pcm_window(7, 0, 24);
        let edge = a.len();
        let mean_difference = b
            .iter()
            .zip(&together[edge..])
            .map(|(split, whole)| (split - whole).abs())
            .sum::<f32>()
            / b.len() as f32;
        assert!(
            mean_difference < 0.001,
            "window join drift: {mean_difference}"
        );
    }

    #[test]
    fn late_session_still_generates_music() {
        let late = continuous_pcm_window(7, 3_600, 1);
        assert_eq!(late.len(), RATE * 2);
        assert!(late.iter().any(|sample| sample.abs() > 0.001));
        assert_eq!(late, continuous_pcm_window(7, 3_600, 1));
    }
}
