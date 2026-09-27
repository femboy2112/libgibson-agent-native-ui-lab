//! FILE STRATA — a file's world-line across time.
//!
//! Shares the atlas camera and ruler, so the transition atlas → strata feels
//! like focusing the same instrument on one trajectory rather than switching
//! screens. The file's existence band shows creation, renames, hot periods,
//! and every touch event; below it, the chronological event ledger.

use std::sync::Arc;

use gibson::surface::Surface;

use crate::app::App;
use crate::theme::{fmt_age, fmt_date, glyphs, truncate, Palette};
use crate::views::widgets::*;

pub fn draw_strata(app: &mut App, w: u16, h: u16, pal: &Palette) -> Arc<Surface> {
    let mut s = Surface::new(w, h);
    let cam = app.camera;

    // Ruler identical to the atlas (visual continuity).
    super::atlas::draw_ruler_pub(&mut s, cam, w, pal);

    let path = app.strata.path.clone();
    let events = app.strata.events.clone();
    if events.is_empty() {
        let msg = if path.is_empty() {
            "no file selected — press f to browse, or s in the diff lens"
        } else {
            "no events recovered for this path within the scan bound"
        };
        let x = ((w as usize).saturating_sub(msg.len())) / 2;
        s.print_str(x as u16, h / 2, msg, pal.s_faint(), Some(w));
        return Arc::new(s);
    }

    let t_min = events.iter().map(|e| e.time).min().unwrap_or(0);
    let t_max = events.iter().map(|e| e.time).max().unwrap_or(1);
    let span = (t_max - t_min).max(1);
    let x_of =
        |t: i64| -> u16 { (((t - t_min) as f32 / span as f32) * (w as f32 - 2.0) + 1.0) as u16 };

    let identity_y = 1u16;
    if identity_y < h {
        let identity = format!(
            " {} ─ {} events, {} to {}",
            truncate(&path, w.saturating_sub(40) as usize),
            events.len(),
            fmt_date(t_min),
            fmt_date(t_max)
        );
        s.print_str(
            0,
            identity_y,
            &truncate(&identity, w as usize),
            pal.s_accent(),
            Some(w),
        );
    }

    // ---- existence band (rows 2-3) --------------------------------------------
    let band_y = 2;
    if band_y + 1 < h {
        let x0 = x_of(t_min);
        let x1 = x_of(t_max);
        for x in x0..=x1.min(w.saturating_sub(1)) {
            s.set_cell(x, band_y, cell(glyphs::STRAND, pal.s_accent()));
            s.set_cell(x, band_y + 1, cell(glyphs::RAIL_FADED, pal.s_border()));
        }
        // events as ticks; magnitude colors the tick height
        let max_churn = events
            .iter()
            .map(|e| e.adds + e.dels)
            .max()
            .unwrap_or(1)
            .max(1);
        for e in &events {
            let x = x_of(e.time);
            if x >= w {
                continue;
            }
            let heavy = (e.adds + e.dels) as f32 / max_churn as f32;
            let style = if heavy > 0.66 {
                pal.s_warning()
            } else if heavy > 0.33 {
                pal.s_tag()
            } else {
                pal.s_muted()
            };
            s.set_cell(x, band_y, cell(glyphs::STRAND_EVENT, style));
            if heavy > 0.85 && x + 1 < w {
                put(
                    &mut s,
                    x as i32 + 1,
                    band_y as i32,
                    glyphs::PLUS,
                    pal.s_added(),
                );
            }
            if e.status == 'R' {
                put(
                    &mut s,
                    x as i32,
                    band_y as i32,
                    glyphs::STRAND_RENAME,
                    pal.s_merge(),
                );
            }
        }
        // first/last touch labels
        print_if_empty(
            &mut s,
            0,
            band_y,
            &truncate(&fmt_age(t_min, app.now), 12),
            pal.s_faint(),
            Some(12),
        );
        let last_label = fmt_age(t_max, app.now);
        let lx = w.saturating_sub(14);
        print_if_empty(&mut s, lx, band_y, &last_label, pal.s_faint(), Some(14));
    }

    // ---- author strip (row 5) ----------------------------------------------------
    let author_y = band_y + 3;
    if author_y < h && w >= 70 {
        let mut counts: std::collections::BTreeMap<&str, u32> = Default::default();
        for e in &events {
            *counts.entry(e.author.as_str()).or_insert(0) += 1;
        }
        let total = events.len() as u32;
        let mut x = 0u16;
        let mut legend = String::new();
        for (ai, (author, n)) in counts.iter().enumerate() {
            let width = ((*n as f32 / total as f32) * w as f32).round() as u16;
            let style = pal.s_lane(ai);
            for dx in 0..width {
                if x + dx < w {
                    s.set_cell(x + dx, author_y, cell(glyphs::BAR_BLOCKS[6], style));
                }
            }
            x += width;
            if x >= w {
                break;
            }
            legend.push_str(&format!("{}:{}, ", truncate(author, 10), n));
        }
        print_if_empty(
            &mut s,
            0,
            author_y + 1,
            &truncate(
                &format!("authors — {}", legend.trim_end_matches(", ")),
                w as usize,
            ),
            pal.s_faint(),
            Some(w),
        );
    }

    // ---- event ledger ----------------------------------------------------------
    let list_top = (author_y + 3).min(h.saturating_sub(1));
    let list_h = h - list_top;
    let cursor = app.strata.cursor;
    let scroll = app.strata.scroll;

    let header = format!(" {} events · newest first · renames followed", events.len());
    if list_top < h {
        s.print_str(
            0,
            list_top,
            &truncate(&header, w as usize),
            pal.s_muted(),
            Some(w),
        );
    }

    for vi in 0..list_h.saturating_sub(1) {
        let idx = scroll + vi as usize;
        let Some(e) = events.get(idx) else { break };
        let y = list_top + 1 + vi;
        if y >= h {
            break;
        }
        let is_cur = idx == cursor;
        let base = if is_cur {
            pal.s_bar_accent()
        } else {
            pal.s_text()
        };
        let status_mark = match e.status {
            'A' => "＋",
            'D' => "−",
            'R' => "↗",
            _ => "·",
        };
        let summary_w = (w as usize).saturating_sub(58);
        let row = format!(
            " {} {} {:<9} {:<11} {} +{}/−{}",
            status_mark,
            fmt_date(e.time),
            e.oid.get(..7).unwrap_or("0000000"),
            truncate(&e.author, 11),
            truncate(&e.summary, summary_w),
            e.adds,
            e.dels,
        );
        list_row(&mut s, y, 0, w, &row, base);
    }
    scrollbar(
        &mut s,
        w.saturating_sub(1),
        list_top + 1,
        list_h.saturating_sub(1),
        scroll,
        events.len(),
    );
    if let Some(e) = events.get(cursor) {
        let note = if let Some(prev) = &e.prev_path {
            format!("renamed from {}", prev)
        } else {
            format!("{} ago", fmt_age(e.time, app.now))
        };
        let y = list_top;
        s.print_str(
            w.saturating_sub(note.len() as u16 + 2),
            y,
            &note,
            pal.s_faint(),
            Some(w),
        );
    }

    Arc::new(s)
}

fn cell(ch: &str, style: gibson::cell::Style) -> gibson::cell::Cell {
    gibson::cell::Cell::new(gibson::cell::Glyph::new(ch), style)
}
