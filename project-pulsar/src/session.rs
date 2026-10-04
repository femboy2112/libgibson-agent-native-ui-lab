//! Application state, commands and exact replay.
//!
//! The whole interactive state is a small value (`Model`) that changes **only**
//! through `Model::apply(Cmd)`. Time is explicit state — a sample index, never a
//! wall-clock read — so:
//!
//! * `same seed + same time + same view ⇒ same frame` (everything downstream is a
//!   pure function of `Model`);
//! * direct seek and running forward are the same thing (`Seek(t)` ≡ many `Step`s);
//! * a session is exactly reproducible from its command log (`Session::replay`),
//!   because wall-clock only ever enters as an already-quantised `Advance(samples)`
//!   command that is itself recorded.

use crate::analysis::{Checkpoint, Engine, Stage};
use crate::scenario::{SigId, CHUNK, FS, N_TOTAL};
use gibson::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum View {
    Stream,
    Spectrum,
    Fold,
    Relation,
    Sky,
    Dossier,
}

impl View {
    pub const ALL: [View; 6] = [
        View::Stream,
        View::Spectrum,
        View::Fold,
        View::Relation,
        View::Sky,
        View::Dossier,
    ];
    pub fn idx(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        ["STREAM", "SPECTRUM", "FOLD", "RELATION", "SKY", "DOSSIER"][self.idx()]
    }
    pub fn blurb(self) -> &'static str {
        [
            "voltage vs time",
            "power vs frequency",
            "phase-folded structure",
            "measured quantities",
            "inferred source geometry",
            "interpretation",
        ][self.idx()]
    }
    pub fn key(self) -> char {
        (b'1' + self.idx() as u8) as char
    }
    pub fn parse(s: &str) -> Option<View> {
        let s = s.to_ascii_lowercase();
        View::ALL.into_iter().find(|v| {
            v.name().to_ascii_lowercase() == s || v.key().to_string() == s
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Focus {
    /// All identities, full range.
    Overview,
    /// The selected identity, zoomed to its own feature.
    Signal,
}

pub const SPEEDS: [u32; 5] = [1, 4, 16, 64, 256];

/// A user-level command. Every state change goes through one of these.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    /// Absolute seek to a sample index (clamped).
    Seek(usize),
    /// Relative step in samples.
    Step(i64),
    /// Playback advance (already quantised to samples by the caller).
    Advance(u32),
    SetView(View),
    Select(SigId),
    CycleSelect(i8),
    ToggleFocus,
    Zoom(i8),
    /// Pin the current time as the "earlier" state for comparison.
    PinHere,
    /// Pin an explicit sample index.
    PinAt(usize),
    ToggleCompare,
    CursorMove(i8, i8),
    ToggleCursor,
    ToggleTruth,
    TogglePlay,
    Speed(i8),
    /// Jump to the previous (-1) / next (+1) analysis event.
    JumpEvent(i8),
    GotoOpen,
    GotoChar(char),
    GotoBackspace,
    GotoSubmit,
    GotoCancel,
    ToggleHelp,
    /// One animation frame elapsed (view dissolve).
    Frame(u32),
    Quit,
}

/// Number of animation frames in a view-to-view dissolve.
pub const XFADE_FRAMES: u8 = 9;

/// The complete, comparable interactive state (everything except the engine).
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub seed: u64,
    /// "Now", in samples received so far.
    pub n: usize,
    pub view: View,
    pub selected: SigId,
    pub focus: Focus,
    pub zoom: u8,
    pub pin: Option<usize>,
    pub compare: bool,
    /// Inspect cursor in normalised hero coordinates (cell centres), if enabled.
    pub cursor: Option<(f32, f32)>,
    pub truth: bool,
    pub playing: bool,
    pub speed: u8,
    pub goto: Option<String>,
    pub help: bool,
    pub quit: bool,
    /// View we are dissolving away from, and the frames elapsed.
    pub xfade: Option<(View, u8)>,
}

#[derive(Clone)]
pub struct Model {
    pub st: State,
    pub engine: Arc<Engine>,
}

impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Model({:?})", self.st)
    }
}

pub fn secs_to_samples(t: f64) -> usize {
    if !t.is_finite() {
        return 0;
    }
    ((t.max(0.0) * FS).round() as usize).min(N_TOTAL)
}

impl Model {
    pub fn new(seed: u64) -> Model {
        Model::with_engine(Engine::shared(seed))
    }

    pub fn with_engine(engine: Arc<Engine>) -> Model {
        let seed = engine.seed();
        Model {
            st: State {
                seed,
                n: 0,
                view: View::Stream,
                selected: SigId::Alpha,
                focus: Focus::Overview,
                zoom: 0,
                pin: None,
                compare: false,
                cursor: None,
                truth: false,
                playing: false,
                speed: 1,
                goto: None,
                help: false,
                quit: false,
                xfade: None,
            },
            engine,
        }
    }

    /// Current time in seconds.
    pub fn t(&self) -> f64 {
        self.st.n as f64 / FS
    }

    /// Analysis checkpoint available at "now" (lags the stream by < one chunk).
    pub fn cp(&self) -> Arc<Checkpoint> {
        self.engine.checkpoint_at_sample(self.st.n)
    }

    /// Checkpoint at the pinned earlier time, if any.
    pub fn cp_pin(&self) -> Option<Arc<Checkpoint>> {
        self.st.pin.map(|n| self.engine.checkpoint_at_sample(n))
    }

    /// A sensible earlier state to compare against: when the selected identity
    /// first became a candidate, or a third of the way in.
    pub fn default_pin(&self) -> usize {
        let cp = self.cp();
        match cp.signal(self.st.selected).cand_cp {
            Some(k) => k * CHUNK,
            None => (self.st.n / 3 / CHUNK).max(1) * CHUNK,
        }
    }

    pub fn apply(&mut self, cmd: Cmd) {
        let st = &mut self.st;
        let set_view = |st: &mut State, v: View| {
            if st.view != v {
                st.xfade = Some((st.view, 0));
                st.view = v;
            }
        };
        match cmd {
            Cmd::Seek(n) => {
                st.n = n.min(N_TOTAL);
                st.playing = st.playing && st.n < N_TOTAL;
            }
            Cmd::Step(d) => {
                let n = (st.n as i64 + d).clamp(0, N_TOTAL as i64);
                st.n = n as usize;
                st.playing = st.playing && st.n < N_TOTAL;
            }
            Cmd::Advance(k) => {
                st.n = (st.n + k as usize).min(N_TOTAL);
                if st.n >= N_TOTAL {
                    st.playing = false;
                }
            }
            Cmd::SetView(v) => set_view(st, v),
            Cmd::Select(id) => st.selected = id,
            Cmd::CycleSelect(d) => {
                let i = (st.selected.idx() as i64 + d as i64).rem_euclid(3) as usize;
                st.selected = SigId::from_idx(i);
            }
            Cmd::ToggleFocus => {
                st.focus = match st.focus {
                    Focus::Overview => Focus::Signal,
                    Focus::Signal => Focus::Overview,
                };
                st.zoom = 0;
            }
            Cmd::Zoom(d) => {
                st.zoom = (st.zoom as i16 + d as i16).clamp(0, 2) as u8;
                if st.zoom > 0 {
                    st.focus = Focus::Signal;
                }
            }
            Cmd::PinHere => st.pin = Some(st.n),
            Cmd::PinAt(n) => st.pin = Some(n.min(N_TOTAL)),
            Cmd::ToggleCompare => {
                if !st.compare && st.pin.is_none() {
                    let p = self.default_pin();
                    self.st.pin = Some(p);
                }
                self.st.compare = !self.st.compare;
            }
            Cmd::CursorMove(dx, dy) => {
                let (u, v) = st.cursor.unwrap_or((0.5, 0.5));
                st.cursor = Some((
                    (u + dx as f32 * 0.025).clamp(0.0, 1.0),
                    (v + dy as f32 * 0.04).clamp(0.0, 1.0),
                ));
            }
            Cmd::ToggleCursor => {
                st.cursor = match st.cursor {
                    Some(_) => None,
                    None => Some((0.5, 0.5)),
                }
            }
            Cmd::ToggleTruth => st.truth = !st.truth,
            Cmd::TogglePlay => {
                if st.n >= N_TOTAL {
                    st.n = 0;
                }
                st.playing = !st.playing;
            }
            Cmd::Speed(d) => {
                st.speed = (st.speed as i16 + d as i16).clamp(0, SPEEDS.len() as i16 - 1) as u8;
            }
            Cmd::JumpEvent(dir) => {
                let cp = self.engine.checkpoint(crate::scenario::N_CP);
                let now_cp = self.st.n / CHUNK;
                let mut times: Vec<usize> = cp.events.iter().map(|e| e.cp).collect();
                times.sort_unstable();
                times.dedup();
                let target = if dir >= 0 {
                    times.into_iter().find(|&c| c > now_cp)
                } else {
                    times.into_iter().rev().find(|&c| c < now_cp)
                };
                if let Some(c) = target {
                    self.st.n = c * CHUNK;
                }
            }
            Cmd::GotoOpen => st.goto = Some(String::new()),
            Cmd::GotoChar(c) => {
                if let Some(b) = st.goto.as_mut() {
                    if (c.is_ascii_digit() || c == '.') && b.len() < 7 {
                        b.push(c);
                    }
                }
            }
            Cmd::GotoBackspace => {
                if let Some(b) = st.goto.as_mut() {
                    b.pop();
                }
            }
            Cmd::GotoSubmit => {
                if let Some(b) = st.goto.take() {
                    if let Ok(t) = b.parse::<f64>() {
                        st.n = secs_to_samples(t);
                    }
                }
            }
            Cmd::GotoCancel => st.goto = None,
            Cmd::ToggleHelp => st.help = !st.help,
            Cmd::Frame(k) => {
                if let Some((from, f)) = st.xfade {
                    let nf = f.saturating_add(k.min(255) as u8);
                    st.xfade = if nf >= XFADE_FRAMES { None } else { Some((from, nf)) };
                }
            }
            Cmd::Quit => st.quit = true,
        }
    }

    /// Status of each identity at "now" (for chrome).
    pub fn stages(&self) -> [Stage; 3] {
        let cp = self.cp();
        [
            cp.signals[0].stage,
            cp.signals[1].stage,
            cp.signals[2].stage,
        ]
    }
}

/// Translate a key press into a command. Depends on the model only for modal input
/// (the goto prompt owns the keyboard while open).
pub fn cmd_for_key(st: &State, key: KeyEvent) -> Option<Cmd> {
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    if st.goto.is_some() {
        return match key.code {
            KeyCode::Char(c) => Some(Cmd::GotoChar(c)),
            KeyCode::Backspace => Some(Cmd::GotoBackspace),
            KeyCode::Enter => Some(Cmd::GotoSubmit),
            KeyCode::Esc => Some(Cmd::GotoCancel),
            _ => None,
        };
    }
    let secs = |s: f64| (s * FS).round() as i64;
    Some(match key.code {
        KeyCode::Char('q') | KeyCode::Esc if !st.help => Cmd::Quit,
        KeyCode::Esc | KeyCode::Char('?') => Cmd::ToggleHelp,
        KeyCode::Char(c @ '1'..='6') => Cmd::SetView(View::ALL[(c as u8 - b'1') as usize]),
        KeyCode::Right => Cmd::Step(if shift { secs(1.0) } else { CHUNK as i64 }),
        KeyCode::Left => Cmd::Step(-(if shift { secs(1.0) } else { CHUNK as i64 })),
        KeyCode::Char('.') | KeyCode::Char('>') => Cmd::Step(secs(1.0)),
        KeyCode::Char(',') | KeyCode::Char('<') => Cmd::Step(-secs(1.0)),
        KeyCode::PageDown => Cmd::Step(secs(60.0)),
        KeyCode::PageUp => Cmd::Step(-secs(60.0)),
        KeyCode::Home => Cmd::Seek(0),
        KeyCode::End => Cmd::Seek(N_TOTAL),
        KeyCode::Tab => Cmd::CycleSelect(1),
        KeyCode::BackTab => Cmd::CycleSelect(-1),
        KeyCode::Char('a') => Cmd::Select(SigId::Alpha),
        KeyCode::Char('b') => Cmd::Select(SigId::Beta),
        KeyCode::Char('c') => Cmd::Select(SigId::Gamma),
        KeyCode::Char('f') => Cmd::ToggleFocus,
        KeyCode::Char('+') | KeyCode::Char('=') => Cmd::Zoom(1),
        KeyCode::Char('-') | KeyCode::Char('_') => Cmd::Zoom(-1),
        KeyCode::Char('p') => Cmd::PinHere,
        KeyCode::Char('m') => Cmd::ToggleCompare,
        KeyCode::Char('i') => Cmd::ToggleCursor,
        KeyCode::Char('h') => Cmd::CursorMove(-1, 0),
        KeyCode::Char('l') => Cmd::CursorMove(1, 0),
        KeyCode::Char('k') => Cmd::CursorMove(0, -1),
        KeyCode::Char('j') => Cmd::CursorMove(0, 1),
        KeyCode::Char('t') => Cmd::ToggleTruth,
        KeyCode::Char(' ') => Cmd::TogglePlay,
        KeyCode::Char(']') => Cmd::Speed(1),
        KeyCode::Char('[') => Cmd::Speed(-1),
        KeyCode::Char('n') => Cmd::JumpEvent(1),
        KeyCode::Char('N') => Cmd::JumpEvent(-1),
        KeyCode::Char('g') => Cmd::GotoOpen,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// text script / replay log
// ---------------------------------------------------------------------------

/// Render a command as one line of the replay-log language.
pub fn cmd_to_line(c: &Cmd) -> String {
    match c {
        Cmd::Seek(n) => format!("seek {n}"),
        Cmd::Step(d) => format!("step {d}"),
        Cmd::Advance(k) => format!("advance {k}"),
        Cmd::SetView(v) => format!("view {}", v.name().to_ascii_lowercase()),
        Cmd::Select(id) => format!("select {}", id.name().to_ascii_lowercase()),
        Cmd::CycleSelect(d) => format!("cycle {d}"),
        Cmd::ToggleFocus => "focus".into(),
        Cmd::Zoom(d) => format!("zoom {d}"),
        Cmd::PinHere => "pin".into(),
        Cmd::PinAt(n) => format!("pinat {n}"),
        Cmd::ToggleCompare => "compare".into(),
        Cmd::CursorMove(x, y) => format!("cursor {x} {y}"),
        Cmd::ToggleCursor => "inspect".into(),
        Cmd::ToggleTruth => "truth".into(),
        Cmd::TogglePlay => "play".into(),
        Cmd::Speed(d) => format!("speed {d}"),
        Cmd::JumpEvent(d) => format!("event {d}"),
        Cmd::GotoOpen => "goto-open".into(),
        Cmd::GotoChar(c) => format!("goto-char {c}"),
        Cmd::GotoBackspace => "goto-bs".into(),
        Cmd::GotoSubmit => "goto-go".into(),
        Cmd::GotoCancel => "goto-cancel".into(),
        Cmd::ToggleHelp => "help".into(),
        Cmd::Frame(k) => format!("frame {k}"),
        Cmd::Quit => "quit".into(),
    }
}

/// Parse one line of the script/log language. Blank lines and `#` comments are `Ok(None)`.
/// Human-friendly forms (`t=120`, `seek 120s`) are accepted in addition to the exact log forms.
pub fn parse_line(line: &str) -> Result<Option<Cmd>, String> {
    let line = line.split('#').next().unwrap_or("").trim();
    if line.is_empty() {
        return Ok(None);
    }
    let mut it = line.split_whitespace();
    let head = it.next().unwrap_or("");
    let arg = it.next();
    let arg2 = it.next();
    let num = |a: Option<&str>| -> Result<i64, String> {
        a.ok_or_else(|| format!("`{head}` needs a number"))?
            .parse::<i64>()
            .map_err(|e| format!("bad number in `{line}`: {e}"))
    };
    // seconds argument: "120", "120.5", "120s"
    let secs = |a: Option<&str>| -> Result<usize, String> {
        let a = a.ok_or_else(|| format!("`{head}` needs a time"))?;
        let a = a.trim_end_matches('s');
        a.parse::<f64>()
            .map(secs_to_samples)
            .map_err(|e| format!("bad time in `{line}`: {e}"))
    };
    let c = match head {
        "seek" => {
            // `seek N` = samples (exact log form); `seek 12.5s` / `at 12.5` = seconds
            match arg {
                Some(a) if a.ends_with('s') || a.contains('.') => Cmd::Seek(secs(arg)?),
                _ => Cmd::Seek(num(arg)?.max(0) as usize),
            }
        }
        "at" => Cmd::Seek(secs(arg)?),
        "step" => Cmd::Step(num(arg)?),
        "advance" => Cmd::Advance(num(arg)?.max(0) as u32),
        "view" => Cmd::SetView(
            View::parse(arg.unwrap_or("")).ok_or_else(|| format!("unknown view in `{line}`"))?,
        ),
        "select" => Cmd::Select(match arg.map(|a| a.to_ascii_lowercase()).as_deref() {
            Some("alpha") | Some("a") => SigId::Alpha,
            Some("beta") | Some("b") => SigId::Beta,
            Some("gamma") | Some("c") => SigId::Gamma,
            _ => return Err(format!("unknown identity in `{line}`")),
        }),
        "cycle" => Cmd::CycleSelect(num(arg)? as i8),
        "focus" => Cmd::ToggleFocus,
        "zoom" => Cmd::Zoom(num(arg)? as i8),
        "pin" => Cmd::PinHere,
        "pinat" => Cmd::PinAt(num(arg)?.max(0) as usize),
        "pin-at" => Cmd::PinAt(secs(arg)?),
        "compare" => Cmd::ToggleCompare,
        "cursor" => Cmd::CursorMove(num(arg)? as i8, num(arg2)? as i8),
        "inspect" => Cmd::ToggleCursor,
        "truth" => Cmd::ToggleTruth,
        "play" => Cmd::TogglePlay,
        "speed" => Cmd::Speed(num(arg)? as i8),
        "event" => Cmd::JumpEvent(num(arg)? as i8),
        "goto-open" => Cmd::GotoOpen,
        "goto-char" => Cmd::GotoChar(
            arg.and_then(|a| a.chars().next())
                .ok_or("goto-char needs a char")?,
        ),
        "goto-bs" => Cmd::GotoBackspace,
        "goto-go" => Cmd::GotoSubmit,
        "goto-cancel" => Cmd::GotoCancel,
        "help" => Cmd::ToggleHelp,
        "frame" => Cmd::Frame(num(arg)?.max(0) as u32),
        "quit" => Cmd::Quit,
        _ => return Err(format!("unknown command `{line}`")),
    };
    Ok(Some(c))
}

pub fn parse_script(src: &str) -> Result<Vec<Cmd>, String> {
    let mut out = Vec::new();
    for (i, line) in src.lines().enumerate() {
        for part in line.split(';') {
            match parse_line(part) {
                Ok(Some(c)) => out.push(c),
                Ok(None) => {}
                Err(e) => return Err(format!("line {}: {e}", i + 1)),
            }
        }
    }
    Ok(out)
}

/// A live session: the model plus the exact log of commands applied to it.
pub struct Session {
    pub model: Model,
    pub log: Vec<Cmd>,
}

impl Session {
    pub fn new(seed: u64) -> Session {
        Session {
            model: Model::new(seed),
            log: Vec::new(),
        }
    }

    pub fn apply(&mut self, c: Cmd) {
        // Fold consecutive Advance/Frame into one record so long sessions stay small.
        match (&c, self.log.last_mut()) {
            (Cmd::Advance(k), Some(Cmd::Advance(p))) => *p = p.saturating_add(*k),
            (Cmd::Frame(k), Some(Cmd::Frame(p))) => *p = p.saturating_add(*k),
            _ => self.log.push(c.clone()),
        }
        self.model.apply(c);
    }

    /// Rebuild a session from its log: the state it reaches is, by construction,
    /// identical to the original (see tests).
    pub fn replay(seed: u64, log: &[Cmd]) -> Session {
        let mut s = Session::new(seed);
        for c in log {
            s.apply(c.clone());
        }
        s
    }

    pub fn log_text(&self) -> String {
        let mut out = format!("# project-pulsar replay log\n# seed {}\n", self.model.st.seed);
        for c in &self.log {
            out.push_str(&cmd_to_line(c));
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_roundtrip() {
        let cmds = vec![
            Cmd::Seek(1234),
            Cmd::SetView(View::Sky),
            Cmd::Select(SigId::Gamma),
            Cmd::Zoom(1),
            Cmd::CursorMove(1, -1),
            Cmd::Step(-32),
            Cmd::Advance(77),
            Cmd::ToggleCompare,
        ];
        let text: String = cmds.iter().map(|c| cmd_to_line(c) + "\n").collect();
        assert_eq!(parse_script(&text).unwrap(), cmds);
    }

    #[test]
    fn friendly_forms() {
        assert_eq!(parse_line("at 120").unwrap(), Some(Cmd::Seek(3840)));
        assert_eq!(parse_line("seek 12.5s").unwrap(), Some(Cmd::Seek(400)));
        assert!(parse_line("bogus").is_err());
        assert_eq!(parse_line("# c").unwrap(), None);
    }
}
