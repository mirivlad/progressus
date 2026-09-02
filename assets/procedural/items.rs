use super::{Canvas, Rgba8};

pub(super) fn wood_stack(canvas: &mut Canvas, variant: u8) {
    const BARK: Rgba8 = Rgba8::rgb(90, 51, 26);
    const WOOD: Rgba8 = Rgba8::rgb(151, 91, 43);
    const CUT: Rgba8 = Rgba8::rgb(205, 148, 76);
    canvas.ellipse(8, 12, 5, 2, Rgba8::rgba(20, 20, 20, 70));
    let shift = i32::from(variant & 1);
    for (x, y) in [(3, 9), (5, 6), (3 + shift, 3)] {
        canvas.rect(x, y, 10, 3, BARK);
        canvas.rect(x + 1, y, 8, 2, WOOD);
        canvas.circle(x + 9, y + 1, 1, CUT);
        canvas.pixel(x + 9, y + 1, BARK);
    }
}

pub(super) fn stone_stack(canvas: &mut Canvas, variant: u8) {
    const DARK: Rgba8 = Rgba8::rgb(73, 75, 78);
    const MID: Rgba8 = Rgba8::rgb(124, 126, 128);
    const LIGHT: Rgba8 = Rgba8::rgb(169, 170, 168);
    canvas.ellipse(8, 12, 6, 2, Rgba8::rgba(20, 20, 20, 70));
    let shift = i32::from(variant & 1);
    for (x, y, r) in [(5, 9, 3), (10, 9, 3), (8 + shift, 5, 3)] {
        canvas.circle(x, y, r, DARK);
        canvas.circle(x - 1, y - 1, r - 1, MID);
        canvas.pixel(x - 1, y - 2, LIGHT);
    }
}
