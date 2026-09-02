use super::{Canvas, Rgba8};

pub(super) fn grass(canvas: &mut Canvas, variant: u8) {
    const BASE: Rgba8 = Rgba8::rgb(83, 126, 63);
    const LIGHT: Rgba8 = Rgba8::rgb(105, 149, 76);
    const DARK: Rgba8 = Rgba8::rgb(58, 102, 49);
    canvas.fill(BASE);
    canvas.scatter(
        0x4752_4153_5300 + u64::from(variant),
        18,
        [0, 0, 16, 16],
        &[LIGHT, DARK],
    );
    let shift = i32::from(variant % 3);
    for (x, y) in [(2, 12), (6, 5), (11, 10), (14, 3)] {
        let x = (x + shift).min(15);
        canvas.pixel(x, y, DARK);
        canvas.pixel((x - 1).max(0), y - 1, DARK);
        canvas.pixel((x + 1).min(15), y - 1, LIGHT);
    }
}

pub(super) fn water(canvas: &mut Canvas, variant: u8) {
    const BASE: Rgba8 = Rgba8::rgb(48, 102, 158);
    const DEEP: Rgba8 = Rgba8::rgb(36, 78, 132);
    const FOAM: Rgba8 = Rgba8::rgb(100, 164, 202);
    canvas.fill(BASE);
    canvas.scatter(
        0x5741_5445_5200 + u64::from(variant),
        10,
        [0, 0, 16, 16],
        &[DEEP],
    );
    let offset = i32::from(variant % 4);
    for y in [3 + offset, 9 + (offset / 2)] {
        canvas.line(1, y, 5, y, FOAM);
        canvas.line(5, y, 7, y + 1, FOAM);
        canvas.line(9, y + 1, 14, y + 1, FOAM);
    }
}

pub(super) fn rock(canvas: &mut Canvas, variant: u8) {
    const BASE: Rgba8 = Rgba8::rgb(105, 105, 101);
    const LIGHT: Rgba8 = Rgba8::rgb(132, 132, 126);
    const DARK: Rgba8 = Rgba8::rgb(72, 73, 72);
    canvas.fill(BASE);
    canvas.scatter(
        0x524f_434b_0000 + u64::from(variant),
        22,
        [0, 0, 16, 16],
        &[LIGHT, DARK],
    );
    let shift = i32::from(variant & 1);
    canvas.line(2 + shift, 1, 7 + shift, 6, DARK);
    canvas.line(7 + shift, 6, 5 + shift, 11, DARK);
    canvas.line(7 + shift, 6, 12 + shift, 8, LIGHT);
    canvas.line(12 + shift, 8, 14, 13, DARK);
}
