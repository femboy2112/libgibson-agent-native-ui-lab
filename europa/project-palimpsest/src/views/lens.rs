//! DIFF LENS — a professional source/diff viewer.
//!
//! Left pane: the commit's file ledger (status, path, ±). Right pane: hunk
//! stream with line numbers, intraline emphasis, and light syntax awareness
//! (comments, strings, keywords, numbers) — readable, never pretending to be
//! a real parser. At narrow widths the file ledger collapses into a single
//! context strip.

use std::sync::Arc;

use gibson::cell::{Cell, Glyph};
use gibson::surface::Surface;

use crate::app::lens::flatten;
use crate::app::App;
use crate::git::diff::LineKind;
use crate::theme::{glyphs, truncate, Palette};
use crate::views::widgets::*;

/// The full lens surface for the current commit diff.
pub fn draw_lens(app: &App, w: u16, h: u16, pal: &Palette) -> Arc<Surface> {
    let mut s = Surface::new(w, h);
    let Some(diff) = &app.lens.diff else {
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
        draw_file_pane(&mut s, app, diff, files_w, h, pal);
    }

    // ---- hunk pane ----------------------------------------------------------------
    let hx = files_w;
    draw_hunk_pane(&mut s, app, diff, hx, hunk_w, h, pal);

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

    let inner_w = w.saturating_sub(3);
    let list_h = h.saturating_sub(1);
    let scroll = app.lens.file_scroll;
    let cursor = app.lens.file_cursor;
    // A diff query searches paths and hunks, but it never reorders the file
    // ledger. File focus therefore remains stable as hits are cycled.
    let files = &diff.files;

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
            'A' => pal.s_added(),
            'D' => pal.s_removed(),
            'R' => pal.s_merge(),
            _ => pal.s_muted(),
        };
        let mark = match f.status {
            'A' => glyphs::PLUS,
            'D' => glyphs::MINUS,
            'R' => "↗",
            _ => "·",
        };
        let counts = format!("+{:<3}−{:<3}", f.adds, f.dels);
        let cx = inner_w.saturating_sub(counts.len() as u16);
        // Path begins at column six. Reserve one blank cell before the
        // right-set counts (and another before the binary marker).
        let path_w = cx.saturating_sub(if f.binary { 11 } else { 7 });
        let path_disp = truncate(f.path(), path_w as usize);
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
    app: &App,
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

    // One bounded linear view of the commit, addressed by stable file/hunk
    // coordinates rather than by the position of a transient filter result.
    let max_lines = (h as usize).saturating_sub(1);
    let all = flatten(diff, 4096);

    let scroll = app.lens.hunk_scroll.min(all.len().saturating_sub(1));
    for (vi, dl) in all.iter().skip(scroll).take(max_lines).enumerate() {
        let y = y as usize + vi;
        if y >= h as usize {
            break;
        }
        let y = y as u16;
        if dl.is_meta() {
            let focused_hunk = dl.file_idx == app.lens.file_cursor
                && dl.hunk_idx == app.lens.hunk_cursor
                && dl.text.starts_with("  @@");
            let style = if focused_hunk {
                pal.s_selection()
            } else if dl.text.starts_with("──") {
                pal.s_accent()
            } else {
                pal.s_border()
            };
            let text = if focused_hunk {
                format!("◈ {}", dl.text.trim_start())
            } else {
                dl.text.clone()
            };
            s.print_str(
                x0,
                y,
                &truncate(&text, w.saturating_sub(2) as usize),
                style,
                Some(w.saturating_sub(2)),
            );
            continue;
        }
        // Explicit inter-column space keeps `5` and `pub fn` from reading as
        // one token, including in mono and at narrow widths.
        let gutter_w = 11;
        let body_x = x0 + gutter_w;
        let body_w = w.saturating_sub(gutter_w + 2);

        // gutter: sign + old/new line numbers, exactly `gutter_w` wide
        let old_s = dl
            .old_no
            .map(|n| format!("{:>4}", n))
            .unwrap_or_else(|| "    ".to_string());
        let new_s = dl
            .new_no
            .map(|n| format!("{:>4}", n))
            .unwrap_or_else(|| "    ".to_string());
        let gutter = format!("{}{}{}  ", sign_of(dl.kind), old_s, new_s);
        let g_style = match dl.kind {
            LineKind::Add => pal.s_added(),
            LineKind::Del => pal.s_removed(),
            LineKind::Context => pal.s_faint(),
        };
        s.print_str(x0, y, &gutter, g_style, Some(gutter_w));

        // body with syntax-lite emphasis and intraline emphasis
        paint_line(s, body_x, y, body_w, dl, pal, &app.lens.query);
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
                " {} hunks · n/N hunk · / search · m/M match · p provenance",
                f.hunks.len()
            )
        } else {
            format!(
                " match {}/{} for `{}` · m/M next/prev · / refine",
                if app.lens.matches.is_empty() {
                    0
                } else {
                    app.lens.match_cursor + 1
                },
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
    query: &str,
) {
    // Only the visible width is tokenized. A single enormous source line
    // should cost O(viewport), not O(file line length), on every frame.
    let chars: Vec<char> = dl.text.chars().take(w as usize).collect();
    let kinds = syntax_kinds(&chars);
    let (start, len) = dl.emph.unwrap_or((0, 0));
    let search_ranges = if query.is_empty() {
        Vec::new()
    } else {
        let hay = dl.text.to_lowercase();
        let needle = query.to_lowercase();
        hay.match_indices(&needle)
            .map(|(at, _)| (hay[..at].chars().count(), needle.chars().count()))
            .collect::<Vec<_>>()
    };

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
        if let Some(k) = kinds[i] {
            style = match k {
                SyntaxKind::Comment => style.dim(),
                SyntaxKind::String => pal.s_accent().overlay(style),
                SyntaxKind::Keyword => style.bold(),
                SyntaxKind::Number => pal.s_tag().overlay(style),
            };
        }
        if search_ranges.iter().any(|&(at, n)| i >= at && i < at + n) {
            style = style.reverse().bold();
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

#[derive(Clone, Copy, Debug, PartialEq)]
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

/// A single linear pass over the visible line. A `//` inside a quoted string
/// stays a string, and keywords are classified once per word.
fn syntax_kinds(chars: &[char]) -> Vec<Option<SyntaxKind>> {
    let mut kinds = vec![None; chars.len()];
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let lead = chars
        .iter()
        .position(|c| !c.is_whitespace())
        .unwrap_or(chars.len());
    let whole_comment = lead < chars.len()
        && (chars[lead] == '#'
            || (lead + 1 < chars.len()
                && matches!((chars[lead], chars[lead + 1]), ('/', '/') | ('-', '-'))));
    if whole_comment {
        kinds[lead..].fill(Some(SyntaxKind::Comment));
        return kinds;
    }
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '/' && chars.get(i + 1) == Some(&'/') {
            kinds[i..].fill(Some(SyntaxKind::Comment));
            break;
        }
        if chars[i] == '"' {
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 2;
                } else if chars[i] == '"' {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            kinds[start..i].fill(Some(SyntaxKind::String));
            continue;
        }
        if is_word(chars[i]) {
            let start = i;
            while i < chars.len() && is_word(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let kind = if KEYWORDS.contains(&word.as_str()) {
                Some(SyntaxKind::Keyword)
            } else if chars[start].is_ascii_digit() {
                Some(SyntaxKind::Number)
            } else {
                None
            };
            kinds[start..i].fill(kind);
            continue;
        }
        i += 1;
    }
    kinds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_scanner_does_not_misread_urls_as_comments() {
        let chars: Vec<char> = "let url = \"https://example.test\"; // note"
            .chars()
            .collect();
        let kinds = syntax_kinds(&chars);
        assert_eq!(kinds[0], Some(SyntaxKind::Keyword));
        let inner_slash = chars.iter().position(|&c| c == '/').unwrap();
        assert_eq!(kinds[inner_slash], Some(SyntaxKind::String));
        assert_eq!(*kinds.last().unwrap(), Some(SyntaxKind::Comment));
    }
}
