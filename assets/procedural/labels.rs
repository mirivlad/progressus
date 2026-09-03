use super::{Canvas, Rgba8};

const GLYPH_WIDTH: i32 = 3;
const GLYPH_HEIGHT: i32 = 5;
const GLYPH_GAP: i32 = 1;
const PADDING: i32 = 1;

pub(super) fn quantity_dimensions(quantity: u32) -> (u32, u32) {
    let digits = quantity.to_string().len() as i32;
    let width = PADDING * 2 + digits * GLYPH_WIDTH + (digits - 1).max(0) * GLYPH_GAP;
    let height = PADDING * 2 + GLYPH_HEIGHT;
    (width as u32, height as u32)
}

pub(super) fn quantity_label(canvas: &mut Canvas, quantity: u32) {
    const BACKGROUND: Rgba8 = Rgba8::rgba(18, 20, 22, 220);
    const FOREGROUND: Rgba8 = Rgba8::rgb(247, 241, 205);
    canvas.fill(BACKGROUND);
    for (index, digit) in quantity.to_string().bytes().enumerate() {
        let x = PADDING + index as i32 * (GLYPH_WIDTH + GLYPH_GAP);
        draw_digit(canvas, x, PADDING, digit - b'0', FOREGROUND);
    }
}

fn draw_digit(canvas: &mut Canvas, x: i32, y: i32, digit: u8, color: Rgba8) {
    let rows = DIGITS[digit as usize];
    for (dy, row) in rows.into_iter().enumerate() {
        for dx in 0..GLYPH_WIDTH {
            if row & (1 << (GLYPH_WIDTH - 1 - dx)) != 0 {
                canvas.pixel(x + dx, y + dy as i32, color);
            }
        }
    }
}

const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];
