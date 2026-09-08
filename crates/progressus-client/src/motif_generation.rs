//! Deterministic, presentation-only motif generation for listening auditions.

use std::collections::HashSet;

pub const SAMPLE_RATE: u32 = 24_000;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EndingRole {
    Open,
    Resting,
    Resolving,
}

impl EndingRole {
    pub const ALL: [Self; 3] = [Self::Open, Self::Resting, Self::Resolving];
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Motif {
    pub degrees: Vec<i8>,
    pub durations: Vec<u8>,
    pub role: EndingRole,
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn choose<T: Copy>(&mut self, values: &[T]) -> T {
        values[self.next() as usize % values.len()]
    }
}

fn normalized_signature(motif: &Motif) -> (Vec<i8>, Vec<u8>) {
    let root = motif.degrees[0];
    (
        motif.degrees.iter().map(|degree| degree - root).collect(),
        motif.durations.clone(),
    )
}

fn is_valid(motif: &Motif) -> bool {
    if !(3..=7).contains(&motif.degrees.len())
        || motif.degrees.len() != motif.durations.len()
    {
        return false;
    }
    let range = motif.degrees.iter().max().unwrap() - motif.degrees.iter().min().unwrap();
    let large_leaps = motif
        .degrees
        .windows(2)
        .filter(|pair| (pair[1] - pair[0]).abs() > 2)
        .count();
    let repeats_phrase = motif
        .degrees
        .windows(6)
        .any(|notes| notes[..3] == notes[3..]);
    let repeats_note = motif
        .degrees
        .windows(3)
        .any(|notes| notes[0] == notes[1] && notes[1] == notes[2]);
    let fixed_pulse = motif
        .durations
        .windows(2)
        .all(|durations| durations[0] == durations[1]);
    let ending_valid = match motif.role {
        EndingRole::Resolving => motif.degrees.last() == Some(&0),
        EndingRole::Resting => matches!(motif.degrees.last(), Some(2 | 4)),
        EndingRole::Open => matches!(motif.degrees.last(), Some(1 | 3 | 5 | 6)),
    };
    range <= 6
        && large_leaps <= 1
        && !repeats_phrase
        && !repeats_note
        && !fixed_pulse
        && ending_valid
}

pub fn candidate_pool(seed: u64) -> Vec<Motif> {
    const STEPS: [i8; 9] = [-3, -2, -1, -1, 0, 1, 1, 2, 3];
    const DURATIONS: [u8; 4] = [2, 3, 4, 6];
    const ENDINGS: [&[i8]; 3] = [&[1, 3, 5, 6], &[2, 4], &[0]];

    let mut rng = Rng(seed);
    let mut result = Vec::with_capacity(48);
    let mut signatures = HashSet::with_capacity(48);
    for attempt in 0..20_000 {
        if result.len() == 48 {
            break;
        }
        let role = EndingRole::ALL[attempt % EndingRole::ALL.len()];
        let note_count = 3 + rng.next() as usize % 5;
        let mut degrees = vec![(rng.next() % 7) as i8];
        while degrees.len() + 1 < note_count {
            let previous = *degrees.last().unwrap();
            degrees.push((previous + rng.choose(&STEPS)).clamp(0, 6));
        }
        let endings = ENDINGS[attempt % ENDINGS.len()];
        degrees.push(rng.choose(endings));
        let durations = (0..note_count)
            .map(|_| rng.choose(&DURATIONS))
            .collect::<Vec<_>>();
        let motif = Motif {
            degrees,
            durations,
            role,
        };
        let signature = normalized_signature(&motif);
        if is_valid(&motif) && signatures.insert(signature) {
            result.push(motif);
        }
    }
    assert_eq!(
        result.len(),
        48,
        "neutral grammar must produce its bounded candidate pool"
    );
    result
}

fn motif_distance(left: &Motif, right: &Motif) -> usize {
    let left = normalized_signature(left);
    let right = normalized_signature(right);
    let shared = left.0.len().min(right.0.len());
    let length_distance = left.0.len().abs_diff(right.0.len()) * 6;
    let pitch_distance = (0..shared)
        .map(|index| left.0[index].abs_diff(right.0[index]) as usize)
        .sum::<usize>();
    let rhythm_distance = (0..shared)
        .filter(|index| left.1[*index] != right.1[*index])
        .count()
        * 2;
    length_distance + pitch_distance + rhythm_distance
}

pub fn shortlist(seed: u64, count: usize) -> Vec<Motif> {
    let pool = candidate_pool(seed);
    let mut selected = Vec::with_capacity(count.min(pool.len()));
    let mut used = vec![false; pool.len()];
    while selected.len() < count.min(pool.len()) {
        let desired_role = EndingRole::ALL[selected.len() % EndingRole::ALL.len()];
        let mut best: Option<(usize, usize)> = None;
        for (index, motif) in pool.iter().enumerate() {
            if used[index] || motif.role != desired_role {
                continue;
            }
            let distance = selected
                .iter()
                .map(|chosen| motif_distance(motif, chosen))
                .min()
                .unwrap_or(usize::MAX);
            if best.is_none_or(|(_, best_distance)| distance > best_distance) {
                best = Some((index, distance));
            }
        }
        let index = best
            .map(|(index, _)| index)
            .expect("candidate pool must cover every ending role");
        used[index] = true;
        selected.push(pool[index].clone());
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const NEUTRAL_SEED: u64 = 0x5052_4f47_5245_5353;

    fn valid_motif(motif: &Motif) -> bool {
        let note_count_valid = (3..=7).contains(&motif.degrees.len());
        let duration_count_valid = motif.degrees.len() == motif.durations.len();
        let range = motif.degrees.iter().max().unwrap() - motif.degrees.iter().min().unwrap();
        let large_leaps = motif
            .degrees
            .windows(2)
            .filter(|pair| (pair[1] - pair[0]).abs() > 2)
            .count();
        let repeated_triplet = motif
            .degrees
            .windows(6)
            .any(|notes| notes[..3] == notes[3..]);
        let repeated_note = motif.degrees.windows(3).any(|notes| notes[0] == notes[1] && notes[1] == notes[2]);
        let ending_valid = match motif.role {
            EndingRole::Resolving => motif.degrees.last() == Some(&0),
            EndingRole::Resting => matches!(motif.degrees.last(), Some(2 | 4)),
            EndingRole::Open => matches!(motif.degrees.last(), Some(1 | 3 | 5 | 6)),
        };
        note_count_valid
            && duration_count_valid
            && range <= 6
            && large_leaps <= 1
            && !repeated_triplet
            && !repeated_note
            && ending_valid
    }

    #[test]
    fn neutral_pool_is_reproducible_and_structurally_valid() {
        let pool = candidate_pool(NEUTRAL_SEED);
        assert_eq!(pool.len(), 48);
        assert_eq!(pool, candidate_pool(NEUTRAL_SEED));
        assert!(pool.iter().all(valid_motif));
        let signatures = pool.iter().map(normalized_signature).collect::<HashSet<_>>();
        assert_eq!(signatures.len(), pool.len());
    }

    #[test]
    fn shortlist_is_diverse_and_covers_ending_roles() {
        let selected = shortlist(NEUTRAL_SEED, 10);
        assert_eq!(selected.len(), 10);
        assert!(
            EndingRole::ALL
                .into_iter()
                .all(|role| selected.iter().any(|motif| motif.role == role))
        );
        let signatures = selected
            .iter()
            .map(normalized_signature)
            .collect::<HashSet<_>>();
        assert_eq!(signatures.len(), selected.len());
    }
}
