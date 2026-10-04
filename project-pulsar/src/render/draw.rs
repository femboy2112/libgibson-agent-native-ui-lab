//! Small drawing toolkit shared by every view: styled text, blits, glyph ramps,
//! and the one function that turns a `gibson::plot` spec into pixels *and* a probe
//! that tests (and the inspect cursor) can read back.

use gibson::plot::{self, PlotError, PlotLayout, PlotReport, PlotSpec, PlotView};
use gibson::{Cell, Color, Rect, Style, SubcellGlyphMode, Surface};

pub type Rgb = (u8, u8, u8);

pub const INK: Rgb = (214, 224, 238);
pub const MUTED: Rgb = (128, 140, 160);
pub const FAINT: Rgb = (74, 84, 102);
pub const NOISE: Rgb = (118, 128, 148);
pub const WARN: Rgb = (255, 138, 72);
pub const GOOD: Rgb = (120, 232, 150);

pub fn style(rgb: Rgb) -> Style {
    Style::new().fg(Color::rgb(rgb.0, rgb.1, rgb.2))
}

pub fn style_b(rgb: Rgb, bold: bool, dim: bool) -> Style {
    let mut s = style(rgb);
    if bold {
        s = s.bold();
    }
    if dim {
        s = s.dim();
    }
    s
}

pub fn scale(rgb: Rgb, k: f64) -> Rgb {
    let f = |v: u8| ((v as f64 * k).round().clamp(0.0, 255.0)) as u8;
    (f(rgb.0), f(rgb.1), f(rgb.2))
}

pub fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let f = |x: u8, y: u8| (x as f64 * (1.0 - t) + y as f64 * t).round().clamp(0.0, 255.0) as u8;
    (f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
}

/// Print `text` at `(x, y)` (clipped; negative origins are skipped).
pub fn put(s: &mut Surface, x: i32, y: i32, text: &str, st: Style) {
    if x < 0 || y < 0 || x >= s.width as i32 || y >= s.height as i32 {
        return;
    }
    s.print_str(x as u16, y as u16, text, st, None);
}

pub fn put_max(s: &mut Surface, x: i32, y: i32, text: &str, st: Style, max: u16) {
    if x < 0 || y < 0 || x >= s.width as i32 || y >= s.height as i32 {
        return;
    }
    s.print_str(x as u16, y as u16, text, st, Some(max));
}

/// Right-align `text` so it ends at column `x_end` (exclusive).
pub fn put_right(s: &mut Surface, x_end: i32, y: i32, text: &str, st: Style) {
    let w = text.chars().count() as i32;
    put(s, x_end - w, y, text, st);
}

pub fn put_center(s: &mut Surface, x0: i32, x1: i32, y: i32, text: &str, st: Style) {
    let w = text.chars().count() as i32;
    put(s, x0 + (x1 - x0 - w) / 2, y, text, st);
}

/// Copy `src` into `dst` at `(ox, oy)`.
pub fn blit(dst: &mut Surface, src: &Surface, ox: u16, oy: u16) {
    for y in 0..src.height {
        for x in 0..src.width {
            if let Some(c) = src.get(x, y) {
                if c.is_continuation {
                    continue;
                }
                let cell: Cell = c.clone();
                dst.set_cell(ox + x, oy + y, cell);
            }
        }
    }
}

pub fn truncate(text: &str, w: usize) -> String {
    text.chars().take(w).collect()
}

/// Shade ramp (luminance carried by shape; colour only enhances).
pub fn ramp_char(level: f64, mode: SubcellGlyphMode) -> char {
    let l = level.clamp(0.0, 1.0);
    match mode {
        SubcellGlyphMode::Ascii => {
            let r = [' ', '.', ':', '-', '=', '+', '*', '#', '@'];
            r[((l * (r.len() - 1) as f64).round() as usize).min(r.len() - 1)]
        }
        _ => {
            let r = [' ', '·', '░', '▒', '▓', '█'];
            r[((l * (r.len() - 1) as f64).round() as usize).min(r.len() - 1)]
        }
    }
}

/// A horizontal meter `▮▮▮▯▯`-style bar of `w` cells for `frac ∈ [0,1]`.
pub fn meter(frac: f64, w: usize, mode: SubcellGlyphMode) -> String {
    let full = (frac.clamp(0.0, 1.0) * w as f64).round() as usize;
    let (on, off) = match mode {
        SubcellGlyphMode::Ascii => ('#', '-'),
        _ => ('█', '░'),
    };
    (0..w).map(|i| if i < full { on } else { off }).collect()
}

/// Everything a test or the inspect cursor needs to know about one drawn plot.
#[derive(Clone, Debug)]
pub struct PlotProbe {
    pub name: String,
    pub spec: PlotSpec,
    pub view: PlotView,
    /// Where the plot was placed, in the coordinates of the surface it was drawn into.
    pub origin: (u16, u16),
    pub layout: PlotLayout,
    pub report: PlotReport,
}

impl PlotProbe {
    /// Is `cell` (surface coordinates) inside the plot rectangle?
    pub fn contains(&self, cell: (u16, u16)) -> bool {
        let r = self.layout.plot_rect;
        let (x, y) = (cell.0 as i32 - self.origin.0 as i32, cell.1 as i32 - self.origin.1 as i32);
        x >= r.x as i32
            && x < (r.x + r.width) as i32
            && y >= r.y as i32
            && y < (r.y + r.height) as i32
    }

    /// Data coordinates of the centre of `cell`, through the *same* transform the
    /// renderer used (`PlotTransform2D::unproject`).
    pub fn data_at(&self, cell: (u16, u16)) -> Option<(f64, f64)> {
        if !self.contains(cell) {
            return None;
        }
        let t = self.layout.transform.as_ref()?;
        let r = self.layout.plot_rect;
        let lx = cell.0 as i32 - self.origin.0 as i32 - r.x as i32;
        let ly = cell.1 as i32 - self.origin.1 as i32 - r.y as i32;
        let px = lx as f64 * 2.0 + 0.5;
        let py = ly as f64 * 4.0 + 1.5;
        Some(t.unproject(px, py))
    }

    /// Cell (surface coordinates) a data point lands in, if it is drawn.
    pub fn cell_of(&self, x: f64, y: f64) -> Option<(u16, u16)> {
        let t = self.layout.transform.as_ref()?;
        let (px, py) = t.project(x, y)?;
        let r = self.layout.plot_rect;
        let (ix, iy) = (px.round() as i32, py.round() as i32);
        if ix < 0 || iy < 0 || ix >= self.layout.px_w as i32 || iy >= self.layout.px_h as i32 {
            return None;
        }
        Some((
            self.origin.0 + r.x + (ix / 2) as u16,
            self.origin.1 + r.y + (iy / 4) as u16,
        ))
    }
}

/// A valid `FiniteRange`, widening a degenerate request (equal ends, reversed ends,
/// non-finite) rather than failing: a flat-lined or empty observable must still
/// plot, honestly, on a sane axis.
pub fn range(lo: f64, hi: f64) -> plot::FiniteRange {
    let (mut a, mut b) = (lo, hi);
    if !a.is_finite() || !b.is_finite() {
        a = 0.0;
        b = 1.0;
    }
    if a > b {
        std::mem::swap(&mut a, &mut b);
    }
    if b - a < 1e-12 * a.abs().max(1.0) {
        let pad = 0.5 * a.abs().max(1.0) * 1e-3 + 0.5;
        a -= pad;
        b += pad;
    }
    plot::FiniteRange::new(a, b).unwrap_or_else(|| plot::FiniteRange::new(0.0, 1.0).unwrap())
}

/// Positive range for a Log10 axis.
pub fn log_range(lo: f64, hi: f64) -> plot::FiniteRange {
    let lo = if lo.is_finite() && lo > 0.0 { lo } else { 1e-3 };
    let hi = if hi.is_finite() && hi > lo * 1.0001 { hi } else { lo * 10.0 };
    plot::FiniteRange::new(lo, hi).unwrap_or_else(|| plot::FiniteRange::new(1e-3, 1.0).unwrap())
}

/// Compile + realize a plot into `surf` at `area`; returns the probe. A
/// *configuration* error (which cannot happen through the range helpers above, but
/// is handled rather than assumed) is drawn as an explicit message — never as an
/// empty frame that pretends nothing was wrong.
pub fn draw_plot(
    surf: &mut Surface,
    area: Rect,
    name: &str,
    spec: PlotSpec,
    view: PlotView,
    mode: SubcellGlyphMode,
) -> Option<PlotProbe> {
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let local = Rect::new(0, 0, area.width, area.height);
    match plot::compile(&spec, &view, local) {
        Ok((layout, report)) => {
            let s = plot::render(&layout, mode);
            blit(surf, &s, area.x, area.y);
            Some(PlotProbe {
                name: name.to_string(),
                spec,
                view,
                origin: (area.x, area.y),
                layout,
                report,
            })
        }
        Err(e) => {
            let msg = match e {
                PlotError::InvalidXAxisDomain => "invalid x-axis domain",
                PlotError::InvalidYAxisDomain => "invalid y-axis domain",
            };
            put(
                surf,
                area.x as i32,
                area.y as i32 + area.height as i32 / 2,
                &truncate(&format!("[{name}: {msg}]"), area.width as usize),
                style(WARN),
            );
            None
        }
    }
}
