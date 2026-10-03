//! Chrome colours and tiny text helpers shared by every representation.

use gibson::{Cell, Color, Glyph, Style, Surface};

pub type Rgb = (u8, u8, u8);

pub const INK: Rgb = (218, 228, 244);
pub const SOFT: Rgb = (160, 176, 202);
pub const DIM: Rgb = (104, 118, 144);
pub const FAINT: Rgb = (62, 72, 92);
pub const ACCENT: Rgb = (122, 204, 255);
pub const WARN: Rgb = (255, 176, 96);
pub const GOOD: Rgb = (120, 232, 150);
pub const TRACE: Rgb = (138, 188, 232);

pub fn st(rgb: Rgb) -> Style {
    Style::new().fg(Color::rgb(rgb.0, rgb.1, rgb.2))
}

pub fn st_bold(rgb: Rgb) -> Style {
    let mut s = st(rgb);
    s.bold = true;
    s
}

pub fn scale(rgb: Rgb, k: f32) -> Rgb {
    let f = |c: u8| (c as f32 * k).clamp(0.0, 255.0) as u8;
    (f(rgb.0), f(rgb.1), f(rgb.2))
}

pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    (f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
}

/// Print `text` at signed coordinates, clipped to the surface.
pub fn put(s: &mut Surface, x: i32, y: i32, text: &str, style: Style) {
    if y < 0 || x >= s.width as i32 || y >= s.height as i32 {
        return;
    }
    if x >= 0 {
        s.print_str(x as u16, y as u16, text, style, None);
        return;
    }
    // clip the left side by skipping leading characters
    let skip = (-x) as usize;
    let rest: String = text.chars().skip(skip).collect();
    if !rest.is_empty() {
        s.print_str(0, y as u16, &rest, style, None);
    }
}

/// Print at most `max` columns of `text`.
pub fn put_clipped(s: &mut Surface, x: i32, y: i32, text: &str, style: Style, max: usize) {
    let t: String = text.chars().take(max).collect();
    put(s, x, y, &t, style);
}

/// Copy every non-blank cell of `src` onto `dst` at offset `(dx, dy)`.
pub fn blit_ink(dst: &mut Surface, src: &Surface, dx: u16, dy: u16) {
    for y in 0..src.height {
        for x in 0..src.width {
            if let Some(c) = src.get(x, y) {
                if c.is_continuation {
                    continue;
                }
                let blank = c.glyph.is_empty() || c.glyph.grapheme.as_str() == " ";
                if !blank {
                    dst.set_cell(x + dx, y + dy, c.clone());
                }
            }
        }
    }
}

/// Copy every cell (blank or not) of `src` onto `dst` at offset `(dx, dy)`.
pub fn blit_all(dst: &mut Surface, src: &Surface, dx: u16, dy: u16) {
    for y in 0..src.height {
        for x in 0..src.width {
            if let Some(c) = src.get(x, y) {
                if c.is_continuation {
                    continue;
                }
                dst.set_cell(x + dx, y + dy, c.clone());
            }
        }
    }
}

pub fn set_char(s: &mut Surface, x: i32, y: i32, ch: char, style: Style) {
    if x < 0 || y < 0 || x >= s.width as i32 || y >= s.height as i32 {
        return;
    }
    s.set_cell(x as u16, y as u16, Cell::new(Glyph::from_char(ch), style));
}

/// Text of one surface row, used by tests and the capture path.
pub fn row_text(s: &Surface, y: u16) -> String {
    let mut line = String::new();
    for x in 0..s.width {
        if let Some(c) = s.get(x, y) {
            if c.is_continuation {
                continue;
            }
            if c.glyph.is_empty() {
                line.push(' ');
            } else {
                line.push_str(c.glyph.grapheme.as_str());
            }
        }
    }
    line
}

/// Half-block realisation of an RGB raster with honest blanks: a dark pixel
/// pair becomes a space (so captures stay legible and the terminal background
/// shows through), one lit pixel becomes `▀` / `▄`, two become `█` or `▀` with
/// a coloured background.
pub fn raster_to_surface(px: &gibson::raster::RgbRaster, dark: u32) -> Surface {
    let w = px.width();
    let h = px.height();
    let rows = h.div_ceil(2);
    let mut s = Surface::new(w, rows);
    let lum = |c: Rgb| 54 * c.0 as u32 + 183 * c.1 as u32 + 19 * c.2 as u32;
    for y in 0..rows {
        for x in 0..w {
            let top = px.get(x as i32, y as i32 * 2).unwrap_or_default();
            let bot = px.get(x as i32, y as i32 * 2 + 1).unwrap_or_default();
            let (lt, lb) = (lum(top) / 256, lum(bot) / 256);
            let (t_on, b_on) = (lt > dark, lb > dark);
            let cell = match (t_on, b_on) {
                (false, false) => continue,
                (true, false) => Cell::new(Glyph::new("▀"), st(top)),
                (false, true) => Cell::new(Glyph::new("▄"), st(bot)),
                (true, true) => {
                    let mut style = st(top);
                    style.bg = Some(Color::rgb(bot.0, bot.1, bot.2));
                    Cell::new(Glyph::new("▀"), style)
                }
            };
            s.set_cell(x, y, cell);
        }
    }
    s
}

/// Luminance-dithered Braille realisation (Mono-safe): density carries the value.
pub fn raster_to_mono(px: &gibson::raster::RgbRaster) -> Surface {
    let mut s = px.to_mono_surface();
    // `to_mono_surface` writes U+2800 (blank Braille) for empty cells; make them true spaces.
    for y in 0..s.height {
        for x in 0..s.width {
            if let Some(c) = s.get_mut(x, y) {
                if c.glyph.grapheme.as_str() == "\u{2800}" {
                    c.glyph = Glyph::space();
                }
            }
        }
    }
    s
}
