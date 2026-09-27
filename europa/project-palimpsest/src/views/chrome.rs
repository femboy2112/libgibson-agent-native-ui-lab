//! Chrome: the editorial masthead, the command/status line, and overlays
//! (help, file browser, command palette) built as nested modal layers.

use gibson::cell::{RichText, Style};
use gibson::node::{Node, WrapMode};
use gibson::surface::{BorderType, Surface};

use crate::app::{App, InputMode};
use crate::theme::{pad_to, truncate, Palette};
use crate::views::widgets::*;

/// The masthead: PALIMPSEST · repo · view · counts. One reverse strip, the
/// editorial title bar of the atlas. The view title is semantic identity —
/// it is never the part that gets truncated.
pub fn header_node(app: &App, w: u16, pal: &Palette) -> Node {
    let repo_name = app.repo.name();
    let view = app.view.title();
    let counts = format!(
        "{}c · {}b · {}t",
        app.hist.len(),
        app.branches.iter().filter(|b| !b.is_remote).count(),
        app.tags.len()
    );
    let sel = app
        .selection
        .map(|i| {
            let r = &app.hist.rows[i as usize];
            format!("◈ {} {}", r.short, truncate(&r.summary, 30))
        })
        .unwrap_or_else(|| "◈ —".to_string());

    let left = if w >= 84 {
        format!(" PALIMPSEST ─ {}", truncate(&repo_name, 18))
    } else {
        " PALIMPSEST".to_string()
    };
    if w < 60 {
        // hostile/constrained: keep the identity (app + view + selection)
        // and drop everything else
        let code = app.view.short();
        let budget = (w as usize).saturating_sub(11 + code.len() + 6).max(4);
        let text = format!("{} │ {} │ {}", left, code, truncate(&sel, budget));
        return Node::text(
            pad_to(&truncate(&text, w as usize), w as usize),
            pal.s_bar_accent(),
        )
        .width(w as f32);
    }
    // identity block always fits: the view title is pinned to the right and
    // never truncated; counts drop out first, then the selection summary
    let right = if w >= 84 {
        format!("{} │ {}", counts, view)
    } else {
        view.to_string()
    };
    let right_w = crate::theme::width_of(&right) as u16;
    // budget: left + sel + 4 (separator) + right == w when pad == 0
    let sel_budget = (w as usize)
        .saturating_sub(left.len() + right_w as usize + 4)
        .max(4);
    let sel_text = truncate(&sel, sel_budget);
    let pad = (w as usize)
        .saturating_sub(left.len() + crate::theme::width_of(&sel_text) + right_w as usize + 4)
        .min(12);
    let text = format!(
        "{}{}{}{}{}",
        left,
        " ".repeat(pad),
        sel_text,
        " ".repeat(4),
        right
    );
    Node::text(
        pad_to(&truncate(&text, w as usize), w as usize),
        pal.s_bar_accent(),
    )
    .width(w as f32)
}

/// The command line: search input, command palette, or the status message.
/// While typing, a caret block marks the insertion point on this line.
pub fn cmdline_node(app: &App, w: u16, pal: &Palette) -> Node {
    match app.mode {
        InputMode::Search => {
            let text = format!(
                "⌕ {}▌  {} matches · Enter accept · Esc cancel",
                app.search.query,
                app.search.hits.len()
            );
            Node::text(
                pad_to(&truncate(&text, w as usize), w as usize),
                pal.s_accent(),
            )
            .width(w as f32)
        }
        InputMode::Command => {
            let text = format!(": {}▌  — goto <rev> · file <path> · author <n> · zoom <0-5> · export · report · help · quit", app.status);
            Node::text(
                pad_to(&truncate(&text, w as usize), w as usize),
                pal.s_warning(),
            )
            .width(w as f32)
        }
        InputMode::FileBrowser => {
            let b = app.browser.as_ref();
            let q = b.map(|b| b.filter.clone()).unwrap_or_default();
            let n = b.map(|b| b.entries.len()).unwrap_or(0);
            let text = format!(
                "FILES {} · {} entries · ↑↓ navigate · type to filter · Enter open · Esc close",
                q, n
            );
            Node::text(
                pad_to(&truncate(&text, w as usize), w as usize),
                pal.s_text(),
            )
            .width(w as f32)
        }
        _ => {
            let style = if app.status_err {
                Style::new().fg(app.palette.warning).bold()
            } else {
                pal.s_muted()
            };
            Node::text(
                pad_to(&truncate(&app.status, w as usize), w as usize),
                style,
            )
            .width(w as f32)
        }
    }
}

/// The key hint line (responsive: degrades from full legend to two marks).
pub fn hintline_node(app: &App, w: u16, pal: &Palette) -> Node {
    let s = match app.view {
        crate::app::View::Atlas => {
            if w >= 120 {
                " j/k step · J/K lane · ,. pan · +- zoom · Enter lens · f files · / search · : cmd · ? help · q quit"
            } else if w >= 70 {
                " j/k · ,. pan · +- zoom · Enter lens · / search · ? help"
            } else {
                " j/k · Enter lens · ? help"
            }
        }
        crate::app::View::Lens => {
            if w >= 100 {
                " Tab pane · j/k scroll · n/p hunk · / search · p provenance · s strata · E export · Esc atlas"
            } else {
                " j/k · n/p hunk · Esc back"
            }
        }
        crate::app::View::Provenance => {
            if w >= 90 {
                " j/k line · u unfold fiber · Enter jump · c commit · Esc back"
            } else {
                " j/k · u unfold · Esc back"
            }
        }
        crate::app::View::Strata => {
            if w >= 90 {
                " j/k event · Enter lens · r recenter · f files · Esc back"
            } else {
                " j/k · Enter lens · Esc back"
            }
        }
        crate::app::View::Health => " j/k scroll · R commit report · 1 atlas · q quit",
    };
    let text = format!(" {}", truncate(s, w.saturating_sub(2) as usize));
    Node::text(pad_to(&text, w as usize), pal.s_faint()).width(w as f32)
}

/// The help overlay: a dimmed veil with a centered card. Focus is captured in
/// the ring when opened, released on close.
pub fn help_overlay(_app: &App, w: u16, h: u16, pal: &Palette) -> Node {
    let mut rich = RichText::styled(" THE TIME MACHINE — KEYS ", pal.s_accent());
    rich.push_line(gibson::cell::Line::styled("", pal.s_text()));
    let rows: &[(&str, &str)] = &[
        ("1..5", "atlas · strata · diff lens · provenance · health"),
        ("j/k or ←/→", "step through commits along the braids"),
        ("J / K", "jump one lane down / up"),
        (", / .", "pan the camera through time"),
        ("+ / -", "zoom the time-lens (scale ladder)"),
        ("z0..z5", "lens presets: eon → day"),
        ("Enter", "zoom in: commit → diff lens"),
        ("f", "browse files at the selected commit"),
        ("/", "search history; n/N cycle hits"),
        (":", "command palette (goto, author, zoom, export…)"),
        ("c", "commit the dossier to terminal scrollback"),
        ("E / R", "export diff / health report to scrollback"),
        ("u", "(provenance) unfold a line into its fiber"),
        ("g / G", "jump to newest / oldest indexed commit"),
        ("Tab", "cycle pane focus (diff lens)"),
        ("q / Esc", "back out one level / quit"),
    ];
    for (k, v) in rows {
        let mut l = gibson::cell::Line::new();
        l.push(gibson::cell::Span::styled(
            format!("  {:<12}", k),
            pal.s_selection(),
        ));
        l.push(gibson::cell::Span::styled(v, pal.s_text()));
        rich.push_line(l);
    }
    let mut l = gibson::cell::Line::styled("", pal.s_text());
    l.push(gibson::cell::Span::styled(
        "  every metric is bounded and labeled; sampled values carry ≈",
        pal.s_faint(),
    ));
    rich.push_line(l);

    let card_w = w.saturating_mul(3).div_ceil(4).clamp(30, 78);
    let card_h = (rows.len() as u16 + 5).min(h.saturating_sub(2));
    let card = Node::panel(" ? ", BorderType::Rounded, pal.s_border())
        .width(card_w as f32)
        .height(card_h as f32)
        .child(Node::rich_text_wrapped(rich, WrapMode::NoWrap));
    let ox = ((w - card_w) / 2) as f32;
    let oy = ((h.saturating_sub(card_h)) / 2) as f32;
    card.offset(ox, oy)
}

/// The file browser overlay: a scrollable, filterable tree list painted onto
/// a surface, presented in a bordered card.
pub fn browser_card(app: &App, w: u16, h: u16, pal: &Palette) -> Node {
    let Some(b) = &app.browser else {
        return Node::col();
    };
    let card_w = w.saturating_mul(7).div_ceil(10).clamp(30, 90);
    let card_h = (h.saturating_sub(4)).min(30);
    let card_h = card_h.max(8).min(h.saturating_sub(2));

    let inner_w = card_w.saturating_sub(2);
    let mut s = Surface::new(inner_w, card_h.saturating_sub(2).max(1));
    let filtered: Vec<&crate::git::filelog::TreeEntry> = if b.filter.is_empty() {
        b.entries.iter().collect()
    } else {
        let n = b.filter.to_lowercase();
        b.entries
            .iter()
            .filter(|e| e.path.to_lowercase().contains(&n))
            .collect()
    };
    let view_h = (card_h.saturating_sub(2)) as usize;
    let scroll = b.cursor.saturating_sub(view_h.saturating_sub(1) / 2);
    for vi in 0..view_h {
        let Some(e) = filtered.get(scroll + vi) else {
            break;
        };
        let y = vi as u16;
        let is_cur = scroll + vi == b.cursor;
        let style = if is_cur {
            pal.s_bar_accent()
        } else if e.kind == crate::git::filelog::EntryKind::Dir {
            pal.s_muted()
        } else {
            pal.s_text()
        };
        let mark = if e.kind == crate::git::filelog::EntryKind::Dir {
            "▸"
        } else {
            "·"
        };
        let row = format!(" {} {}", mark, truncate(&e.path, inner_w as usize));
        list_row(&mut s, y, 0, inner_w, &row, style);
    }
    scrollbar(
        &mut s,
        inner_w.saturating_sub(1),
        0,
        view_h as u16,
        b.cursor,
        filtered.len().max(1),
    );

    let title = format!(
        " FILES @ {} — {} ",
        app.hist.rows[b.at_commit as usize].short,
        filtered.len()
    );
    Node::panel(title, BorderType::Rounded, pal.s_border())
        .width(card_w as f32)
        .height(card_h as f32)
        .child(Node::raster(s))
        .offset(
            ((w - card_w) / 2) as f32,
            ((h.saturating_sub(card_h)) / 2) as f32,
        )
}
