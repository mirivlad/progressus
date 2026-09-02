use super::{Canvas, Rgba8};

const OUTLINE: Rgba8 = Rgba8::rgb(45, 39, 35);
const SKIN: Rgba8 = Rgba8::rgb(222, 174, 124);
const HAIR: Rgba8 = Rgba8::rgb(77, 52, 34);
const TROUSERS: Rgba8 = Rgba8::rgb(55, 62, 68);

pub(super) fn human(canvas: &mut Canvas, variant: u8) {
    let shirts = [
        Rgba8::rgb(201, 156, 45),
        Rgba8::rgb(71, 126, 164),
        Rgba8::rgb(144, 82, 68),
        Rgba8::rgb(90, 137, 82),
        Rgba8::rgb(126, 91, 153),
    ];
    let shirt = shirts[usize::from(variant) % shirts.len()];

    canvas.ellipse(8, 13, 4, 2, Rgba8::rgba(20, 20, 20, 80));
    canvas.rect(5, 9, 3, 5, OUTLINE);
    canvas.rect(9, 9, 3, 5, OUTLINE);
    canvas.rect(6, 9, 2, 4, TROUSERS);
    canvas.rect(9, 9, 2, 4, TROUSERS);
    canvas.ellipse(8, 8, 5, 4, OUTLINE);
    canvas.ellipse(8, 8, 4, 3, shirt);
    canvas.circle(8, 4, 3, OUTLINE);
    canvas.circle(8, 4, 2, SKIN);
    canvas.rect(6, 2, 5, 2, HAIR);
    canvas.pixel(7, 4, OUTLINE);
    canvas.pixel(9, 4, OUTLINE);
    canvas.pixel(8, 5, Rgba8::rgb(150, 86, 62));
}
