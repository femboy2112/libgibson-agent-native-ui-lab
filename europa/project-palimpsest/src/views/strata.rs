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

    let path = &app.strata.path;
    let events = &app.strata.events;
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
    // The file occupies the same time coordinates as the atlas and ruler.
    // Fitting it independently to the width made a commit appear under an
    // incorrect date on row 0, breaking the atlas -> strata transition.
    let x_of = |t: i64| -> i32 { cam.x_of(t, w) };

    let identity_y = 1u16;
    if identity_y < h {
        let identity = format!(
            " {} ─ {} events, {} to {}",
            truncate(path, w.saturating_sub(40) as usize),
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
        for x in x0.max(0)..=x1.min(w as i32 - 1) {
            s.set_cell(x as u16, band_y, cell(glyphs::STRAND, pal.s_accent()));
            s.set_cell(
                x as u16,
                band_y + 1,
                cell(glyphs::RAIL_FADED, pal.s_border()),
            );
        }
        // events as ticks; magnitude colors the tick height
        let max_churn = events
            .iter()
            .map(|e| e.adds + e.dels)
            .max()
            .unwrap_or(1)
            .max(1);
        for e in events {
            let x = x_of(e.time);
            if x < 0 || x >= w as i32 {
                continue;
            }
            let x = x as u16;
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
        // The atlas selection remains a pin; the ledger cursor picks a touch
        // event on that same axis. Both anchors survive without color.
        if let Some(selected) = app.sel_row() {
            let x = x_of(selected.time);
            if (0..w as i32).contains(&x) {
                s.set_cell(x as u16, band_y + 1, cell(glyphs::PIN, pal.s_selection()));
            }
        }
        if let Some(e) = events.get(app.strata.cursor) {
            let x = x_of(e.time);
            if (0..w as i32).contains(&x) {
                s.set_cell(
                    x as u16,
                    band_y,
                    cell(glyphs::NODE_SELECTED, pal.s_selection()),
                );
            }
        }
        // Age labels live below the band. Painting them on the event rail
        // could leave partial words such as `1mo` pierced by a touch mark.
        let age_line = format!(
            " first touch {} ago · latest touch {} ago",
            fmt_age(t_min, app.now),
            fmt_age(t_max, app.now)
        );
        s.print_str(
            0,
            band_y + 2,
            &truncate(&age_line, w as usize),
            pal.s_faint(),
            Some(w),
        );
    }

    // ---- author strip (row 5) ----------------------------------------------------
    let author_y = band_y + 3;
    if author_y < h && w >= 70 {
        let mut counts: std::collections::BTreeMap<&str, u32> = Default::default();
        for e in events {
            *counts.entry(e.author.as_str()).or_insert(0) += 1;
        }
        let total = events.len() as u32;
        let mut x = 0u16;
        let mut legend = String::new();
        let marks = [
            glyphs::RAIL_ACTIVE,
            glyphs::STRAND_DORMANT,
            glyphs::RAIL,
            glyphs::RAIL_FADED,
        ];
        for (ai, (author, n)) in counts.iter().enumerate() {
            let width = ((*n as f32 / total as f32) * w as f32).round() as u16;
            let style = pal.s_lane(ai);
            for dx in 0..width {
                if x + dx < w {
                    s.set_cell(x + dx, author_y, cell(marks[ai % marks.len()], style));
                }
            }
            if x + width < w {
                s.set_cell(x + width, author_y, cell(glyphs::VERT, pal.s_border()));
            }
            legend.push_str(&format!(
                "{} {}:{}, ",
                marks[ai % marks.len()],
                truncate(author, 10),
                n
            ));
            x += width;
            if x >= w {
                break;
            }
        }
        print_if_empty(
            &mut s,
            0,
            author_y + 1,
            &truncate(
                &format!("contributors · {}", legend.trim_end_matches(", ")),
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
        let short = e.oid.get(..7).unwrap_or("0000000");
        let row = if w >= 100 {
            format!(
                " {} {} {:<7} {:<11} {} +{}/−{}",
                status_mark,
                fmt_date(e.time),
                short,
                truncate(&e.author, 11),
                truncate(&e.summary, w.saturating_sub(58) as usize),
                e.adds,
                e.dels,
            )
        } else if w >= 70 {
            format!(
                " {} {} {} {} +{}/−{}",
                status_mark,
                fmt_date(e.time),
                short,
                truncate(&e.summary, w.saturating_sub(34) as usize),
                e.adds,
                e.dels,
            )
        } else {
            format!(
                " {} {} {} +{}/−{}",
                status_mark,
                short,
                truncate(&e.summary, w.saturating_sub(21) as usize),
                e.adds,
                e.dels,
            )
        };
        list_row(&mut s, y, 0, w.saturating_sub(2), &row, base);
    }
    scrollbar(
        &mut s,
        w.saturating_sub(1),
        list_top + 1,
        list_h.saturating_sub(1),
        scroll,
        events.len(),
    );
    if w >= 80 {
        if let Some(e) = events.get(cursor) {
            let note = if let Some(prev) = &e.prev_path {
                format!("renamed from {}", prev)
            } else {
                format!("{} ago", fmt_age(e.time, app.now))
            };
            let y = list_top;
            print_if_empty(
                &mut s,
                w.saturating_sub(note.len() as u16 + 2),
                y,
                &note,
                pal.s_faint(),
                None,
            );
        }
    }

    Arc::new(s)
}

fn cell(ch: &str, style: gibson::cell::Style) -> gibson::cell::Cell {
    gibson::cell::Cell::new(gibson::cell::Glyph::new(ch), style)
}
