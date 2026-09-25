// Score and phrase shapes from the approved ambient audition.
#[derive(Clone, Copy)]
pub(super) struct Note {
    pub beat: f32,
    pub pitch: i32,
    pub length: f32,
}
impl Note {
    pub(super) const fn new(beat: f32, pitch: i32, length: f32) -> Self {
        Self {
            beat,
            pitch,
            length,
        }
    }
}
#[derive(Clone, Copy)]
pub(super) struct Chord {
    pub notes: &'static [i32],
    pub root: i32,
}
pub(super) const G_MINOR: [Chord; 8] = [
    Chord {
        notes: &[43, 50, 53, 58, 62],
        root: 31,
    },
    Chord {
        notes: &[39, 46, 51, 55, 58],
        root: 27,
    },
    Chord {
        notes: &[46, 53, 58, 62, 65],
        root: 34,
    },
    Chord {
        notes: &[41, 48, 53, 55, 60],
        root: 29,
    },
    Chord {
        notes: &[48, 55, 58, 63, 67],
        root: 36,
    },
    Chord {
        notes: &[39, 46, 51, 55, 58],
        root: 27,
    },
    Chord {
        notes: &[43, 50, 53, 58, 62],
        root: 31,
    },
    Chord {
        notes: &[50, 57, 60, 65],
        root: 38,
    },
];
pub(super) const F_MAJOR: [Chord; 8] = [
    Chord {
        notes: &[46, 53, 57, 60, 65],
        root: 34,
    },
    Chord {
        notes: &[41, 48, 52, 57, 60, 67],
        root: 29,
    },
    Chord {
        notes: &[50, 53, 57, 60, 64],
        root: 38,
    },
    Chord {
        notes: &[48, 55, 60, 62, 67],
        root: 36,
    },
    Chord {
        notes: &[46, 53, 57, 60, 65],
        root: 34,
    },
    Chord {
        notes: &[41, 48, 52, 57, 60, 67],
        root: 29,
    },
    Chord {
        notes: &[43, 50, 53, 58, 62],
        root: 31,
    },
    Chord {
        notes: &[48, 55, 60, 62, 67],
        root: 36,
    },
];
pub(super) const G_MAJOR: [Chord; 8] = [
    Chord {
        notes: &[48, 52, 55, 59, 62],
        root: 36,
    },
    Chord {
        notes: &[43, 50, 54, 59, 62],
        root: 31,
    },
    Chord {
        notes: &[40, 47, 52, 55, 59],
        root: 28,
    },
    Chord {
        notes: &[50, 57, 62, 64, 67],
        root: 38,
    },
    Chord {
        notes: &[45, 52, 55, 60, 64],
        root: 33,
    },
    Chord {
        notes: &[48, 52, 55, 59, 62],
        root: 36,
    },
    Chord {
        notes: &[43, 50, 54, 59, 62],
        root: 31,
    },
    Chord {
        notes: &[50, 57, 62, 64, 67],
        root: 38,
    },
];
pub(super) const G_MINOR_PHRASE: [&[Note]; 8] = [
    &[Note::new(0.58, 67, 0.78), Note::new(2.55, 70, 0.77)],
    &[
        Note::new(0.4, 63, 0.7),
        Note::new(1.55, 65, 0.57),
        Note::new(3.06, 67, 0.61),
    ],
    &[Note::new(0.7, 62, 0.94), Note::new(2.6, 65, 0.72)],
    &[Note::new(1.1, 69, 0.75), Note::new(2.77, 67, 0.8)],
    &[
        Note::new(0.38, 70, 0.7),
        Note::new(1.85, 67, 0.57),
        Note::new(3.12, 63, 0.56),
    ],
    &[Note::new(0.83, 67, 0.86), Note::new(2.58, 65, 0.8)],
    &[
        Note::new(0.5, 74, 0.63),
        Note::new(1.85, 70, 0.62),
        Note::new(3.13, 69, 0.56),
    ],
    &[Note::new(1.02, 65, 0.82), Note::new(2.7, 69, 0.68)],
];
pub(super) const G_MINOR_SECOND: [&[Note]; 8] = [
    &[
        Note::new(0.55, 62, 0.78),
        Note::new(1.75, 65, 0.7),
        Note::new(3.05, 67, 0.65),
    ],
    &[
        Note::new(0.4, 67, 0.71),
        Note::new(1.64, 70, 0.68),
        Note::new(2.88, 75, 0.67),
    ],
    &[
        Note::new(0.52, 74, 0.75),
        Note::new(1.79, 72, 0.65),
        Note::new(3.04, 70, 0.7),
    ],
    &[
        Note::new(0.6, 69, 0.78),
        Note::new(1.96, 67, 0.69),
        Note::new(3.1, 65, 0.68),
    ],
    &[
        Note::new(0.49, 67, 0.74),
        Note::new(1.72, 72, 0.66),
        Note::new(3.01, 70, 0.75),
    ],
    &[
        Note::new(0.58, 75, 0.75),
        Note::new(1.92, 74, 0.62),
        Note::new(3.1, 70, 0.72),
    ],
    &[
        Note::new(0.45, 70, 0.72),
        Note::new(1.78, 69, 0.68),
        Note::new(3.05, 67, 0.72),
    ],
    &[
        Note::new(0.62, 65, 0.83),
        Note::new(1.94, 69, 0.68),
        Note::new(3.1, 72, 0.67),
    ],
];
pub(super) const G_MINOR_THIRD: [&[Note]; 8] = [
    &[
        Note::new(0.45, 70, 0.64),
        Note::new(1.32, 74, 0.56),
        Note::new(2.18, 72, 0.59),
        Note::new(3.18, 67, 0.55),
    ],
    &[
        Note::new(0.55, 75, 0.74),
        Note::new(1.93, 74, 0.61),
        Note::new(3.12, 70, 0.66),
    ],
    &[
        Note::new(0.6, 65, 0.75),
        Note::new(1.92, 70, 0.62),
        Note::new(3.11, 74, 0.65),
    ],
    &[
        Note::new(0.57, 72, 0.73),
        Note::new(1.87, 69, 0.63),
        Note::new(3.1, 67, 0.69),
    ],
    &[
        Note::new(0.48, 63, 0.59),
        Note::new(1.34, 67, 0.55),
        Note::new(2.18, 70, 0.55),
        Note::new(3.15, 72, 0.62),
    ],
    &[
        Note::new(0.54, 70, 0.7),
        Note::new(1.88, 67, 0.61),
        Note::new(3.12, 63, 0.65),
    ],
    &[
        Note::new(0.45, 69, 0.6),
        Note::new(1.34, 72, 0.56),
        Note::new(2.24, 70, 0.54),
        Note::new(3.13, 67, 0.62),
    ],
    &[
        Note::new(0.58, 74, 0.75),
        Note::new(1.88, 72, 0.63),
        Note::new(3.05, 69, 0.69),
    ],
];
pub(super) const F_MAJOR_PHRASE: [&[Note]; 8] = [
    &[Note::new(0.92, 65, 1.21), Note::new(2.86, 69, 0.72)],
    &[Note::new(0.44, 69, 1.36), Note::new(2.73, 72, 0.82)],
    &[Note::new(1.04, 65, 1.1), Note::new(3.02, 64, 0.62)],
    &[Note::new(1.23, 67, 1.28)],
    &[Note::new(0.9, 62, 1.1), Note::new(2.95, 65, 0.63)],
    &[Note::new(1.1, 69, 1.3)],
    &[Note::new(0.78, 70, 1.04), Note::new(2.84, 69, 0.73)],
    &[Note::new(1.15, 64, 1.15), Note::new(2.9, 67, 0.7)],
];
pub(super) const F_MAJOR_SECOND: [&[Note]; 8] = [
    &[
        Note::new(0.55, 62, 0.78),
        Note::new(1.82, 65, 0.7),
        Note::new(3.06, 69, 0.63),
    ],
    &[
        Note::new(0.5, 72, 0.8),
        Note::new(1.88, 69, 0.68),
        Note::new(3.06, 67, 0.65),
    ],
    &[
        Note::new(0.55, 69, 0.74),
        Note::new(1.85, 72, 0.67),
        Note::new(3.03, 76, 0.66),
    ],
    &[Note::new(0.82, 67, 0.94), Note::new(2.75, 64, 0.78)],
    &[
        Note::new(0.55, 70, 0.72),
        Note::new(1.82, 69, 0.64),
        Note::new(3.08, 65, 0.64),
    ],
    &[
        Note::new(0.57, 76, 0.72),
        Note::new(1.88, 72, 0.67),
        Note::new(3.03, 69, 0.69),
    ],
    &[
        Note::new(0.58, 74, 0.75),
        Note::new(1.86, 70, 0.69),
        Note::new(3.06, 69, 0.64),
    ],
    &[
        Note::new(0.56, 64, 0.74),
        Note::new(1.85, 67, 0.67),
        Note::new(3.05, 72, 0.69),
    ],
];
pub(super) const F_MAJOR_THIRD: [&[Note]; 8] = [
    &[
        Note::new(0.57, 69, 0.76),
        Note::new(1.92, 72, 0.66),
        Note::new(3.09, 70, 0.7),
    ],
    &[
        Note::new(0.48, 65, 0.65),
        Note::new(1.47, 69, 0.58),
        Note::new(2.42, 72, 0.6),
        Note::new(3.3, 76, 0.48),
    ],
    &[
        Note::new(0.64, 65, 0.73),
        Note::new(1.95, 64, 0.67),
        Note::new(3.07, 62, 0.7),
    ],
    &[
        Note::new(0.55, 72, 0.73),
        Note::new(1.91, 67, 0.67),
        Note::new(3.09, 64, 0.69),
    ],
    &[
        Note::new(0.51, 74, 0.73),
        Note::new(1.82, 72, 0.65),
        Note::new(3.07, 70, 0.7),
    ],
    &[
        Note::new(0.56, 69, 0.75),
        Note::new(1.87, 67, 0.69),
        Note::new(3.06, 64, 0.68),
    ],
    &[
        Note::new(0.56, 70, 0.73),
        Note::new(1.91, 69, 0.65),
        Note::new(3.05, 67, 0.67),
    ],
    &[
        Note::new(0.57, 67, 0.7),
        Note::new(1.82, 72, 0.67),
        Note::new(3.09, 74, 0.69),
    ],
];
pub(super) const G_MAJOR_SECOND: [&[Note]; 8] = [
    &[
        Note::new(0.6, 67, 0.78),
        Note::new(1.96, 71, 0.68),
        Note::new(3.06, 76, 0.72),
    ],
    &[
        Note::new(0.55, 74, 0.76),
        Note::new(1.93, 69, 0.67),
        Note::new(3.05, 66, 0.71),
    ],
    &[
        Note::new(0.56, 59, 0.74),
        Note::new(1.91, 64, 0.68),
        Note::new(3.09, 67, 0.73),
    ],
    &[
        Note::new(0.55, 69, 0.74),
        Note::new(1.94, 74, 0.64),
        Note::new(3.05, 72, 0.7),
    ],
    &[
        Note::new(0.56, 64, 0.77),
        Note::new(1.9, 69, 0.66),
        Note::new(3.06, 72, 0.7),
    ],
    &[
        Note::new(0.52, 71, 0.73),
        Note::new(1.88, 67, 0.66),
        Note::new(3.11, 64, 0.7),
    ],
    &[
        Note::new(0.57, 66, 0.76),
        Note::new(1.87, 69, 0.64),
        Note::new(3.08, 71, 0.72),
    ],
    &[
        Note::new(0.56, 69, 0.75),
        Note::new(1.93, 66, 0.68),
        Note::new(3.05, 62, 0.74),
    ],
];
pub(super) const G_MAJOR_THIRD: [&[Note]; 8] = [
    &[
        Note::new(0.49, 64, 0.62),
        Note::new(1.36, 67, 0.6),
        Note::new(2.22, 72, 0.57),
        Note::new(3.17, 71, 0.62),
    ],
    &[
        Note::new(0.53, 67, 0.77),
        Note::new(1.88, 71, 0.67),
        Note::new(3.07, 74, 0.71),
    ],
    &[
        Note::new(0.47, 76, 0.59),
        Note::new(1.34, 74, 0.58),
        Note::new(2.22, 71, 0.57),
        Note::new(3.17, 67, 0.64),
    ],
    &[
        Note::new(0.53, 66, 0.75),
        Note::new(1.87, 69, 0.68),
        Note::new(3.07, 74, 0.69),
    ],
    &[
        Note::new(0.47, 72, 0.58),
        Note::new(1.35, 69, 0.58),
        Note::new(2.24, 67, 0.56),
        Note::new(3.15, 64, 0.62),
    ],
    &[
        Note::new(0.53, 67, 0.75),
        Note::new(1.9, 72, 0.66),
        Note::new(3.1, 71, 0.7),
    ],
    &[
        Note::new(0.53, 74, 0.75),
        Note::new(1.86, 71, 0.67),
        Note::new(3.06, 66, 0.7),
    ],
    &[
        Note::new(0.49, 69, 0.6),
        Note::new(1.38, 66, 0.58),
        Note::new(2.23, 64, 0.59),
        Note::new(3.16, 62, 0.65),
    ],
];
pub(super) const G_MAJOR_PHRASE: [&[Note]; 8] = [
    &[
        Note::new(0.82, 64, 0.54),
        Note::new(1.62, 67, 0.55),
        Note::new(2.48, 71, 0.68),
    ],
    &[
        Note::new(0.47, 74, 0.48),
        Note::new(1.31, 71, 0.53),
        Note::new(2.14, 69, 0.53),
        Note::new(3.05, 67, 0.61),
    ],
    &[
        Note::new(0.78, 67, 0.53),
        Note::new(1.69, 66, 0.49),
        Note::new(2.67, 64, 0.73),
    ],
    &[
        Note::new(0.61, 69, 0.57),
        Note::new(1.54, 67, 0.48),
        Note::new(2.48, 66, 0.74),
    ],
    &[
        Note::new(0.4, 72, 0.51),
        Note::new(1.19, 71, 0.52),
        Note::new(2.04, 69, 0.5),
        Note::new(3.07, 67, 0.65),
    ],
    &[
        Note::new(0.83, 64, 0.54),
        Note::new(1.69, 67, 0.55),
        Note::new(2.63, 71, 0.71),
    ],
    &[
        Note::new(0.55, 74, 0.48),
        Note::new(1.42, 71, 0.53),
        Note::new(2.37, 69, 0.69),
    ],
    &[
        Note::new(0.6, 66, 0.51),
        Note::new(1.55, 64, 0.53),
        Note::new(2.6, 62, 0.78),
    ],
];
pub(super) const RINGS: [&[Chord; 8]; 3] = [&G_MINOR, &F_MAJOR, &G_MAJOR];
pub(super) const PHRASES: [[&[&[Note]; 8]; 3]; 3] = [
    [&G_MINOR_PHRASE, &G_MINOR_SECOND, &G_MINOR_THIRD],
    [&F_MAJOR_PHRASE, &F_MAJOR_SECOND, &F_MAJOR_THIRD],
    [&G_MAJOR_PHRASE, &G_MAJOR_SECOND, &G_MAJOR_THIRD],
];
