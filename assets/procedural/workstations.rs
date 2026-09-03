use super::{Canvas, Rgba8};

pub(super) fn workbench(canvas: &mut Canvas, variant: u8) {
    const OUTLINE: Rgba8 = Rgba8::rgb(48, 31, 20);
    const DARK: Rgba8 = Rgba8::rgb(92, 52, 28);
    const WOOD: Rgba8 = Rgba8::rgb(166, 98, 48);
    const LIGHT: Rgba8 = Rgba8::rgb(226, 159, 78);
    const METAL: Rgba8 = Rgba8::rgb(132, 143, 146);
    canvas.ellipse(8, 14, 6, 2, Rgba8::rgba(8, 8, 8, 105));
    canvas.rect(2, 5, 12, 5, OUTLINE);
    canvas.rect(3, 6, 10, 3, WOOD);
    canvas.line(4, 6, 11, 6, LIGHT);
    canvas.rect(3, 9, 2, 5, OUTLINE);
    canvas.rect(11, 9, 2, 5, OUTLINE);
    canvas.rect(4, 9, 1, 4, DARK);
    canvas.rect(11, 9, 1, 4, DARK);
    let shift = i32::from(variant & 1);
    canvas.rect(9 + shift, 2, 4, 2, OUTLINE);
    canvas.rect(10 + shift, 2, 2, 1, METAL);
    canvas.line(6, 4, 8, 2, OUTLINE);
    canvas.line(6, 4, 8, 3, METAL);
}
