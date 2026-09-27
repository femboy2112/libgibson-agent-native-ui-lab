//! PROVENANCE — blame as fibers that unfold backward into history.
//!
//! Each source line carries its origin: an age glyph, the introducing
//! commit's short oid, and the author's lane color (the same braid colors as
//! the atlas — authorship stays legible across views). Selecting a line
//! "unfolds" it into a fiber panel: the introducing commit, then later edits
//! within one numbered row of its position, each a nearby station you can
//! jump to. These stations are positional context, not additional authors of
//! the selected text. The fiber stays in the same screen.

use std::sync::Arc;

use gibson::cell::{Cell, Glyph};
use gibson::surface::Surface;

use crate::app::App;
use crate::theme::{fmt_age, fmt_date, glyphs, truncate, Palette};
use crate::views::widgets::*;

pub fn draw_prov(app: &mut App, w: u16, h: u16, pal: &Palette) -> Arc<Surface> {
    let mut s = Surface::new(w, h);
    let Some(blame) = &app.prov.blame else {
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
    let gutter_w: u16 = if narrow { 16 } else { 27 };
    let text_x = gutter_w;
    let text_w = w.saturating_sub(gutter_w + 2);

    let unfold_h = if app.prov.unfolded {
        (h / 3).clamp(4, 12)
    } else {
        0
    };
    // Reserve a separator and the bottom hint instead of painting either on
    // top of the last visible source line.
    let body_h = h.saturating_sub(2 + unfold_h);

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
        let top = 2 + body_h;
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

        // Later commits with changed rows near this numbered position.
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
                "   · no later nearby edits within the indexed history",
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

/// Unfold the selected line: its blame origin plus later nearby changed rows
/// (within one numbered line). A hunk's context lines are never evidence of
/// edits. Stations are positional context, not claims of shared line identity.
/// The bounded computation is cached in `prov.fiber`.
pub fn unfold_fiber(app: &mut App) {
    app.prov.fiber.clear();
    let Some(b) = app.prov.blame.clone() else {
        return;
    };
    let Some(cl) = b.lines.get(app.prov.line_cursor) else {
        return;
    };
    let selected_idx = app.hist.idx_of(&b.oid);
    let events = crate::git::filelog::file_events(&app.repo, &app.hist, &b.path, 3000);
    let mut fiber = Vec::new();
    for e in events {
        if e.time <= cl.time || e.oid == cl.oid {
            continue;
        }
        // A provenance snapshot must not claim edits made after its commit.
        if let (Some(selected), Some(event)) = (selected_idx, app.hist.idx_of(&e.oid)) {
            if event < selected {
                continue;
            }
        }
        if changed_near_line(&app.repo, &e.oid, &b.path, cl.line_no) {
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

/// Look at changed diff lines themselves, never the surrounding hunk context.
/// Added rows use new-file numbers; removed rows use old-file numbers. This is
/// explicitly nearby positional context, not a trace of the same line text.
fn changed_near_line(repo: &crate::git::repo::Repo, oid: &str, path: &str, line_no: u32) -> bool {
    let diff = match crate::git::diff::commit_diff(repo, oid, 400) {
        Ok(d) => d,
        Err(_) => return false,
    };
    for f in &diff.files {
        if f.path() != path && f.old_path != path {
            continue;
        }
        if f.hunks.iter().flat_map(|h| h.lines.iter()).any(|line| {
            let changed_no = match line.kind {
                crate::git::diff::LineKind::Add => line.new_no,
                crate::git::diff::LineKind::Del => line.old_no,
                crate::git::diff::LineKind::Context => None,
            };
            changed_no.is_some_and(|n| line_no.abs_diff(n) <= 1)
        }) {
            return true;
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
