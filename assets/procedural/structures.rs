use super::{Canvas, Rgba8};

pub(super) fn stone_wall_blueprint(canvas: &mut Canvas, variant: u8) {
    const LINE: Rgba8 = Rgba8::rgba(84, 220, 238, 220);
    const FAINT: Rgba8 = Rgba8::rgba(84, 220, 238, 70);
    canvas.rect(2, 3, 12, 10, FAINT);
    for x in (2..14).step_by(2) {
        canvas.pixel(x, 3, LINE);
        canvas.pixel(x, 12, LINE);
    }
    for y in (3..13).step_by(2) {
        canvas.pixel(2, y, LINE);
        canvas.pixel(13, y, LINE);
    }
    let shift = i32::from(variant & 1);
    canvas.line(4, 10, 11 + shift, 5, LINE);
    canvas.line(4, 5, 11 + shift, 10, LINE);
}

pub(super) fn stone_wall(canvas: &mut Canvas, variant: u8) {
    const OUTLINE: Rgba8 = Rgba8::rgb(47, 49, 53);
    const DARK: Rgba8 = Rgba8::rgb(91, 96, 101);
    const MID: Rgba8 = Rgba8::rgb(139, 145, 149);
    const LIGHT: Rgba8 = Rgba8::rgb(187, 190, 188);
    canvas.rect(1, 3, 14, 10, OUTLINE);
    canvas.rect(2, 4, 12, 8, MID);
    for y in [7, 10] {
        canvas.rect(2, y, 12, 1, DARK);
    }
    let offset = i32::from(variant & 1) * 2;
    for x in [5 + offset, 10] {
        canvas.rect(x, 4, 1, 3, DARK);
    }
    for x in [3 + offset, 8, 12] {
        canvas.rect(x, 8, 1, 3, DARK);
    }
    canvas.line(3, 4, 12, 4, LIGHT);
    canvas.pixel(3, 5, LIGHT);
    canvas.ellipse(8, 13, 6, 1, Rgba8::rgba(10, 10, 10, 90));
}
