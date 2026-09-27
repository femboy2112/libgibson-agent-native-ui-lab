//! Shared surface-painting primitives for the atlas grammar.

use gibson::cell::{Cell, Color, Glyph, Style};
use gibson::surface::Surface;

use crate::theme::{glyphs, pad_left, pad_to, truncate};

/// Print a string only into cells that are currently spaces (labels never
/// overwrite nodes of other lanes).
pub fn print_if_empty(
    s: &mut Surface,
    x: u16,
    y: u16,
    text: &str,
    style: Style,
    max_w: Option<u16>,
) {
    if y >= s.height {
        return;
    }
    let mut cx = x;
    let mut budget = max_w.unwrap_or(s.width.saturating_sub(x));
    for ch in text.chars() {
        if cx >= s.width || budget == 0 {
            break;
        }
        if ch.is_control() {
            continue;
        }
        let w = unicode_width_of(ch);
        if w == 0 {
            continue;
        }
        if cx + w as u16 > s.width {
            break;
        }
        let empty = s
            .get(cx, y)
            .map(|c| c.glyph.grapheme.as_str() == " " || c.glyph.is_empty())
            .unwrap_or(true);
        if empty {
            let mut tmp = [0u8; 4];
            s.set_cell(
                cx,
                y,
                Cell::new(Glyph::new(ch.encode_utf8(&mut tmp)), style),
            );
        }
        cx += w as u16;
        budget = budget.saturating_sub(w as u16);
    }
}

fn unicode_width_of(c: char) -> usize {
    use unicode_width::UnicodeWidthChar;
    c.width().unwrap_or(1)
}

/// Horizontal line from x0 to x1 (inclusive) on row y, painting only empty cells.
pub fn hline_if_empty(s: &mut Surface, x0: i32, x1: i32, y: u16, ch: &str, style: Style) {
    if y >= s.height {
        return;
    }
    let (a, b) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
    let mut x = a.max(0);
    while x <= b && (x as u16) < s.width {
        let empty = s
            .get(x as u16, y)
            .map(|c| c.glyph.grapheme.as_str() == " " || c.glyph.is_empty())
            .unwrap_or(false);
        if empty {
            s.set_cell(x as u16, y, Cell::new(Glyph::new(ch), style));
        }
        x += 1;
    }
}

/// Vertical line from y0 to y1 (inclusive) on column x, painting only empty cells.
pub fn vline_if_empty(s: &mut Surface, x: u16, y0: i32, y1: i32, ch: &str, style: Style) {
    if x >= s.width {
        return;
    }
    let (a, b) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
    let mut y = a.max(0);
    while y <= b && (y as u16) < s.height {
        let empty = s
            .get(x, y as u16)
            .map(|c| c.glyph.grapheme.as_str() == " " || c.glyph.is_empty())
            .unwrap_or(false);
        if empty {
            s.set_cell(x, y as u16, Cell::new(Glyph::new(ch), style));
        }
        y += 1;
    }
}

/// Force-set a cell (nodes overwrite edges).
pub fn put(s: &mut Surface, x: i32, y: i32, ch: &str, style: Style) {
    if x < 0 || y < 0 {
        return;
    }
    if (x as u16) < s.width && (y as u16) < s.height {
        s.set_cell(x as u16, y as u16, Cell::new(Glyph::new(ch), style));
    }
}

/// Horizontal bar of `width` cells showing `frac` in [0,1].
pub fn bar(
    s: &mut Surface,
    x: u16,
    y: u16,
    width: u16,
    frac: f32,
    style: Style,
    empty_style: Style,
) {
    if y >= s.height {
        return;
    }
    let w = width.min(s.width.saturating_sub(x));
    let filled = ((frac.clamp(0.0, 1.0)) * w as f32).round() as u16;
    for i in 0..w {
        let ch = glyphs::BAR_BLOCKS[if i < filled { 7 } else { 0 }];
        let st = if i < filled { style } else { empty_style };
        if x + i < s.width {
            s.set_cell(x + i, y, Cell::new(Glyph::new(ch), st));
        }
    }
}

/// A two-column editorial row: `label` left, `value` right-aligned within `w`.
#[allow(clippy::too_many_arguments)]
pub fn kv_row(
    s: &mut Surface,
    y: u16,
    x: u16,
    w: u16,
    label: &str,
    value: &str,
    label_style: Style,
    value_style: Style,
) {
    if y >= s.height || w < 8 {
        return;
    }
    let lab = truncate(label, (w as usize).saturating_sub(1));
    let lw = crate::theme::width_of(&lab) as u16;
    s.print_str(x, y, &lab, label_style, Some(lw));
    let vw = w.saturating_sub(lw.saturating_add(1));
    let val = pad_left(&truncate(value, vw as usize), vw as usize);
    let vx = x + lw.saturating_add(1);
    s.print_str(vx, y, &val, value_style, Some(vw));
}

/// Sparkline row from `values` (0..1 normalized externally), `w` cells wide.
pub fn sparkline(s: &mut Surface, x: u16, y: u16, w: u16, values: &[u32], max: u32, style: Style) {
    if y >= s.height || values.is_empty() {
        return;
    }
    let w = w.min(s.width.saturating_sub(x)) as usize;
    let step = (values.len() as f32 / w as f32).max(1.0) as usize;
    for i in 0..w {
        let lo = i * step;
        let hi = ((i + 1) * step).min(values.len());
        if lo >= values.len() {
            break;
        }
        let peak = values[lo..hi].iter().copied().max().unwrap_or(0);
        let level = if max == 0 {
            0
        } else {
            ((peak as f32 / max as f32) * 7.0).round() as usize
        };
        s.set_cell(
            x + i as u16,
            y,
            Cell::new(Glyph::new(glyphs::DENSITY[level.min(7)]), style),
        );
    }
}

/// Reverse-video status strip.
pub fn strip(s: &mut Surface, y: u16, text: &str, style: Style) {
    if y >= s.height {
        return;
    }
    for x in 0..s.width {
        s.set_cell(x, y, Cell::new(Glyph::new(" "), style));
    }
    let t = truncate(text, s.width as usize);
    s.print_str(0, y, &t, style, Some(s.width));
}

/// Draw a scrollbar thumb on the right edge for `content` rows in `viewport`.
pub fn scrollbar(s: &mut Surface, x: u16, y0: u16, h: u16, scroll: usize, content: usize) {
    if x >= s.width || h == 0 || content <= h as usize {
        return;
    }
    let thumb_h = ((h as f32 / content as f32) * h as f32).round().max(1.0) as u16;
    let max_scroll = content.saturating_sub(h as usize);
    let pos: u16 = if max_scroll == 0 {
        0
    } else {
        ((scroll as f32 / max_scroll as f32) * (h - thumb_h) as f32) as u16
    };
    for dy in 0..h {
        let ch = if dy >= pos && dy < pos + thumb_h {
            "▓"
        } else {
            "░"
        };
        let st = Style::new().fg(Color::Ansi256(238));
        if y0 + dy < s.height {
            s.set_cell(x, y0 + dy, Cell::new(Glyph::new(ch), st));
        }
    }
}

/// Print a padded, truncated list row.
pub fn list_row(s: &mut Surface, y: u16, x: u16, w: u16, text: &str, style: Style) {
    if y >= s.height {
        return;
    }
    let t = pad_to(&truncate(text, w as usize), w as usize);
    s.print_str(x, y, &t, style, Some(w));
}
