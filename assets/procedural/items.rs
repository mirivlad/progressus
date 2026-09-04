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

pub(super) fn primitive_tool(canvas: &mut Canvas, variant: u8) {
    const OUTLINE: Rgba8 = Rgba8::rgb(38, 31, 25);
    const HANDLE: Rgba8 = Rgba8::rgb(151, 86, 42);
    const HANDLE_LIGHT: Rgba8 = Rgba8::rgb(220, 145, 70);
    const METAL: Rgba8 = Rgba8::rgb(157, 165, 169);
    const METAL_LIGHT: Rgba8 = Rgba8::rgb(227, 230, 224);
    canvas.ellipse(8, 13, 5, 2, Rgba8::rgba(10, 10, 10, 100));
    let shift = i32::from(variant & 1);
    canvas.line(5 + shift, 12, 10 + shift, 5, OUTLINE);
    canvas.line(6 + shift, 12, 11 + shift, 5, HANDLE);
    canvas.pixel(7 + shift, 10, HANDLE_LIGHT);
    canvas.rect(7 + shift, 3, 6, 4, OUTLINE);
    canvas.rect(8 + shift, 4, 4, 2, METAL);
    canvas.line(9 + shift, 4, 11 + shift, 4, METAL_LIGHT);
}

pub(super) fn berries_stack(canvas: &mut Canvas, variant: u8) {
    const OUTLINE: Rgba8 = Rgba8::rgb(48, 22, 38);
    const DARK: Rgba8 = Rgba8::rgb(112, 31, 73);
    const BERRY: Rgba8 = Rgba8::rgb(190, 48, 93);
    const LIGHT: Rgba8 = Rgba8::rgb(246, 119, 142);
    const LEAF: Rgba8 = Rgba8::rgb(72, 128, 62);
    canvas.ellipse(8, 13, 5, 2, Rgba8::rgba(10, 10, 10, 95));
    let shift = i32::from(variant & 1);
    for (x, y) in [(5, 9), (9, 9), (7 + shift, 6), (11, 6), (8, 11)] {
        canvas.circle(x, y, 2, OUTLINE);
        canvas.circle(x, y, 1, BERRY);
        canvas.pixel(x - 1, y - 1, LIGHT);
        canvas.pixel(x + 1, y + 1, DARK);
    }
    canvas.line(7, 4, 9, 6, LEAF);
    canvas.line(8, 4, 11, 4, LEAF);
}
