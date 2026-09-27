//! DIFF LENS — a professional source/diff viewer.
//!
//! Left pane: the commit's file ledger (status, path, ±). Right pane: hunk
//! stream with line numbers, intraline emphasis, and light syntax awareness
//! (comments, strings, keywords, numbers) — readable, never pretending to be
//! a real parser. At narrow widths the file ledger collapses into a single
//! context strip.

use std::sync::Arc;

use gibson::cell::{Cell, Color, Glyph, Style};
use gibson::surface::Surface;

use crate::app::lens::{find_matches, flatten};
use crate::app::App;
use crate::git::diff::LineKind;
use crate::theme::{glyphs, truncate, Palette};
use crate::views::widgets::*;

/// The full lens surface for the current commit diff.
pub fn draw_lens(app: &mut App, w: u16, h: u16, pal: &Palette) -> Arc<Surface> {
    let mut s = Surface::new(w, h);
    let Some(diff) = app.lens.diff.clone() else {
        s.print_str(
            1,
            1,
            "no diff loaded — select a commit and open the lens (Enter)",
            pal.s_faint(),
            Some(w),
        );
        return Arc::new(s);
    };

    let wide = w >= 100;
    let files_w: u16 = if wide {
        (w * 28 / 100).clamp(24, 40)
    } else {
        0
    };
    let hunk_w = w - files_w;

    // ---- file ledger pane -------------------------------------------------------
    if wide {
        draw_file_pane(&mut s, app, &diff, files_w, h, pal);
    }

    // ---- hunk pane ----------------------------------------------------------------
    let hx = files_w;
    draw_hunk_pane(&mut s, app, &diff, hx, hunk_w, h, pal);

    Arc::new(s)
}

fn draw_file_pane(
    s: &mut Surface,
    app: &App,
    diff: &crate::git::diff::CommitDiff,
    w: u16,
    h: u16,
    pal: &Palette,
) {
    // pane border
    let border_style = if app.lens.focus_files {
        pal.s_accent()
    } else {
        pal.s_border()
    };
    for y in 0..h {
        put(s, w as i32 - 1, y as i32, glyphs::VERT, border_style);
    }
    s.print_str(1, 0, " FILES ", border_style.bold(), Some(w));
    let total_adds: u32 = diff.files.iter().map(|f| f.adds).sum();
    let total_dels: u32 = diff.files.iter().map(|f| f.dels).sum();
    let stats = format!("+{} −{}", total_adds, total_dels);
    s.print_str(
        w.saturating_sub(stats.len() as u16 + 3),
        0,
        &stats,
        pal.s_muted(),
        Some(w),
    );

    let inner_w = w.saturating_sub(2);
    let list_h = h.saturating_sub(1);
    let scroll = app.lens.file_scroll;
    let cursor = app.lens.file_cursor;
    let files: Vec<&crate::git::diff::FileDiff> = {
        let mut v: Vec<_> = diff.files.iter().collect();
        if !app.lens.query.is_empty() {
            let n = app.lens.query.to_lowercase();
            v.retain(|f| f.path().to_lowercase().contains(&n));
        }
        v
    };

    for vi in 0..list_h as usize {
        let idx = scroll + vi;
        let Some(f) = files.get(idx) else { break };
        let y = 1 + vi as u16;
        if y >= h {
            break;
        }
        let is_cur = idx == cursor;
        let row_style = if is_cur {
            pal.s_bar_accent()
        } else {
            pal.s_text()
        };
        let status_style = match f.status {
            'A' => Style::new().fg(Color::Ansi256(71)),
            'D' => Style::new().fg(Color::Ansi256(167)),
            'R' => Style::new().fg(Color::Ansi256(140)),
            _ => pal.s_muted(),
        };
        let mark = match f.status {
            'A' => glyphs::PLUS,
            'D' => glyphs::MINUS,
            'R' => "↗",
            _ => "·",
        };
        let path_disp = truncate(f.path(), (inner_w as usize).saturating_sub(10));
        let counts = format!("+{:<3}−{:<3}", f.adds, f.dels);
        let line = format!(" {} {:<2} {}", mark, "", path_disp);
        s.print_str(
            0,
            y,
            &crate::theme::pad_to(&line, inner_w as usize),
            row_style,
            Some(inner_w),
        );
        // status marker overlay
        s.set_cell(
            1,
            y,
            Cell::new(
                Glyph::new(mark),
                if is_cur { row_style } else { status_style },
            ),
        );
        // counts right-aligned
        let cx = inner_w.saturating_sub(counts.len() as u16);
        s.print_str(
            cx,
            y,
            &counts,
            if is_cur { row_style } else { pal.s_muted() },
            Some(inner_w - cx),
        );
        if f.binary {
            s.print_str(cx.saturating_sub(4), y, "bin ", pal.s_warning(), Some(6));
        }
    }
    scrollbar(
        s,
        w.saturating_sub(2),
        1,
        list_h,
        scroll,
        files.len().max(1),
    );
}

fn draw_hunk_pane(
    s: &mut Surface,
    app: &mut App,
    diff: &crate::git::diff::CommitDiff,
    x0: u16,
    w: u16,
    h: u16,
    pal: &Palette,
) {
    // header: focused file + commit identity (the visual anchor carried over
    // from the atlas pin)
    let Some(f) = diff.files.get(app.lens.file_cursor) else {
        return;
    };
    let sel_short = app
        .selection
        .map(|i| app.hist.rows[i as usize].short.clone())
        .unwrap_or_default();
    let anchor = format!(
        " ◈ {}  {} ",
        sel_short,
        truncate(f.path(), (w as usize).saturating_sub(16))
    );
    s.print_str(
        x0,
        0,
        &crate::theme::pad_to(&anchor, w as usize),
        pal.s_bar_accent(),
        Some(w),
    );
    let y = 1u16;

    // flatten with the cursor's hunk first when navigating hunks
    let max_lines = (h as usize).saturating_sub(1);
    let all = flatten(diff, 4096);

    // diff search: filter scroll to matches
    if !app.lens.query.is_empty() {
        app.lens.matches = find_matches(diff, &app.lens.query, 200);
    } else {
        app.lens.matches.clear();
    }

    // scroll to keep the hunk cursor visible
    let hunk_of = |line_idx: usize| -> Option<usize> { all.get(line_idx).map(|l| l.hunk_idx) };
    let _ = hunk_of;

    // find display index of the hunk cursor's first line for the focused file
    if app.lens.hunk_cursor > 0 || app.lens.hunk_scroll > 0 {
        let target = all
            .iter()
            .position(|l| {
                l.file_idx == app.lens.file_cursor
                    && l.line_idx.is_none()
                    && l.hunk_idx == app.lens.hunk_cursor
            })
            .unwrap_or(0);
        if target < app.lens.hunk_scroll || target >= app.lens.hunk_scroll + max_lines {
            app.lens.hunk_scroll = target.saturating_sub(2);
        }
    }

    let scroll = app.lens.hunk_scroll.min(all.len().saturating_sub(1));
    for (vi, dl) in all.iter().skip(scroll).take(max_lines).enumerate() {
        let y = y as usize + vi;
        if y >= h as usize {
            break;
        }
        let y = y as u16;
        if dl.is_meta() {
            let style = if dl.text.starts_with("──") {
                pal.s_accent()
            } else {
                pal.s_border()
            };
            s.print_str(x0, y, &truncate(&dl.text, w as usize), style, Some(w));
            continue;
        }
        let gutter_w = 9;
        let body_x = x0 + gutter_w;
        let body_w = w.saturating_sub(gutter_w);

        // gutter: sign + old/new line numbers, exactly `gutter_w` wide
        let old_s = dl
            .old_no
            .map(|n| format!("{:>4}", n))
            .unwrap_or_else(|| "    ".to_string());
        let new_s = dl
            .new_no
            .map(|n| format!("{:>4}", n))
            .unwrap_or_else(|| "    ".to_string());
        let gutter = format!("{}{}{}", sign_of(dl.kind), old_s, new_s);
        let g_style = match dl.kind {
            LineKind::Add => pal.s_added(),
            LineKind::Del => pal.s_removed(),
            LineKind::Context => pal.s_faint(),
        };
        s.print_str(x0, y, &gutter, g_style, Some(gutter_w));

        // body with syntax-lite emphasis and intraline emphasis
        paint_line(s, body_x, y, body_w, dl, pal);
    }
    scrollbar(
        s,
        x0 + w.saturating_sub(1),
        1,
        h.saturating_sub(1),
        scroll,
        all.len().max(1),
    );

    // footer hint row
    if h >= 3 {
        let hint = if app.lens.query.is_empty() {
            format!(
                " {} hunks · n/p next/prev hunk · / search · o old/new",
                f.hunks.len()
            )
        } else {
            format!(
                " match {}/{} for `{}` · Enter jump · Esc clear",
                app.lens.match_cursor + 1,
                app.lens.matches.len(),
                app.lens.query
            )
        };
        s.print_str(
            x0,
            h - 1,
            &truncate(&hint, w as usize),
            pal.s_faint(),
            Some(w),
        );
    }
}

fn sign_of(kind: LineKind) -> &'static str {
    match kind {
        LineKind::Add => glyphs::PLUS,
        LineKind::Del => glyphs::MINUS,
        LineKind::Context => " ",
    }
}

/// Paint one diff line with light syntax awareness:
/// comments / strings / keywords / numbers get tinted on context lines; added
/// and removed lines stay colored by their diff kind (with intraline emphasis
/// underlined).
fn paint_line(
    s: &mut Surface,
    x: u16,
    y: u16,
    w: u16,
    dl: &crate::app::lens::DisplayLine,
    pal: &Palette,
) {
    let chars: Vec<char> = dl.text.chars().collect();
    let (start, len) = dl.emph.unwrap_or((0, 0));

    for (i, &ch) in chars.iter().enumerate() {
        if i as u16 >= w {
            break;
        }
        let mut style = match dl.kind {
            LineKind::Add => pal.s_added(),
            LineKind::Del => pal.s_removed(),
            LineKind::Context => pal.s_text(),
        };
        // intraline emphasis: the differing interior is underlined
        if len > 0 && i >= start && i < start + len {
            style = style.underline();
        }
        // syntax-lite on all lines
        if let Some(k) = syntax_kind_at(&chars, i, dl.text.as_str()) {
            style = match k {
                SyntaxKind::Comment => style.dim(),
                SyntaxKind::String => Style::new().fg(Color::Ansi256(150)).overlay(style),
                SyntaxKind::Keyword => style.bold(),
                SyntaxKind::Number => Style::new().fg(Color::Ansi256(179)).overlay(style),
            };
        }
        let gx = x + i as u16;
        if gx < s.width {
            let mut tmp = [0u8; 4];
            s.set_cell(
                gx,
                y,
                Cell::new(Glyph::new(ch.encode_utf8(&mut tmp)), style),
            );
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SyntaxKind {
    Comment,
    String,
    Keyword,
    Number,
}

const KEYWORDS: &[&str] = &[
    "fn", "pub", "let", "const", "mut", "return", "if", "else", "match", "struct", "impl", "use",
    "mod", "enum", "trait", "for", "while", "loop", "self", "Self", "crate", "type", "where",
    "async", "await", "move", "dyn", "unsafe", "extern", "as", "in", "break", "continue", "true",
    "false", "None", "Some", "Ok", "Err", "def", "class", "import", "from", "public", "private",
    "static", "void", "int", "new", "null", "nil",
];

/// Classify the character at position `i` in `chars` by a tiny scanner.
/// Deliberately shallow: whole-line and trailing comments, strings on this
/// line only, a keyword list, leading-digit numbers.
fn syntax_kind_at(chars: &[char], i: usize, _line: &str) -> Option<SyntaxKind> {
    let text: String = chars.iter().collect();
    let trimmed = text.trim_start();
    let lead_ws = chars.len() - trimmed.len();

    // whole-line comments (// # --)
    let line_comment =
        trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("--");
    if line_comment && i >= lead_ws {
        return Some(SyntaxKind::Comment);
    }
    // trailing // comment
    if let Some(pos) = trimmed.find("//") {
        if i >= lead_ws + pos {
            return Some(SyntaxKind::Comment);
        }
    }
    // strings: toggle across unescaped quotes up to i
    let mut in_str = false;
    let mut str_start = 0usize;
    let mut prev_esc = false;
    for (j, &c) in chars.iter().enumerate() {
        if j > i {
            break;
        }
        if c == '"' && !prev_esc {
            in_str = !in_str;
            str_start = j;
        }
        prev_esc = c == '\\' && !prev_esc;
    }
    if in_str && i > str_start {
        return Some(SyntaxKind::String);
    }
    // keyword: the word containing i
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    if !is_word(chars[i]) {
        return None;
    }
    let a = (0..=i)
        .rev()
        .find(|&j| j == 0 || !is_word(chars[j - 1]))
        .unwrap_or(0);
    let b = (i..chars.len())
        .find(|&j| !is_word(chars[j]))
        .unwrap_or(chars.len());
    let word: String = chars[a..b].iter().collect();
    if KEYWORDS.contains(&word.as_str()) {
        return Some(SyntaxKind::Keyword);
    }
    if word
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        return Some(SyntaxKind::Number);
    }
    None
}
