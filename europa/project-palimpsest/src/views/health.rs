//! REPOSITORY HEALTH — an editorial metrics report.
//!
//! SWISS_SIGNAL discipline: rules separate sections, numbers are right-set in
//! columns, bars carry magnitude, and sampled quantities are explicitly
//! marked as estimates. Scrollable as one document.

use std::sync::Arc;

use gibson::surface::Surface;

use crate::app::App;
use crate::theme::{fmt_age, fmt_date, glyphs, truncate, Palette};
use crate::views::widgets::*;

pub fn draw_health(app: &mut App, w: u16, h: u16, pal: &Palette) -> Arc<Surface> {
    let mut s = Surface::new(w, h);
    app.ensure_metrics();
    let Some(m) = &app.metrics else {
        s.print_str(1, 1, "metrics not computed", pal.s_faint(), Some(w));
        return Arc::new(s);
    };

    // We paint into a virtual page, then slice the visible window.
    // Keep a blank column between right-set values and the scroll rail.
    let page_w = w.saturating_sub(2);
    let mut page = Surface::new(page_w, 200);
    let mut y = 0u16;
    let title = " REPOSITORY HEALTH ";
    page.print_str(
        0,
        y,
        &truncate(
            &format!(
                "{}─{}─",
                glyphs::RAIL.repeat(3),
                glyphs::RAIL.repeat((page_w as usize).saturating_sub(title.len() + 5))
            ),
            page_w as usize,
        ),
        pal.s_border(),
        Some(page_w),
    );
    page.print_str(4, y, title, pal.s_accent(), Some(page_w - 4));
    y += 1;
    y = section(&mut page, y, page_w, "OVERVIEW", pal);
    kv(
        &mut page,
        y,
        page_w,
        "commits indexed",
        &format!("{} of {} scanned", m.commits, m.scanned),
        pal,
    );
    kv(
        &mut page,
        y + 1,
        page_w,
        "window truncated",
        if m.truncated {
            "yes — raise --limit"
        } else {
            "no"
        },
        pal,
    );
    kv(
        &mut page,
        y + 2,
        page_w,
        "history span",
        &format!("{} days", m.span_days),
        pal,
    );
    kv(
        &mut page,
        y + 3,
        page_w,
        "merges",
        &format!("{} ({:.0}%)", m.merge_count, m.merge_ratio * 100.0),
        pal,
    );
    kv(
        &mut page,
        y + 4,
        page_w,
        "tags",
        &m.tag_count.to_string(),
        pal,
    );
    kv(
        &mut page,
        y + 5,
        page_w,
        "distinct authors",
        &m.authors.len().to_string(),
        pal,
    );
    y += 7;

    // authors table
    y = section(&mut page, y, page_w, "AUTHOR DISTRIBUTION", pal);
    let top_authors = m.authors.iter().take(8);
    let max_commits = top_authors
        .clone()
        .map(|a| a.commits)
        .max()
        .unwrap_or(1)
        .max(1);
    for a in top_authors {
        let name = truncate(&a.name, (page_w / 3).max(14) as usize);
        page.print_str(1, y, &name, pal.s_text(), Some(page_w));
        let bar_w = (page_w as usize)
            .saturating_sub(page_w as usize / 2)
            .max(10) as u16;
        let bx = page_w / 3;
        bar(
            &mut page,
            bx,
            y,
            bar_w,
            a.commits as f32 / max_commits as f32,
            pal.s_accent(),
            pal.s_border(),
        );
        let num = format!("{:>5}  {:>5.1}%", a.commits, a.share * 100.0);
        page.print_str(
            page_w.saturating_sub(num.len() as u16 + 1),
            y,
            &num,
            pal.s_muted(),
            Some(page_w),
        );
        if y + 1 < 200 {
            y += 1;
        }
    }
    y += 1;

    // hottest files
    y = section(&mut page, y, page_w, "HOTTEST FILES (churn)", pal);
    if m.hottest.is_empty() {
        page.print_str(
            1,
            y,
            "no file activity sampled",
            pal.s_faint(),
            Some(page_w),
        );
        y += 2;
    } else {
        let max_t = m
            .hottest
            .iter()
            .map(|f| f.touches)
            .max()
            .unwrap_or(1)
            .max(1);
        let sample_note = if m.complete_scan {
            "complete scan".to_string()
        } else {
            format!("sampled {} commits, extrapolated", m.sample_size)
        };
        page.print_str(1, y, &sample_note, pal.s_faint(), Some(page_w));
        y += 1;
        for f in m.hottest.iter().take(8) {
            let path = truncate(&f.path, (page_w / 2).max(16) as usize);
            page.print_str(1, y, &path, pal.s_text(), Some(page_w));
            let est = if f.estimated { "≈" } else { " " };
            let num = format!("{}{:>6} touches", est, f.touches);
            let num_x = page_w.saturating_sub(num.len() as u16 + 1);
            let bx = page_w / 2;
            let bar_w = num_x.saturating_sub(bx + 2);
            bar(
                &mut page,
                bx,
                y,
                bar_w,
                f.touches as f32 / max_t as f32,
                pal.s_warning(),
                pal.s_border(),
            );
            page.print_str(num_x, y, &num, pal.s_muted(), Some(page_w));
            if y + 1 < 200 {
                y += 1;
            }
        }
        y += 1;
        kv(
            &mut page,
            y,
            page_w,
            "concentration (top-10 share)",
            &format!("{:.0}% of file events", m.concentration * 100.0),
            pal,
        );
        y += 2;
    }

    // largest commits
    let largest_title = if m.complete_scan {
        "LARGEST COMMITS (by paths touched)".to_string()
    } else {
        format!("LARGEST COMMITS (of {} sampled)", m.sample_size)
    };
    y = section(&mut page, y, page_w, &largest_title, pal);
    for c in m.largest.iter().take(5) {
        let left = format!(
            " {} {}",
            c.short,
            truncate(&c.summary, (page_w as usize).saturating_sub(30))
        );
        page.print_str(1, y, &left, pal.s_text(), Some(page_w));
        let right = format!(
            "{} paths · {} · {} ago",
            c.files,
            truncate(&c.author, 12),
            fmt_age(c.time, app.now)
        );
        page.print_str(
            page_w.saturating_sub(right.len() as u16 + 1),
            y,
            &right,
            pal.s_muted(),
            Some(page_w),
        );
        if y + 1 < 200 {
            y += 1;
        }
    }
    y += 1;

    // branch ages
    y = section(
        &mut page,
        y,
        page_w,
        "BRANCH AGES (* outside indexed history)",
        pal,
    );
    for b in m.branches.iter().take(6) {
        let left = format!(
            " {} {}{}",
            if b.is_remote { "◇" } else { "●" },
            truncate(&b.name, (page_w / 2).max(16) as usize),
            if b.in_window { "" } else { "*" }
        );
        page.print_str(1, y, &left, pal.s_text(), Some(page_w));
        let right = format!(
            "{} ({} ago)",
            fmt_date(b.last_commit),
            fmt_age(b.last_commit, app.now)
        );
        page.print_str(
            page_w.saturating_sub(right.len() as u16 + 1),
            y,
            &right,
            pal.s_muted(),
            Some(page_w),
        );
        if y + 1 < 200 {
            y += 1;
        }
    }
    y += 2;

    // activity sparkline
    y = section(&mut page, y, page_w, "ACTIVITY (commits per month)", pal);
    let max_month = m.monthly.iter().map(|(_, c)| *c).max().unwrap_or(1).max(1);
    let first = m.monthly.len().saturating_sub(page_w as usize / 2);
    let vis: Vec<(String, u32)> = m.monthly[first.min(m.monthly.len())..].to_vec();
    if !vis.is_empty() {
        let vals: Vec<u32> = vis.iter().map(|(_, c)| *c).collect();
        sparkline(
            &mut page,
            1,
            y,
            (page_w as usize / 2).max(10) as u16,
            &vals,
            max_month,
            pal.s_author(),
        );
        let label = format!("{} … {}", vis[0].0, vis[vis.len() - 1].0);
        page.print_str(page_w / 2 + 2, y, &label, pal.s_faint(), Some(page_w));
        y += 1;
    }
    y += 1;

    let content_h = y as usize;

    // slice visible window
    let scroll = app.health_scroll.min(content_h.saturating_sub(h as usize));
    let src = &page;
    for row in 0..h as usize {
        let sy = row + scroll;
        if sy >= src.height as usize {
            break;
        }
        for x in 0..w as usize {
            if let Some(c) = src.get(x as u16, sy as u16) {
                s.set_cell(x as u16, row as u16, c.clone());
            }
        }
    }
    scrollbar(&mut s, w.saturating_sub(1), 0, h, scroll, content_h.max(1));
    Arc::new(s)
}

fn section(s: &mut Surface, y: u16, w: u16, title: &str, pal: &Palette) -> u16 {
    if y >= s.height {
        return y;
    }
    let line = format!(" {} ", title);
    s.print_str(0, y, &truncate(&line, w as usize), pal.s_accent(), Some(w));
    let dash_from = line.len() as u16 + 1;
    if dash_from < w {
        for x in dash_from..w {
            s.set_cell(x, y, cell_of(glyphs::RAIL, pal.s_border()));
        }
    }
    y + 1
}

fn kv(s: &mut Surface, y: u16, w: u16, k: &str, v: &str, pal: &Palette) {
    if y >= s.height {
        return;
    }
    kv_row(
        s,
        y,
        1,
        w.saturating_sub(2),
        k,
        v,
        pal.s_muted(),
        pal.s_text(),
    );
}

fn cell_of(ch: &str, style: gibson::cell::Style) -> gibson::cell::Cell {
    gibson::cell::Cell::new(gibson::cell::Glyph::new(ch), style)
}

/// The full health report as plain text (for scrollback commit via `R`).
pub fn report_text(app: &App) -> String {
    let Some(m) = &app.metrics else {
        return "metrics not computed".to_string();
    };
    let mut out = String::new();
    out.push_str(&format!(
        "REPOSITORY HEALTH ─ {}\n",
        crate::theme::fmt_date(app.now)
    ));
    out.push_str(&format!(
        "  commits {} / scanned {} · span {}d · merges {} ({:.0}%) · tags {} · authors {}\n",
        m.commits,
        m.scanned,
        m.span_days,
        m.merge_count,
        m.merge_ratio * 100.0,
        m.tag_count,
        m.authors.len()
    ));
    out.push_str("AUTHORS\n");
    for a in m.authors.iter().take(6) {
        out.push_str(&format!(
            "  {:<16} {:>5} commits  {:>5.1}%\n",
            truncate(&a.name, 16),
            a.commits,
            a.share * 100.0
        ));
    }
    out.push_str("HOTTEST FILES\n");
    for f in m.hottest.iter().take(6) {
        out.push_str(&format!(
            "  {}{:<5} {:>44} {:>5} touches\n",
            if f.estimated { "≈" } else { " " },
            "",
            truncate(&f.path, 44),
            f.touches
        ));
    }
    if m.complete_scan {
        out.push_str("LARGEST COMMITS (by paths touched)\n");
    } else {
        out.push_str(&format!("LARGEST COMMITS (of {} sampled)\n", m.sample_size));
    }
    for c in m.largest.iter().take(5) {
        out.push_str(&format!(
            "  {} {:>3} paths  {}\n",
            c.short,
            c.files,
            truncate(&c.summary, 60)
        ));
    }
    out.push_str("BRANCH AGES (* outside indexed history)\n");
    for b in m.branches.iter().take(6) {
        out.push_str(&format!(
            "  {:<24} {} ({} ago)\n",
            truncate(
                &format!("{}{}", b.name, if b.in_window { "" } else { "*" }),
                24
            ),
            fmt_date(b.last_commit),
            fmt_age(b.last_commit, app.now)
        ));
    }
    out
}
