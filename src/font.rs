// Loads a system font and rasterizes glyphs on demand, caching results since
// the same characters (key labels, legend) get redrawn on every frame.

use std::cell::RefCell;
use std::collections::HashMap;

use fontdue::{Font, FontSettings, Metrics};

use crate::util::die;

pub const DEFAULT_PATH: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf";

type GlyphCache = HashMap<(char, u32), (Metrics, Vec<u8>)>;

/// A pixel buffer and its dimensions, bundled so drawing functions don't
/// have to take `(pixels, width, height)` as three separate arguments.
pub struct Canvas<'a> {
    pub pixels: &'a mut [u32],
    pub width: i32,
    pub height: i32,
}

impl<'a> Canvas<'a> {
    pub fn new(pixels: &'a mut [u32], width: i32, height: i32) -> Canvas<'a> {
        Canvas { pixels, width, height }
    }
}

pub struct Rasterizer {
    font: Font,
    cache: RefCell<GlyphCache>,
}

impl Rasterizer {
    pub fn load(path: &str) -> Rasterizer {
        let bytes = std::fs::read(path).unwrap_or_else(|e| die(&format!("cannot read font {path}: {e}")));
        let font = Font::from_bytes(bytes, FontSettings::default())
            .unwrap_or_else(|e| die(&format!("cannot parse font {path}: {e}")));
        Rasterizer { font, cache: RefCell::new(HashMap::new()) }
    }

    fn glyph(&self, ch: char, px: f32) -> (Metrics, Vec<u8>) {
        let key = (ch, px.to_bits());
        if let Some(g) = self.cache.borrow().get(&key) {
            return g.clone();
        }
        let g = self.font.rasterize(ch, px);
        self.cache.borrow_mut().insert(key, g.clone());
        g
    }

    pub fn advance(&self, ch: char, px: f32) -> f32 {
        self.font.metrics(ch, px).advance_width
    }

    pub fn ascent(&self, px: f32) -> f32 {
        self.font.horizontal_line_metrics(px).map(|m| m.ascent).unwrap_or(px)
    }

    /// Draws `ch` with its baseline-left origin at `(x, baseline_y)`, blending
    /// the glyph's coverage into `canvas` over whatever is already there.
    pub fn draw(&self, canvas: &mut Canvas, x: i32, baseline_y: i32, px: f32, ch: char, color: u32) {
        let (m, bitmap) = self.glyph(ch, px);
        let gx = x + m.xmin;
        let gy = baseline_y - m.height as i32 - m.ymin;

        for row in 0..m.height as i32 {
            for col in 0..m.width as i32 {
                let a = bitmap[(row as usize) * m.width + col as usize];
                if a == 0 {
                    continue;
                }
                let fx = gx + col;
                let fy = gy + row;
                if fx < 0 || fx >= canvas.width || fy < 0 || fy >= canvas.height {
                    continue;
                }
                let idx = (fy * canvas.width + fx) as usize;
                canvas.pixels[idx] = blend(canvas.pixels[idx], color, a);
            }
        }
    }
}

fn blend(bg: u32, fg: u32, a: u8) -> u32 {
    if a == 255 {
        return fg;
    }
    let a = a as u32;
    let mix = |shift: u32| {
        let bg_c = (bg >> shift) & 0xff;
        let fg_c = (fg >> shift) & 0xff;
        ((fg_c * a + bg_c * (255 - a)) / 255) << shift
    };
    0xff000000 | mix(16) | mix(8) | mix(0)
}
