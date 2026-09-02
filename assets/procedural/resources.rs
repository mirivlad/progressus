use super::{Canvas, Rgba8};

pub(super) fn tree(canvas: &mut Canvas, variant: u8) {
    const SHADOW: Rgba8 = Rgba8::rgba(15, 20, 14, 85);
    const TRUNK: Rgba8 = Rgba8::rgb(92, 55, 31);
    const DARK: Rgba8 = Rgba8::rgb(42, 91, 47);
    const MID: Rgba8 = Rgba8::rgb(61, 121, 57);
    const LIGHT: Rgba8 = Rgba8::rgb(88, 146, 67);
    canvas.ellipse(8, 13, 6, 2, SHADOW);
    canvas.rect(7, 8, 3, 6, TRUNK);
    let shift = i32::from(variant % 3) - 1;
    canvas.circle(5 + shift, 7, 4, DARK);
    canvas.circle(11 + shift, 7, 4, DARK);
    canvas.circle(8 + shift, 4, 5, DARK);
    canvas.circle(5 + shift, 6, 3, MID);
    canvas.circle(10 + shift, 6, 3, MID);
    canvas.circle(8 + shift, 3, 3, LIGHT);
    canvas.scatter(
        0x5452_4545_0000 + u64::from(variant),
        7,
        [3, 1, 11, 8],
        &[LIGHT, MID],
    );
}

pub(super) fn stone_outcrop(canvas: &mut Canvas, variant: u8) {
    const SHADOW: Rgba8 = Rgba8::rgba(15, 15, 15, 80);
    const DARK: Rgba8 = Rgba8::rgb(72, 73, 76);
    const MID: Rgba8 = Rgba8::rgb(118, 120, 123);
    const LIGHT: Rgba8 = Rgba8::rgb(163, 164, 162);
    canvas.ellipse(8, 13, 7, 2, SHADOW);
    let shift = i32::from(variant & 1);
    canvas.circle(5, 9, 4, DARK);
    canvas.circle(11, 9, 4, DARK);
    canvas.circle(8 + shift, 6, 5, DARK);
    canvas.circle(5, 8, 3, MID);
    canvas.circle(10, 8, 3, MID);
    canvas.circle(8 + shift, 5, 3, MID);
    canvas.line(7 + shift, 3, 9 + shift, 7, LIGHT);
    canvas.line(9 + shift, 7, 8 + shift, 9, DARK);
}
