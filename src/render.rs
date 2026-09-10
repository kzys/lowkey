use crate::font::Rasterizer;
use crate::keys::{COLS, KEYS, LEGEND, ROWS};

pub const LEGEND_H: i32 = 18;
pub const DEFAULT_HEIGHT: i32 = 180 + LEGEND_H;

pub const COLOR_BG: u32 = 0xff1d1d1d;
pub const COLOR_KEY: u32 = 0xff2f2f36;
pub const COLOR_SELECTED: u32 = 0xff5f6f9f;
pub const COLOR_LATCHED: u32 = 0xff8f5f3f;
pub const COLOR_TEXT: u32 = 0xffffffff;
pub const COLOR_LEGEND: u32 = 0xff9a9aa5;

fn text_width(font: &Rasterizer, text: &str, px: f32) -> f32 {
    text.chars().map(|ch| font.advance(ch, px)).sum()
}

/// Draws `text` left-aligned with its top-left corner at `(x, y)`.
fn draw_text(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    px: f32,
    text: &str,
    color: u32,
    font: &Rasterizer,
) {
    let baseline_y = y + font.ascent(px).round() as i32;
    let mut cursor = x as f32;
    for ch in text.chars() {
        font.draw(pixels, width, height, cursor.round() as i32, baseline_y, px, ch, color);
        cursor += font.advance(ch, px);
    }
}

fn fill(pixels: &mut [u32], width: i32, height: i32, x: i32, y: i32, w: i32, h: i32, color: u32) {
    for py in y..y + h {
        if py < 0 || py >= height {
            continue;
        }
        for px in x..x + w {
            if px >= 0 && px < width {
                pixels[(py * width + px) as usize] = color;
            }
        }
    }
}

/// Renders the full overlay: the legend line, the key grid with `sel_row`/
/// `sel_col` highlighted (shifted labels if `shift`), and the latched-modifier
/// tint in the grid's top-left corner if `latched`.
pub fn draw(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    sel_row: usize,
    sel_col: usize,
    shift: bool,
    latched: bool,
    font: &Rasterizer,
) {
    let grid_y0 = LEGEND_H;
    let grid_h = height - LEGEND_H;
    let cw = width / COLS as i32;
    let ch = grid_h / ROWS as i32;
    let key_px = (ch as f32 / 2.5).max(8.0);
    let legend_px = (LEGEND_H as f32 - 4.0).max(8.0);

    fill(pixels, width, height, 0, 0, width, height, COLOR_BG);

    draw_text(pixels, width, height, 4, 2, legend_px, LEGEND, COLOR_LEGEND, font);

    for r in 0..ROWS {
        for c in 0..COLS {
            let key = &KEYS[r][c];
            let label = if shift { key.shifted } else { key.label };
            let x = c as i32 * cw;
            let y = grid_y0 + r as i32 * ch;
            let bg = if r == sel_row && c == sel_col { COLOR_SELECTED } else { COLOR_KEY };

            fill(pixels, width, height, x + 1, y + 1, cw - 2, ch - 2, bg);

            let label_w = text_width(font, label, key_px);
            let tx = x + ((cw as f32 - label_w) / 2.0).round() as i32;
            let ty = y + ((ch as f32 - key_px) / 2.0).round() as i32;
            draw_text(pixels, width, height, tx, ty, key_px, label, COLOR_TEXT, font);
        }
    }

    // A latched modifier tints the top-left corner of the grid, which is
    // cheaper to read at a glance than a status line.
    if latched {
        fill(pixels, width, height, 0, grid_y0, cw / 4, ch / 4, COLOR_LATCHED);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: i32 = 640;
    const HEIGHT: i32 = DEFAULT_HEIGHT;

    fn render(sel_row: usize, sel_col: usize, shift: bool, latched: bool) -> Vec<u32> {
        let mut pixels = vec![0u32; (WIDTH * HEIGHT) as usize];
        let font = Rasterizer::load(crate::font::DEFAULT_PATH);
        draw(&mut pixels, WIDTH, HEIGHT, sel_row, sel_col, shift, latched, &font);
        pixels
    }

    fn px(pixels: &[u32], x: i32, y: i32) -> u32 {
        pixels[(y * WIDTH + x) as usize]
    }

    #[test]
    fn selected_cell_gets_the_selected_color() {
        let pixels = render(0, 0, false, false);
        assert_eq!(px(&pixels, 2, LEGEND_H + 2), COLOR_SELECTED);
    }

    #[test]
    fn unselected_cell_gets_the_key_color() {
        let pixels = render(2, 2, false, false);
        assert_eq!(px(&pixels, 2, LEGEND_H + 2), COLOR_KEY);
    }

    #[test]
    fn latched_modifier_tints_the_grids_corner() {
        let pixels = render(2, 2, false, true);
        assert_eq!(px(&pixels, 2, LEGEND_H + 2), COLOR_LATCHED);
    }

    #[test]
    fn legend_strip_draws_visible_text() {
        let pixels = render(0, 0, false, false);
        let drawn = (0..WIDTH)
            .flat_map(|x| (0..LEGEND_H).map(move |y| (x, y)))
            .filter(|&(x, y)| px(&pixels, x, y) != COLOR_BG)
            .count();
        assert!(drawn > 100, "expected the legend line to paint many pixels, got {drawn}");
    }
}
