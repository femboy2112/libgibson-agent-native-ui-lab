//! The Chronoscope model: a cursor moving through a tree of histories.
//!
//! Two clocks, never mixed:
//! * **history time** — the cursor `(branch, position)`; everything semantic (state, epochs,
//!   atmosphere story, audio position) is a function of it and can be rebuilt exactly;
//! * **presentation time** — `time`, a monotone wall/virtual clock that only drives motion
//!   (camera glide, fork bloom, toasts). `UiRuntime` clamps time backwards, so history time
//!   must never be fed to it.

use crate::audio::*;
use crate::director::*;
use crate::epoch::*;
use crate::fixture::*;
use crate::history::*;
use crate::view3d::{self, BranchView, CompareView, RecView, Role, Snapshot};
use crate::vm::*;
use gibson::audio::human_music::WorldId;
use gibson::story::StoryDirector;
use std::time::Duration;

/// A player that exits sooner than this after starting is treated as unusable.
pub const PLAYER_MIN_LIFE_SECS: f32 = 1.5;
pub const SPEEDS: [f64; 4] = [1.0, 2.0, 4.0, 16.0];

/// How much of HumanMusic the session uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioMode {
    /// No HumanMusic at all: no performances, no energy glow, no playback.
    Off,
    /// Performances are composed/rendered (the ribbons glow with their energy; A/B and WAVs
    /// work) but nothing is sent to a sound device.
    Silent,
    /// Silent + external player when one exists.
    Play,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub world: WorldId,
    pub strategy: Strategy,
    pub threaded_audio: bool,
    pub audio: AudioMode,
    pub seed: u64,
    pub script: Script,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            world: WorldId::BlackIce,
            // measured (docs/evidence/audio.md): the whole-trace composition lands on the parent's
            // chord at the fork far more often than a future-only piece does
            strategy: Strategy::FullTrace,
            threaded_audio: true,
            audio: AudioMode::Play,
            seed: DEMO_SEED,
            script: demo_script(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Modal {
    None,
    Inspect,
    Fork(Vec<(String, Edit)>),
    Help,
}

#[derive(Clone, Debug)]
pub enum Action {
    /// An event delivered to the focused viewport (its `on_event` sink).
    Key(gibson::Event),
    Pick(BranchId),
    Fork(usize),
    OpenFork,
    CloseModal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Jump {
    Start,
    End,
    NextLandmark,
    PrevLandmark,
    NextDecision,
    PrevDecision,
    NextEpoch,
    PrevEpoch,
    NextCheckpoint,
    PrevCheckpoint,
    NextInput,
    PrevInput,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    TogglePlay,
    Play,
    Pause,
    Step(i32),
    Jump(Jump),
    GoTo(u32),
    OpenFork,
    DoFork(usize),
    ForkEdit(Edit),
    CycleBranch(i32),
    SwitchBranch(BranchId),
    ToggleCompare,
    CycleCompare,
    ToggleAb,
    ToggleCollapse,
    Speed(i32),
    Turn,
    ToggleInside,
    ToggleMute,
    Inspect,
    CloseModal,
    Help,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cam {
    pub z: f32,
    pub x: f32,
    pub y: f32,
    pub vz: f32,
    pub yaw: f32,
    pub vyaw: f32,
}

pub struct Model {
    pub hist: History,
    pub audio: AudioStore,
    pub player: Player,
    pub atmo: Atmosphere,
    pub cur_b: BranchId,
    pub cursor: u32,
    pub frac: f64,
    pub playing: bool,
    pub speed: usize,
    pub compare: Option<(BranchId, BranchId)>,
    cmp_cache: Option<(BranchId, BranchId, u32, u32, Compare)>,
    pub ab_b: bool,
    pub modal: Modal,
    pub mute: bool,
    pub time: f32,
    pub cam: Cam,
    pub bloom: Option<(BranchId, f32)>,
    pub toast: Option<(String, f32)>,
    pub journal: Vec<String>,
    pub quit: bool,
    pub collapse: bool,
    pub spread: f32,
    /// Which way the camera faces: false = into the future, true = back along the past.
    pub facing_back: bool,
    /// Inside-the-tunnel camera instead of the outside chase camera.
    pub inside: bool,
    pub frames: u64,
    pub options: Options,
    audio_dirty: bool,
    audio_branch: Option<BranchId>,
    /// Presentation time at which the external player was last started.
    player_started_at: Option<f32>,
    /// The player exited immediately (no sound server?): stop retrying until `m` is pressed.
    pub audio_broken: bool,
    last_pick: Option<BranchId>,
    pub story_dir: StoryDirector,
    pub story_key: (BranchId, u32),
    /// Branches ever visited, most recent last (drives ghost selection and compare defaults).
    pub visits: Vec<BranchId>,
    /// The guided demonstration, if running (any key cancels it).
    pub demo: Option<crate::demo::Demo>,
}

pub fn lane_of(id: BranchId) -> (f32, f32, f32) {
    if id == 0 {
        return (0.0, 0.0, 0.0);
    }
    const LANES: [(f32, f32, f32); 8] = [
        (4.6, 1.3, 0.38),
        (-4.6, -1.2, -0.38),
        (8.6, -1.8, 0.2),
        (-8.6, 1.9, -0.2),
        (0.0, 4.6, 0.62),
        (0.0, -4.4, -0.62),
        (12.6, 1.8, 0.5),
        (-12.6, -1.5, -0.5),
    ];
    LANES[(id as usize - 1) % LANES.len()]
}

impl Model {
    pub fn new(options: Options) -> Model {
        let mut hist = History::new(colony(), options.seed, options.script.clone());
        hist.run_to_end(0);
        let mut audio = AudioStore::new(options.world, options.strategy, options.threaded_audio);
        if options.audio != AudioMode::Off {
            audio.request(&mut hist, 0);
        }
        let player = if options.audio == AudioMode::Play {
            Player::detect()
        } else {
            Player::none()
        };
        let atmo = Atmosphere::new();
        let story_dir = atmo.fresh();
        let mut m = Model {
            hist,
            audio,
            player,
            atmo,
            cur_b: 0,
            cursor: 0,
            frac: 0.0,
            playing: false,
            speed: 0,
            compare: None,
            cmp_cache: None,
            ab_b: false,
            modal: Modal::None,
            mute: false,
            time: 0.0,
            cam: Cam {
                z: 0.0,
                x: 0.0,
                y: 0.0,
                vz: 0.0,
                yaw: 0.0,
                vyaw: 0.0,
            },
            bloom: None,
            toast: None,
            journal: vec![],
            quit: false,
            collapse: false,
            spread: 1.0,
            facing_back: false,
            inside: false,
            frames: 0,
            options,
            audio_dirty: true,
            audio_branch: None,
            player_started_at: None,
            audio_broken: false,
            last_pick: None,
            story_dir,
            story_key: (0, 0),
            visits: vec![0],
            demo: None,
        };
        m.journal.push(format!(
            "recorded {} (seed {}, {} inputs): {} steps, {}",
            m.hist.prog.name,
            m.options.seed,
            m.options.script.inputs.len(),
            m.hist.branch(0).end(),
            describe_terminal(m.hist.branch(0).terminal)
        ));
        m
    }

    // --- derived ---------------------------------------------------------------------

    pub fn end(&self) -> u32 {
        self.hist.branch(self.cur_b).end()
    }

    /// The selected world's tempo: the audio clock, and therefore the 1× playback rate.
    pub fn world_tempo(&self) -> f64 {
        gibson::audio::human_music::MusicWorld::from_id(self.options.world).tempo_bpm as f64
    }

    /// Steps per second at 1×: locked to the audio (`tempo × STEPS_PER_BEAT / 60`).
    pub fn steps_per_sec_1x(&self) -> f64 {
        self.world_tempo() * STEPS_PER_BEAT / 60.0
    }

    pub fn rec_here(&self) -> Option<&Rec> {
        self.cursor
            .checked_sub(1)
            .and_then(|s| self.hist.rec_at(self.cur_b, s))
    }

    pub fn epoch_here(&self) -> Epoch {
        self.rec_here().map(|r| r.epoch).unwrap_or(Epoch::Stable)
    }

    /// Ask for a branch's performance (a no-op when the session runs without HumanMusic).
    pub fn want_audio(&mut self, b: BranchId) {
        if self.options.audio != AudioMode::Off {
            self.audio.request(&mut self.hist, b);
        }
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), self.time + 3.0));
    }

    pub fn audio_branch_now(&self) -> BranchId {
        match self.compare {
            Some((a, b)) => {
                if self.ab_b {
                    b
                } else {
                    a
                }
            }
            None => self.cur_b,
        }
    }

    pub fn status_of(&self, b: BranchId) -> &'static str {
        if self.audio.get(b).is_some() {
            "♪"
        } else if self.audio.is_pending(b) {
            "…"
        } else {
            "·"
        }
    }

    pub fn branch_desc(&self, b: BranchId) -> String {
        let br = self.hist.branch(b);
        match br.parent {
            None => format!(
                "root · {} steps · {}",
                br.end(),
                describe_terminal(br.terminal)
            ),
            Some(p) => format!(
                "{}@{} {} · {} steps · {}",
                self.hist.branch(p).label,
                br.fork_at,
                br.edit.describe(),
                br.end(),
                describe_terminal(br.terminal)
            ),
        }
    }

    // --- time travel --------------------------------------------------------------------

    fn set_cursor(&mut self, pos: u32) {
        let end = self.end();
        let pos = pos.min(end);
        if pos != self.cursor {
            self.cursor = pos;
            self.audio_dirty = true;
        }
    }

    pub fn step(&mut self, delta: i32) {
        self.frac = 0.0;
        if delta != 0 {
            self.facing_back = delta < 0;
        }
        let p = (self.cursor as i64 + delta as i64).clamp(0, self.end() as i64) as u32;
        self.set_cursor(p);
    }

    fn landmark_positions(&mut self, j: Jump) -> Vec<u32> {
        let b = self.cur_b;
        self.hist.ensure_chain(b);
        let lm = landmarks(&self.hist, b);
        match j {
            Jump::NextLandmark | Jump::PrevLandmark => lm.iter().map(|l| l.pos).collect(),
            Jump::NextDecision | Jump::PrevDecision => lm
                .iter()
                .filter(|l| l.kind == LandmarkKind::Fate)
                .map(|l| l.pos)
                .collect(),
            Jump::NextEpoch | Jump::PrevEpoch => lm
                .iter()
                .filter(|l| l.kind == LandmarkKind::Epoch)
                .map(|l| l.pos)
                .collect(),
            Jump::NextCheckpoint | Jump::PrevCheckpoint => lm
                .iter()
                .filter(|l| l.kind == LandmarkKind::Checkpoint)
                .map(|l| l.pos)
                .collect(),
            Jump::NextInput | Jump::PrevInput => lm
                .iter()
                .filter(|l| l.kind == LandmarkKind::Input)
                .map(|l| l.pos)
                .collect(),
            _ => vec![],
        }
    }

    pub fn jump(&mut self, j: Jump) {
        self.frac = 0.0;
        let before = self.cursor;
        self.jump_inner(j);
        if self.cursor != before {
            self.facing_back = self.cursor < before;
        }
    }

    fn jump_inner(&mut self, j: Jump) {
        match j {
            Jump::Start => self.set_cursor(0),
            Jump::End => self.set_cursor(self.end()),
            Jump::NextLandmark
            | Jump::NextDecision
            | Jump::NextEpoch
            | Jump::NextCheckpoint
            | Jump::NextInput => {
                let ps = self.landmark_positions(j);
                if let Some(p) = ps.into_iter().find(|p| *p > self.cursor) {
                    self.set_cursor(p);
                } else {
                    self.toast("no later landmark of that kind");
                }
            }
            Jump::PrevLandmark
            | Jump::PrevDecision
            | Jump::PrevEpoch
            | Jump::PrevCheckpoint
            | Jump::PrevInput => {
                let ps = self.landmark_positions(j);
                if let Some(p) = ps.into_iter().rev().find(|p| *p < self.cursor) {
                    self.set_cursor(p);
                } else {
                    self.toast("no earlier landmark of that kind");
                }
            }
        }
    }

    pub fn switch_branch(&mut self, b: BranchId) {
        if (b as usize) >= self.hist.branches.len() || b == self.cur_b {
            return;
        }
        let old = self.cur_b;
        self.cur_b = b;
        self.last_pick = Some(old);
        self.visits.retain(|x| *x != b);
        self.visits.push(b);
        let end = self.end();
        self.cursor = self.cursor.min(end);
        self.audio_dirty = true;
        self.hist.ensure_chain(b);
        self.want_audio(b);
        if self.compare.is_some() {
            self.compare = Some((b, old));
        }
    }

    pub fn cycle_branch(&mut self, d: i32) {
        let n = self.hist.branches.len() as i32;
        if n < 2 {
            return;
        }
        let nb = (self.cur_b as i32 + d).rem_euclid(n) as BranchId;
        self.switch_branch(nb);
    }

    pub fn fork_options(&mut self) -> Vec<(String, Edit)> {
        let b = self.cur_b;
        let at = self.cursor;
        let mut out: Vec<(String, Edit)> = vec![];
        if at >= self.end() && self.hist.branch(b).terminal.is_some() {
            return out;
        }
        self.hist.ensure_chain(b);
        if let Some(r) = self.hist.rec_at(b, at) {
            if r.ev.is_decision() {
                let (v, bound) = (r.ev.after, r.ev.aux);
                let alts: Vec<i32> = if r.ev.is_fate() {
                    // a fate point: show the two interesting outcomes
                    (0..bound.min(3)).filter(|x| *x != v).collect()
                } else {
                    [v + 1, v - 1]
                        .into_iter()
                        .filter(|x| *x >= 0 && *x < bound)
                        .collect()
                };
                for a in alts {
                    out.push((
                        format!("decision at step {at}: roll {v} → {a}"),
                        Edit::Override(a),
                    ));
                }
                // the null fork: same outcome, new branch (determinism witness)
                out.push((
                    format!("decision at step {at}: keep {v} (identical twin)"),
                    Edit::Override(v),
                ));
            }
        }
        if self.hist.branch(b).script.cmd_at(at).is_some() {
            out.push((format!("drop the command at step {at}"), Edit::DropCmd));
            out.push((
                format!("replace the command at step {at} with THROTTLE"),
                Edit::ReplaceCmd(2),
            ));
            out.push((
                format!("replace the command at step {at} with EMERGENCY"),
                Edit::ReplaceCmd(5),
            ));
        }
        out.push((format!("insert THROTTLE at step {at}"), Edit::InsertCmd(2)));
        out.push((format!("insert EMERGENCY at step {at}"), Edit::InsertCmd(5)));
        out.push((format!("insert BOOST at step {at}"), Edit::InsertCmd(1)));
        out
    }

    pub fn open_fork(&mut self) {
        let opts = self.fork_options();
        if opts.is_empty() {
            self.toast("nothing to change at the end of history: rewind first");
            return;
        }
        self.modal = Modal::Fork(opts);
        self.playing = false;
    }

    /// Fork the cursor branch at the cursor with `edit`. The old future stays, as a ghost.
    pub fn fork_with(&mut self, edit: Edit) -> Result<BranchId, HistoryError> {
        let parent = self.cur_b;
        let at = self.cursor;
        let id = self.hist.fork(parent, at, edit.clone())?;
        self.hist.run_to_end(id);
        self.want_audio(id);
        self.bloom = Some((id, 0.0));
        if self.journal.len() > 400 {
            self.journal.drain(..100);
        }
        self.journal.push(format!(
            "fork {} from {}@{}: {} → {} steps, {}",
            self.hist.branch(id).label,
            self.hist.branch(parent).label,
            at,
            edit.describe(),
            self.hist.branch(id).end(),
            describe_terminal(self.hist.branch(id).terminal)
        ));
        self.cur_b = id;
        self.last_pick = Some(parent);
        self.visits.retain(|x| *x != id);
        self.visits.push(id);
        self.cursor = at;
        self.audio_dirty = true;
        self.playing = false;
        self.modal = Modal::None;
        if self.compare.is_some() {
            self.compare = Some((id, parent));
        }
        self.hist.enforce_budget(&[id, parent]);
        let l = self.hist.branch(id).label.clone();
        self.toast(format!(
            "branch {l} born at step {at} — the old future persists as a ghost"
        ));
        Ok(id)
    }

    pub fn toggle_compare(&mut self) {
        if self.compare.is_some() {
            self.compare = None;
            self.audio_dirty = true;
            return;
        }
        if self.hist.branches.len() < 2 {
            self.toast("fork first: there is only one history");
            return;
        }
        let other = self.compare_default();
        self.compare = Some((self.cur_b, other));
        self.ab_b = false;
        self.audio_dirty = true;
        self.hist.ensure_chain(other);
        self.want_audio(other);
    }

    fn compare_default(&self) -> BranchId {
        if let Some(p) = self.last_pick {
            if p != self.cur_b {
                return p;
            }
        }
        match self.hist.branch(self.cur_b).parent {
            Some(p) => p,
            None => self
                .hist
                .branches
                .iter()
                .find(|b| b.parent == Some(self.cur_b))
                .map(|b| b.id)
                .unwrap_or(0),
        }
    }

    pub fn cycle_compare(&mut self) {
        if let Some((a, b)) = self.compare {
            let n = self.hist.branches.len() as u16;
            let mut nb = (b + 1) % n;
            if nb == a {
                nb = (nb + 1) % n;
            }
            self.compare = Some((a, nb));
            self.hist.ensure_chain(nb);
            self.want_audio(nb);
            self.audio_dirty = true;
        }
    }

    pub fn compare_result(&mut self) -> Option<&Compare> {
        let (a, b) = self.compare?;
        let (ea, eb) = (self.hist.branch(a).end(), self.hist.branch(b).end());
        let stale = match &self.cmp_cache {
            Some((ca, cb, xa, xb, _)) => (*ca, *cb, *xa, *xb) != (a, b, ea, eb),
            None => true,
        };
        if stale {
            self.hist.ensure_chain(a);
            self.hist.ensure_chain(b);
            let c = compare(&self.hist, a, b)?;
            self.cmp_cache = Some((a, b, ea, eb, c));
        }
        self.cmp_cache.as_ref().map(|c| &c.4)
    }

    pub fn do_cmd(&mut self, c: Cmd) {
        match c {
            Cmd::TogglePlay => {
                if self.playing {
                    self.playing = false;
                } else if self.cursor >= self.end() {
                    self.toast("at the end of this history: rewind, or fork");
                } else {
                    self.playing = true;
                    self.facing_back = false;
                }
                self.audio_dirty = true;
            }
            Cmd::Play => {
                if self.cursor < self.end() {
                    self.playing = true;
                    self.facing_back = false;
                    self.audio_dirty = true;
                }
            }
            Cmd::Pause => {
                self.playing = false;
                self.audio_dirty = true;
            }
            Cmd::Step(d) => {
                self.playing = false;
                self.step(d)
            }
            Cmd::Jump(j) => self.jump(j),
            Cmd::GoTo(p) => {
                self.frac = 0.0;
                self.set_cursor(p)
            }
            Cmd::OpenFork => self.open_fork(),
            Cmd::DoFork(i) => {
                if let Modal::Fork(opts) = self.modal.clone() {
                    if let Some((_, e)) = opts.get(i) {
                        if let Err(err) = self.fork_with(e.clone()) {
                            self.toast(format!("cannot fork there: {err}"));
                        }
                    }
                }
            }
            Cmd::ForkEdit(e) => {
                if let Err(err) = self.fork_with(e) {
                    self.toast(format!("cannot fork there: {err}"));
                }
            }
            Cmd::CycleBranch(d) => self.cycle_branch(d),
            Cmd::SwitchBranch(b) => self.switch_branch(b),
            Cmd::ToggleCompare => self.toggle_compare(),
            Cmd::CycleCompare => self.cycle_compare(),
            Cmd::ToggleAb => {
                if self.compare.is_some() {
                    self.ab_b = !self.ab_b;
                    self.audio_dirty = true;
                    let l = self.hist.branch(self.audio_branch_now()).label.clone();
                    self.toast(format!("♪ A/B → history {l}"));
                } else {
                    self.toast("A/B needs compare mode (v)");
                }
            }
            Cmd::ToggleCollapse => {
                self.collapse = !self.collapse;
            }
            Cmd::ToggleInside => {
                self.inside = !self.inside;
            }
            Cmd::Turn => {
                self.facing_back = !self.facing_back;
            }
            Cmd::Speed(d) => {
                self.speed = (self.speed as i32 + d).clamp(0, SPEEDS.len() as i32 - 1) as usize;
                self.audio_dirty = true;
            }
            Cmd::ToggleMute => {
                self.mute = !self.mute;
                self.audio_broken = false;
                self.audio_dirty = true;
            }
            Cmd::Inspect => {
                self.modal = if self.modal == Modal::Inspect {
                    Modal::None
                } else {
                    Modal::Inspect
                };
                self.playing = false;
            }
            Cmd::CloseModal => self.modal = Modal::None,
            Cmd::Help => {
                self.modal = if self.modal == Modal::Help {
                    Modal::None
                } else {
                    Modal::Help
                }
            }
            Cmd::Quit => self.quit = true,
        }
    }

    pub fn apply(&mut self, a: Action) {
        match a {
            Action::Key(ev) => {
                if let Some(c) = crate::ui::key_cmd(&ev) {
                    self.do_cmd(c);
                }
            }
            Action::Pick(b) => {
                self.switch_branch(b);
            }
            Action::Fork(i) => self.do_cmd(Cmd::DoFork(i)),
            Action::OpenFork => {
                self.modal = Modal::None;
                self.open_fork();
            }
            Action::CloseModal => self.modal = Modal::None,
        }
    }

    // --- presentation clock ---------------------------------------------------------------

    /// Advance presentation time by `dt` (and the cursor, if playing).
    pub fn tick(&mut self, dt: Duration) {
        let d = dt.as_secs_f64();
        if let Some(mut demo) = self.demo.take() {
            demo.tick(self, dt.as_secs_f32());
            if !demo.finished() {
                self.demo = Some(demo);
            }
        }
        self.time += d as f32;
        self.frames += 1;
        if let Some((id, age)) = self.bloom {
            let a = age + d as f32;
            self.bloom = if a > 2.5 { None } else { Some((id, a)) };
        }
        if let Some((_, until)) = &self.toast {
            if self.time > *until {
                self.toast = None;
            }
        }
        // collapse/rejoin animation
        let target = if self.collapse { 0.0 } else { 1.0 };
        let k = (d as f32 * 5.0).min(1.0);
        self.spread += (target - self.spread) * k;
        if (self.spread - target).abs() < 0.01 {
            self.spread = target;
        }
        if self.playing && self.modal == Modal::None {
            let rate = self.steps_per_sec_1x() * SPEEDS[self.speed];
            self.frac += d * rate;
            while self.frac >= 1.0 && self.cursor < self.end() {
                self.frac -= 1.0;
                self.cursor += 1;
            }
            if self.cursor >= self.end() {
                self.frac = 0.0;
                self.playing = false;
                self.audio_dirty = true;
                let t = self.hist.branch(self.cur_b).terminal;
                self.toast(format!("end of history: {}", describe_terminal(t)));
            }
        }
        // camera: critically damped glide toward the cursor (presentation time only)
        self.glide(d as f32);
        let _ = self.audio.poll();
        self.audio.touch(self.cur_b);
        self.audio.touch(self.audio_branch_now());
        if self.audio.resident_bytes() > self.audio.budget_bytes {
            let keep = [self.cur_b, self.audio_branch_now()];
            self.audio.enforce_budget(&self.hist, &keep);
        }
        self.sync_audio();
    }

    fn cam_target(&self) -> (f32, f32, f32) {
        let b = self.cur_b;
        let z = self.cursor as f32 + self.frac as f32;
        // lateral position of the cursor branch's tunnel centre (needs the lane geometry)
        let (x, y) = lane_xy(&self.hist, b, z, self.spread);
        (z, x, y)
    }

    fn glide(&mut self, dt: f32) {
        let (tz, tx, ty) = self.cam_target();
        // closed-form critically damped spring: unconditionally stable for any frame time (the
        // old semi-implicit Euler integrator diverged above ~92 ms per frame), deterministic for
        // a given dt, and time here is *presentation* time only
        let dt = dt.clamp(0.0, 0.25);
        spring(&mut self.cam.z, &mut self.cam.vz, tz, 9.0, dt);
        let tyaw = if self.facing_back {
            std::f32::consts::PI
        } else {
            0.0
        };
        spring(&mut self.cam.yaw, &mut self.cam.vyaw, tyaw, 3.74, dt);
        self.cam.x += (tx - self.cam.x) * (dt * 6.0).min(1.0);
        self.cam.y += (ty - self.cam.y) * (dt * 6.0).min(1.0);
        let finite = self.cam.z.is_finite()
            && self.cam.yaw.is_finite()
            && self.cam.x.is_finite()
            && self.cam.y.is_finite();
        if !finite {
            self.settle();
        }
        // the view window arithmetic assumes the camera is near the recorded range
        self.cam.z = self.cam.z.clamp(-8.0, MAX_STEPS as f32 + 8.0);
    }

    /// Snap the camera to the cursor (deterministic capture / tests).
    pub fn settle(&mut self) {
        let (z, x, y) = self.cam_target();
        self.cam = Cam {
            z,
            x,
            y,
            vz: 0.0,
            yaw: if self.facing_back {
                std::f32::consts::PI
            } else {
                0.0
            },
            vyaw: 0.0,
        };
        self.spread = if self.collapse { 0.0 } else { 1.0 };
    }

    fn sync_audio(&mut self) {
        let want = self.options.audio == AudioMode::Play
            && !self.mute
            && !self.audio_broken
            && self.playing
            && self.speed == 0
            && self.modal == Modal::None;
        if !want || !self.player.available() {
            if self.player.is_playing() {
                self.player.stop();
            }
            self.audio_branch = None;
            self.player_started_at = None;
            return;
        }
        let b = self.audio_branch_now();
        self.want_audio(b);
        if self.audio.get(b).is_none() {
            return;
        }
        let same = self.audio_branch == Some(b) && !self.audio_dirty;
        if same && !self.player.is_playing() {
            // The player ended by itself. Reaching the end of the PCM is fine (nothing to
            // restart); dying within a moment of starting means there is no usable sound server,
            // and restarting every frame would write a ~20 MB WAV and spawn a process 30×/s.
            if let Some(t0) = self.player_started_at.take() {
                if self.time - t0 < PLAYER_MIN_LIFE_SECS {
                    self.audio_broken = true;
                    self.toast("the audio player exited immediately: audio off (m retries)");
                }
            }
            return;
        }
        if !same {
            if self.cursor >= self.end().min(self.hist.branch(b).end()) {
                return;
            }
            let start = self.audio.clock.sample_of(self.cursor);
            let end = self.audio.get(b).map(|p| p.end_sample()).unwrap_or(start);
            // the chain's PCM from the cursor to the end of the branch
            if let Some(pcm) =
                self.audio
                    .fetch(&self.hist, b, start, end.saturating_sub(start) as usize)
            {
                let spawned = self.player.spawned;
                self.player.play(pcm);
                if self.player.spawned == spawned {
                    self.audio_broken = true;
                    self.toast("could not start the audio player: audio off (m retries)");
                }
                self.audio_branch = Some(b);
                self.audio_dirty = false;
                self.player_started_at = Some(self.time);
            }
        }
    }

    // --- atmosphere (Story/Scene at history time) ---------------------------------------

    pub fn director(&mut self) -> StoryDirector {
        let key = (self.cur_b, self.cursor);
        if key != self.story_key {
            self.story_dir = self.atmo.director_at(&self.hist, key.0, key.1);
            self.story_key = key;
        }
        self.story_dir.clone()
    }

    // --- snapshot for the renderer ----------------------------------------------------------

    pub fn visible_branches(&self, max_ghosts: usize) -> Vec<BranchId> {
        let mut out: Vec<BranchId> = vec![];
        let mut cur = Some(self.cur_b);
        while let Some(c) = cur {
            out.push(c);
            cur = self.hist.branch(c).parent;
        }
        if let Some((a, b)) = self.compare {
            for x in [a, b] {
                let mut cur = Some(x);
                while let Some(c) = cur {
                    if !out.contains(&c) {
                        out.push(c);
                    }
                    cur = self.hist.branch(c).parent;
                }
            }
        }
        // ghosts: most recently visited / created first
        let mut rest: Vec<BranchId> = self.visits.iter().rev().copied().collect();
        for b in self.hist.branches.iter().rev().take(max_ghosts + 4) {
            if !rest.contains(&b.id) {
                rest.push(b.id);
            }
        }
        // children of lineage branches are the interesting ghosts: the old futures
        for b in self.hist.branches.iter() {
            if let Some(p) = b.parent {
                if out.contains(&p) && !rest.contains(&b.id) {
                    rest.push(b.id);
                }
            }
        }
        let mut n = 0;
        for g in rest {
            if !out.contains(&g) && n < max_ghosts && !(self.collapse && self.spread < 0.05) {
                out.push(g);
                n += 1;
            }
        }
        out
    }

    pub fn snapshot(&mut self, lod: view3d::Lod) -> Snapshot {
        let ids = self.visible_branches(lod.ghosts_max);
        for &b in &ids {
            self.hist.ensure_chain(b);
        }
        self.hist.enforce_budget(&ids);
        let lo = (self.cam.z.floor() as i64 - lod.fwd as i64 - 2).max(0) as u32;
        let hi = (self.cam.z.ceil() as u32 + lod.fwd + 2).min(MAX_STEPS + 2);
        // lineage of the active branch: who is an ancestor, and until where is it "lived"
        let mut live_until: std::collections::HashMap<BranchId, u32> = Default::default();
        {
            let mut child = self.cur_b;
            while let Some(p) = self.hist.branch(child).parent {
                live_until.insert(p, self.hist.branch(child).fork_at);
                child = p;
            }
        }
        let mut branches = vec![];
        for &id in &ids {
            let br = self.hist.branch(id);
            let role = if id == self.cur_b {
                Role::Active
            } else if let Some(u) = live_until.get(&id) {
                Role::Ancestor { live_until: *u }
            } else {
                Role::Ghost
            };
            let first = br.fork_at.max(lo);
            let last = br.end().min(hi + 1);
            let mut recs = vec![];
            for s in first..last {
                if let Some(r) = self.hist.rec_at(id, s) {
                    recs.push(RecView {
                        step: s,
                        epoch: r.epoch,
                        vars: r.vars,
                        tcode: r.tcode,
                        task: r.ev.task,
                        class: r.ev.class,
                        flags: r.ev.flags,
                        obj: r.ev.obj,
                        aux: r.ev.aux,
                        parents: r.ev.parents,
                        energy: self.audio.energy(&self.hist, id, s).unwrap_or(0.0),
                        dead: r.dead,
                        locks: r.locks,
                    });
                }
            }
            branches.push(BranchView {
                id,
                label: br.label.clone(),
                parent: br.parent,
                fork_at: br.fork_at,
                end: br.end(),
                terminal: br.terminal,
                lane: lane_of(id),
                role,
                first_step: first,
                recs,
            });
        }
        // ancestors that are needed only for lane geometry must be present even if not visible
        let compare = self.compare.and_then(|(a, b)| {
            let cmp = self.compare_result()?.clone();
            let rows = cmp
                .rows
                .iter()
                .map(|r| (r.verdict, r.rel, r.root))
                .collect();
            Some(CompareView { a, b, rows })
        });
        // causal ancestry of the last executed event
        let ancestry = match self.cursor.checked_sub(1) {
            Some(s) => ancestry(&self.hist, self.cur_b, s, 24)
                .into_iter()
                .filter(|(_, d)| *d <= 3)
                .collect(),
            None => vec![],
        };
        Snapshot {
            branches,
            active: self.cur_b,
            cursor: self.cursor,
            compare,
            time: self.time,
            bloom: self.bloom,
            cam: (self.cam.z, self.cam.x, self.cam.y),
            yaw: self.cam.yaw,
            inside: self.inside,
            ancestry,
            inspect_branch: self.cur_b,
            spread: self.spread,
        }
    }
}

/// Exact step of a critically damped spring `x'' = -ω²(x - target) - 2ω x'` over `dt`.
pub fn spring(x: &mut f32, v: &mut f32, target: f32, omega: f32, dt: f32) {
    let d = *x - target;
    let e = (-omega * dt).exp();
    let c2 = *v + omega * d;
    *x = target + (d + c2 * dt) * e;
    *v = (*v - omega * c2 * dt) * e;
}

/// Lane centre (x, y) of branch `b` at step `z`, shared by the camera and the renderer so
/// they agree on where the tunnel is.
pub fn lane_xy(h: &History, b: BranchId, z: f32, spread: f32) -> (f32, f32) {
    let br = h.branch(b);
    let (lx, ly, _) = lane_of(b);
    let t = {
        let t = ((z - br.fork_at as f32) / view3d::SPLIT).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let (px, py) = match br.parent {
        Some(p) => lane_xy(h, p, z, spread),
        None => (0.0, 0.0),
    };
    if br.parent.is_none() {
        return (0.0, 0.0);
    }
    (px + (lx * spread - px) * t, py + (ly * spread - py) * t)
}

pub fn describe_terminal(t: Option<Terminal>) -> &'static str {
    match t {
        Some(Terminal::Meltdown) => "MELTDOWN",
        Some(Terminal::Wedged) => "wedged",
        Some(Terminal::Halted) => "halted",
        Some(Terminal::StepCap) => "survived to the step cap",
        None => "running",
    }
}

pub fn describe_event(h: &History, b: BranchId, step: u32) -> String {
    let Some(r) = h.rec_at(b, step) else {
        return "(fossil: not resident)".into();
    };
    let e = &r.ev;
    let who = if e.task == T_SYSTEM {
        "SYSTEM".to_string()
    } else {
        TASK_NAMES[e.task as usize % MAX_TASKS].to_string()
    };
    let prog = h.prog.clone();
    let op = if e.task == T_SYSTEM {
        "idle".to_string()
    } else {
        prog.tasks[e.task as usize]
            .get(e.pc as usize)
            .map(|o| format!("{o:?}"))
            .unwrap_or_else(|| "halt".into())
    };
    let extra = match e.class {
        Class::Fate | Class::Decision => format!(
            " · rolled {} of {} → {}{}",
            e.before,
            e.aux,
            e.after,
            if e.flags & F_OVERRIDDEN != 0 {
                " (OVERRIDDEN)"
            } else {
                ""
            }
        ),
        Class::Mark => format!(" · {}", crate::fixture::mark_name(e.aux as u8)),
        Class::Rmw | Class::Write | Class::Read if (e.obj as usize) < NVARS => {
            format!(" · {} {}→{}", VAR_NAMES[e.obj as usize], e.before, e.after)
        }
        Class::LockWait => format!(
            " · waits for {} held by {}",
            LOCK_NAMES[e.obj as usize % NLOCKS],
            TASK_NAMES[e.aux as usize % MAX_TASKS]
        ),
        Class::LockAcq | Class::LockRel => format!(" · {}", LOCK_NAMES[e.obj as usize % NLOCKS]),
        Class::Send | Class::Recv | Class::SendWait | Class::RecvWait => format!(
            " · {} payload {}",
            CHAN_NAMES[e.obj as usize % NCHAN],
            e.aux
        ),
        Class::Fail => format!(" · CRASH code {}", e.aux),
        Class::Restart => format!(" · restarts {}", TASK_NAMES[e.obj as usize % MAX_TASKS]),
        _ => String::new(),
    };
    let flag = if e.flags & F_INPUT != 0 {
        " [INPUT]"
    } else {
        ""
    };
    let cat = if e.flags & F_CATASTROPHE != 0 {
        " [MELTDOWN]"
    } else {
        ""
    };
    format!("{who:<10} {op}{extra}{flag}{cat}")
}
