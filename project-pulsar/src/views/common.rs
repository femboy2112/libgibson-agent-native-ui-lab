//! Helpers shared by the `gibson::plot`-based representations.

use gibson::plot::{
    self, AxisScale, AxisSpec, FiniteRange, PlotLayout, PlotReport, PlotSpec, PlotTheme, PlotView,
};
use gibson::{Rect, SubcellGlyphMode, Surface};

use crate::theme::{DIM, INK, SOFT};

pub struct PlotOut {
    pub surface: Surface,
    pub layout: PlotLayout,
    pub report: PlotReport,
}

pub fn theme() -> PlotTheme {
    PlotTheme {
        axis: DIM,
        label: SOFT,
        title: INK,
    }
}

/// Compile + render a plot into `w x h` cells (local origin).
pub fn draw_plot(
    spec: &PlotSpec,
    view: &PlotView,
    w: u16,
    h: u16,
    mode: SubcellGlyphMode,
) -> PlotOut {
    let (layout, report) = plot::compile(spec, view, Rect::new(0, 0, w.max(1), h.max(1)));
    let surface = plot::render_themed(&layout, mode, &theme());
    PlotOut {
        surface,
        layout,
        report,
    }
}

pub fn range(a: f64, b: f64) -> FiniteRange {
    FiniteRange::new(a, b)
        .or_else(|| FiniteRange::new(a - 0.5, a + 0.5))
        .unwrap_or_else(|| FiniteRange::new(0.0, 1.0).expect("unit range"))
}

/// "label (unit)" caption for an axis, derived from the same `AxisSpec` the plot used.
pub fn caption(ax: &AxisSpec) -> String {
    match &ax.unit {
        Some(u) if !u.is_empty() => format!("{} ({})", ax.label, u),
        _ => ax.label.clone(),
    }
}

/// Data -> hero-local cell (column, row) through the SAME transform the renderer used.
pub fn cell_of(layout: &PlotLayout, x: f64, y: f64) -> Option<(i32, i32)> {
    let t = layout.transform?;
    let (px, py) = t.project(x, y)?;
    let cx = layout.plot_rect.x as i32 + (px / 2.0).floor() as i32;
    let cy = layout.plot_rect.y as i32 + (py / 4.0).floor() as i32;
    Some((cx, cy))
}

/// Normalised plot-area position `(u right, v up)` -> data, via `unproject`.
pub fn data_at(layout: &PlotLayout, u: f64, v: f64) -> Option<(f64, f64)> {
    let t = layout.transform?;
    let px = u.clamp(0.0, 1.0) * layout.px_w as f64;
    let py = (1.0 - v.clamp(0.0, 1.0)) * layout.px_h as f64;
    Some(t.unproject(px, py))
}

/// Axis transform helper for the x axis of a view (used to place cursor lines
/// before the plot is compiled).
pub fn unproject_axis(scale: AxisScale, r: FiniteRange, u: f64) -> f64 {
    plot::AxisTransform::new(scale, r)
        .map(|t| t.unproject(u.clamp(0.0, 1.0)))
        .unwrap_or(r.min())
}

pub fn fmt_hz(f: f64) -> String {
    format!("{f:.4} Hz")
}

/// Row budget of a representation inside the hero rectangle.
#[derive(Clone, Copy, Debug)]
pub struct Rows {
    pub title: Option<u16>,
    pub plot_y: u16,
    pub plot_h: u16,
    pub xcap: Option<u16>,
    pub read: u16,
}

pub fn rows(h: u16) -> Rows {
    if h >= 13 {
        Rows {
            title: Some(0),
            plot_y: 1,
            plot_h: h - 3,
            xcap: Some(h - 2),
            read: h - 1,
        }
    } else if h >= 6 {
        Rows {
            title: None,
            plot_y: 0,
            plot_h: h - 2,
            xcap: Some(h - 2),
            read: h - 1,
        }
    } else {
        Rows {
            title: None,
            plot_y: 0,
            plot_h: h.saturating_sub(1),
            xcap: None,
            read: h.saturating_sub(1),
        }
    }
}

use crate::theme::{put, put_clipped, st, FAINT, WARN};

/// Everything the in-world captions of a representation say.
pub struct Captions<'a> {
    pub y: &'a str,
    pub x: &'a str,
    pub describe: &'a str,
    /// A "then versus now" comparison, shown instead of `describe` while comparing.
    pub note: Option<&'a str>,
    pub readout: &'a str,
    pub plot_x: u16,
    pub plot_w: u16,
}

/// Draw the in-world captions: y caption + description on the title row, x
/// caption centred under the plot, readout on the last row.
pub fn draw_chrome(s: &mut Surface, r: &Rows, c: &Captions) {
    let w = s.width as usize;
    if let Some(t) = r.title {
        let y = format!("\u{25B4} {}", c.y);
        put_clipped(s, 0, t as i32, &y, st(SOFT), w);
        let used = y.chars().count() + 2;
        let (text, style) = match c.note {
            Some(n) => (n, st(WARN)),
            None => (c.describe, st(DIM)),
        };
        if w > used + text.chars().count() + 2 {
            let x = w - text.chars().count();
            put(s, x as i32, t as i32, text, style);
        }
    }
    if let Some(x) = r.xcap {
        let text = if r.title.is_some() {
            c.x.to_string()
        } else {
            format!("{} \u{00B7} {}", c.x, c.y)
        };
        let n = text.chars().count() as i32;
        let centre = c.plot_x as i32 + c.plot_w as i32 / 2;
        let x0 = (centre - n / 2).max(0);
        put_clipped(s, x0, x as i32, &text, st(SOFT), w);
    }
    put_clipped(s, 0, r.read as i32, c.readout, st(INK), w);
    let _ = FAINT;
}
