//! A bounded, editable four-shaft weaving draft and its interlaced cloth.
//! Geometry and loom semantics are consumer-owned; the frozen adapter owns I/O.
use crate::ui::{self, App};
use gibson::{
    BorderType, BrailleCanvas, Color, ColorDepth, KeyCode, KeyEvent, Rect, Style, Surface,
};

pub const REPEAT: usize = 16;
pub const ROW_MS: u64 = 600;
pub const WEAVE_MS: u64 = ROW_MS * REPEAT as u64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    /// One shaft per warp end, numbered 0..3.
    pub threading: [u8; REPEAT],
    /// One treadle per pick, numbered 0..3.
    pub treadling: [u8; REPEAT],
    /// Each treadle's four-bit set of lifted shafts.
    pub tie_up: [u8; 4],
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            threading: std::array::from_fn(|i| (i % 4) as u8),
            treadling: std::array::from_fn(|i| (i % 4) as u8),
            tie_up: [0b0011, 0b0110, 0b1100, 0b1001],
        }
    }
}

impl Draft {
    pub fn warp_above(&self, warp: usize, pick: usize) -> bool {
        let shaft = self.threading[warp % REPEAT] % 4;
        let treadle = self.treadling[pick % REPEAT] as usize % 4;
        self.tie_up[treadle] & (1 << shaft) != 0
    }

    pub fn cloth(&self) -> [[bool; REPEAT]; REPEAT] {
        std::array::from_fn(|pick| std::array::from_fn(|warp| self.warp_above(warp, pick)))
    }

    pub fn floats(&self) -> FloatSummary {
        let mut result = FloatSummary::default();
        for strand in 0..REPEAT {
            let warp = std::array::from_fn(|pick| self.warp_above(strand, pick));
            let weft = std::array::from_fn(|end| !self.warp_above(end, strand));
            result.warp = result.warp.max(cyclic_run(warp));
            result.weft = result.weft.max(cyclic_run(weft));
        }
        result
    }
}

/// Scan two repeats so a run crossing the boundary is counted; cap at one repeat.
fn cyclic_run(values: [bool; REPEAT]) -> u8 {
    let (mut best, mut run) = (0, 0);
    for value in values.into_iter().cycle().take(REPEAT * 2) {
        run = if value {
            (run + 1).min(REPEAT as u8)
        } else {
            0
        };
        best = best.max(run);
    }
    best
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FloatSummary {
    /// Saturated at one repeat: 16 means the strand stays above across repeats.
    pub warp: u8,
    /// Saturated at one repeat: 16 means the strand stays above across repeats.
    pub weft: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Focus {
    #[default]
    Cloth,
    Threading,
    TieUp,
    Treadling,
}

impl Focus {
    fn cycle(self, backwards: bool) -> Self {
        let index = self as usize;
        [Self::Cloth, Self::Threading, Self::TieUp, Self::Treadling]
            [(index + if backwards { 3 } else { 1 }) % 4]
    }

    fn name(self) -> &'static str {
        match self {
            Self::Cloth => "CLOTH",
            Self::Threading => "THREADING",
            Self::TieUp => "TIE-UP",
            Self::Treadling => "TREADLING",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub rows: usize,
    pub columns: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Weavebench {
    pub draft: Draft,
    pub focus: Focus,
    /// Display coordinates; repeated cells share the same underlying draft.
    pub selected: (usize, usize),
    /// Shaft column and treadle row of the tie-up editor.
    pub tie_cursor: (usize, usize),
    pub four_repeats: bool,
    pub follow_shuttle: bool,
    pub playing: bool,
    base_ms: u64,
    anchor_ms: u64,
}

impl Default for Weavebench {
    fn default() -> Self {
        Self {
            draft: Draft::default(),
            focus: Focus::Cloth,
            selected: (0, 0),
            tie_cursor: (0, 0),
            four_repeats: false,
            follow_shuttle: true,
            playing: true,
            base_ms: 0,
            anchor_ms: 0,
        }
    }
}

impl Weavebench {
    pub fn elapsed_ms(&self, at_ms: u64) -> u64 {
        self.base_ms
            .saturating_add(if self.playing {
                at_ms.saturating_sub(self.anchor_ms)
            } else {
                0
            })
            .min(WEAVE_MS)
    }

    pub fn progress(&self, at_ms: u64) -> Progress {
        let elapsed = self.elapsed_ms(at_ms);
        Progress {
            rows: (elapsed / ROW_MS) as usize,
            columns: ((elapsed % ROW_MS) * REPEAT as u64 / ROW_MS) as usize,
        }
    }

    fn extent(&self) -> usize {
        if self.four_repeats {
            REPEAT * 2
        } else {
            REPEAT
        }
    }

    fn move_selection(&mut self, dx: isize, dy: isize) {
        self.follow_shuttle = false;
        let extent = self.extent() as isize;
        self.selected.0 = (self.selected.0 as isize + dx).rem_euclid(extent) as usize;
        self.selected.1 = (self.selected.1 as isize + dy).rem_euclid(extent) as usize;
    }

    fn edit_assignment(&mut self, delta: u8) {
        let assignment = match self.focus {
            Focus::Threading => &mut self.draft.threading[self.selected.0 % REPEAT],
            Focus::Treadling => &mut self.draft.treadling[self.selected.1 % REPEAT],
            _ => return,
        };
        *assignment = (*assignment + delta) % 4;
    }

    fn draw_cloth(&self, out: &mut Surface, rect: Rect, at_ms: u64, mono: bool) {
        out.draw_border(rect, BorderType::Rounded, Style::new());
        label(out, rect.x + 2, rect.y, " CLOTH ", Style::new().bold());
        if rect.width < 6 || rect.height < 5 {
            return;
        }
        // Six by eight samples are the minimum that leaves BOTH lower-strand
        // ends visible around a gap wider than the shared intersection dot.
        let cell_w = if rect.width >= 70 { 4 } else { 3 };
        let cell_h = 2;
        let cols = ((rect.width - 2) / cell_w).min(self.extent() as u16) as usize;
        let rows = ((rect.height - 3) / cell_h).min(self.extent() as u16) as usize;
        if cols == 0 || rows == 0 {
            return;
        }
        let progress = self.progress(at_ms);
        let shuttle_col = if progress.rows.is_multiple_of(2) {
            progress.columns
        } else {
            REPEAT - 1 - progress.columns
        };
        let center = if self.follow_shuttle {
            (shuttle_col, progress.rows.min(REPEAT - 1))
        } else {
            self.selected
        };
        let start_x = center.0.saturating_sub(cols / 2).min(self.extent() - cols);
        let start_y = center.1.saturating_sub(rows / 2).min(self.extent() - rows);
        let mut canvas = BrailleCanvas::new(cols as u16 * cell_w, rows as u16 * cell_h);
        for row in 0..rows {
            for col in 0..cols {
                draw_crossing(
                    &mut canvas,
                    (
                        col as i32 * cell_w as i32 * 2,
                        row as i32 * cell_h as i32 * 4,
                    ),
                    (cell_w as i32 * 2, cell_h as i32 * 4),
                    self.draft.warp_above(start_x + col, start_y + row),
                );
            }
        }
        let mut cloth = canvas.to_surface(Style::new());
        for row in 0..rows {
            for col in 0..cols {
                let x = start_x + col;
                let y = start_y + row;
                let base_x = x % REPEAT;
                let base_y = y % REPEAT;
                let woven = base_y < progress.rows
                    || (base_y == progress.rows
                        && if base_y.is_multiple_of(2) {
                            base_x < progress.columns
                        } else {
                            base_x >= REPEAT - progress.columns
                        });
                let mut style = thread_style(self.draft.warp_above(x, y), mono);
                if !woven {
                    style = style.dim();
                }
                if (x, y) == self.selected {
                    style = style.reverse().bold();
                }
                for cy in 0..cell_h {
                    for cx in 0..cell_w {
                        if let Some(cell) =
                            cloth.get_mut(col as u16 * cell_w + cx, row as u16 * cell_h + cy)
                        {
                            cell.style = style;
                        }
                    }
                }
            }
        }
        out.blit_transparent_at(&cloth, rect.x + 1, rect.y + 1);
        let row = progress.rows;
        if row < REPEAT {
            let col = if row.is_multiple_of(2) {
                progress.columns
            } else {
                REPEAT - 1 - progress.columns
            };
            if col >= start_x && col < start_x + cols && row >= start_y && row < start_y + rows {
                label(
                    out,
                    rect.x + 1 + (col - start_x) as u16 * cell_w,
                    rect.y + 1 + (row - start_y) as u16 * cell_h,
                    if row.is_multiple_of(2) { ">" } else { "<" },
                    Style::new().bold().reverse(),
                );
            }
        }
        label(
            out,
            rect.x + 1,
            rect.y + rect.height - 2,
            &format!(
                "W{:02}-{:02} P{:02}-{:02} {}x repeat",
                start_x + 1,
                start_x + cols,
                start_y + 1,
                start_y + rows,
                if self.four_repeats { 4 } else { 1 }
            ),
            Style::new().dim(),
        );
    }

    fn draw_inspector(&self, out: &mut Surface, rect: Rect, mono: bool) {
        out.draw_border(rect, BorderType::Rounded, Style::new());
        label(out, rect.x + 2, rect.y, " CROSSING ", Style::new().bold());
        if rect.width < 16 || rect.height < 19 {
            return;
        }
        let x = self.selected.0 % REPEAT;
        let y = self.selected.1 % REPEAT;
        let top = self.draft.warp_above(x, y);
        let left = rect.x + 1;
        label(
            out,
            left,
            rect.y + 1,
            &format!("Warp {:02} / Pick {:02}", x + 1, y + 1),
            Style::new().bold(),
        );
        label(
            out,
            left,
            rect.y + 2,
            &format!(
                "S{} <- T{} : {}",
                self.draft.threading[x] + 1,
                self.draft.treadling[y] + 1,
                if top { "UP" } else { "DOWN" }
            ),
            Style::new(),
        );
        let mut crossing = BrailleCanvas::new(rect.width - 4, 5);
        let size = (
            crossing.pixel_width() as i32,
            crossing.pixel_height() as i32,
        );
        draw_crossing(&mut crossing, (0, 0), size, top);
        out.blit_transparent_at(
            &crossing.to_surface(thread_style(top, mono)),
            rect.x + 2,
            rect.y + 3,
        );
        label(
            out,
            left,
            rect.y + 8,
            if top {
                "WARP above weft"
            } else {
                "WEFT above warp"
            },
            Style::new().bold(),
        );
        let floats = self.draft.floats();
        label(
            out,
            left,
            rect.y + 9,
            &format!(
                "Max floats W{} F{}",
                float_label(floats.warp),
                float_label(floats.weft)
            ),
            Style::new(),
        );
        label(
            out,
            left,
            rect.y + 11,
            self.focus.name(),
            Style::new().reverse(),
        );
        match self.focus {
            Focus::Threading | Focus::Treadling => {
                let (values, current, word) = if self.focus == Focus::Threading {
                    (&self.draft.threading, x, "Shaft")
                } else {
                    (&self.draft.treadling, y, "Treadle")
                };
                for (i, value) in values.iter().enumerate() {
                    label(
                        out,
                        left + (i % 8) as u16 * 2,
                        rect.y + 13 + (i / 8) as u16 * 2,
                        &(value + 1).to_string(),
                        if i == current {
                            Style::new().reverse().bold()
                        } else {
                            Style::new()
                        },
                    );
                }
                label(
                    out,
                    left,
                    rect.y + 17,
                    &format!("{} {:02} = {}", word, current + 1, values[current] + 1),
                    Style::new(),
                );
            }
            _ => {
                label(
                    out,
                    left,
                    rect.y + 12,
                    "     S1 S2 S3 S4",
                    Style::new().dim(),
                );
                for treadle in 0..4 {
                    label(
                        out,
                        left,
                        rect.y + 13 + treadle as u16,
                        &format!("T{}", treadle + 1),
                        Style::new(),
                    );
                    for shaft in 0..4 {
                        let style =
                            if self.focus == Focus::TieUp && self.tie_cursor == (shaft, treadle) {
                                Style::new().reverse().bold()
                            } else {
                                Style::new()
                            };
                        label(
                            out,
                            left + 5 + shaft as u16 * 3,
                            rect.y + 13 + treadle as u16,
                            if self.draft.tie_up[treadle] & (1 << shaft) != 0 {
                                "X"
                            } else {
                                "."
                            },
                            style,
                        );
                    }
                }
                label(
                    out,
                    left,
                    rect.y + 17,
                    "X lifts; . leaves",
                    Style::new().dim(),
                );
            }
        }
    }
}

fn thread_style(warp: bool, mono: bool) -> Style {
    if mono {
        Style::new()
    } else {
        Style::new().fg(if warp {
            Color::Rgb(97, 208, 226)
        } else {
            Color::Rgb(244, 183, 94)
        })
    }
}

fn float_label(value: u8) -> String {
    if value == REPEAT as u8 {
        "16+".into()
    } else {
        value.to_string()
    }
}

fn label(out: &mut Surface, x: u16, y: u16, text: &str, style: Style) {
    ui::put(out, x, y, text, style);
}

/// The upper strand is unbroken; the lower strand has an actual geometric gap.
fn draw_crossing(canvas: &mut BrailleCanvas, origin: (i32, i32), size: (i32, i32), warp_top: bool) {
    let (w, h) = size;
    let (cx, cy) = (w / 2, h / 2);
    let x_gap = if w >= 12 {
        3
    } else if w >= 6 {
        1
    } else {
        0
    };
    let y_gap = if h >= 12 {
        3
    } else if h >= 6 {
        1
    } else {
        0
    };
    for x in 0..w {
        if !warp_top || (x - cx).abs() > x_gap {
            canvas.set(origin.0 + x, origin.1 + cy);
        }
    }
    for y in 0..h {
        if warp_top || (y - cy).abs() > y_gap {
            canvas.set(origin.0 + cx, origin.1 + y);
        }
    }
}

impl App for Weavebench {
    fn frame(&self, at_ms: u64, w: u16, h: u16, depth: ColorDepth) -> Surface {
        let (w, h) = ui::dimensions(w, h);
        let mut out = Surface::new(w, h);
        let progress = self.progress(at_ms);
        label(
            &mut out,
            0,
            0,
            &format!(
                "WEAVEBENCH / {}  woven {:02}/16 {}",
                self.focus.name(),
                progress.rows,
                if progress.rows == REPEAT {
                    "DONE"
                } else if self.playing {
                    "RUN"
                } else {
                    "PAUSE"
                }
            ),
            Style::new().bold(),
        );
        let help = match self.focus {
            Focus::Cloth => "f follows shuttle; arrows pin crossing; dim=draft",
            Focus::Threading => "Left/Right warp; Up/Down shaft; Enter cycles shaft",
            Focus::TieUp => "Arrows choose shaft/treadle; Enter toggles lift",
            Focus::Treadling => "Up/Down pick; Left/Right treadle; Enter cycles",
        };
        label(&mut out, 0, 1, help, Style::new().dim());
        if w >= 40 && h >= 23 {
            let inspector_w = if w >= 100 { 26 } else { 21 };
            let cloth_w = w - inspector_w - 1;
            self.draw_cloth(
                &mut out,
                Rect::new(0, 2, cloth_w, h - 4),
                at_ms,
                depth == ColorDepth::Mono,
            );
            self.draw_inspector(
                &mut out,
                Rect::new(cloth_w + 1, 2, inspector_w, h - 4),
                depth == ColorDepth::Mono,
            );
        } else {
            label(
                &mut out,
                0,
                3,
                "Expand to 56x24 for cloth + crossing inspector",
                Style::new(),
            );
        }
        if h >= 2 {
            label(
                &mut out,
                0,
                h - 2,
                "Tab focus  Arrows move  Enter edit  r repeats",
                Style::new(),
            );
            label(
                &mut out,
                0,
                h - 1,
                "w play/pause  n row  0 rewind  Space host pause",
                Style::new().dim(),
            );
        }
        ui::finish(out, depth)
    }

    fn key(&mut self, at_ms: u64, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => self.focus = self.focus.cycle(false),
            KeyCode::BackTab => self.focus = self.focus.cycle(true),
            KeyCode::Char('c') => self.focus = Focus::Cloth,
            KeyCode::Char('t') => self.focus = Focus::Threading,
            KeyCode::Char('u') => self.focus = Focus::TieUp,
            KeyCode::Char('p') => self.focus = Focus::Treadling,
            KeyCode::Char('f') => self.follow_shuttle = !self.follow_shuttle,
            KeyCode::Char('r') => {
                self.four_repeats = !self.four_repeats;
                self.selected.0 %= self.extent();
                self.selected.1 %= self.extent();
            }
            KeyCode::Char('w') => {
                self.base_ms = self.elapsed_ms(at_ms);
                self.anchor_ms = at_ms;
                self.playing = !self.playing;
            }
            KeyCode::Char('n') => {
                self.base_ms = ((self.elapsed_ms(at_ms) / ROW_MS + 1) * ROW_MS).min(WEAVE_MS);
                self.anchor_ms = at_ms;
                self.playing = false;
            }
            KeyCode::Char('0') => {
                self.base_ms = 0;
                self.anchor_ms = at_ms;
            }
            code => match self.focus {
                Focus::Cloth => match code {
                    KeyCode::Left => self.move_selection(-1, 0),
                    KeyCode::Right => self.move_selection(1, 0),
                    KeyCode::Up => self.move_selection(0, -1),
                    KeyCode::Down => self.move_selection(0, 1),
                    _ => {}
                },
                Focus::Threading => match code {
                    KeyCode::Left => self.move_selection(-1, 0),
                    KeyCode::Right => self.move_selection(1, 0),
                    KeyCode::Up | KeyCode::Enter => self.edit_assignment(1),
                    KeyCode::Down => self.edit_assignment(3),
                    _ => {}
                },
                Focus::Treadling => match code {
                    KeyCode::Up => self.move_selection(0, -1),
                    KeyCode::Down => self.move_selection(0, 1),
                    KeyCode::Right | KeyCode::Enter => self.edit_assignment(1),
                    KeyCode::Left => self.edit_assignment(3),
                    _ => {}
                },
                Focus::TieUp => match code {
                    KeyCode::Left => self.tie_cursor.0 = (self.tie_cursor.0 + 3) % 4,
                    KeyCode::Right => self.tie_cursor.0 = (self.tie_cursor.0 + 1) % 4,
                    KeyCode::Up => self.tie_cursor.1 = (self.tie_cursor.1 + 3) % 4,
                    KeyCode::Down => self.tie_cursor.1 = (self.tie_cursor.1 + 1) % 4,
                    KeyCode::Enter => {
                        self.draft.tie_up[self.tie_cursor.1] ^= 1 << self.tie_cursor.0
                    }
                    _ => {}
                },
            },
        }
    }

    fn end_ms(&self) -> u64 {
        18_000
    }
}
