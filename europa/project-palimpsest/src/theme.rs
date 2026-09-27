//! PALIMPSEST visual language.
//!
//! An *engineering atlas* / *historical manuscript* / *graph-analysis instrument*.
//! SWISS_SIGNAL editorial discipline: meaning is carried first by shape, lane,
//! marker, hierarchy, spacing and rules; color is an enhancement that never
//! carries meaning alone, so `--mono` remains fully usable.

use gibson::cell::{Color, Style};

/// Semantic palette. Every role degrades to attributes-only in mono mode.
#[derive(Clone, Copy)]
pub struct Palette {
    pub text: Color,
    pub muted: Color,
    pub faint: Color,
    pub border: Color,
    pub accent: Color,
    pub selection: Color,
    pub added: Color,
    pub removed: Color,
    pub merge: Color,
    pub tag: Color,
    pub head: Color,
    pub author: Color,
    pub warning: Color,
    /// Braided branch-lane hues, indexed by lane.
    pub lanes: [Color; 8],
}

impl Palette {
    /// Full-color editorial palette (256-color space, degrades gracefully).
    pub fn color() -> Self {
        Self {
            text: Color::Reset,
            muted: Color::Ansi256(245),
            faint: Color::Ansi256(240),
            border: Color::Ansi256(237),
            accent: Color::Ansi256(80),
            selection: Color::Ansi256(214),
            added: Color::Ansi256(71),
            removed: Color::Ansi256(167),
            merge: Color::Ansi256(140),
            tag: Color::Ansi256(179),
            head: Color::Ansi256(51),
            author: Color::Ansi256(117),
            warning: Color::Ansi256(178),
            lanes: [
                Color::Ansi256(80),  // teal
                Color::Ansi256(214), // gold
                Color::Ansi256(111), // periwinkle
                Color::Ansi256(150), // sage
                Color::Ansi256(176), // orchid
                Color::Ansi256(173), // rose
                Color::Ansi256(216), // apricot
                Color::Ansi256(67),  // indigo
            ],
        }
    }

    /// ANSI-16 palette for constrained terminals.
    pub fn ansi16() -> Self {
        Self {
            text: Color::Reset,
            muted: Color::BrightBlack,
            faint: Color::BrightBlack,
            border: Color::BrightBlack,
            accent: Color::BrightCyan,
            selection: Color::Yellow,
            added: Color::Green,
            removed: Color::Red,
            merge: Color::Magenta,
            tag: Color::Yellow,
            head: Color::BrightCyan,
            author: Color::Blue,
            warning: Color::Yellow,
            lanes: [
                Color::BrightCyan,
                Color::Yellow,
                Color::BrightBlue,
                Color::BrightGreen,
                Color::BrightMagenta,
                Color::Red,
                Color::Blue,
                Color::Green,
            ],
        }
    }

    /// Monochrome: shape-only grammar. No foreground colors at all.
    pub fn mono() -> Self {
        let none = Color::Reset;
        Self {
            text: none,
            muted: none,
            faint: none,
            border: none,
            accent: none,
            selection: none,
            added: none,
            removed: none,
            merge: none,
            tag: none,
            head: none,
            author: none,
            warning: none,
            lanes: [none; 8],
        }
    }

    #[inline]
    pub fn lane(&self, lane: usize) -> Color {
        self.lanes[lane % self.lanes.len()]
    }

    // ---- style roles ------------------------------------------------------

    pub fn s_text(&self) -> Style {
        Style::new().fg(self.text)
    }
    pub fn s_muted(&self) -> Style {
        Style::new().fg(self.muted)
    }
    pub fn s_faint(&self) -> Style {
        Style::new().fg(self.faint)
    }
    pub fn s_border(&self) -> Style {
        Style::new().fg(self.border)
    }
    pub fn s_accent(&self) -> Style {
        Style::new().fg(self.accent).bold()
    }
    pub fn s_head(&self) -> Style {
        Style::new().fg(self.head).bold()
    }
    pub fn s_added(&self) -> Style {
        Style::new().fg(self.added)
    }
    pub fn s_removed(&self) -> Style {
        Style::new().fg(self.removed)
    }
    pub fn s_merge(&self) -> Style {
        Style::new().fg(self.merge).bold()
    }
    pub fn s_author(&self) -> Style {
        Style::new().fg(self.author)
    }
    pub fn s_tag(&self) -> Style {
        Style::new().fg(self.tag).bold()
    }
    pub fn s_selection(&self) -> Style {
        Style::new().fg(self.selection).bold()
    }
    pub fn s_warning(&self) -> Style {
        Style::new().fg(self.warning).bold()
    }
    /// Reverse-video bar (header, selection row). Works in mono.
    pub fn s_bar(&self) -> Style {
        Style::new().reverse()
    }
    pub fn s_bar_accent(&self) -> Style {
        Style::new().reverse().bold()
    }
    pub fn s_lane(&self, lane: usize) -> Style {
        Style::new().fg(self.lane(lane))
    }
    pub fn s_lane_bold(&self, lane: usize) -> Style {
        Style::new().fg(self.lane(lane)).bold()
    }
}

/// The topology glyph vocabulary.
///
/// In mono mode these shapes alone carry: normal / heavy / merge / root /
/// selected / search-hit, plus rail, join and strand characters.
pub mod glyphs {
    // commit nodes
    pub const NODE_ROOT: &str = "○";
    pub const NODE: &str = "●";
    pub const NODE_HEAVY: &str = "◆";
    pub const NODE_MERGE: &str = "◉";
    pub const NODE_SELECTED: &str = "◈";
    pub const NODE_SEARCH: &str = "◎";
    pub const NODE_SELECTED_SEARCH: &str = "◉";

    // selected-column pin
    pub const PIN: &str = "┊";

    // rails / lanes / joins
    pub const RAIL: &str = "─";
    pub const RAIL_ACTIVE: &str = "━";
    pub const RAIL_FADED: &str = "·";
    pub const JOG_N: &str = "┐";
    pub const JOG_S: &str = "┘";
    pub const VERT: &str = "│";
    pub const FORK_DOWN: &str = "┬";
    pub const FORK_UP: &str = "┴";
    pub const CROSS: &str = "┼";
    pub const LANE_TURN_LEFT: &str = "└";
    pub const LANE_TURN_RIGHT: &str = "┌";

    // anchors
    pub const TAG: &str = "◆";
    pub const TAG_HOLLOW: &str = "◇";
    pub const HEAD_MARK: &str = "▶";

    // strata (file world-lines)
    pub const STRAND: &str = "━";
    pub const STRAND_EVENT: &str = "│";
    pub const STRAND_RENAME: &str = "↗";
    pub const STRAND_HOT: &str = "▓";
    pub const STRAND_DORMANT: &str = "╌";

    // provenance fibers
    pub const FIBER: &str = "‖";
    pub const AGE_NEW: &str = "◆";
    pub const AGE_RECENT: &str = "◈";
    pub const AGE_OLD: &str = "◇";
    pub const AGE_ANCIENT: &str = "·";

    // density / scrub
    pub const DENSITY: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    pub const SCRUB_EDGE_L: &str = "❰";
    pub const SCRUB_EDGE_R: &str = "❱";
    pub const SCRUB_INSIDE: &str = "·";

    // diff lens
    pub const PLUS: &str = "+";
    pub const MINUS: &str = "−";

    // bars for metrics (eighths)
    pub const BAR_BLOCKS: [&str; 8] = [" ", "▏", "▎", "▍", "▌", "▊", "▉", "█"];

    // chrome
    pub const ELLIPSIS: &str = "…";
    pub const RULER_TICK: &str = "┬";
    pub const RULER_TICK_MINOR: &str = "╷";
    pub const RULER_LINE: &str = "─";
}

/// Clamp a string to `max` display columns, appending an ellipsis when cut.
/// Cluster-aware through `unicode_width` per `char`; combining marks are rare
/// in repo metadata and the renderer's cell model absorbs clusters safely.
///
/// ```
/// use palimpsest::theme::truncate;
/// assert_eq!(truncate("hello", 10), "hello");
/// assert_eq!(truncate("hello world", 8), "hello w…");
/// ```
pub fn truncate(s: &str, max: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    if width_of(s) <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    if max == 1 {
        return glyphs::ELLIPSIS.to_string();
    }
    let lim = max - 1;
    let mut out = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = ch.width().unwrap_or(0);
        if w + cw > lim {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push_str(glyphs::ELLIPSIS);
    out
}

/// Pad (or hard-clip) a string to exactly `w` display columns.
///
/// Unlike [`truncate`], no ellipsis is inserted: this is the tabular policy
/// where a fixed column grid must stay aligned. A wide glyph that cannot fit
/// the remaining columns is skipped and the cell space-filled, so the output
/// is *always* exactly `w` columns wide.
///
/// ```
/// use palimpsest::theme::pad_to;
/// assert_eq!(pad_to("ab", 5), "ab   ");
/// assert_eq!(pad_to("abcdef", 3), "abc");   // hard clip, no ellipsis
/// assert_eq!(pad_to("日本", 3), "日 ");     // CJK remainder space-filled
/// ```
pub fn pad_to(s: &str, w: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    let mut out = String::with_capacity(w);
    let mut width = 0usize;
    for ch in s.chars() {
        let cw = ch.width().unwrap_or(0);
        if width + cw > w {
            break;
        }
        out.push(ch);
        width += cw;
        if width == w {
            break;
        }
    }
    while width < w {
        out.push(' ');
        width += 1;
    }
    out
}

/// Right-align in `w` columns (hard-clip the tail if over — the same tabular
/// policy as [`pad_to`], mirrored).
///
/// ```
/// use palimpsest::theme::pad_left;
/// assert_eq!(pad_left("42", 5), "   42");
/// assert_eq!(pad_left("abcdef", 3), "abc");
/// ```
pub fn pad_left(s: &str, w: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    let sw = width_of(s);
    if sw <= w {
        let mut out = String::with_capacity(w);
        for _ in sw..w {
            out.push(' ');
        }
        out.push_str(s);
        return out;
    }
    // over budget: hard-clip the tail to fit, then left-pad the remainder
    let mut tail = String::new();
    let mut width = 0usize;
    for ch in s.chars() {
        let cw = ch.width().unwrap_or(0);
        if width + cw > w {
            break;
        }
        tail.push(ch);
        width += cw;
        if width == w {
            break;
        }
    }
    let mut out = String::with_capacity(w);
    for _ in width..w {
        out.push(' ');
    }
    out.push_str(&tail);
    out
}

/// Display width of a string.
pub fn width_of(s: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    UnicodeWidthStr::width(s)
}

/// Format a unix timestamp as `YYYY-MM-DD` (UTC, proleptic Gregorian).
pub fn fmt_date(ts: i64) -> String {
    let days = ts.div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// Format a unix timestamp as `YYYY-MM-DD HH:MM` (UTC).
pub fn fmt_datetime(ts: i64) -> String {
    let secs_of_day = ts.rem_euclid(86_400);
    let h = secs_of_day / 3600;
    let m = (secs_of_day % 3600) / 60;
    format!("{} {:02}:{:02}", fmt_date(ts), h, m)
}

/// Compact relative age: `2h`, `5d`, `3w`, `7mo`, `1y`.
pub fn fmt_age(ts: i64, now: i64) -> String {
    let d = now.saturating_sub(ts).max(0);
    let s = d;
    if s < 60 {
        format!("{}s", s)
    } else if s < 3600 {
        format!("{}m", s / 60)
    } else if s < 86_400 {
        format!("{}h", s / 3600)
    } else if s < 86_400 * 30 {
        format!("{}d", s / 86_400)
    } else if s < 86_400 * 365 {
        format!("{}mo", s / (86_400 * 30))
    } else {
        format!("{}y", s / (86_400 * 365))
    }
}

/// Howard Hinnant's days-to-civil conversion.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_appends_ellipsis_only_when_cutting() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello", 5), "hello");
        assert_eq!(truncate("hello", 4), "hel…");
        assert_eq!(truncate("hello", 1), "…");
        assert_eq!(truncate("hello", 0), "");
        assert_eq!(truncate("", 5), "");
    }

    #[test]
    fn truncate_counts_display_columns_not_chars() {
        // each CJK glyph is 2 columns; the ellipsis costs one, so a 4-col
        // budget holds only one glyph (2) + ellipsis (1) = 3 cols, never
        // exceeding the budget
        assert_eq!(truncate("日本語", 4), "日…");
        assert_eq!(truncate("日本語", 6), "日本語");
        assert_eq!(truncate("日本語", 5), "日本…");
        assert_eq!(truncate("日本語", 2), "…");
        // mixed scripts
        assert_eq!(truncate("a日b", 4), "a日b", "fits exactly: no ellipsis");
        assert_eq!(
            truncate("a日b", 3),
            "a…",
            "the ellipsis needs a column of its own"
        );
    }

    #[test]
    fn pad_to_is_exact_width_after_clipping() {
        for (input, w, want) in [
            ("ab", 5usize, "ab   "),
            ("abcdef", 3, "abc"),
            ("日本", 5, "日本 "),
            ("日本", 4, "日本"),
            ("日本", 3, "日 "), // ragged CJK remainder is space-filled
            ("", 2, "  "),
        ] {
            let got = pad_to(input, w);
            assert_eq!(got, want, "pad_to({:?}, {w})", input);
            assert_eq!(
                width_of(&got),
                w,
                "exact width for pad_to({:?}, {w})",
                input
            );
        }
    }

    #[test]
    fn pad_left_right_aligns() {
        assert_eq!(pad_left("42", 5), "   42");
        assert_eq!(pad_left("42", 2), "42");
        assert_eq!(pad_left("abcdef", 3), "abc", "hard clip, no ellipsis");
        assert_eq!(pad_left("日", 4), "  日");
        assert_eq!(
            width_of(&pad_left("日", 3)),
            3,
            "ragged CJK tail space-filled"
        );
    }

    #[test]
    fn civil_dates_cover_epoch_boundaries_and_leap_days() {
        assert_eq!(fmt_date(0), "1970-01-01");
        assert_eq!(fmt_date(951_782_400), "2000-02-29", "leap day");
        assert_eq!(fmt_date(1_583_020_800), "2020-03-01", "day after leap day");
        assert_eq!(fmt_date(1_704_067_200), "2024-01-01");
        assert_eq!(fmt_date(2_147_483_647), "2038-01-19");
        assert_eq!(fmt_date(-86_400), "1969-12-31", "pre-epoch");
        assert_eq!(fmt_datetime(1), "1970-01-01 00:00");
    }

    #[test]
    fn age_buckets_are_human_readable() {
        let now = 1_700_000_000;
        assert_eq!(fmt_age(now, now), "0s");
        assert_eq!(fmt_age(now - 59, now), "59s");
        assert_eq!(fmt_age(now - 120, now), "2m");
        assert_eq!(fmt_age(now - 7_200, now), "2h");
        assert_eq!((fmt_age(now - 86_400 * 3, now)), "3d");
        assert_eq!(fmt_age(now - 86_400 * 45, now), "1mo");
        assert_eq!(fmt_age(now - 86_400 * 400, now), "1y");
    }

    #[test]
    fn width_of_matches_unicode_expectations() {
        assert_eq!(width_of("abc"), 3);
        assert_eq!(width_of("日本"), 4);
        assert_eq!(width_of(""), 0);
    }

    #[test]
    fn glyph_vocabulary_is_box_drawing_safe() {
        // The atlas grammar must be single-width everywhere so mono terminals
        // with ambiguous-width handling still align the braids.
        for g in [
            glyphs::NODE,
            glyphs::NODE_MERGE,
            glyphs::NODE_SELECTED,
            glyphs::PIN,
            glyphs::RAIL,
            glyphs::VERT,
            glyphs::TAG,
            glyphs::STRAND,
            glyphs::SCRUB_EDGE_L,
        ] {
            assert_eq!(width_of(g), 1, "glyph {g:?} must be exactly one column");
        }
    }
}
