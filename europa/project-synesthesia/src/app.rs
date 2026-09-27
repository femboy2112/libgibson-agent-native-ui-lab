//! Keyboard instrument state and editing rules.

use std::time::Instant;

use gibson::input::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::cli::Options;
use crate::engine::{Engine, Patch, Step, Waveform, STEPS_PER_TRACK, TRACK_COUNT};
use crate::visual::{Focus, SpectrumHistory};

#[derive(Clone, Debug)]
enum Modal {
    Filter { field: usize, text: String },
    Envelope { field: usize, text: String },
}

impl Modal {
    fn label(&self, patch: &Patch) -> String {
        match self {
            Self::Filter { field, text } => {
                let label = if *field == 0 {
                    "CUTOFF Hz"
                } else {
                    "RESONANCE"
                };
                let value = if text.is_empty() {
                    if *field == 0 {
                        format!("{:.0}", patch.cutoff_hz)
                    } else {
                        format!("{:.2}", patch.resonance)
                    }
                } else {
                    text.clone()
                };
                format!("VCF EDIT / {label}  [{value}]   type value · Tab selects field · Enter applies · Esc cancels")
            }
            Self::Envelope { field, text } => {
                let values = [patch.attack, patch.decay, patch.sustain, patch.release];
                let labels = ["ATTACK", "DECAY", "SUSTAIN", "RELEASE"];
                let value = if text.is_empty() {
                    format!("{:.3}", values[*field])
                } else {
                    text.clone()
                };
                format!(
                    "ADSR EDIT / {}  [{value}]   Tab selects stage · Enter applies · Esc cancels",
                    labels[*field]
                )
            }
        }
    }
}

pub struct AppModel {
    pub engine: Engine,
    pub history: SpectrumHistory,
    pub playing: bool,
    pub performance: bool,
    pub focus: Focus,
    pub selected_track: usize,
    pub selected_step: usize,
    pub seed: u64,
    pub fps: u32,
    pub song_ms: u64,
    pub events_seen: u64,
    pub key_events_seen: u64,
    pub resize_events_seen: u64,
    play_started: Instant,
    modal: Option<Modal>,
    pub should_quit: bool,
}

impl AppModel {
    pub fn from_options(options: &Options) -> Result<Self, String> {
        let mut engine = Engine::default();
        match options.pattern.as_str() {
            "pulse" | "demo" => {}
            "glass" => {
                engine.patch.waveform = Waveform::Sine;
                engine.patch.cutoff_hz = 4_400.0;
                engine.patch.release = 0.42;
                engine.tracks[0].steps.fill(Step::EMPTY);
                engine.tracks[0].gain = 0.38;
                for step in [0, 5, 8, 13] {
                    engine.tracks[0].steps[step] = Step::note([60, 67, 72, 79][step % 4], 0.8);
                }
                engine.tracks[1].steps.fill(Step::EMPTY);
                for step in [2, 10] {
                    engine.tracks[1].steps[step] = Step::note(48 + step as u8, 0.62);
                }
                engine.tracks[2].steps.fill(Step::EMPTY);
                engine.tracks[3].steps.fill(Step::EMPTY);
            }
            "offbeat" => {
                engine.bpm = 96.0;
                engine.tracks[0].steps.fill(Step::EMPTY);
                for step in [0, 6, 11] {
                    engine.tracks[0].steps[step] = Step::note(36, 0.95);
                }
                engine.tracks[1].steps.fill(Step::EMPTY);
                for step in [3, 7, 12, 15] {
                    engine.tracks[1].steps[step] = Step::note(41, 0.76);
                }
                engine.tracks[2].steps.fill(Step::EMPTY);
                for step in (1..STEPS_PER_TRACK).step_by(3) {
                    engine.tracks[2].steps[step] = Step::note(46, 0.42);
                }
                engine.tracks[3].steps.fill(Step::EMPTY);
                for step in [0, 5, 9, 14] {
                    engine.tracks[3].steps[step] = Step::note([48, 55, 58, 62][step % 4], 0.72);
                }
            }
            "empty" => {
                for track in &mut engine.tracks {
                    track.steps.fill(Step::EMPTY);
                }
            }
            unknown => {
                return Err(format!(
                    "unknown pattern '{unknown}' (choose pulse, glass, offbeat, or empty)"
                ))
            }
        }
        Ok(Self {
            engine,
            history: SpectrumHistory::default(),
            playing: options.demo,
            performance: options.performance,
            focus: Focus::Grid,
            selected_track: 0,
            selected_step: 0,
            seed: options.seed,
            fps: options.fps,
            song_ms: options.at_ms.unwrap_or(0),
            events_seen: 0,
            key_events_seen: 0,
            resize_events_seen: 0,
            play_started: Instant::now(),
            modal: None,
            should_quit: false,
        })
    }

    pub fn music_time_ms(&self, now: Instant) -> u64 {
        if self.playing {
            self.song_ms
                .saturating_add(now.saturating_duration_since(self.play_started).as_millis() as u64)
        } else {
            self.song_ms
        }
    }

    pub fn toggle_playing(&mut self, now: Instant) {
        if self.playing {
            self.song_ms = self.music_time_ms(now);
            self.playing = false;
        } else {
            self.play_started = now;
            self.playing = true;
        }
    }

    pub fn handle_event(&mut self, event: &Event, now: Instant) {
        self.events_seen = self.events_seen.saturating_add(1);
        match event {
            Event::Key(key) => {
                self.key_events_seen = self.key_events_seen.saturating_add(1);
                self.handle_key(*key, now);
            }
            Event::Paste(value) => {
                if let Some(modal) = &mut self.modal {
                    let text = match modal {
                        Modal::Filter { text, .. } | Modal::Envelope { text, .. } => text,
                    };
                    text.extend(
                        value
                            .chars()
                            .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-'),
                    );
                    if text.len() > 12 {
                        text.truncate(12);
                    }
                }
            }
            Event::Resize(_, _) => {
                self.resize_events_seen = self.resize_events_seen.saturating_add(1);
            }
            Event::Tick => {}
            _ => {}
        }
    }

    fn handle_key(&mut self, key: KeyEvent, now: Instant) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }
        if self.modal.is_some() {
            self.handle_modal_key(key);
            return;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char(' ') => self.toggle_playing(now),
            KeyCode::Char('p') => self.performance = !self.performance,
            KeyCode::Char('f') => {
                self.modal = Some(Modal::Filter {
                    field: 0,
                    text: String::new(),
                })
            }
            KeyCode::Char('e') => {
                self.modal = Some(Modal::Envelope {
                    field: 0,
                    text: String::new(),
                })
            }
            KeyCode::Char('w') => self.cycle_waveform(),
            // Always available, even when the current grid step contains a
            // note (Up/Down edit its pitch in that case).
            KeyCode::Char('j') => self.selected_track = (self.selected_track + 1) % TRACK_COUNT,
            KeyCode::Char('k') => {
                self.selected_track = (self.selected_track + TRACK_COUNT - 1) % TRACK_COUNT
            }
            KeyCode::Char('m') => {
                self.engine.tracks[self.selected_track].muted =
                    !self.engine.tracks[self.selected_track].muted
            }
            KeyCode::Char('s') => {
                self.engine.tracks[self.selected_track].solo =
                    !self.engine.tracks[self.selected_track].solo
            }
            KeyCode::Char('[') => self.engine.bpm = (self.engine.bpm - 1.0).clamp(20.0, 300.0),
            KeyCode::Char(']') => self.engine.bpm = (self.engine.bpm + 1.0).clamp(20.0, 300.0),
            KeyCode::Char('l') => self.cycle_pattern_length(),
            KeyCode::Char('+') | KeyCode::Char('=') => self.change_velocity(0.05),
            KeyCode::Char('-') => self.change_velocity(-0.05),
            KeyCode::Char(',') => self.transpose(-1),
            KeyCode::Char('.') => self.transpose(1),
            KeyCode::Tab => self.cycle_focus(1),
            KeyCode::BackTab => self.cycle_focus(-1),
            KeyCode::Enter => self.toggle_note(),
            KeyCode::Left => match self.focus {
                Focus::Grid => {
                    self.selected_step =
                        (self.selected_step + STEPS_PER_TRACK - 1) % STEPS_PER_TRACK
                }
                Focus::Patch => {
                    self.engine.patch.resonance =
                        (self.engine.patch.resonance - 0.02).clamp(0.0, 0.98)
                }
                Focus::Mixer => {
                    self.engine.tracks[self.selected_track].pan =
                        (self.engine.tracks[self.selected_track].pan - 0.05).clamp(-1.0, 1.0)
                }
            },
            KeyCode::Right => match self.focus {
                Focus::Grid => self.selected_step = (self.selected_step + 1) % STEPS_PER_TRACK,
                Focus::Patch => {
                    self.engine.patch.resonance =
                        (self.engine.patch.resonance + 0.02).clamp(0.0, 0.98)
                }
                Focus::Mixer => {
                    self.engine.tracks[self.selected_track].pan =
                        (self.engine.tracks[self.selected_track].pan + 0.05).clamp(-1.0, 1.0)
                }
            },
            KeyCode::Up => match self.focus {
                Focus::Grid => self.edit_pitch_or_track(1),
                Focus::Patch => {
                    self.engine.patch.cutoff_hz =
                        (self.engine.patch.cutoff_hz * 1.12).clamp(40.0, 18_000.0)
                }
                Focus::Mixer => {
                    self.engine.tracks[self.selected_track].gain =
                        (self.engine.tracks[self.selected_track].gain + 0.05).clamp(0.0, 1.5)
                }
            },
            KeyCode::Down => match self.focus {
                Focus::Grid => self.edit_pitch_or_track(-1),
                Focus::Patch => {
                    self.engine.patch.cutoff_hz =
                        (self.engine.patch.cutoff_hz / 1.12).clamp(40.0, 18_000.0)
                }
                Focus::Mixer => {
                    self.engine.tracks[self.selected_track].gain =
                        (self.engine.tracks[self.selected_track].gain - 0.05).clamp(0.0, 1.5)
                }
            },
            _ => {}
        }
    }

    fn cycle_focus(&mut self, direction: i32) {
        let index = match self.focus {
            Focus::Grid => 0_i32,
            Focus::Patch => 1,
            Focus::Mixer => 2,
        };
        self.focus = match (index + direction).rem_euclid(3) {
            0 => Focus::Grid,
            1 => Focus::Patch,
            _ => Focus::Mixer,
        };
    }

    fn edit_pitch_or_track(&mut self, delta: i8) {
        let mut step = self.engine.tracks[self.selected_track].steps[self.selected_step];
        if let Some(note) = step.note.as_mut() {
            *note = (*note as i16 + delta as i16).clamp(0, 127) as u8;
            self.engine
                .set_step(self.selected_track, self.selected_step, step);
        } else {
            self.selected_track =
                (self.selected_track as i32 - delta as i32).rem_euclid(TRACK_COUNT as i32) as usize;
        }
    }

    fn toggle_note(&mut self) {
        const DEFAULT_NOTES: [u8; TRACK_COUNT] = [36, 38, 42, 48];
        self.engine.toggle_step(
            self.selected_track,
            self.selected_step,
            DEFAULT_NOTES[self.selected_track],
            0.78,
        );
    }

    fn change_velocity(&mut self, delta: f32) {
        let step = &mut self.engine.tracks[self.selected_track].steps[self.selected_step];
        if step.note.is_some() {
            step.velocity = (step.velocity + delta).clamp(0.05, 1.0);
        }
    }

    fn transpose(&mut self, delta: i16) {
        let step = &mut self.engine.tracks[self.selected_track].steps[self.selected_step];
        if let Some(note) = step.note.as_mut() {
            *note = (*note as i16 + delta * 12).clamp(0, 127) as u8;
        }
    }

    fn cycle_waveform(&mut self) {
        self.engine.patch.waveform = match self.engine.patch.waveform {
            Waveform::Sine => Waveform::Triangle,
            Waveform::Triangle => Waveform::Saw,
            Waveform::Saw => Waveform::Square,
            Waveform::Square => Waveform::Sine,
        };
    }

    fn cycle_pattern_length(&mut self) {
        let track = &mut self.engine.tracks[self.selected_track];
        track.pattern_length = match track.pattern_length {
            16 => 12,
            12 => 8,
            8 => 4,
            _ => 16,
        };
    }

    fn handle_modal_key(&mut self, key: KeyEvent) {
        let mut close = false;
        let mut apply = false;
        if let Some(modal) = &mut self.modal {
            match key.code {
                KeyCode::Esc => close = true,
                KeyCode::Enter => apply = true,
                KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => match modal {
                    Modal::Filter { field, text } => {
                        *field = (*field + 1) % 2;
                        text.clear();
                    }
                    Modal::Envelope { field, text } => {
                        *field = (*field + 1) % 4;
                        text.clear();
                    }
                },
                KeyCode::Backspace => match modal {
                    Modal::Filter { text, .. } | Modal::Envelope { text, .. } => {
                        text.pop();
                    }
                },
                KeyCode::Up | KeyCode::Down => {
                    let delta = if key.code == KeyCode::Up { 1.0 } else { -1.0 };
                    match modal {
                        Modal::Filter { field: 0, .. } => {
                            self.engine.patch.cutoff_hz =
                                (self.engine.patch.cutoff_hz + delta * 100.0).clamp(40.0, 18_000.0)
                        }
                        Modal::Filter { .. } => {
                            self.engine.patch.resonance =
                                (self.engine.patch.resonance + delta * 0.02).clamp(0.0, 0.98)
                        }
                        Modal::Envelope { field, .. } => {
                            adjust_envelope(&mut self.engine.patch, *field, delta * 0.01)
                        }
                    }
                }
                KeyCode::Char(c) if c.is_ascii_digit() || c == '.' || c == '-' => match modal {
                    Modal::Filter { text, .. } | Modal::Envelope { text, .. } => {
                        if text.len() < 12 {
                            text.push(c);
                        }
                    }
                },
                _ => {}
            }
        }
        if apply {
            if let Some(modal) = &self.modal {
                let (field, value) = match modal {
                    Modal::Filter { field, text } => (*field, text.parse::<f32>().ok()),
                    Modal::Envelope { field, text } => (*field + 2, text.parse::<f32>().ok()),
                };
                if let Some(value) = value.filter(|v| v.is_finite()) {
                    match field {
                        0 => self.engine.patch.cutoff_hz = value.clamp(40.0, 18_000.0),
                        1 => self.engine.patch.resonance = value.clamp(0.0, 0.98),
                        _ => set_envelope(&mut self.engine.patch, field - 2, value),
                    }
                }
            }
            close = true;
        }
        if close {
            self.modal = None;
        }
    }

    pub fn modal_label(&self) -> Option<String> {
        self.modal.as_ref().map(|m| m.label(&self.engine.patch))
    }
}

fn adjust_envelope(patch: &mut Patch, field: usize, delta: f32) {
    match field {
        0 => patch.attack = (patch.attack + delta).clamp(0.001, 5.0),
        1 => patch.decay = (patch.decay + delta).clamp(0.0, 5.0),
        2 => patch.sustain = (patch.sustain + delta).clamp(0.0, 1.0),
        _ => patch.release = (patch.release + delta).clamp(0.001, 5.0),
    }
}

fn set_envelope(patch: &mut Patch, field: usize, value: f32) {
    match field {
        0 => patch.attack = value.clamp(0.001, 5.0),
        1 => patch.decay = value.clamp(0.0, 5.0),
        2 => patch.sustain = value.clamp(0.0, 1.0),
        _ => patch.release = value.clamp(0.001, 5.0),
    }
}
