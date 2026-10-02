//! The application model: simulation + incident semantics + music + view, plus the
//! ordered journal that makes the whole thing replayable.
//!
//! `App::step` is the one deterministic transition. It is a pure function of
//! `(state, journal, scenario beats)` — no wall clock, no terminal, no entropy.

use std::time::Duration;

use gibson::capability::ColorDepth;
use gibson::input::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::{Action, ActionKind, Journal};
use crate::incident::{IncidentRecord, Phase, PhaseTracker};
use crate::music::MusicDirector;
use crate::scenario::ScenarioDirector;
use crate::sim::{Engine, Fixture, ServiceId};
use crate::visual::{Camera, Layout, Scale};

/// A text block destined for native scrollback.
#[derive(Debug, Clone)]
pub struct ScrollEntry {
    pub text: String,
    /// `true` for a finalized report (committed as a block), `false` for streaming
    /// milestones (inserted above the live region).
    pub finalized: bool,
}

/// What one simulation step produced for the host to act on.
#[derive(Debug, Default)]
pub struct StepOutcome {
    pub scrollback: Vec<ScrollEntry>,
    pub phase_changed: bool,
    pub resolved: bool,
}

pub struct App {
    pub engine: Engine,
    pub layout: Layout,
    pub camera: Camera,
    pub tracker: PhaseTracker,
    pub journal: Journal,
    pub music: MusicDirector,
    pub scenario: Option<ScenarioDirector>,
    pub selected: ServiceId,
    pub scale: Scale,
    pub frame: u32,
    pub paused: bool,
    pub should_quit: bool,
    pub request_export: bool,
    pub show_help: bool,
    pub message: String,
    pub seed: u64,
    pub enable_music: bool,
    pub color_depth: ColorDepth,
    seq: u32,
    note_idx: usize,
    reported: bool,
    last_phase: Phase,
}

const CANNED_NOTES: [&str; 5] = [
    "suspect shared ledger pressure",
    "rollback window looks safe",
    "watch the auth tier",
    "queues still climbing in VAULT",
    "recovery holding",
];

impl App {
    pub fn new(
        seed: u64,
        world: gibson::audio::human_music::world::WorldId,
        with_scenario: bool,
    ) -> App {
        let fixture = Fixture::cathedral();
        let engine = Engine::new(fixture);
        let layout = Layout::build(&engine);
        let scenario = if with_scenario {
            Some(ScenarioDirector::new(&engine))
        } else {
            None
        };
        let selected = 0;
        let mut camera = Camera::new();
        camera.aim(&engine, &layout, Scale::Whole, selected, 120.0, 36.0);
        App {
            engine,
            layout,
            camera,
            tracker: PhaseTracker::new(),
            journal: Journal::new(seed, "cathedral"),
            music: MusicDirector::new(seed, world),
            scenario,
            selected,
            scale: Scale::Whole,
            frame: 0,
            paused: false,
            should_quit: false,
            request_export: false,
            show_help: false,
            message: "nominal".into(),
            seed,
            enable_music: true,
            color_depth: ColorDepth::TrueColor,
            seq: 0,
            note_idx: 0,
            reported: false,
            last_phase: Phase::Normal,
        }
    }

    /// Append an action to the ordered journal. It is applied at the next step.
    pub fn enqueue(&mut self, kind: ActionKind, target: ServiceId, magnitude: f32, note: &str) {
        self.seq += 1;
        let a = Action::new(self.frame, self.seq, kind, target, magnitude, note);
        self.journal.push(a);
    }

    /// Advance exactly one tick.
    pub fn step(&mut self) -> StepOutcome {
        let mut out = StepOutcome::default();
        if self.paused {
            return out;
        }

        // 1. The scenario may issue directives at this exact frame.
        if let Some(scenario) = &mut self.scenario {
            for d in scenario.tick() {
                let note = match d.kind {
                    ActionKind::InjectFault => "scenario: minor deployment fault",
                    ActionKind::Rollback => "scenario: rollback",
                    ActionKind::Reroute => "scenario: reroute",
                    ActionKind::Acknowledge => "scenario: acknowledge",
                    _ => "scenario",
                };
                self.seq += 1;
                let a = Action::new(self.frame, self.seq, d.kind, d.target, 0.6, note);
                self.journal.push(a);
            }
        }

        // 2. Apply every journaled action stamped for this frame, in order.
        let actions: Vec<Action> = self.journal.at(self.frame).cloned().collect();
        for a in &actions {
            self.tracker.observe_action(a);
            self.engine.apply(a);
            // Operator actions and scripted directives are permanent historical material.
            out.scrollback.push(ScrollEntry {
                text: format!(
                    "◇ {:<12} t={:<6} target=#{:<3} {}",
                    a.kind.label().to_uppercase(),
                    a.frame,
                    if a.target == u16::MAX {
                        "—".to_string()
                    } else {
                        a.target.to_string()
                    },
                    a.note
                ),
                finalized: false,
            });
        }

        // 3. Advance the system.
        self.engine.step();
        self.frame += 1;

        // 4. Reclassify the incident.
        if let Some(rec) = self.tracker.update(&self.engine) {
            out.phase_changed = true;
            self.last_phase = rec.phase;
            if matches!(
                rec.phase,
                Phase::Overload | Phase::LocalFault | Phase::Cascade | Phase::Restored
            ) {
                out.scrollback.push(ScrollEntry {
                    text: format!(
                        "◆ INCIDENT   t={:<6} {:<20} {}",
                        rec.frame,
                        rec.phase.label(),
                        rec.reason
                    ),
                    finalized: false,
                });
            }
            if rec.phase == Phase::Restored && !self.reported {
                self.reported = true;
                out.resolved = true;
                out.scrollback.push(self.incident_report(&rec));
            }
        }

        // 5. Music responds to the semantic trajectory (bounded rebuild cadence).
        if self.enable_music {
            let phase = self.tracker.phase;
            self.music
                .maybe_rebuild(self.frame, phase, &self.tracker.records);
        }

        // 6. Camera eases toward its semantic aim (fixed viewport for headless; the
        //    interactive host re-aims with the real size each frame).
        self.camera.update();

        self.message = if self.tracker.phase == Phase::Normal {
            "nominal".into()
        } else {
            self.tracker.phase.label().to_lowercase()
        };
        out
    }

    /// Re-aim the camera for the current viewport.
    pub fn aim(&mut self, vw: f32, vh: f32) {
        self.camera.aim(
            &self.engine,
            &self.layout,
            self.scale,
            self.selected,
            vw,
            vh,
        );
    }

    fn incident_report(&self, rec: &IncidentRecord) -> ScrollEntry {
        let m = &rec.metrics;
        let top = self.top_hotspots(3);
        let mut text = String::new();
        text.push_str("╔══ INCIDENT REPORT ─────────────────────────────────────────────╗\n");
        text.push_str(&format!(
            "  resolved at t={} after {} phase transitions\n",
            rec.frame,
            self.tracker.records.len()
        ));
        text.push_str(&format!(
            "  peak: mean integrity {:.3}  critical {:.1}%  breakers {}\n",
            m.mean_health,
            m.frac_critical * 100.0,
            m.open_breakers
        ));
        text.push_str("  hottest districts at resolution:\n");
        for (name, d) in &top {
            text.push_str(&format!("    {:<28?} distress {:.2}\n", name, d));
        }
        text.push_str(&format!(
            "  operator actions: {}   journal digest: {}\n",
            self.journal.len(),
            &self.journal.digest()[..16]
        ));
        if let Some(take) = &self.music.take {
            text.push_str(&format!(
                "  score: {}  form {}  checked={}\n",
                take.world_name,
                &take.form_digest[..16],
                take.receipt_ok
            ));
        }
        text.push_str("╚════════════════════════════════════════════════════════════════╝");
        ScrollEntry {
            text,
            finalized: true,
        }
    }

    fn top_hotspots(&self, k: usize) -> Vec<(String, f32)> {
        let mut v: Vec<(f32, usize)> = (0..self.engine.len())
            .map(|i| {
                (
                    self.engine.states[i].distress(&self.engine.fixture.services[i]),
                    i,
                )
            })
            .collect();
        v.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        v.into_iter()
            .take(k)
            .map(|(d, i)| (self.engine.fixture.services[i].name.clone(), d))
            .collect()
    }

    /// Operator keyboard handling. Returns `true` if the view changed materially.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
                true
            }
            KeyCode::Char('c') if ctrl => {
                self.should_quit = true;
                true
            }
            KeyCode::Char(' ') => {
                self.paused = !self.paused;
                true
            }
            KeyCode::Char('f') | KeyCode::Char('F') => {
                let t = self.selected;
                self.enqueue(ActionKind::InjectFault, t, 0.6, "operator inject");
                true
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                let t = self.selected;
                self.enqueue(ActionKind::Rollback, t, 0.0, "operator rollback");
                true
            }
            KeyCode::Char('i') | KeyCode::Char('I') => {
                let t = self.selected;
                self.enqueue(ActionKind::Isolate, t, 0.0, "operator isolate");
                true
            }
            KeyCode::Char('x') | KeyCode::Char('X') => {
                let t = self.selected;
                self.enqueue(ActionKind::Reroute, t, 0.0, "operator reroute");
                true
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                let t = self.selected;
                self.enqueue(ActionKind::ShedLoad, t, 0.5, "operator shed-load 50%");
                true
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                self.enqueue(
                    ActionKind::Acknowledge,
                    u16::MAX,
                    0.0,
                    "operator acknowledge",
                );
                true
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                let note = CANNED_NOTES[self.note_idx % CANNED_NOTES.len()];
                self.note_idx += 1;
                self.enqueue(ActionKind::Annotate, u16::MAX, 0.0, note);
                true
            }
            KeyCode::Char('w') | KeyCode::Char('W') => {
                self.request_export = true;
                true
            }
            KeyCode::Char('h') | KeyCode::Char('H') | KeyCode::Char('?') => {
                self.show_help = !self.show_help;
                true
            }
            KeyCode::Char('1') => {
                self.scale = Scale::Whole;
                true
            }
            KeyCode::Char('2') => {
                let c = self.engine.fixture.services[self.selected as usize].cluster;
                self.scale = Scale::Cluster(c);
                true
            }
            KeyCode::Char('3') => {
                self.scale = Scale::Chain(self.selected);
                true
            }
            KeyCode::Char('4') => {
                self.scale = Scale::Service(self.selected);
                true
            }
            KeyCode::Tab => {
                self.scale = self.scale.next(&self.engine, self.selected);
                true
            }
            KeyCode::Right => {
                self.selected = (self.selected + 1) % self.engine.len() as ServiceId;
                if let Scale::Service(_) | Scale::Chain(_) = self.scale {
                    self.scale = match self.scale {
                        Scale::Service(_) => Scale::Service(self.selected),
                        _ => Scale::Chain(self.selected),
                    };
                }
                true
            }
            KeyCode::Left => {
                self.selected = if self.selected == 0 {
                    self.engine.len() as ServiceId - 1
                } else {
                    self.selected - 1
                };
                true
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                // Jump the focus to the most distressed service.
                let mut best = (f32::MIN, 0u16);
                for i in 0..self.engine.len() as u16 {
                    let d = self.engine.states[i as usize]
                        .distress(&self.engine.fixture.services[i as usize]);
                    if d > best.0 {
                        best = (d, i);
                    }
                }
                self.selected = best.1;
                true
            }
            _ => false,
        }
    }

    /// A canonical digest of the whole semantic state, for replay comparison.
    pub fn digest(&self) -> String {
        format!(
            "sim:{:016x}|phase:{}|journal:{}|recs:{}|music:{}",
            self.engine.semantic_digest(),
            self.tracker.phase as u8,
            self.journal.len(),
            self.tracker.records.len(),
            self.music
                .take
                .as_ref()
                .map(|t| t.form_digest.clone())
                .unwrap_or_default()
        )
    }

    pub fn tick_duration(&self) -> Duration {
        crate::scenario::TICK
    }
}
