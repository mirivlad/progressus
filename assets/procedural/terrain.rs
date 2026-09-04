use super::{Canvas, Rgba8};

const NORTH: u8 = 1 << 0;
const NORTH_EAST: u8 = 1 << 1;
const EAST: u8 = 1 << 2;
const SOUTH_EAST: u8 = 1 << 3;
const SOUTH: u8 = 1 << 4;
const SOUTH_WEST: u8 = 1 << 5;
const WEST: u8 = 1 << 6;
const NORTH_WEST: u8 = 1 << 7;

pub(super) fn grass(canvas: &mut Canvas, variant: u8) {
    const BASE: Rgba8 = Rgba8::rgb(83, 126, 63);
    const LIGHT: Rgba8 = Rgba8::rgb(105, 149, 76);
    const DARK: Rgba8 = Rgba8::rgb(58, 102, 49);
    canvas.fill(BASE);
    canvas.scatter(
        0x4752_4153_5300 + u64::from(variant),
        18,
        [0, 0, 16, 16],
        &[LIGHT, DARK],
    );
    let shift = i32::from(variant % 3);
    for (x, y) in [(2, 12), (6, 5), (11, 10), (14, 3)] {
        let x = (x + shift).min(15);
        canvas.pixel(x, y, DARK);
        canvas.pixel((x - 1).max(0), y - 1, DARK);
        canvas.pixel((x + 1).min(15), y - 1, LIGHT);
    }
}

pub(super) fn water(canvas: &mut Canvas, variant: u8, connections: u8) {
    const BASE: Rgba8 = Rgba8::rgb(48, 102, 158);
    const DEEP: Rgba8 = Rgba8::rgb(36, 78, 132);
    const SHALLOW: Rgba8 = Rgba8::rgb(73, 139, 180);
    const FOAM: Rgba8 = Rgba8::rgb(118, 183, 211);
    const SHORE: Rgba8 = Rgba8::rgb(174, 159, 111);
    const SHORE_LIGHT: Rgba8 = Rgba8::rgb(196, 181, 127);

    canvas.fill(BASE);
    canvas.scatter(
        0x5741_5445_5200 + u64::from(variant),
        10,
        [0, 0, 16, 16],
        &[DEEP],
    );

    // Only a known non-water neighbour clears a connection bit. Unknown
    // neighbours are treated as continuous water by presentation, so the
    // shoreline never leaks hidden terrain.
    if connections & NORTH == 0 {
        canvas.rect(0, 0, 16, 2, SHORE);
        canvas.rect(0, 2, 16, 1, SHORE_LIGHT);
        canvas.line(0, 3, 15, 3, SHALLOW);
    }
    if connections & EAST == 0 {
        canvas.rect(14, 0, 2, 16, SHORE);
        canvas.rect(13, 0, 1, 16, SHORE_LIGHT);
        canvas.line(12, 0, 12, 15, SHALLOW);
    }
    if connections & SOUTH == 0 {
        canvas.rect(0, 14, 16, 2, SHORE);
        canvas.rect(0, 13, 16, 1, SHORE_LIGHT);
        canvas.line(0, 12, 15, 12, SHALLOW);
    }
    if connections & WEST == 0 {
        canvas.rect(0, 0, 2, 16, SHORE);
        canvas.rect(2, 0, 1, 16, SHORE_LIGHT);
        canvas.line(3, 0, 3, 15, SHALLOW);
    }

    // Diagonal-only exposure creates a small inward cove instead of a hard
    // square corner. Cardinal exposure receives a larger rounded beach wedge.
    shoreline_corner(
        canvas,
        connections,
        NORTH,
        WEST,
        NORTH_WEST,
        0,
        0,
        SHORE,
        SHORE_LIGHT,
    );
    shoreline_corner(
        canvas,
        connections,
        NORTH,
        EAST,
        NORTH_EAST,
        15,
        0,
        SHORE,
        SHORE_LIGHT,
    );
    shoreline_corner(
        canvas,
        connections,
        SOUTH,
        EAST,
        SOUTH_EAST,
        15,
        15,
        SHORE,
        SHORE_LIGHT,
    );
    shoreline_corner(
        canvas,
        connections,
        SOUTH,
        WEST,
        SOUTH_WEST,
        0,
        15,
        SHORE,
        SHORE_LIGHT,
    );

    let offset = i32::from(variant % 4);
    for y in [4 + offset, 9 + (offset / 2)] {
        canvas.line(4, y, 7, y, FOAM);
        canvas.line(9, y + 1, 12, y + 1, FOAM);
    }
}

#[allow(clippy::too_many_arguments)]
fn shoreline_corner(
    canvas: &mut Canvas,
    connections: u8,
    first: u8,
    second: u8,
    diagonal: u8,
    x: i32,
    y: i32,
    shore: Rgba8,
    light: Rgba8,
) {
    let first_open = connections & first == 0;
    let second_open = connections & second == 0;
    let diagonal_open = connections & diagonal == 0;
    if first_open && second_open {
        rounded_external_corner(canvas, x, y, shore, light);
    } else if diagonal_open && !first_open && !second_open {
        rounded_diagonal_notch(canvas, x, y, shore, light);
    }
}

fn rounded_external_corner(
    canvas: &mut Canvas,
    corner_x: i32,
    corner_y: i32,
    edge: Rgba8,
    inner: Rgba8,
) {
    let center_x = if corner_x == 0 { 5 } else { 10 };
    let center_y = if corner_y == 0 { 5 } else { 10 };
    let x_range = if corner_x == 0 { 0..=5 } else { 10..=15 };
    let y_range = if corner_y == 0 { 0..=5 } else { 10..=15 };
    const TRANSPARENT: Rgba8 = Rgba8::rgba(0, 0, 0, 0);
    for py in y_range {
        for px in x_range.clone() {
            let dx = px - center_x;
            let dy = py - center_y;
            let distance = dx * dx + dy * dy;
            if distance > 25 {
                canvas.pixel(px, py, TRANSPARENT);
            } else if distance >= 16 {
                canvas.pixel(px, py, edge);
            } else if distance >= 9 {
                canvas.pixel(px, py, inner);
            }
        }
    }
}

fn rounded_diagonal_notch(
    canvas: &mut Canvas,
    corner_x: i32,
    corner_y: i32,
    edge: Rgba8,
    inner: Rgba8,
) {
    const TRANSPARENT: Rgba8 = Rgba8::rgba(0, 0, 0, 0);
    let x_range = if corner_x == 0 { 0..=4 } else { 11..=15 };
    let y_range = if corner_y == 0 { 0..=4 } else { 11..=15 };
    for py in y_range {
        for px in x_range.clone() {
            let dx = px - corner_x;
            let dy = py - corner_y;
            let distance = dx * dx + dy * dy;
            if distance <= 4 {
                canvas.pixel(px, py, TRANSPARENT);
            } else if distance <= 10 {
                canvas.pixel(px, py, edge);
            } else if distance <= 16 {
                canvas.pixel(px, py, inner);
            }
        }
    }
}

pub(super) fn rock(canvas: &mut Canvas, variant: u8, connections: u8) {
    const BASE: Rgba8 = Rgba8::rgb(105, 105, 101);
    const LIGHT: Rgba8 = Rgba8::rgb(132, 132, 126);
    const DARK: Rgba8 = Rgba8::rgb(72, 73, 72);
    const TALUS: Rgba8 = Rgba8::rgb(123, 118, 103);
    const SOIL: Rgba8 = Rgba8::rgb(109, 104, 78);
    const MOSS: Rgba8 = Rgba8::rgb(78, 111, 61);

    canvas.fill(BASE);
    canvas.scatter(
        0x524f_434b_0000 + u64::from(variant),
        22,
        [0, 0, 16, 16],
        &[LIGHT, DARK],
    );
    let shift = i32::from(variant & 1);
    canvas.line(2 + shift, 1, 7 + shift, 6, DARK);
    canvas.line(7 + shift, 6, 5 + shift, 11, DARK);
    canvas.line(7 + shift, 6, 12 + shift, 8, LIGHT);
    canvas.line(12 + shift, 8, 14, 13, DARK);

    if connections & NORTH == 0 {
        foothill_edge(canvas, 0, 0, 16, 3, true, TALUS, SOIL, MOSS);
    }
    if connections & EAST == 0 {
        foothill_edge(canvas, 13, 0, 3, 16, false, TALUS, SOIL, MOSS);
    }
    if connections & SOUTH == 0 {
        foothill_edge(canvas, 0, 13, 16, 3, true, TALUS, SOIL, MOSS);
    }
    if connections & WEST == 0 {
        foothill_edge(canvas, 0, 0, 3, 16, false, TALUS, SOIL, MOSS);
    }

    foothill_corner(
        canvas,
        connections,
        NORTH,
        WEST,
        NORTH_WEST,
        0,
        0,
        TALUS,
        SOIL,
        MOSS,
    );
    foothill_corner(
        canvas,
        connections,
        NORTH,
        EAST,
        NORTH_EAST,
        15,
        0,
        TALUS,
        SOIL,
        MOSS,
    );
    foothill_corner(
        canvas,
        connections,
        SOUTH,
        EAST,
        SOUTH_EAST,
        15,
        15,
        TALUS,
        SOIL,
        MOSS,
    );
    foothill_corner(
        canvas,
        connections,
        SOUTH,
        WEST,
        SOUTH_WEST,
        0,
        15,
        TALUS,
        SOIL,
        MOSS,
    );
}

#[allow(clippy::too_many_arguments)]
fn foothill_edge(
    canvas: &mut Canvas,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    horizontal: bool,
    talus: Rgba8,
    soil: Rgba8,
    moss: Rgba8,
) {
    canvas.rect(x, y, width, height, talus);
    if horizontal {
        let line_y = if y == 0 { y + 2 } else { y };
        canvas.line(x, line_y, x + width - 1, line_y, soil);
    } else {
        let line_x = if x == 0 { x + 2 } else { x };
        canvas.line(line_x, y, line_x, y + height - 1, soil);
    }
    canvas.scatter(
        0x5441_4c55_5300 ^ ((x as u64) << 8) ^ (y as u64),
        5,
        [x, y, width, height],
        &[soil, moss],
    );
}

#[allow(clippy::too_many_arguments)]
fn foothill_corner(
    canvas: &mut Canvas,
    connections: u8,
    first: u8,
    second: u8,
    diagonal: u8,
    x: i32,
    y: i32,
    talus: Rgba8,
    soil: Rgba8,
    moss: Rgba8,
) {
    let first_open = connections & first == 0;
    let second_open = connections & second == 0;
    let diagonal_open = connections & diagonal == 0;
    if first_open && second_open {
        rounded_external_corner(canvas, x, y, soil, talus);
        let inner_x = if x == 0 { 4 } else { 11 };
        let inner_y = if y == 0 { 4 } else { 11 };
        canvas.pixel(inner_x, inner_y, moss);
    } else if diagonal_open && !first_open && !second_open {
        rounded_diagonal_notch(canvas, x, y, soil, talus);
        let moss_x = if x == 0 { 3 } else { 12 };
        let moss_y = if y == 0 { 3 } else { 12 };
        canvas.pixel(moss_x, moss_y, moss);
    }
}
