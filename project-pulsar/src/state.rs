//! The interactive model. Time is EXPLICIT state: `frame = f(model)` and a
//! model is reproduced exactly by replaying its key log.

use std::sync::Arc;
use std::time::Duration;

use gibson::{KeyCode, KeyEvent, KeyModifiers};

use crate::observatory::Observatory;
use crate::sim::{DURATION, EPOCH_S, SLOTS};
use crate::track::{epoch_at, EventKind, State};
use crate::views::View;

/// Mission seconds advanced per real second while playing.
pub const SPEEDS: [f64; 6] = [2.0, 4.0, 8.0, 16.0, 32.0, 64.0];
pub const DEFAULT_SPEED: usize = 2;
/// Length (real s) of a view-change transport.
pub const TRANSPORT_S: f64 = 0.55;

#[derive(Clone)]
pub struct Model {
    pub obs: Arc<Observatory>,
    pub t: f64,
    pub view: View,
    /// Previous view and transport progress `0..1` while an identity transport runs.
    pub from: Option<View>,
    pub trans: f32,
    pub sel: Option<usize>,
    pub zoom: u8,
    pub nudge: i32,
    pub cursor: (f64, f64),
    pub cursor_on: bool,
    pub mark: Option<f64>,
    pub compare: bool,
    pub playing: bool,
    pub speed: usize,
    pub help: bool,
    pub quit: bool,
    clock: Option<Duration>,
    pub keylog: Vec<String>,
    t_after_key: f64,
}

impl Model {
    pub fn new(obs: Arc<Observatory>) -> Model {
        Model {
            obs,
            t: 0.0,
            view: View::Trace,
            from: None,
            trans: 1.0,
            sel: None,
            zoom: 0,
            nudge: 0,
            cursor: (0.5, 0.5),
            cursor_on: false,
            mark: None,
            compare: false,
            playing: false,
            speed: DEFAULT_SPEED,
            help: false,
            quit: false,
            clock: None,
            keylog: Vec::new(),
            t_after_key: 0.0,
        }
    }

    pub fn ep(&self) -> usize {
        epoch_at(self.t)
    }

    pub fn ref_ep(&self) -> Option<usize> {
        if !self.compare {
            return None;
        }
        let r = epoch_at(self.mark?);
        (r >= 1 && r < self.ep()).then_some(r)
    }

    pub fn set_view(&mut self, v: View) {
        if v != self.view {
            self.from = Some(self.view);
            self.trans = 0.0;
            self.view = v;
            self.cursor_on = false;
        }
    }

    pub fn seek(&mut self, t: f64) {
        if t.is_finite() {
            self.t = t.clamp(0.0, DURATION);
        }
    }

    fn active_slots(&self) -> Vec<usize> {
        match self.obs.timeline.epoch(self.ep()) {
            Some(e) => (0..SLOTS)
                .filter(|s| e.slots[*s].state.is_active())
                .collect(),
            None => Vec::new(),
        }
    }

    fn cycle_selection(&mut self, dir: i32) {
        let act = self.active_slots();
        if act.is_empty() {
            self.sel = None;
            return;
        }
        let cur = self.sel.and_then(|s| act.iter().position(|a| *a == s));
        let n = act.len() as i32;
        let next = match cur {
            Some(i) => (i as i32 + dir).rem_euclid(n),
            None => {
                if dir > 0 {
                    0
                } else {
                    n - 1
                }
            }
        };
        self.sel = Some(act[next as usize]);
        self.nudge = 0;
    }

    /// Jump to the next / previous pipeline event (lock, rejection, ...).
    fn jump_event(&mut self, dir: i32) {
        let cur = self.t;
        let times: Vec<f64> = self.obs.timeline.events.iter().map(|e| e.t).collect();
        let target = if dir > 0 {
            times.iter().copied().find(|t| *t > cur + 1e-6)
        } else {
            times.iter().rev().copied().find(|t| *t < cur - 1e-6)
        };
        match target {
            Some(t) => self.seek(t),
            None => self.seek(if dir > 0 { DURATION } else { 0.0 }),
        }
    }

    /// The earlier, still-uncertain state worth comparing against: when the first
    /// persistent-source candidate appeared.
    pub fn default_mark(&self) -> f64 {
        let ev = self
            .obs
            .timeline
            .events
            .iter()
            .find(|e| e.kind == EventKind::Detected && matches!(e.slot, Some(0..=2)));
        match ev {
            Some(e) if e.t + EPOCH_S < self.t => e.t + 2.0 * EPOCH_S,
            _ => (self.t / 3.0).max(2.0 * EPOCH_S),
        }
    }

    fn log_key(&mut self, token: &str) {
        if (self.t - self.t_after_key).abs() > 1e-9 {
            self.keylog.push(format!("@{:.6}", self.t));
        }
        self.keylog.push(token.to_string());
    }

    /// Apply one key. Returns the canonical script token for it.
    pub fn key(&mut self, k: KeyEvent) {
        let shift = k.modifiers.contains(KeyModifiers::SHIFT);
        let token = key_token(k);
        self.log_key(&token);
        match k.code {
            KeyCode::Char(c @ '1'..='5') => {
                let idx = c as usize - '1' as usize;
                self.set_view(View::ALL[idx]);
            }
            KeyCode::Tab => self.set_view(self.view.next()),
            KeyCode::BackTab => self.set_view(self.view.prev()),
            KeyCode::Left | KeyCode::Char('H') => self.seek(
                self.t
                    - if shift || k.code == KeyCode::Char('H') {
                        16.0
                    } else {
                        EPOCH_S
                    },
            ),
            KeyCode::Right | KeyCode::Char('L') => self.seek(
                self.t
                    + if shift || k.code == KeyCode::Char('L') {
                        16.0
                    } else {
                        EPOCH_S
                    },
            ),
            KeyCode::Home => self.seek(0.0),
            KeyCode::End | KeyCode::Char('g') => self.seek(DURATION),
            KeyCode::Up => self.cycle_selection(-1),
            KeyCode::Down => self.cycle_selection(1),
            KeyCode::Char(' ') => {
                if self.t >= DURATION {
                    self.seek(0.0);
                }
                self.playing = !self.playing;
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.speed = (self.speed + 1).min(SPEEDS.len() - 1)
            }
            KeyCode::Char('-') | KeyCode::Char('_') => self.speed = self.speed.saturating_sub(1),
            KeyCode::Char('m') => {
                self.mark = Some(self.ep() as f64 * EPOCH_S);
                self.compare = false;
            }
            KeyCode::Char('c') => {
                if !self.compare {
                    if self.mark.is_none() || self.ref_ep_for(self.mark).is_none() {
                        self.mark = Some(self.default_mark());
                    }
                    self.compare = true;
                } else {
                    self.compare = false;
                }
            }
            KeyCode::Char('z') => self.zoom = (self.zoom + 1) % 3,
            KeyCode::Char(',') | KeyCode::Char('<') => self.nudge -= 1,
            KeyCode::Char('.') | KeyCode::Char('>') => self.nudge += 1,
            KeyCode::Char('0') => {
                self.zoom = 0;
                self.nudge = 0;
                self.cursor_on = false;
            }
            KeyCode::Char('h') => self.move_cursor(-1.0, 0.0),
            KeyCode::Char('l') => self.move_cursor(1.0, 0.0),
            KeyCode::Char('j') => self.move_cursor(0.0, -1.0),
            KeyCode::Char('k') => self.move_cursor(0.0, 1.0),
            KeyCode::Char('x') => self.cursor_on = false,
            KeyCode::Char('n') | KeyCode::PageDown => self.jump_event(1),
            KeyCode::Char('p') | KeyCode::PageUp => self.jump_event(-1),
            KeyCode::Char('r') => {
                let obs = self.obs.clone();
                let log = std::mem::take(&mut self.keylog);
                *self = Model::new(obs);
                self.keylog = log;
            }
            KeyCode::Char('?') => self.help = !self.help,
            KeyCode::Esc => {
                if self.help {
                    self.help = false;
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Char('q') => self.quit = true,
            _ => {}
        }
        self.t_after_key = self.t;
    }

    fn ref_ep_for(&self, mark: Option<f64>) -> Option<usize> {
        let r = epoch_at(mark?);
        (r >= 1 && r < self.ep()).then_some(r)
    }

    fn move_cursor(&mut self, du: f64, dv: f64) {
        if !self.cursor_on {
            self.cursor_on = true;
            self.cursor = (0.5, 0.5);
            return;
        }
        let step = 1.0 / 40.0;
        self.cursor.0 = (self.cursor.0 + du * step).clamp(0.0, 1.0);
        self.cursor.1 = (self.cursor.1 + dv * step).clamp(0.0, 1.0);
    }

    /// Advance real time (cumulative `elapsed`, as delivered by the UI loop).
    pub fn tick(&mut self, elapsed: Duration) {
        let dt = match self.clock {
            Some(prev) => elapsed.saturating_sub(prev).as_secs_f64(),
            None => 0.0,
        };
        self.clock = Some(elapsed);
        self.advance(dt);
    }

    /// Advance by `dt` real seconds.
    pub fn advance(&mut self, dt: f64) {
        if self.playing {
            self.seek(self.t + SPEEDS[self.speed] * dt);
            if self.t >= DURATION {
                self.playing = false;
            }
        }
        if self.from.is_some() {
            self.trans += (dt / TRANSPORT_S) as f32;
            if self.trans >= 1.0 {
                self.trans = 1.0;
                self.from = None;
            }
        }
    }

    /// Finish any running identity transport (a capture is a still, not a mid-air frame).
    pub fn settle(&mut self) {
        self.from = None;
        self.trans = 1.0;
    }

    pub fn replay_script(&self) -> String {
        self.keylog.join(" ")
    }

    pub fn state_of(&self, slot: usize) -> State {
        self.obs
            .timeline
            .epoch(self.ep())
            .map(|e| e.slots[slot].state)
            .unwrap_or(State::Quiet)
    }
}

/// Canonical script token of a key event.
pub fn key_token(k: KeyEvent) -> String {
    let shift = k.modifiers.contains(KeyModifiers::SHIFT);
    match k.code {
        KeyCode::Char(' ') => "space".into(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Left => if shift { "S-left" } else { "left" }.into(),
        KeyCode::Right => if shift { "S-right" } else { "right" }.into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "pgup".into(),
        KeyCode::PageDown => "pgdn".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::BackTab => "btab".into(),
        KeyCode::Esc => "esc".into(),
        KeyCode::Enter => "enter".into(),
        KeyCode::Backspace => "bksp".into(),
        KeyCode::Delete => "del".into(),
        _ => "?".into(),
    }
}

/// Parse one script token. `@T` seeks; `wait:S` advances real time; anything
/// else is a key.
pub enum ScriptItem {
    Key(KeyEvent),
    Seek(f64),
    Wait(f64),
}

pub fn parse_token(tok: &str) -> Option<ScriptItem> {
    if let Some(v) = tok.strip_prefix('@') {
        return v.parse::<f64>().ok().map(ScriptItem::Seek);
    }
    if let Some(v) = tok.strip_prefix("wait:") {
        return v.parse::<f64>().ok().map(ScriptItem::Wait);
    }
    let none = KeyModifiers::empty();
    let k = |code| Some(ScriptItem::Key(KeyEvent::new(code, none)));
    match tok {
        "space" => k(KeyCode::Char(' ')),
        "left" => k(KeyCode::Left),
        "right" => k(KeyCode::Right),
        "S-left" => Some(ScriptItem::Key(KeyEvent::new(
            KeyCode::Left,
            KeyModifiers::SHIFT,
        ))),
        "S-right" => Some(ScriptItem::Key(KeyEvent::new(
            KeyCode::Right,
            KeyModifiers::SHIFT,
        ))),
        "up" => k(KeyCode::Up),
        "down" => k(KeyCode::Down),
        "home" => k(KeyCode::Home),
        "end" => k(KeyCode::End),
        "pgup" => k(KeyCode::PageUp),
        "pgdn" => k(KeyCode::PageDown),
        "tab" => k(KeyCode::Tab),
        "btab" => k(KeyCode::BackTab),
        "esc" => k(KeyCode::Esc),
        "enter" => k(KeyCode::Enter),
        s if s.chars().count() == 1 => k(KeyCode::Char(s.chars().next()?)),
        _ => None,
    }
}

/// Run a whitespace-separated script against a model (the replay mechanism).
pub fn run_script(m: &mut Model, script: &str) -> Result<(), String> {
    for tok in script.split_whitespace() {
        match parse_token(tok) {
            Some(ScriptItem::Key(k)) => m.key(k),
            Some(ScriptItem::Seek(t)) => {
                m.seek(t);
                m.t_after_key = m.t;
                m.keylog.push(tok.to_string());
            }
            Some(ScriptItem::Wait(s)) => {
                m.keylog.push(tok.to_string());
                let mut left = s;
                while left > 1e-9 {
                    let dt = left.min(0.1);
                    m.advance(dt);
                    left -= dt;
                }
            }
            None => return Err(format!("unknown script token {tok:?}")),
        }
    }
    Ok(())
}
