//! Audio: one deterministic HumanMusic performance per branch future.
//!
//! What the public API does and does not allow (measured in `docs`, see FRICTION.md):
//! * `HumanMusicSynth` is a forward-only stateful DSP engine. It cannot seek, and rendering
//!   with a jumped `RenderCtx::start` is *not* a seek (it fires every skipped event at once).
//! * `compose` is not prefix-causal: changing a trace's future changes audio from beat 0.
//!
//! So history is never "re-composed in place". Instead:
//! * the **past is immutable scrollback** — a fork keeps the parent's PCM bit-for-bit up to
//!   the fork sample;
//! * the **future is a new whole performance** composed from the child's own semantic trace,
//!   crossfaded in over [`XFADE_SECS`] *after* the fork sample;
//! * every performance is a pure function of `(world, seed, trace)`, so an evicted one is
//!   rebuilt by deterministic re-render (verified by hash), and "seek" is an offset into
//!   already-rendered PCM.

use crate::epoch::*;
use crate::history::*;
use crate::vm::*;
use gibson::audio::human_music::semantic::{
    Density, Elevation, Emphasis, EventKind, SemanticEvent, SemanticState, SemanticTrace, Tone,
};
use gibson::audio::human_music::synth::HumanMusicSynth;
use gibson::audio::human_music::{compose, MusicWorld, WorldId};
use gibson::audio::render::OfflineRenderer;
use gibson::audio::{SampleRate, StereoBlock};
use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

pub const STEPS_PER_BEAT: f64 = 4.0;
pub const XFADE_SECS: f64 = 0.35;
pub const MIN_SPAN_STEPS: u32 = 12;
pub const MIN_TOTAL_BEATS: f64 = 16.0;
pub const SR: SampleRate = SampleRate::STUDIO;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    /// Compose only the child's future (from the fork) with the fork-point state as carry-in.
    FutureOnly,
    /// Compose the child's whole trace (prefix + future) and keep only the future part.
    FullTrace,
}

/// Maps VM steps to audio samples. A pure function of the step number (no accumulation).
#[derive(Clone, Copy, Debug)]
pub struct StepClock {
    pub sps: f64,
}

impl StepClock {
    pub fn for_world(w: &MusicWorld) -> StepClock {
        StepClock {
            sps: SR.as_f64() * 60.0 / (w.tempo_bpm as f64 * STEPS_PER_BEAT),
        }
    }
    pub fn sample_of(&self, step: u32) -> u64 {
        (step as f64 * self.sps).round() as u64
    }
    pub fn step_of(&self, sample: u64) -> u32 {
        (sample as f64 / self.sps).floor() as u32
    }
}

// --- epoch -> semantics --------------------------------------------------------------

/// The musical meaning of an epoch (+ escalation level 0..=2).
pub fn epoch_state(e: Epoch, level: u8) -> (SemanticState, EventKind) {
    let st = |tone, emphasis, density, elevation| SemanticState {
        tone,
        emphasis,
        density,
        elevation,
    };
    match e {
        Epoch::Stable => (
            st(
                Tone::Neutral,
                Emphasis::Normal,
                Density::Spacious,
                Elevation::Flat,
            ),
            EventKind::ToneShift,
        ),
        Epoch::Uncertain => (
            st(
                Tone::Info,
                Emphasis::Muted,
                Density::Normal,
                Elevation::Raised,
            ),
            EventKind::ToneShift,
        ),
        Epoch::Escalating => match level {
            0 => (
                st(
                    Tone::Warning,
                    Emphasis::Normal,
                    Density::Normal,
                    Elevation::Raised,
                ),
                EventKind::ToneShift,
            ),
            1 => (
                st(
                    Tone::Warning,
                    Emphasis::Strong,
                    Density::Compact,
                    Elevation::Raised,
                ),
                EventKind::Impact,
            ),
            _ => (
                st(
                    Tone::Danger,
                    Emphasis::Strong,
                    Density::Compact,
                    Elevation::Overlay,
                ),
                EventKind::Impact,
            ),
        },
        Epoch::Deadlock => (
            st(
                Tone::Warning,
                Emphasis::Faint,
                Density::Spacious,
                Elevation::Overlay,
            ),
            EventKind::ModalEntered,
        ),
        Epoch::Contradiction => (
            st(
                Tone::Danger,
                Emphasis::Normal,
                Density::Compact,
                Elevation::Raised,
            ),
            EventKind::Impact,
        ),
        Epoch::Resolution => (
            st(
                Tone::Success,
                Emphasis::Normal,
                Density::Normal,
                Elevation::Raised,
            ),
            EventKind::Confirmation,
        ),
        Epoch::Convergence => (
            st(
                Tone::Accent,
                Emphasis::Normal,
                Density::Spacious,
                Elevation::Raised,
            ),
            EventKind::SectionResolved,
        ),
        Epoch::Catastrophe => (
            st(
                Tone::Danger,
                Emphasis::Strong,
                Density::Compact,
                Elevation::Overlay,
            ),
            EventKind::Impact,
        ),
    }
}

fn escalation_level(heat: i32) -> u8 {
    match heat {
        i32::MIN..=79 => 0,
        80..=89 => 1,
        _ => 2,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub epoch: Epoch,
    pub level: u8,
}

/// Epoch spans over steps `[from, to)` of branch `b`, smoothed so no span is shorter than
/// [`MIN_SPAN_STEPS`] (a musical phrase cannot follow a 3-step epoch).
pub fn spans_of(h: &History, b: BranchId, from: u32, to: u32) -> Vec<Span> {
    let mut raw: Vec<Span> = vec![];
    for s in from..to {
        let Some(r) = h.rec_at(b, s) else { continue };
        let level = if r.epoch == Epoch::Escalating {
            escalation_level(r.vars[V_HEAT as usize])
        } else {
            0
        };
        match raw.last_mut() {
            Some(l) if l.epoch == r.epoch && l.level == level => l.end = s + 1,
            _ => raw.push(Span {
                start: s,
                end: s + 1,
                epoch: r.epoch,
                level,
            }),
        }
    }
    let mut out: Vec<Span> = vec![];
    for sp in raw {
        let short = sp.end - sp.start < MIN_SPAN_STEPS && sp.epoch != Epoch::Catastrophe;
        match out.last_mut() {
            Some(prev) if short => prev.end = sp.end,
            _ => out.push(sp),
        }
    }
    // a short *first* span folds forward
    if out.len() > 1 && out[0].end - out[0].start < MIN_SPAN_STEPS {
        let first = out.remove(0);
        out[0].start = first.start;
    }
    // merge equal neighbours produced by folding
    let mut merged: Vec<Span> = vec![];
    for sp in out {
        match merged.last_mut() {
            Some(p) if p.epoch == sp.epoch && p.level == sp.level => p.end = sp.end,
            _ => merged.push(sp),
        }
    }
    merged
}

/// The semantic trace (what HumanMusic is told) for steps `[from, to)` of branch `b`.
pub fn trace_of(h: &History, b: BranchId, from: u32, to: u32) -> SemanticTrace {
    let spans = spans_of(h, b, from, to);
    let mut events = vec![];
    for (i, sp) in spans.iter().enumerate() {
        let (state, mut kind) = epoch_state(sp.epoch, sp.level);
        if i == 0 {
            kind = EventKind::ActChanged;
        }
        events.push(SemanticEvent {
            at_beat: (sp.start - from) as f64 / STEPS_PER_BEAT,
            state,
            kind,
        });
    }
    let n = to.saturating_sub(from) as f64 / STEPS_PER_BEAT;
    let coda = if spans.last().map(|s| s.epoch) == Some(Epoch::Catastrophe) {
        3.0
    } else {
        0.0
    };
    // events must start before total_beats; a trace shorter than a phrase is held to MIN_TOTAL_BEATS
    let total = (n + coda).max(MIN_TOTAL_BEATS);
    SemanticTrace::new(events, total)
}

fn splitmix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Content-addressed seed: the same branch lineage always sounds the same.
pub fn seed_for(h: &History, b: BranchId) -> u64 {
    let br = h.branch(b);
    let mut x = splitmix(h.seed ^ 0xC120_C0DE);
    x = splitmix(x ^ br.fork_at as u64);
    for i in &br.script.inputs {
        x = splitmix(x ^ ((i.at as u64) << 8));
        x = splitmix(
            x ^ match i.kind {
                InputKind::Cmd(c) => c as u64 + 1,
                InputKind::Override(v) => 0x100 + v as u64,
            },
        );
    }
    x
}

// --- performances ---------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct PerfStats {
    pub compose_ms: f64,
    pub render_ms: f64,
    pub audio_secs: f64,
    pub rtf: f64,
    pub peak: f32,
    pub rms: f32,
    pub max_voices: usize,
    pub nonfinite: bool,
    pub notes: usize,
    pub drums: usize,
    pub chords: usize,
    pub error: Option<String>,
}

pub struct Performance {
    pub branch: BranchId,
    pub from_step: u32,
    pub to_step: u32,
    pub world: WorldId,
    pub seed: u64,
    pub strategy: Strategy,
    /// Global sample of the performance's beat 0.
    pub start_sample: u64,
    /// Interleaved stereo i16, raw (before any fork crossfade).
    pub pcm: Vec<i16>,
    pub hash: u64,
    /// RMS energy per step in `[from_step, to_step)`.
    pub env: Vec<f32>,
    pub stats: PerfStats,
}

impl Performance {
    pub fn frames(&self) -> usize {
        self.pcm.len() / 2
    }
    pub fn end_sample(&self) -> u64 {
        self.start_sample + self.frames() as u64
    }
    pub fn bytes(&self) -> usize {
        self.pcm.len() * 2 + self.env.len() * 4
    }
}

pub fn hash_pcm(pcm: &[i16]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for s in pcm {
        h ^= *s as u16 as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Everything a worker needs; plain data, no references into `History`.
#[derive(Clone)]
pub struct PerfSpec {
    pub branch: BranchId,
    pub from_step: u32,
    pub to_step: u32,
    pub world: WorldId,
    pub seed: u64,
    pub strategy: Strategy,
    pub trace: SemanticTrace,
    /// For `FullTrace`: how many beats of prefix precede `from_step` in `trace`.
    pub skip_beats: f64,
    pub clock: StepClock,
}

pub fn spec_for(h: &History, b: BranchId, world: WorldId, strategy: Strategy) -> PerfSpec {
    let br = h.branch(b);
    let w = MusicWorld::from_id(world);
    let clock = StepClock::for_world(&w);
    let (from, to) = (br.fork_at, br.end());
    let (trace, skip_beats) = match strategy {
        Strategy::FutureOnly => (trace_of(h, b, from, to), 0.0),
        Strategy::FullTrace => (trace_of(h, b, 0, to), from as f64 / STEPS_PER_BEAT),
    };
    PerfSpec {
        branch: b,
        from_step: from,
        to_step: to,
        world,
        seed: seed_for(h, b),
        strategy,
        trace,
        skip_beats,
        clock,
    }
}

pub fn render_spec(spec: &PerfSpec) -> Performance {
    let world = MusicWorld::from_id(spec.world);
    let t0 = Instant::now();
    let composed =
        std::panic::catch_unwind(AssertUnwindSafe(|| compose(&spec.trace, &world, spec.seed)));
    let compose_ms = t0.elapsed().as_secs_f64() * 1e3;
    let start_sample = spec.clock.sample_of(spec.from_step);
    match composed {
        Err(_) => Performance {
            branch: spec.branch,
            from_step: spec.from_step,
            to_step: spec.to_step,
            world: spec.world,
            seed: spec.seed,
            strategy: spec.strategy,
            start_sample,
            pcm: vec![],
            hash: 0,
            env: vec![],
            stats: PerfStats {
                compose_ms,
                render_ms: 0.0,
                audio_secs: 0.0,
                rtf: 0.0,
                peak: 0.0,
                rms: 0.0,
                max_voices: 0,
                nonfinite: false,
                notes: 0,
                drums: 0,
                chords: 0,
                error: Some("compose panicked".into()),
            },
        },
        Ok(score) => {
            let (notes, drums, chords) = (score.notes.len(), score.drums.len(), score.chords.len());
            let mut synth = HumanMusicSynth::new(&score, &world, SR);
            let frames = synth.total_samples();
            let t1 = Instant::now();
            let rendered = OfflineRenderer::new(SR, 512).render(&mut synth, frames);
            let render_ms = t1.elapsed().as_secs_f64() * 1e3;
            let skip =
                (spec.skip_beats * SR.as_f64() * 60.0 / world.tempo_bpm as f64).round() as usize;
            let total = rendered.audio.frames();
            let from = skip.min(total);
            let mut pcm = Vec::with_capacity((total - from) * 2);
            for i in from..total {
                pcm.push(to_i16(rendered.audio.left[i]));
                pcm.push(to_i16(rendered.audio.right[i]));
            }
            let hash = hash_pcm(&pcm);
            // per-step RMS envelope
            let mut env = Vec::with_capacity((spec.to_step - spec.from_step) as usize);
            for s in spec.from_step..spec.to_step {
                let a = (spec.clock.sample_of(s) - start_sample) as usize;
                let b = ((spec.clock.sample_of(s + 1) - start_sample) as usize).min(pcm.len() / 2);
                let mut acc = 0f64;
                let mut n = 0usize;
                for i in a.min(b)..b {
                    let l = pcm[2 * i] as f64 / 32768.0;
                    let r = pcm[2 * i + 1] as f64 / 32768.0;
                    acc += l * l + r * r;
                    n += 2;
                }
                env.push(if n == 0 {
                    0.0
                } else {
                    (acc / n as f64).sqrt() as f32
                });
            }
            let secs = pcm.len() as f64 / 2.0 / SR.as_f64();
            Performance {
                branch: spec.branch,
                from_step: spec.from_step,
                to_step: spec.to_step,
                world: spec.world,
                seed: spec.seed,
                strategy: spec.strategy,
                start_sample,
                pcm,
                hash,
                env,
                stats: PerfStats {
                    compose_ms,
                    render_ms,
                    audio_secs: secs,
                    rtf: if render_ms > 0.0 {
                        secs / (render_ms / 1e3)
                    } else {
                        0.0
                    },
                    peak: rendered.peak,
                    rms: rendered.rms,
                    max_voices: rendered.max_active_voices,
                    nonfinite: rendered.had_nonfinite,
                    notes,
                    drums,
                    chords,
                    error: None,
                },
            }
        }
    }
}

fn to_i16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

// --- the store ----------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct AudioStats {
    pub builds: u64,
    pub rebuilds: u64,
    pub evictions: u64,
    pub hash_mismatch_on_rebuild: u64,
    pub total_render_ms: f64,
    pub total_compose_ms: f64,
    pub total_audio_secs: f64,
    pub failed: u64,
    pub rebuild_hashes_checked: u64,
}

struct Done {
    perf: Performance,
}

pub struct AudioStore {
    pub world: WorldId,
    pub strategy: Strategy,
    pub clock: StepClock,
    pub budget_bytes: usize,
    perfs: HashMap<BranchId, Arc<Performance>>,
    lru: VecDeque<BranchId>,
    bytes: usize,
    pending: HashSet<BranchId>,
    seen_hash: HashMap<BranchId, u64>,
    tx: Option<Sender<PerfSpec>>,
    rx: Option<Receiver<Done>>,
    worker: Option<std::thread::JoinHandle<()>>,
    pub stats: AudioStats,
}

impl AudioStore {
    /// `threaded = false` builds inline (deterministic tests, headless capture).
    pub fn new(world: WorldId, strategy: Strategy, threaded: bool) -> AudioStore {
        let clock = StepClock::for_world(&MusicWorld::from_id(world));
        let (mut tx, mut rx, mut worker) = (None, None, None);
        if threaded {
            let (jtx, jrx) = channel::<PerfSpec>();
            let (dtx, drx) = channel::<Done>();
            worker = Some(std::thread::spawn(move || {
                while let Ok(spec) = jrx.recv() {
                    let perf = render_spec(&spec);
                    if dtx.send(Done { perf }).is_err() {
                        break;
                    }
                }
            }));
            tx = Some(jtx);
            rx = Some(drx);
        }
        AudioStore {
            world,
            strategy,
            clock,
            budget_bytes: 96 << 20,
            perfs: HashMap::new(),
            lru: VecDeque::new(),
            bytes: 0,
            pending: HashSet::new(),
            seen_hash: HashMap::new(),
            tx,
            rx,
            worker,
            stats: AudioStats::default(),
        }
    }

    pub fn is_threaded(&self) -> bool {
        self.tx.is_some()
    }
    pub fn resident_bytes(&self) -> usize {
        self.bytes
    }
    pub fn resident_count(&self) -> usize {
        self.perfs.len()
    }
    pub fn is_pending(&self, b: BranchId) -> bool {
        self.pending.contains(&b)
    }
    pub fn get(&self, b: BranchId) -> Option<&Arc<Performance>> {
        self.perfs.get(&b)
    }

    fn install(&mut self, perf: Performance) {
        let b = perf.branch;
        let rebuild = self.seen_hash.contains_key(&b);
        if let Some(prev) = self.seen_hash.get(&b) {
            self.stats.rebuild_hashes_checked += 1;
            if *prev != perf.hash {
                self.stats.hash_mismatch_on_rebuild += 1;
            }
        }
        self.seen_hash.insert(b, perf.hash);
        if rebuild {
            self.stats.rebuilds += 1;
        } else {
            self.stats.builds += 1;
        }
        self.stats.total_render_ms += perf.stats.render_ms;
        self.stats.total_compose_ms += perf.stats.compose_ms;
        self.stats.total_audio_secs += perf.stats.audio_secs;
        if perf.stats.error.is_some() {
            self.stats.failed += 1;
        }
        self.bytes += perf.bytes();
        self.pending.remove(&b);
        if let Some(old) = self.perfs.insert(b, Arc::new(perf)) {
            self.bytes -= old.bytes();
        }
        self.lru.retain(|x| *x != b);
        self.lru.push_back(b);
    }

    /// Ask for branch `b`'s performance. Inline when not threaded.
    pub fn request(&mut self, h: &mut History, b: BranchId) {
        if self.perfs.contains_key(&b) || self.pending.contains(&b) {
            return;
        }
        h.ensure_chain(b);
        let spec = spec_for(h, b, self.world, self.strategy);
        match &self.tx {
            Some(tx) => {
                self.pending.insert(b);
                let _ = tx.send(spec);
            }
            None => {
                let perf = render_spec(&spec);
                self.install(perf);
            }
        }
    }

    /// Collect finished background builds. Returns the branches that became ready.
    pub fn poll(&mut self) -> Vec<BranchId> {
        let mut ready = vec![];
        let mut got = vec![];
        if let Some(rx) = &self.rx {
            while let Ok(d) = rx.try_recv() {
                got.push(d.perf);
            }
        }
        for p in got {
            ready.push(p.branch);
            self.install(p);
        }
        ready
    }

    /// Block until `b` is ready (used by headless capture and tests).
    pub fn wait(&mut self, h: &mut History, b: BranchId) {
        self.request(h, b);
        while !self.perfs.contains_key(&b) {
            if self.rx.is_none() {
                break;
            }
            let ready = self.poll();
            if ready.is_empty() {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        self.touch(b);
    }

    pub fn touch(&mut self, b: BranchId) {
        if self.perfs.contains_key(&b) {
            self.lru.retain(|x| *x != b);
            self.lru.push_back(b);
        }
    }

    /// Evict least-recently-used performances (never `protect`) down to the byte budget.
    /// Eviction is *safe by construction*: a rebuild is bit-identical (counted + checked).
    pub fn enforce_budget(&mut self, protect: &[BranchId]) {
        while self.bytes > self.budget_bytes {
            let victim = self.lru.iter().copied().find(|b| !protect.contains(b));
            match victim {
                Some(v) => {
                    self.lru.retain(|x| *x != v);
                    if let Some(p) = self.perfs.remove(&v) {
                        self.bytes -= p.bytes();
                        self.stats.evictions += 1;
                    }
                }
                None => break,
            }
        }
    }

    pub fn evict(&mut self, b: BranchId) {
        if let Some(p) = self.perfs.remove(&b) {
            self.bytes -= p.bytes();
            self.lru.retain(|x| *x != b);
            self.stats.evictions += 1;
        }
    }

    /// Step energy for `branch` at `step`, reading through ancestors for the shared past.
    pub fn energy(&self, h: &History, b: BranchId, step: u32) -> Option<f32> {
        let br = h.branch(b);
        if step < br.fork_at {
            return self.energy(h, br.parent?, step);
        }
        let p = self.perfs.get(&b)?;
        p.env.get((step - p.from_step) as usize).copied()
    }

    /// Fetch `frames` interleaved stereo frames starting at global sample `start`, following
    /// the branch chain: the past is the parent's PCM, the future the child's, with an
    /// equal-power crossfade over [`XFADE_SECS`] *after* the fork sample. `None` if any
    /// needed performance is not resident (the caller requests it and tries again).
    pub fn fetch(&self, h: &History, b: BranchId, start: u64, frames: usize) -> Option<Vec<i16>> {
        let mut out = vec![0i16; frames * 2];
        self.fetch_into(h, b, start, frames, &mut out)?;
        Some(out)
    }

    fn fetch_into(
        &self,
        h: &History,
        b: BranchId,
        start: u64,
        frames: usize,
        out: &mut [i16],
    ) -> Option<()> {
        let br = h.branch(b);
        let fork_s = self.clock.sample_of(br.fork_at);
        let xf = (XFADE_SECS * SR.as_f64()) as u64;
        let xf_end = fork_s + xf;
        let end = start + frames as u64;
        let p = self.perfs.get(&b)?;
        let own = |s: u64| -> (f32, f32) {
            match s.checked_sub(p.start_sample) {
                Some(r) if (r as usize) < p.frames() => (
                    p.pcm[2 * r as usize] as f32,
                    p.pcm[2 * r as usize + 1] as f32,
                ),
                _ => (0.0, 0.0),
            }
        };
        match br.parent {
            None => {
                for s in start..end {
                    let (l, r) = own(s);
                    let i = (s - start) as usize;
                    out[2 * i] = l as i16;
                    out[2 * i + 1] = r as i16;
                }
            }
            Some(parent) => {
                // 1) the past: strictly before the fork sample, the parent's PCM bit-for-bit
                if start < fork_s {
                    let n = (fork_s.min(end) - start) as usize;
                    self.fetch_into(h, parent, start, n, &mut out[..n * 2])?;
                }
                // 2) crossfade: the parent's old future sounding under the new performance
                let (a, z) = (start.max(fork_s), end.min(xf_end));
                if a < z {
                    let n = (z - a) as usize;
                    let mut old = vec![0i16; n * 2];
                    self.fetch_into(h, parent, a, n, &mut old)?;
                    for s in a..z {
                        let t = (s - fork_s) as f32 / xf as f32;
                        let g_new = (t * std::f32::consts::FRAC_PI_2).sin();
                        let g_old = (t * std::f32::consts::FRAC_PI_2).cos();
                        let (cl, cr) = own(s);
                        let i = (s - start) as usize;
                        let j = (s - a) as usize;
                        out[2 * i] = (cl * g_new + old[2 * j] as f32 * g_old)
                            .round()
                            .clamp(-32768.0, 32767.0) as i16;
                        out[2 * i + 1] = (cr * g_new + old[2 * j + 1] as f32 * g_old)
                            .round()
                            .clamp(-32768.0, 32767.0)
                            as i16;
                    }
                }
                // 3) the new future proper
                for s in start.max(xf_end)..end {
                    let (l, r) = own(s);
                    let i = (s - start) as usize;
                    out[2 * i] = l as i16;
                    out[2 * i + 1] = r as i16;
                }
            }
        }
        Some(())
    }
}

impl Drop for AudioStore {
    fn drop(&mut self) {
        // Closing the job channel ends the worker *between* jobs. A render already in flight
        // cannot be cancelled (`OfflineRenderer::render` has no cancellation hook), so joining
        // here would make quitting wait for a whole HumanMusic performance. Detach instead:
        // the process exits and takes the thread with it. Tests that need a clean join call
        // `AudioStore::shutdown`.
        self.tx.take();
        self.worker.take();
    }
}

impl AudioStore {
    /// Close the job channel and wait for the worker (including any in-flight render).
    pub fn shutdown(&mut self) {
        self.tx.take();
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

// --- playback (honest, external, optional) -------------------------------------------------

/// Plays PCM through an external player process if one exists. There is no device path in
/// the headless/CI configuration and none of this is asserted by tests beyond argv and file
/// integrity; see EXPERIMENT_REPORT.md.
pub struct Player {
    pub program: Option<(String, Vec<String>)>,
    child: Option<std::process::Child>,
    file: Option<PathBuf>,
    pub spawned: u32,
    pub stopped: u32,
    counter: u32,
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

impl Player {
    pub fn none() -> Player {
        Player {
            program: None,
            child: None,
            file: None,
            spawned: 0,
            stopped: 0,
            counter: 0,
        }
    }
    pub fn detect() -> Player {
        if std::env::var("CHRONO_AUDIO")
            .map(|v| v == "off")
            .unwrap_or(false)
        {
            return Player::none();
        }
        let program = if which("pw-play") {
            Some(("pw-play".to_string(), vec![]))
        } else if which("paplay") {
            Some(("paplay".to_string(), vec![]))
        } else if which("aplay") {
            Some(("aplay".to_string(), vec!["-q".to_string()]))
        } else {
            None
        };
        Player {
            program,
            child: None,
            file: None,
            spawned: 0,
            stopped: 0,
            counter: 0,
        }
    }
    pub fn available(&self) -> bool {
        self.program.is_some()
    }
    pub fn is_playing(&mut self) -> bool {
        match self.child.as_mut() {
            Some(c) => matches!(c.try_wait(), Ok(None)),
            None => false,
        }
    }
    pub fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
            self.stopped += 1;
        }
        if let Some(f) = self.file.take() {
            let _ = std::fs::remove_file(f);
        }
    }
    /// Start playing interleaved stereo PCM (short fade-in so a restart never clicks).
    pub fn play(&mut self, mut pcm: Vec<i16>) {
        self.stop();
        let Some((bin, args)) = self.program.clone() else {
            return;
        };
        let fade = (0.04 * SR.as_f64()) as usize;
        for i in 0..fade.min(pcm.len() / 2) {
            let g = i as f32 / fade as f32;
            pcm[2 * i] = (pcm[2 * i] as f32 * g) as i16;
            pcm[2 * i + 1] = (pcm[2 * i + 1] as f32 * g) as i16;
        }
        let mut block = StereoBlock::new(pcm.len() / 2);
        for i in 0..block.frames() {
            block.left[i] = pcm[2 * i] as f32 / 32768.0;
            block.right[i] = pcm[2 * i + 1] as f32 / 32768.0;
        }
        self.counter += 1;
        let path = std::env::temp_dir().join(format!(
            "chronoscope-{}-{}.wav",
            std::process::id(),
            self.counter
        ));
        if gibson::audio::wav::write_wav_i16(&path, &block, SR).is_err() {
            return;
        }
        let child = std::process::Command::new(&bin)
            .args(&args)
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if let Ok(c) = child {
            self.child = Some(c);
            self.file = Some(path);
            self.spawned += 1;
        } else {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Write interleaved PCM to a WAV through the public `gibson::audio::wav` writer.
pub fn write_wav(path: &std::path::Path, pcm: &[i16]) -> std::io::Result<()> {
    let mut block = StereoBlock::new(pcm.len() / 2);
    for i in 0..block.frames() {
        block.left[i] = pcm[2 * i] as f32 / 32768.0;
        block.right[i] = pcm[2 * i + 1] as f32 / 32768.0;
    }
    gibson::audio::wav::write_wav_i16(path, &block, SR)
}
