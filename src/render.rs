use crate::font::Rasterizer;
use crate::keys::{Cell, COLS, KEYS, LEGEND, ROWS};

pub const LEGEND_H: i32 = 14;
pub const DEFAULT_HEIGHT: i32 = 28 * crate::keys::ROWS as i32 + LEGEND_H;

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
/// `sel_col` highlighted (shifted labels if `shift`), the Shift/Ctrl/R1/R2
/// indicator cells lit up while their chord is held, and the
/// latched-modifier tint in the grid's top-left corner if `latched`.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    sel_row: usize,
    sel_col: usize,
    shift: bool,
    ctrl: bool,
    r1: bool,
    r2: bool,
    latched: bool,
    font: &Rasterizer,
) {
    let grid_y0 = LEGEND_H;
    let grid_h = height - LEGEND_H;
    let cw = width / COLS as i32;
    let ch = grid_h / ROWS as i32;
    let key_px = (ch as f32 / 2.0).max(9.0);
    // Leaves room for descenders: at legend_px this size, a line (ascent +
    // descent) runs close to LEGEND_H tall, and drawing from baseline alone
    // (as draw_text does) doesn't clip to LEGEND_H, so a too-large size here
    // bleeds descenders into the grid's top row.
    let legend_px = (LEGEND_H as f32 - 4.0).max(8.0);

    fill(pixels, width, height, 0, 0, width, height, COLOR_BG);

    draw_text(pixels, width, height, 4, 1, legend_px, LEGEND, COLOR_LEGEND, font);

    for r in 0..ROWS {
        for c in 0..COLS {
            let x = c as i32 * cw;
            let y = grid_y0 + r as i32 * ch;
            let selected = r == sel_row && c == sel_col;

            let (label, bg) = match &KEYS[r][c] {
                Cell::Empty => {
                    // An empty cell (the grid runs short of a full rectangle
                    // in places) reads as a gap, except when selected: still
                    // show that, so landing here isn't mistaken for a stuck
                    // cursor.
                    if selected {
                        fill(pixels, width, height, x + 1, y + 1, cw - 2, ch - 2, COLOR_SELECTED);
                    }
                    continue;
                }
                Cell::Key(key) => {
                    let label = if shift { key.shifted } else { key.label };
                    (label, if selected { COLOR_SELECTED } else { COLOR_KEY })
                }
                // Held state wins over selection: knowing the chord is down
                // matters more than where the cursor happens to be.
                Cell::Shift => ("Shift", if shift { COLOR_LATCHED } else if selected { COLOR_SELECTED } else { COLOR_KEY }),
                Cell::Ctrl => ("Ctrl", if ctrl { COLOR_LATCHED } else if selected { COLOR_SELECTED } else { COLOR_KEY }),
                Cell::R1 => ("R1", if r1 { COLOR_LATCHED } else if selected { COLOR_SELECTED } else { COLOR_KEY }),
                Cell::R2 => ("R2", if r2 { COLOR_LATCHED } else if selected { COLOR_SELECTED } else { COLOR_KEY }),
            };

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

    #[allow(clippy::too_many_arguments)]
    fn render(
        sel_row: usize,
        sel_col: usize,
        shift: bool,
        ctrl: bool,
        r1: bool,
        r2: bool,
        latched: bool,
    ) -> Vec<u32> {
        let mut pixels = vec![0u32; (WIDTH * HEIGHT) as usize];
        let font = Rasterizer::load(crate::font::DEFAULT_PATH);
        draw(&mut pixels, WIDTH, HEIGHT, sel_row, sel_col, shift, ctrl, r1, r2, latched, &font);
        pixels
    }

    fn px(pixels: &[u32], x: i32, y: i32) -> u32 {
        pixels[(y * WIDTH + x) as usize]
    }

    // The grid's top-left corner (row 0, col 0) is always a real key ('`'),
    // never an indicator or gap, so it's a safe stand-in for "some key cell".
    #[test]
    fn selected_cell_gets_the_selected_color() {
        let pixels = render(0, 0, false, false, false, false, false);
        assert_eq!(px(&pixels, 2, LEGEND_H + 2), COLOR_SELECTED);
    }

    #[test]
    fn unselected_cell_gets_the_key_color() {
        let pixels = render(2, 2, false, false, false, false, false);
        assert_eq!(px(&pixels, 2, LEGEND_H + 2), COLOR_KEY);
    }

    #[test]
    fn latched_modifier_tints_the_grids_corner() {
        let pixels = render(2, 2, false, false, false, false, true);
        assert_eq!(px(&pixels, 2, LEGEND_H + 2), COLOR_LATCHED);
    }

    fn row_h() -> i32 {
        (DEFAULT_HEIGHT - LEGEND_H) / ROWS as i32
    }

    // Column 0's pixel offset into a cell; see the analogous helper for the
    // last column below.
    fn left_cell_y(row: i32) -> i32 {
        LEGEND_H + row * row_h() + 2
    }

    fn cell_xy(row: i32, col: i32) -> (i32, i32) {
        (col * (WIDTH / COLS as i32) + 2, LEGEND_H + row * row_h() + 2)
    }

    // Row 2 col 0 is the Ctrl indicator, row 3 col 0 is the Shift indicator
    // (see keys::KEYS); neither is ever the current selection in these
    // cases, so any highlighting comes only from the held chord.
    #[test]
    fn ctrl_indicator_highlights_only_while_ctrl_is_held() {
        let y = left_cell_y(2);

        let pixels = render(0, 0, false, false, false, false, false);
        assert_eq!(px(&pixels, 2, y), COLOR_KEY);

        let pixels = render(0, 0, false, true, false, false, false);
        assert_eq!(px(&pixels, 2, y), COLOR_LATCHED);
    }

    #[test]
    fn shift_indicator_highlights_only_while_shift_is_held() {
        let y = left_cell_y(3);

        let pixels = render(0, 0, false, false, false, false, false);
        assert_eq!(px(&pixels, 2, y), COLOR_KEY);

        let pixels = render(0, 0, true, false, false, false, false);
        assert_eq!(px(&pixels, 2, y), COLOR_LATCHED);
    }

    // Bottom row col 12 is the R2 indicator, col 13 is R1 (see keys::KEYS):
    // left to right, R2 then R1, matching reaching across the shoulder.
    #[test]
    fn r2_indicator_highlights_only_while_r2_is_held() {
        let (x, y) = cell_xy(3, 12);

        let pixels = render(0, 0, false, false, false, false, false);
        assert_eq!(px(&pixels, x, y), COLOR_KEY);

        let pixels = render(0, 0, false, false, false, true, false);
        assert_eq!(px(&pixels, x, y), COLOR_LATCHED);
    }

    #[test]
    fn r1_indicator_highlights_only_while_r1_is_held() {
        let (x, y) = cell_xy(3, 13);

        let pixels = render(0, 0, false, false, false, false, false);
        assert_eq!(px(&pixels, x, y), COLOR_KEY);

        let pixels = render(0, 0, false, false, true, false, false);
        assert_eq!(px(&pixels, x, y), COLOR_LATCHED);
    }

    #[test]
    fn legend_strip_draws_visible_text() {
        let pixels = render(0, 0, false, false, false, false, false);
        let drawn = (0..WIDTH)
            .flat_map(|x| (0..LEGEND_H).map(move |y| (x, y)))
            .filter(|&(x, y)| px(&pixels, x, y) != COLOR_BG)
            .count();
        assert!(drawn > 100, "expected the legend line to paint many pixels, got {drawn}");
    }
}
