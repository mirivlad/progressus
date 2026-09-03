use super::{Canvas, Rgba8};

const NORTH: u8 = 1 << 0;
const EAST: u8 = 1 << 1;
const SOUTH: u8 = 1 << 2;
const WEST: u8 = 1 << 3;

pub(super) fn stone_wall_blueprint(canvas: &mut Canvas, connections: u8) {
    const LINE: Rgba8 = Rgba8::rgba(84, 220, 238, 225);
    const FAINT: Rgba8 = Rgba8::rgba(84, 220, 238, 72);
    connected_body(canvas, connections, FAINT);
    connected_edges(canvas, connections, LINE);
    canvas.line(5, 10, 10, 5, LINE);
    canvas.line(5, 5, 10, 10, LINE);
}

pub(super) fn stone_wall(canvas: &mut Canvas, connections: u8) {
    const OUTLINE: Rgba8 = Rgba8::rgb(47, 49, 53);
    const DARK: Rgba8 = Rgba8::rgb(91, 96, 101);
    const MID: Rgba8 = Rgba8::rgb(139, 145, 149);
    const LIGHT: Rgba8 = Rgba8::rgb(187, 190, 188);

    connected_body(canvas, connections, OUTLINE);
    connected_inset(canvas, connections, MID);

    // A few stable masonry joints. The connected arms deliberately reach the
    // canvas edge so adjacent cardinal tiles meet without presentation gaps.
    canvas.rect(4, 7, 8, 1, DARK);
    canvas.rect(7, 4, 1, 8, DARK);
    if connections & EAST != 0 {
        canvas.rect(8, 7, 8, 1, DARK);
        canvas.line(8, 5, 15, 5, LIGHT);
    }
    if connections & WEST != 0 {
        canvas.rect(0, 7, 8, 1, DARK);
        canvas.line(0, 5, 7, 5, LIGHT);
    }
    // Canvas Y grows downward while world Y grows upward. North therefore
    // reaches y=0 and south reaches y=15. Keep each arm's masonry detail
    // local so a one-sided connection never implies a neighbour on both sides.
    if connections & NORTH != 0 {
        canvas.rect(7, 0, 1, 8, DARK);
        canvas.line(5, 0, 5, 7, LIGHT);
    }
    if connections & SOUTH != 0 {
        canvas.rect(7, 8, 1, 8, DARK);
        canvas.line(5, 8, 5, 15, LIGHT);
    }
    canvas.line(5, 4, 10, 4, LIGHT);
}

fn connected_body(canvas: &mut Canvas, connections: u8, color: Rgba8) {
    canvas.rect(3, 3, 10, 10, color);
    if connections & NORTH != 0 {
        canvas.rect(3, 0, 10, 8, color);
    }
    if connections & EAST != 0 {
        canvas.rect(8, 3, 8, 10, color);
    }
    if connections & SOUTH != 0 {
        canvas.rect(3, 8, 10, 8, color);
    }
    if connections & WEST != 0 {
        canvas.rect(0, 3, 8, 10, color);
    }
}

fn connected_inset(canvas: &mut Canvas, connections: u8, color: Rgba8) {
    canvas.rect(4, 4, 8, 8, color);
    if connections & NORTH != 0 {
        canvas.rect(4, 0, 8, 8, color);
    }
    if connections & EAST != 0 {
        canvas.rect(8, 4, 8, 8, color);
    }
    if connections & SOUTH != 0 {
        canvas.rect(4, 8, 8, 8, color);
    }
    if connections & WEST != 0 {
        canvas.rect(0, 4, 8, 8, color);
    }
}

fn connected_edges(canvas: &mut Canvas, connections: u8, color: Rgba8) {
    canvas.rect(3, 3, 10, 1, color);
    canvas.rect(3, 12, 10, 1, color);
    canvas.rect(3, 3, 1, 10, color);
    canvas.rect(12, 3, 1, 10, color);
    if connections & NORTH != 0 {
        canvas.rect(3, 0, 10, 1, color);
    }
    if connections & EAST != 0 {
        canvas.rect(15, 3, 1, 10, color);
    }
    if connections & SOUTH != 0 {
        canvas.rect(3, 15, 10, 1, color);
    }
    if connections & WEST != 0 {
        canvas.rect(0, 3, 1, 10, color);
    }
}
