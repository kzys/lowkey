use crate::font8x8::FONT8X8_BASIC;
use crate::keys::{COLS, KEYS, LEGEND, ROWS};

pub const GLYPH: i32 = 8;
pub const LEGEND_H: i32 = 18;
pub const DEFAULT_HEIGHT: i32 = 180 + LEGEND_H;

pub const COLOR_BG: u32 = 0xff1d1d1d;
pub const COLOR_KEY: u32 = 0xff2f2f36;
pub const COLOR_SELECTED: u32 = 0xff5f6f9f;
pub const COLOR_LATCHED: u32 = 0xff8f5f3f;
pub const COLOR_TEXT: u32 = 0xffffffff;
pub const COLOR_LEGEND: u32 = 0xff9a9aa5;

fn draw_glyph(pixels: &mut [u32], width: i32, height: i32, x: i32, y: i32, scale: i32, ch: char) {
    draw_glyph_color(pixels, width, height, x, y, scale, ch, COLOR_TEXT);
}

fn draw_glyph_color(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    scale: i32,
    ch: char,
    color: u32,
) {
    if !ch.is_ascii() {
        return;
    }
    let rows = &FONT8X8_BASIC[ch as usize];

    for gy in 0..GLYPH {
        for gx in 0..GLYPH {
            if rows[gy as usize] & (1 << gx) == 0 {
                continue;
            }
            for py in 0..scale {
                for px in 0..scale {
                    let fx = x + gx * scale + px;
                    let fy = y + gy * scale + py;
                    if fx >= 0 && fx < width && fy >= 0 && fy < height {
                        pixels[(fy * width + fx) as usize] = color;
                    }
                }
            }
        }
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
) {
    let grid_y0 = LEGEND_H;
    let grid_h = height - LEGEND_H;
    let cw = width / COLS as i32;
    let ch = grid_h / ROWS as i32;
    let scale = (ch / (GLYPH * 2)).max(1);

    fill(pixels, width, height, 0, 0, width, height, COLOR_BG);

    for (i, ch_) in LEGEND.chars().enumerate() {
        draw_glyph_color(pixels, width, height, 4 + i as i32 * GLYPH, 2, 1, ch_, COLOR_LEGEND);
    }

    for r in 0..ROWS {
        for c in 0..COLS {
            let key = &KEYS[r][c];
            let label = if shift { key.shifted } else { key.label };
            let len = label.chars().count() as i32;
            let x = c as i32 * cw;
            let y = grid_y0 + r as i32 * ch;
            let bg = if r == sel_row && c == sel_col { COLOR_SELECTED } else { COLOR_KEY };

            fill(pixels, width, height, x + 1, y + 1, cw - 2, ch - 2, bg);

            let tx = x + (cw - len * GLYPH * scale) / 2;
            let ty = y + (ch - GLYPH * scale) / 2;
            for (i, ch_) in label.chars().enumerate() {
                draw_glyph(pixels, width, height, tx + i as i32 * GLYPH * scale, ty, scale, ch_);
            }
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
        draw(&mut pixels, WIDTH, HEIGHT, sel_row, sel_col, shift, latched);
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
