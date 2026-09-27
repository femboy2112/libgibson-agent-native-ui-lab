//! PROVENANCE — blame as fibers that unfold backward into history.
//!
//! Each source line carries its origin: an age glyph, the introducing
//! commit's short oid, and the author's lane color (the same braid colors as
//! the atlas — authorship stays legible across views). Selecting a line
//! "unfolds" it into a fiber panel: the introducing commit, then every later
//! hunk that overlapped this line position, each one a station you can jump
//! to. The transformation makes the line appear to extend backward into the
//! topology instead of opening a new screen.

use std::sync::Arc;

use gibson::cell::{Cell, Glyph};
use gibson::surface::Surface;

use crate::app::App;
use crate::theme::{fmt_age, fmt_date, glyphs, truncate, Palette};
use crate::views::widgets::*;

pub fn draw_prov(app: &mut App, w: u16, h: u16, pal: &Palette) -> Arc<Surface> {
    let mut s = Surface::new(w, h);
    let Some(blame) = app.prov.blame.clone() else {
        s.print_str(
            1,
            1,
            "no file under the provenance lens — open the diff lens, focus a file, press p",
            pal.s_faint(),
            Some(w),
        );
        return Arc::new(s);
    };

    let sel_short = app
        .selection
        .map(|i| app.hist.rows[i as usize].short.clone())
        .unwrap_or_default();
    let header = format!(
        " ◈ {} · FIBERS · {} · {} lines",
        sel_short,
        truncate(&blame.path, (w as usize).saturating_sub(34)),
        blame.lines.len()
    );
    strip(&mut s, 0, &header, pal.s_bar_accent());

    let narrow = w < 100;
    let gutter_w: u16 = if narrow { 14 } else { 22 };
    let text_x = gutter_w;
    let text_w = w.saturating_sub(gutter_w);

    let unfold_h = if app.prov.unfolded {
        (h / 3).clamp(4, 12)
    } else {
        0
    };
    let body_h = h.saturating_sub(1 + unfold_h);

    let lines = &blame.lines;
    let cursor = app.prov.line_cursor;
    let scroll = app.prov.line_scroll;
    let now = app.now;

    for vi in 0..body_h as usize {
        let idx = scroll + vi;
        let Some(l) = lines.get(idx) else { break };
        let y = 1 + vi as u16;
        let is_cur = idx == cursor;

        // gutter: line number + age fiber + origin oid
        let age_glyph = age_glyph_of(l.time, now);
        let in_hist = app
            .hist
            .idx_of(&l.oid)
            .map(|i| app.hist.rows[i as usize].lane)
            .unwrap_or(0);
        let gutter_style = if is_cur {
            pal.s_bar_accent()
        } else {
            pal.s_lane(in_hist as usize)
        };
        let no = format!("{:>4}", l.line_no);
        let oid_short = l.oid.get(..7).unwrap_or("0000000");
        let gutter = if narrow {
            format!("{} {} {}", age_glyph, no, oid_short)
        } else {
            format!(
                "{} {} {} {:<10}",
                age_glyph,
                no,
                oid_short,
                truncate(&l.author, 10)
            )
        };
        s.print_str(
            0,
            y,
            &crate::theme::pad_to(&gutter, gutter_w as usize),
            gutter_style,
            Some(gutter_w),
        );

        // source text
        let tstyle = if is_cur {
            pal.s_bar_accent()
        } else {
            pal.s_text()
        };
        let shown = truncate(&l.text, text_w as usize);
        s.print_str(
            text_x,
            y,
            &crate::theme::pad_to(&shown, text_w as usize),
            tstyle,
            Some(text_w),
        );
    }
    scrollbar(&mut s, w.saturating_sub(1), 1, body_h, scroll, lines.len());

    // ---- the unfolded fiber panel --------------------------------------------
    if unfold_h > 0 {
        let top = 1 + body_h;
        // separator: the fold line
        for x in 0..w {
            s.set_cell(
                x,
                top.saturating_sub(1),
                Cell::new(Glyph::new(glyphs::FIBER), pal.s_selection()),
            );
        }
        let Some(cl) = lines.get(cursor) else {
            return Arc::new(s);
        };
        let label = format!(
            " ┆ {} │ line {} born in {} by {}, {}",
            cl.line_no,
            cl.orig_line,
            cl.oid.get(..7).unwrap_or("0000000"),
            truncate(&cl.author, 16),
            fmt_age(cl.time, now)
        );
        s.print_str(
            0,
            top,
            &truncate(&label, w as usize),
            pal.s_selection(),
            Some(w),
        );

        // later commits whose hunks overlapped this line's position
        // (cached in prov.fiber at unfold time — never recomputed per frame)
        let mut shown = 0usize;
        for e in app.prov.fiber.iter() {
            if shown + 2 >= unfold_h as usize {
                break;
            }
            if e.time <= cl.time || e.oid == cl.oid {
                continue;
            }
            let y = top + 1 + shown as u16;
            let row = format!(
                "   ◇ {} {} {} +{}/−{}",
                fmt_date(e.time),
                e.oid.get(..7).unwrap_or("0000000"),
                truncate(&e.summary, (w as usize).saturating_sub(40)),
                e.adds,
                e.dels
            );
            let style = pal.s_muted();
            s.print_str(0, y, &truncate(&row, w as usize), style, Some(w));
            shown += 1;
        }
        if shown == 0 {
            s.print_str(
                0,
                top + 1,
                "   · no later hunks overlapped this line within the scan bound",
                pal.s_faint(),
                Some(w),
            );
        }
        let hint = " ┆ Enter: jump to fiber commit · u: fold";
        s.print_str(
            0,
            h - 1,
            &truncate(hint, w as usize),
            pal.s_faint(),
            Some(w),
        );
    } else {
        let hint =
            " ┆ j/k line · u unfold fiber · Enter jump · c commit summary · Esc back".to_string();
        s.print_str(
            0,
            h - 1,
            &truncate(&hint, w as usize),
            pal.s_faint(),
            Some(w),
        );
    }

    Arc::new(s)
}

/// Age ramp: today's edits are solid, ancient ones fade to dots.
fn age_glyph_of(t: i64, now: i64) -> &'static str {
    let age_days = (now - t).max(0) / 86_400;
    match age_days {
        0..=7 => glyphs::AGE_NEW,
        8..=90 => glyphs::AGE_RECENT,
        91..=730 => glyphs::AGE_OLD,
        _ => glyphs::AGE_ANCIENT,
    }
}

/// Unfold the currently-selected line's fiber: every commit in the file's
/// recovered history (newer than the introducing commit) whose hunks overlap
/// the line's position. Computed once per unfold and cached in `prov.fiber`.
pub fn unfold_fiber(app: &mut App) {
    app.prov.fiber.clear();
    let Some(b) = app.prov.blame.clone() else {
        return;
    };
    let Some(cl) = b.lines.get(app.prov.line_cursor) else {
        return;
    };
    let events = crate::git::filelog::file_events(&app.repo, &app.hist, &b.path, 3000);
    let mut fiber = Vec::new();
    for e in events {
        if e.time <= cl.time || e.oid == cl.oid {
            continue;
        }
        if hunk_overlaps(&app.repo, &e.oid, &b.path, cl.line_no) {
            fiber.push(e);
        }
        if fiber.len() >= 32 {
            break;
        }
    }
    app.prov.fiber = fiber;
    app.prov.unfolded = true;
    app.mark();
}

/// Did this commit's diff hunks (of `path`) overlap line `line_no` in the new
/// file? Hunk new ranges are precomputed in the diff model — no header parsing.
fn hunk_overlaps(repo: &crate::git::repo::Repo, oid: &str, path: &str, line_no: u32) -> bool {
    let diff = match crate::git::diff::commit_diff(repo, oid, 400) {
        Ok(d) => d,
        Err(_) => return false,
    };
    for f in &diff.files {
        if f.path() != path && f.old_path != path {
            continue;
        }
        for h in &f.hunks {
            if line_no + 1 >= h.new_start && line_no < h.new_start.saturating_add(h.new_len) {
                return true;
            }
        }
    }
    false
}

/// The blame summary committed to scrollback (the `c` action).
pub fn blame_summary_text(app: &App) -> String {
    let Some(b) = &app.prov.blame else {
        return String::new();
    };
    let mut out = String::new();
    out.push_str(&format!(
        "PROVENANCE ─ {} @ {}\n",
        b.path,
        b.oid.get(..7).unwrap_or("0000000")
    ));
    let mut distinct = std::collections::BTreeMap::new();
    for l in &b.lines {
        let e = distinct
            .entry(l.oid.clone())
            .or_insert((0usize, l.author.clone(), l.time));
        e.0 += 1;
    }
    let total = b.lines.len().max(1);
    for (oid, (n, author, time)) in distinct {
        out.push_str(&format!(
            "  {} {:>3}% {:<5} lines  {}  {}\n",
            &oid[..7],
            (n * 100) / total,
            n,
            truncate(&author, 16),
            fmt_date(time)
        ));
    }
    out
}
