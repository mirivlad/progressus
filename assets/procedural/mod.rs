use super::{Canvas, Rgba8};

mod characters;
mod items;
mod labels;
mod resources;
mod structures;
mod terrain;
mod workstations;

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

pub(super) fn primitive_tool(canvas: &mut Canvas, variant: u8) {
    items::primitive_tool(canvas, variant);
}

pub(super) fn berries_stack(canvas: &mut Canvas, variant: u8) {
    items::berries_stack(canvas, variant);
}

pub(super) fn tree(canvas: &mut Canvas, variant: u8) {
    resources::tree(canvas, variant);
}

pub(super) fn stone_outcrop(canvas: &mut Canvas, variant: u8) {
    resources::stone_outcrop(canvas, variant);
}

pub(super) fn quantity_dimensions(quantity: u32) -> (u32, u32) {
    labels::quantity_dimensions(quantity)
}

pub(super) fn quantity_label(canvas: &mut Canvas, quantity: u32) {
    labels::quantity_label(canvas, quantity);
}

pub(super) fn workbench(canvas: &mut Canvas, variant: u8) {
    workstations::workbench(canvas, variant);
}

pub(super) fn stone_wall_blueprint(canvas: &mut Canvas, variant: u8) {
    structures::stone_wall_blueprint(canvas, variant);
}

pub(super) fn stone_wall(canvas: &mut Canvas, variant: u8) {
    structures::stone_wall(canvas, variant);
}
