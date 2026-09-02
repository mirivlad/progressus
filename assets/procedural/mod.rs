use super::{Canvas, Rgba8};

mod characters;
mod items;
mod resources;
mod terrain;

pub(super) fn grass(canvas: &mut Canvas, variant: u8) {
    terrain::grass(canvas, variant);
}

pub(super) fn water(canvas: &mut Canvas, variant: u8) {
    terrain::water(canvas, variant);
}

pub(super) fn rock(canvas: &mut Canvas, variant: u8) {
    terrain::rock(canvas, variant);
}

pub(super) fn human(canvas: &mut Canvas, variant: u8) {
    characters::human(canvas, variant);
}

pub(super) fn wood_stack(canvas: &mut Canvas, variant: u8) {
    items::wood_stack(canvas, variant);
}

pub(super) fn stone_stack(canvas: &mut Canvas, variant: u8) {
    items::stone_stack(canvas, variant);
}

pub(super) fn tree(canvas: &mut Canvas, variant: u8) {
    resources::tree(canvas, variant);
}

pub(super) fn stone_outcrop(canvas: &mut Canvas, variant: u8) {
    resources::stone_outcrop(canvas, variant);
}
