use crate::font::{Canvas, Point, Rasterizer, Size};
use crate::keys::{Cell, COLS, KEYS, LEGEND, LEGEND_R1, ROWS};

pub const LEGEND_H: i32 = 14;
pub const DEFAULT_HEIGHT: i32 = 28 * crate::keys::ROWS as i32 + LEGEND_H;

// Grayscale throughout, with exactly two accents: blue marks the
// focused/selected key, red marks a held chord. Nothing else in the grid
// uses color, so either one reads at a glance.
pub const COLOR_BG: u32 = 0xff121212;
pub const COLOR_KEY: u32 = 0xff232323;
pub const COLOR_SELECTED: u32 = 0xff4f8cff;
pub const COLOR_SELECTED_TEXT: u32 = 0xff121212;
pub const COLOR_TEXT: u32 = 0xffececec;
pub const COLOR_LEGEND: u32 = 0xff8a8a8a;
/// Idle background for the Shift/Ctrl/R1 indicator cells — distinct from a
/// regular key so an unheld chord doesn't read as a typeable key.
pub const COLOR_IND: u32 = 0xff1a1a1a;
pub const COLOR_IND_BORDER: u32 = 0xff3a3a3a;
pub const COLOR_IND_TEXT: u32 = 0xff8a8a8a;
/// An indicator lit because its chord is currently held (also used for the
/// latched-modifier corner tint).
pub const COLOR_LATCHED: u32 = 0xffe5484d;
pub const COLOR_LATCHED_TEXT: u32 = 0xff121212;

/// The D-pad/face-button overlay shown while R1 is held, replacing the grid
/// (see `pad::decode` for what each face button sends in this state). Left
/// diamond mirrors the D-pad; right diamond mirrors X/Y/A/B.
const NAV_TILES: [(usize, usize, &str); 8] = [
    (0, 2, "Up"),
    (1, 1, "Left"),
    (1, 3, "Right"),
    (2, 2, "Down"),
    (0, 11, "PgUp"),
    (1, 10, "Tab"),
    (1, 12, "Esc"),
    (2, 11, "PgDn"),
];

fn text_width(font: &Rasterizer, text: &str, px: f32) -> f32 {
    text.chars().map(|ch| font.advance(ch, px)).sum()
}

/// Draws `text` left-aligned with its top-left corner at `pos`. `bold` draws
/// each glyph twice, offset by a pixel, to fake a heavier weight the loaded
/// font file may not have.
fn draw_text(canvas: &mut Canvas, pos: Point, px: f32, text: &str, color: u32, font: &Rasterizer, bold: bool) {
    let baseline_y = pos.y + font.ascent(px).round() as i32;
    let mut cursor = pos.x as f32;
    for ch in text.chars() {
        let cx = cursor.round() as i32;
        font.draw(canvas, cx, baseline_y, px, ch, color);
        if bold {
            font.draw(canvas, cx + 1, baseline_y, px, ch, color);
        }
        cursor += font.advance(ch, px);
    }
}

fn fill(canvas: &mut Canvas, pos: Point, size: Size, color: u32) {
    for py in pos.y..pos.y + size.height {
        if py < 0 || py >= canvas.height {
            continue;
        }
        for px in pos.x..pos.x + size.width {
            if px >= 0 && px < canvas.width {
                canvas.pixels[(py * canvas.width + px) as usize] = color;
            }
        }
    }
}

/// Fills a cell with a 1px border ring around a (possibly different) fill
/// color, so indicator cells read distinctly from typeable keys.
fn fill_bordered(canvas: &mut Canvas, pos: Point, size: Size, border: u32, fill_color: u32) {
    fill(canvas, pos, size, border);
    fill(canvas, Point::new(pos.x + 1, pos.y + 1), Size::new(size.width - 2, size.height - 2), fill_color);
}

/// Draws `label` centered in the cell at grid position `pos`-`size`.
#[allow(clippy::too_many_arguments)]
fn draw_label(canvas: &mut Canvas, pos: Point, size: Size, key_px: f32, label: &str, color: u32, bold: bool, font: &Rasterizer) {
    let label_w = text_width(font, label, key_px);
    let tx = pos.x + ((size.width as f32 - label_w) / 2.0).round() as i32;
    let ty = pos.y + ((size.height as f32 - key_px) / 2.0).round() as i32;
    draw_text(canvas, Point::new(tx, ty), key_px, label, color, font, bold);
}

/// Draws a Shift/Ctrl/R1 indicator cell: bordered and dim while idle, solid
/// red with bold dark text while its chord is held.
fn draw_indicator(canvas: &mut Canvas, pos: Point, size: Size, key_px: f32, label: &str, held: bool, font: &Rasterizer) {
    let (border, fill_color, text_color) =
        if held { (COLOR_LATCHED, COLOR_LATCHED, COLOR_LATCHED_TEXT) } else { (COLOR_IND_BORDER, COLOR_IND, COLOR_IND_TEXT) };
    fill_bordered(canvas, Point::new(pos.x + 1, pos.y + 1), Size::new(size.width - 2, size.height - 2), border, fill_color);
    draw_label(canvas, pos, size, key_px, label, text_color, held, font);
}

/// Draws the D-pad/face-button overlay in place of the (now dimmed) keys it
/// sits over.
fn draw_nav_overlay(canvas: &mut Canvas, grid_y0: i32, cell: Size, font: &Rasterizer) {
    // Smaller than a regular key's text: tiles carry longer labels ("Right",
    // "PgDn") than a single keycap glyph.
    let nav_px = (cell.height as f32 * 0.32).max(7.0);
    for &(row, col, label) in NAV_TILES.iter() {
        let pos = Point::new(col as i32 * cell.width, grid_y0 + row as i32 * cell.height);
        fill(canvas, Point::new(pos.x + 1, pos.y + 1), Size::new(cell.width - 2, cell.height - 2), COLOR_KEY);
        draw_label(canvas, pos, cell, nav_px, label, COLOR_TEXT, false, font);
    }
}

/// Renders the full overlay: the legend line, the key grid with `sel_row`/
/// `sel_col` highlighted (shifted labels if `shift`), the Shift/Ctrl/R1
/// indicator cells lit up while their chord is held, the latched-modifier
/// tint in the grid's top-left corner if `latched`, and — while R1 is held —
/// the nav overlay in place of the (dimmed) typeable keys.
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
    latched: bool,
    font: &Rasterizer,
) {
    let canvas = &mut Canvas::new(pixels, width, height);

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

    fill(canvas, Point::new(0, 0), Size::new(width, height), COLOR_BG);

    let legend = if r1 { LEGEND_R1 } else { LEGEND };
    draw_text(canvas, Point::new(4, 1), legend_px, legend, COLOR_LEGEND, font, false);

    let cell = Size::new(cw, ch);

    #[allow(clippy::needless_range_loop)]
    for r in 0..ROWS {
        for c in 0..COLS {
            let pos = Point::new(c as i32 * cw, grid_y0 + r as i32 * ch);
            // Selection is meaningless while R1's nav overlay is up: the
            // grid isn't being typed into, it's sending arrows/paging.
            let selected = !r1 && r == sel_row && c == sel_col;

            match &KEYS[r][c] {
                Cell::Empty => {
                    // An empty cell (the grid runs short of a full rectangle
                    // in places) reads as a gap, except when selected: still
                    // show that, so landing here isn't mistaken for a stuck
                    // cursor.
                    if selected {
                        fill(canvas, Point::new(pos.x + 1, pos.y + 1), Size::new(cw - 2, ch - 2), COLOR_SELECTED);
                    }
                }
                Cell::Key(key) => {
                    // Dimmed to nothing while R1's nav overlay is up; some of
                    // these cells get a nav tile drawn over them below.
                    if r1 {
                        continue;
                    }
                    let label = if shift { key.shifted } else { key.label };
                    let (bg, text_color, bold) =
                        if selected { (COLOR_SELECTED, COLOR_SELECTED_TEXT, true) } else { (COLOR_KEY, COLOR_TEXT, false) };
                    fill(canvas, Point::new(pos.x + 1, pos.y + 1), Size::new(cw - 2, ch - 2), bg);
                    draw_label(canvas, pos, cell, key_px, label, text_color, bold, font);
                }
                Cell::Shift => draw_indicator(canvas, pos, cell, key_px, "Shift", shift, font),
                Cell::Ctrl => draw_indicator(canvas, pos, cell, key_px, "Ctrl", ctrl, font),
                Cell::R1 => draw_indicator(canvas, pos, cell, key_px, "R1", r1, font),
            }
        }
    }

    if r1 {
        draw_nav_overlay(canvas, grid_y0, cell, font);
    }

    // A latched modifier tints the top-left corner of the grid, which is
    // cheaper to read at a glance than a status line.
    if latched {
        fill(canvas, Point::new(0, grid_y0), Size::new(cw / 4, ch / 4), COLOR_LATCHED);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: i32 = 640;
    const HEIGHT: i32 = DEFAULT_HEIGHT;

    #[allow(clippy::too_many_arguments)]
    fn render(sel_row: usize, sel_col: usize, shift: bool, ctrl: bool, r1: bool, latched: bool) -> Vec<u32> {
        let mut pixels = vec![0u32; (WIDTH * HEIGHT) as usize];
        let font = Rasterizer::load(crate::font::DEFAULT_PATH);
        draw(&mut pixels, WIDTH, HEIGHT, sel_row, sel_col, shift, ctrl, r1, latched, &font);
        pixels
    }

    fn px(pixels: &[u32], x: i32, y: i32) -> u32 {
        pixels[(y * WIDTH + x) as usize]
    }

    // The grid's top-left corner (row 0, col 0) is always a real key ('`'),
    // never an indicator or gap, so it's a safe stand-in for "some key cell".
    #[test]
    fn selected_cell_gets_the_selected_color() {
        let pixels = render(0, 0, false, false, false, false);
        assert_eq!(px(&pixels, 2, LEGEND_H + 2), COLOR_SELECTED);
    }

    #[test]
    fn unselected_cell_gets_the_key_color() {
        let pixels = render(2, 2, false, false, false, false);
        assert_eq!(px(&pixels, 2, LEGEND_H + 2), COLOR_KEY);
    }

    #[test]
    fn latched_modifier_tints_the_grids_corner() {
        let pixels = render(2, 2, false, false, false, true);
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

        let pixels = render(0, 0, false, false, false, false);
        assert_eq!(px(&pixels, 2, y), COLOR_IND);

        let pixels = render(0, 0, false, true, false, false);
        assert_eq!(px(&pixels, 2, y), COLOR_LATCHED);
    }

    #[test]
    fn shift_indicator_highlights_only_while_shift_is_held() {
        let y = left_cell_y(3);

        let pixels = render(0, 0, false, false, false, false);
        assert_eq!(px(&pixels, 2, y), COLOR_IND);

        let pixels = render(0, 0, true, false, false, false);
        assert_eq!(px(&pixels, 2, y), COLOR_LATCHED);
    }

    // Bottom row col 13 is the R1 indicator (see keys::KEYS).
    #[test]
    fn r1_indicator_highlights_only_while_r1_is_held() {
        let (x, y) = cell_xy(3, 13);

        let pixels = render(0, 0, false, false, false, false);
        assert_eq!(px(&pixels, x, y), COLOR_IND);

        let pixels = render(0, 0, false, false, true, false);
        assert_eq!(px(&pixels, x, y), COLOR_LATCHED);
    }

    // Row 0 col 0 ('`') is a typeable key that R1's nav overlay dims to the
    // plain background, since it isn't one of the eight tiles reused for
    // arrows/paging.
    #[test]
    fn r1_held_dims_ordinary_keys_to_the_background() {
        let pixels = render(0, 0, false, false, true, false);
        assert_eq!(px(&pixels, 2, LEGEND_H + 2), COLOR_BG);
    }

    // Row 1 col 1 ('q' normally) becomes the "Left" arrow tile while R1 is
    // held, styled like a plain key rather than an indicator.
    #[test]
    fn r1_held_draws_a_nav_tile_over_the_dimmed_key_beneath_it() {
        let (x, y) = cell_xy(1, 1);
        let pixels = render(0, 0, false, false, true, false);
        assert_eq!(px(&pixels, x, y), COLOR_KEY);
    }

    #[test]
    fn legend_strip_draws_visible_text() {
        let pixels = render(0, 0, false, false, false, false);
        let drawn = (0..WIDTH)
            .flat_map(|x| (0..LEGEND_H).map(move |y| (x, y)))
            .filter(|&(x, y)| px(&pixels, x, y) != COLOR_BG)
            .count();
        assert!(drawn > 100, "expected the legend line to paint many pixels, got {drawn}");
    }
}
