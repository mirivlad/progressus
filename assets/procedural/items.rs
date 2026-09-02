use super::{Canvas, Rgba8};

pub(super) fn wood_stack(canvas: &mut Canvas, variant: u8) {
    const OUTLINE: Rgba8 = Rgba8::rgb(43, 29, 20);
    const BARK: Rgba8 = Rgba8::rgb(105, 55, 27);
    const WOOD: Rgba8 = Rgba8::rgb(184, 107, 45);
    const CUT: Rgba8 = Rgba8::rgb(238, 180, 94);
    const TIE: Rgba8 = Rgba8::rgb(221, 190, 101);

    canvas.ellipse(8, 13, 6, 2, Rgba8::rgba(10, 10, 10, 105));
    let shift = i32::from(variant & 1);
    for (x, y) in [(2, 9), (4, 6), (2 + shift, 3)] {
        canvas.rect(x - 1, y - 1, 12, 5, OUTLINE);
        canvas.rect(x, y, 10, 3, BARK);
        canvas.rect(x + 1, y, 8, 2, WOOD);
        canvas.circle(x + 9, y + 1, 1, CUT);
        canvas.pixel(x + 9, y + 1, BARK);
    }
    canvas.rect(7, 2, 2, 11, OUTLINE);
    canvas.rect(7, 3, 1, 9, TIE);
}

pub(super) fn stone_stack(canvas: &mut Canvas, variant: u8) {
    const OUTLINE: Rgba8 = Rgba8::rgb(45, 49, 55);
    const DARK: Rgba8 = Rgba8::rgb(82, 88, 96);
    const MID: Rgba8 = Rgba8::rgb(157, 163, 169);
    const LIGHT: Rgba8 = Rgba8::rgb(224, 226, 222);

    canvas.ellipse(8, 13, 6, 2, Rgba8::rgba(10, 10, 10, 105));
    let shift = i32::from(variant & 1);
    for (x, y, r) in [(4, 10, 3), (11, 10, 3), (8 + shift, 5, 3)] {
        canvas.circle(x, y, r + 1, OUTLINE);
        canvas.circle(x, y, r, DARK);
        canvas.circle(x - 1, y - 1, r - 1, MID);
        canvas.pixel(x - 1, y - 2, LIGHT);
        canvas.pixel(x, y - 2, LIGHT);
    }
}
