//! The observatory's analysis: everything the *application* infers from the stream.
//!
//! The pipeline is causal and checkpointed. Checkpoint `k` is a pure function of
//! the first `k·CHUNK` samples (plus the previous checkpoint's state machine), so
//! "seek to t" and "run to t" give bit-identical products.
//!
//! * spectral search: whitened 3-station periodogram, harmonic-sum search for the
//!   pulse trains ALPHA and GAMMA, line finding for interference;
//! * chirp search: causal baseband + Fourier-domain dechirp for BETA's drifting carrier;
//! * coherent follow-up: phasor sums against the fitted phase model (growth curves,
//!   phasor walks), folding, inter-station delays, sky localisation;
//! * a lock state machine per signal and classification of unassociated lines.

pub mod chirp;
pub mod coherent;
pub mod spectral;

use crate::dsp::{log10_fap, sigma_from_log10p};
use crate::scenario::{Receiver, SigId, CHUNK, FS, N_CP};
use chirp::Baseband;
use coherent::{DelayFit, Model, SkyFit, HMAX};
use spectral::{find_lines, harmonic_sum, hs_at, HsCfg, Spectrum};
use std::collections::HashMap;
use std::f64::consts::TAU;
use std::sync::{Arc, Mutex, OnceLock};

/// Catalogue priors (coarse search bands, Hz) — the *only* a-priori knowledge the
/// analysis has about where the sources live.
pub const N_CP_PUB: usize = crate::scenario::N_CP;
pub const BAND_A: (f64, f64) = (1.0, 2.4);
pub const BAND_C: (f64, f64) = (0.25, 0.85);
pub const H_A: usize = 5;
pub const H_C: usize = 3;
/// Harmonics used for inter-station delay estimation.
pub const HD_A: usize = 3;
pub const HD_C: usize = 3;
/// Whitened-power threshold for a spectral line.
pub const LINE_THR: f64 = 7.5;
/// Robust-HS clip on a single harmonic's whitened power.
pub const HS_CLIP: f64 = 12.0;
pub const NB_FOLD: usize = 48;
pub const NB_WF: usize = 32;
/// Candidate / lock thresholds on log10(false-alarm probability after trials).
pub const CAND_LF: f64 = -2.0;
pub const LOCK_LF: f64 = -6.0;
pub const LOCALIZED_SIG: f64 = 0.25;
pub const RESOLVED_SIG: f64 = 0.12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    Unseen,
    Candidate,
    Locked,
    Localized,
    Resolved,
}

impl Stage {
    pub fn label(self) -> &'static str {
        match self {
            Stage::Unseen => "SEARCH",
            Stage::Candidate => "CANDIDATE",
            Stage::Locked => "LOCKED",
            Stage::Localized => "LOCALIZED",
            Stage::Resolved => "RESOLVED",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Stage::Unseen => "SRCH",
            Stage::Candidate => "CAND",
            Stage::Locked => "LOCK",
            Stage::Localized => "FIX",
            Stage::Resolved => "DONE",
        }
    }
}

/// A rival hypothesis for a periodic source: the same data folded at twice the period.
#[derive(Clone, Debug)]
pub struct Rival {
    pub label: &'static str,
    pub profile: Vec<f32>,
    /// Harmonic-sum statistic of the rival (f/2) vs the adopted hypothesis.
    pub s_rival: f64,
    pub s_best: f64,
}

#[derive(Clone, Debug)]
pub struct SignalProduct {
    pub id: SigId,
    pub stage: Stage,
    pub cand_cp: Option<usize>,
    pub lock_cp: Option<usize>,
    pub loc_cp: Option<usize>,
    pub res_cp: Option<usize>,
    lock_run: u32,
    /// Best-fit phase model (always present once data suffice, even before detection).
    pub model: Option<Model>,
    /// Frequency (Hz) of the model at the checkpoint time, and its 1σ.
    pub f_now: f64,
    pub f_sigma: f64,
    /// Search statistic and its significance.
    pub s_search: f64,
    pub log10fap: f64,
    pub sigma_eq: f64,
    pub conf: f64,
    /// Harmonic amplitude / phase of the fitted waveform at station 0 (h = 1..).
    pub harm: Vec<(f64, f64)>,
    pub nh: usize,
    pub profile: Vec<f32>,
    pub profile_err: f32,
    pub rival: Option<Rival>,
    pub waterfall: Vec<Vec<f32>>,
    /// Cumulative phasor walk (normalised so a noise step has unit variance per axis).
    pub walk: Vec<(f64, f64)>,
    /// `(t, snr)` where `snr = √max(S − S₀, 0)` of the coherent statistic at the
    /// *current* model, on the data prefix available at time `t`.
    pub growth: Vec<(f64, f64)>,
    pub delays: Option<DelayFit>,
    pub sky: Option<SkyFit>,
}

impl SignalProduct {
    fn empty(id: SigId) -> SignalProduct {
        SignalProduct {
            id,
            stage: Stage::Unseen,
            cand_cp: None,
            lock_cp: None,
            loc_cp: None,
            res_cp: None,
            lock_run: 0,
            model: None,
            f_now: f64::NAN,
            f_sigma: f64::NAN,
            s_search: 0.0,
            log10fap: 0.0,
            sigma_eq: 0.0,
            conf: 0.0,
            harm: Vec::new(),
            nh: 0,
            profile: Vec::new(),
            profile_err: 0.0,
            rival: None,
            waterfall: Vec::new(),
            walk: Vec::new(),
            growth: Vec::new(),
            delays: None,
            sky: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineClass {
    /// Amplitude differs between stations: not a far-field source.
    Terrestrial,
    /// Was growing like a source, then stopped.
    Transient,
    /// Persistent narrowband line not associated with any catalogue source.
    Unassigned,
}

impl LineClass {
    pub fn label(self) -> &'static str {
        match self {
            LineClass::Terrestrial => "TERRESTRIAL",
            LineClass::Transient => "TRANSIENT",
            LineClass::Unassigned => "UNASSIGNED",
        }
    }
}

/// An unassociated narrowband line (interference or a misleading candidate).
#[derive(Clone, Debug)]
pub struct LineProduct {
    pub label: String,
    pub f: f64,
    pub w_peak: f64,
    pub class: LineClass,
    pub s_tot: f64,
    pub log10fap: f64,
    pub amp_z: f64,
    pub amp_ratio: [f64; 2],
    pub growth: Vec<(f64, f64)>,
    pub walk: Vec<(f64, f64)>,
    pub sky: Option<SkyFit>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Candidate,
    Revised,
    Lock,
    Localized,
    Resolved,
    FlagTerrestrial,
    FlagTransient,
}

impl EventKind {
    pub fn label(self) -> &'static str {
        match self {
            EventKind::Candidate => "candidate",
            EventKind::Revised => "candidate revised",
            EventKind::Lock => "lock acquired",
            EventKind::Localized => "localized",
            EventKind::Resolved => "resolved",
            EventKind::FlagTerrestrial => "flagged terrestrial",
            EventKind::FlagTransient => "flagged transient",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Who {
    Sig(SigId),
    Line(f64),
}

#[derive(Clone, Copy, Debug)]
pub struct Event {
    pub cp: usize,
    pub kind: EventKind,
    pub who: Who,
}

impl Event {
    pub fn t(&self) -> f64 {
        (self.cp * CHUNK) as f64 / FS
    }
}

/// Everything the analysis knows at one instant (the data prefix `n = k·CHUNK`).
#[derive(Clone, Debug)]
pub struct Checkpoint {
    pub k: usize,
    pub n: usize,
    pub t: f64,
    pub spec: Option<Arc<Spectrum>>,
    /// Station-0 RMS of each completed chunk (total-power timeline).
    pub chunk_rms: Vec<f32>,
    pub signals: [SignalProduct; 3],
    pub lines: Vec<LineProduct>,
    /// Lines that were *rejected* (flagged terrestrial / transient), kept after they
    /// stop being visible: the final interpretation must remember what it ruled out.
    pub rejected: Vec<LineProduct>,
    pub events: Vec<Event>,
    /// Number of completed dynamic-spectrum segments.
    pub dyn_rows: usize,
}

impl Checkpoint {
    pub fn empty() -> Checkpoint {
        Checkpoint {
            k: 0,
            n: 0,
            t: 0.0,
            spec: None,
            chunk_rms: Vec::new(),
            signals: [
                SignalProduct::empty(SigId::Alpha),
                SignalProduct::empty(SigId::Beta),
                SignalProduct::empty(SigId::Gamma),
            ],
            lines: Vec::new(),
            rejected: Vec::new(),
            events: Vec::new(),
            dyn_rows: 0,
        }
    }

    pub fn signal(&self, id: SigId) -> &SignalProduct {
        &self.signals[id.idx()]
    }

    /// Interference currently visible plus everything rejected earlier, one entry per
    /// frequency (the live measurement wins), sorted by frequency.
    pub fn interference(&self) -> Vec<&LineProduct> {
        let mut out: Vec<&LineProduct> = self.lines.iter().collect();
        for r in &self.rejected {
            if !out.iter().any(|l| (l.f - r.f).abs() < 0.1) {
                out.push(r);
            }
        }
        out.sort_by(|a, b| a.f.partial_cmp(&b.f).unwrap_or(std::cmp::Ordering::Equal));
        out
    }
}

/// Dynamic spectrum: whitened, station-averaged power in 32 s segments (50 % overlap).
pub struct DynSpec {
    pub seg_len: usize,
    pub hop: usize,
    pub nbins: usize,
    pub df: f64,
    pub rows: Vec<Vec<f32>>,
}

impl DynSpec {
    fn new(rx: &Receiver) -> DynSpec {
        use crate::dsp::{block_floor, hann, Fft, C};
        let seg_len = 1024;
        let hop = 512;
        let m = 2048;
        let fft = Fft::new(m);
        let win = hann(seg_len);
        let nseg = (crate::scenario::N_TOTAL - seg_len) / hop + 1;
        let mut rows = Vec::with_capacity(nseg);
        let mut buf = vec![C::ZERO; m];
        for j in 0..nseg {
            let mut acc = vec![0.0f64; m / 2 + 1];
            for s in 0..3 {
                let x = rx.station(s);
                let seg = &x[j * hop..j * hop + seg_len];
                let mean: f64 = seg.iter().map(|&v| v as f64).sum::<f64>() / seg_len as f64;
                for (i, b) in buf.iter_mut().enumerate() {
                    *b = if i < seg_len {
                        C::new((seg[i] as f64 - mean) * win[i], 0.0)
                    } else {
                        C::ZERO
                    };
                }
                fft.forward(&mut buf);
                let p: Vec<f64> = (0..=m / 2).map(|i| buf[i].norm2()).collect();
                let fl = block_floor(&p, 48);
                for i in 0..=m / 2 {
                    acc[i] += p[i] / fl[i].max(1e-300) / 3.0;
                }
            }
            // max-pool pairs of bins -> 512 bins of 31.25 mHz
            let nbins = m / 4;
            let row: Vec<f32> = (0..nbins)
                .map(|b| acc[2 * b].max(acc[2 * b + 1]) as f32)
                .collect();
            rows.push(row);
        }
        DynSpec {
            seg_len,
            hop,
            nbins: m / 4,
            df: FS / m as f64 * 2.0,
            rows,
        }
    }

    /// Number of segments fully contained in the first `n` samples.
    pub fn rows_at(&self, n: usize) -> usize {
        if n < self.seg_len {
            0
        } else {
            (((n - self.seg_len) / self.hop) + 1).min(self.rows.len())
        }
    }
}

/// The observatory: receiver, precomputed baseband/dynamic spectrum, and the lazily
/// extended chain of checkpoints.
pub struct Engine {
    pub rx: Receiver,
    bb: Baseband,
    pub dynspec: DynSpec,
    cps: Mutex<Vec<Arc<Checkpoint>>>,
}

static SHARED: OnceLock<Mutex<HashMap<u64, Arc<Engine>>>> = OnceLock::new();

impl Engine {
    pub fn new(seed: u64) -> Engine {
        Engine::from_receiver(Receiver::new(seed))
    }

    pub fn from_receiver(rx: Receiver) -> Engine {
        let bb = Baseband::new(&rx);
        let dynspec = DynSpec::new(&rx);
        Engine {
            rx,
            bb,
            dynspec,
            cps: Mutex::new(vec![Arc::new(Checkpoint::empty())]),
        }
    }

    /// Compute every checkpoint in a background thread so interactive seeks never
    /// wait on the DSP. Checkpoints are a pure function of the seed, so racing the UI
    /// thread (both go through the same lock) cannot change any result.
    pub fn prewarm(self: &Arc<Self>) {
        let eng = Arc::clone(self);
        std::thread::spawn(move || {
            for k in 1..=N_CP {
                eng.checkpoint(k);
            }
        });
    }

    /// Process-wide cache so that views, tests and captures share one engine per seed.
    pub fn shared(seed: u64) -> Arc<Engine> {
        let map = SHARED.get_or_init(|| Mutex::new(HashMap::new()));
        let mut g = map.lock().unwrap_or_else(|e| e.into_inner());
        g.entry(seed)
            .or_insert_with(|| Arc::new(Engine::new(seed)))
            .clone()
    }

    pub fn seed(&self) -> u64 {
        self.rx.scn.seed
    }

    /// Checkpoint `k` (0 = nothing received yet), computing predecessors as needed.
    /// `k` is clamped to `N_CP`.
    pub fn checkpoint(&self, k: usize) -> Arc<Checkpoint> {
        let k = k.min(N_CP);
        let mut g = self.cps.lock().unwrap_or_else(|e| e.into_inner());
        while g.len() <= k {
            let next = g.len();
            let cp = analyze(self, g.last().map(|c| c.as_ref()), next);
            g.push(Arc::new(cp));
        }
        g[k].clone()
    }

    /// Checkpoint available at sample index `n`.
    pub fn checkpoint_at_sample(&self, n: usize) -> Arc<Checkpoint> {
        self.checkpoint(n / CHUNK)
    }

    /// Drop all cached checkpoints (used by tests to prove cold == warm).
    pub fn reset(&self) {
        let mut g = self.cps.lock().unwrap_or_else(|e| e.into_inner());
        g.truncate(1);
    }
}

// ---------------------------------------------------------------------------
// the per-checkpoint analysis
// ---------------------------------------------------------------------------

fn comb_params(id: SigId) -> (std::ops::RangeInclusive<usize>, (f64, f64), usize, usize) {
    match id {
        SigId::Alpha => (1..=H_A, BAND_A, H_A, HD_A),
        SigId::Gamma => (1..=H_C, BAND_C, H_C, HD_C),
        SigId::Beta => unreachable!(),
    }
}

struct Hyp {
    model: Model,
    s_search: f64,
    log10fap: f64,
    nh: usize,
    nd: usize,
    /// Gamma shape of the null for the search statistic (3 · harmonics).
    f_assoc: f64,
}

fn analyze(eng: &Engine, prev: Option<&Checkpoint>, k: usize) -> Checkpoint {
    let n = k * CHUNK;
    let rx = &eng.rx;
    let t = n as f64 / FS;
    let spec = Arc::new(Spectrum::compute(rx, n));
    let empty = Checkpoint::empty();
    let prev = prev.unwrap_or(&empty);

    // total-power timeline (station 0)
    let mut chunk_rms = prev.chunk_rms.clone();
    {
        let x = rx.station(0);
        let lo = (k - 1) * CHUNK;
        let m: f64 = x[lo..n].iter().map(|&v| v as f64).sum::<f64>() / CHUNK as f64;
        let v: f64 = x[lo..n].iter().map(|&v| (v as f64 - m).powi(2)).sum::<f64>() / CHUNK as f64;
        chunk_rms.push(v.sqrt() as f32);
    }

    // ---- narrowband lines first: classify, so interference can be masked ----
    let found = find_lines(&spec, LINE_THR, 0.05, 0.15);
    let mut analysed: Vec<LineProduct> = found
        .iter()
        .filter(|l| l.f < chirp::B_BAND.0 || l.f > chirp::B_BAND.1)
        .take(10)
        .map(|l| analyze_line(eng, &spec, n, l.f, l.w))
        .collect();
    let tol_cells = 0.04 + 6.0 / t.max(1.0);
    let mask: Vec<(f64, f64)> = analysed
        .iter()
        .filter(|l| l.class == LineClass::Terrestrial)
        .map(|l| (l.f - tol_cells, l.f + tol_cells))
        .collect();
    let cfg = HsCfg {
        mask: &mask,
        clip: HS_CLIP,
    };

    // ---- hypotheses ----
    let mut hyps: [Option<Hyp>; 3] = [None, None, None];
    for id in [SigId::Alpha, SigId::Gamma] {
        let (_, band, hs, hd) = comb_params(id);
        let (pk, _) = harmonic_sum(&spec, band, hs, cfg);
        let lf = log10_fap(3 * hs, pk.s, pk.trials);
        hyps[id.idx()] = Some(Hyp {
            model: Model { f0: pk.f, fdot: 0.0 },
            s_search: pk.s,
            log10fap: lf,
            nh: hs,
            nd: hd,
            f_assoc: pk.f,
        });
    }
    if let Some(fit) = chirp::dechirp(&eng.bb, n) {
        let lf = log10_fap(3, fit.s, fit.trials);
        hyps[SigId::Beta.idx()] = Some(Hyp {
            model: fit.model,
            s_search: fit.s,
            log10fap: lf,
            nh: 1,
            nd: 1,
            f_assoc: fit.f_ref,
        });
    }

    // ---- per-signal products ----
    let mut events = prev.events.clone();
    let mut signals = [
        prev.signals[0].clone(),
        prev.signals[1].clone(),
        prev.signals[2].clone(),
    ];
    for id in SigId::ALL {
        let p = &mut signals[id.idx()];
        let Some(h) = &hyps[id.idx()] else { continue };
        build_signal(eng, &spec, k, id, h, cfg, p, &mut events);
    }

    // ---- unassociated lines (those no comb hypothesis explains) ----
    let combs: Vec<(f64, usize)> = [SigId::Alpha, SigId::Gamma]
        .iter()
        .filter_map(|&i| {
            let p = &signals[i.idx()];
            let h = hyps[i.idx()].as_ref()?;
            if p.stage >= Stage::Candidate || p.log10fap <= -1.0 {
                Some((h.model.f0, if i == SigId::Alpha { 12 } else { 8 }))
            } else {
                None
            }
        })
        .collect();
    let tol = 0.04 + 6.0 / t.max(1.0);
    analysed.retain(|l| {
        if l.class == LineClass::Terrestrial {
            return true; // interference is always shown, whatever combs it resembles
        }
        !combs.iter().any(|&(f0, hmax)| {
            (1..=hmax).any(|hh| {
                [1.0, 0.5]
                    .iter()
                    .any(|sub| (l.f - hh as f64 * f0 * sub).abs() < tol)
            })
        })
    });
    analysed.truncate(4);
    analysed.sort_by(|a, b| a.f.partial_cmp(&b.f).unwrap_or(std::cmp::Ordering::Equal));
    for lp in &analysed {
        let kind = match lp.class {
            LineClass::Terrestrial => Some(EventKind::FlagTerrestrial),
            LineClass::Transient => Some(EventKind::FlagTransient),
            LineClass::Unassigned => None,
        };
        if let Some(kind) = kind {
            let seen = events
                .iter()
                .any(|e| e.kind == kind && matches!(e.who, Who::Line(f) if (f - lp.f).abs() < 0.1));
            if !seen {
                events.push(Event {
                    cp: k,
                    kind,
                    who: Who::Line(lp.f),
                });
            }
        }
    }
    let lines = analysed;
    let mut rejected = prev.rejected.clone();
    for lp in lines.iter().filter(|l| l.class != LineClass::Unassigned) {
        match rejected.iter_mut().find(|r| (r.f - lp.f).abs() < 0.1) {
            Some(slot) => *slot = lp.clone(),
            None => rejected.push(lp.clone()),
        }
    }

    Checkpoint {
        k,
        n,
        t,
        spec: Some(spec),
        chunk_rms,
        signals,
        lines,
        rejected,
        events,
        dyn_rows: eng.dynspec.rows_at(n),
    }
}

/// Coherent follow-up of one hypothesis, and the lock state machine.
fn build_signal(
    eng: &Engine,
    spec: &Spectrum,
    k: usize,
    id: SigId,
    h: &Hyp,
    cfg: HsCfg,
    p: &mut SignalProduct,
    events: &mut Vec<Event>,
) {
    let rx = &eng.rx;
    let n = k * CHUNK;
    let nf = n as f64;
    let t_now = nf / FS;
    let nh = h.nh;
    let prev_f = p.f_now;
    let prev_model = p.model;
    // Polish the search estimate by coherent maximum likelihood. The noise level for
    // normalisation is read at the *search* frequency (it varies slowly).
    let f_search = if id == SigId::Beta {
        h.model.freq(t_now / 2.0)
    } else {
        h.model.f0
    };
    let sig2_search = |s: usize, hh: usize| spec.sigma2(s, f_search * hh as f64);
    let model = coherent::refine_model(
        rx,
        h.model,
        nh,
        n,
        id == SigId::Beta,
        &sig2_search,
    );

    p.model = Some(model);
    p.nh = nh;
    p.s_search = h.s_search;
    p.log10fap = h.log10fap;
    p.sigma_eq = sigma_from_log10p(h.log10fap);
    p.conf = (-h.log10fap / 10.0).clamp(0.0, 1.0);

    // Reference frequency for noise normalisation / delay phase relation: for a
    // chirp the data are uniformly weighted over [0, T], so ⟨f⟩ = f(T/2).
    let f_eff = if id == SigId::Beta {
        model.freq(t_now / 2.0)
    } else {
        model.f0
    };
    p.f_now = model.freq(t_now);

    let cum = coherent::cumulative(rx, model, nh, n);
    let last = *cum.last().expect("at least one chunk");
    let sig2 = |s: usize, hh: usize| spec.sigma2(s, f_eff * hh as f64);

    // growth curve + coherent S/N (null mean of S is 3·nh)
    let mut growth = Vec::with_capacity(cum.w.len());
    let mut s_eff = 0.0; // CRLB weight: Σ h² (S_debiased)
    for (j, w) in cum.w.iter().enumerate() {
        let nj = ((j + 1) * CHUNK) as f64;
        let mut s_tot = 0.0;
        for s in 0..3 {
            for hh in 0..nh {
                s_tot += w[s][hh].norm2() / (nj * sig2(s, hh + 1).max(1e-12));
            }
        }
        growth.push((nj / FS, (s_tot - 3.0 * nh as f64).max(0.0).sqrt()));
    }
    for s in 0..3 {
        for hh in 0..nh {
            let sn = last[s][hh].norm2() / (nf * sig2(s, hh + 1).max(1e-12));
            s_eff += ((hh + 1) as f64).powi(2) * (sn - 1.0).max(0.0);
        }
    }
    p.growth = growth;
    // Cramér–Rao σ of the mid-span frequency; for a chirp the frequency *now* is the
    // end-epoch value, whose variance carries the jointly-estimated drift: ×4.
    let crlb = if s_eff > 0.5 {
        (6.0 / s_eff).sqrt() / (TAU * t_now)
    } else {
        f64::INFINITY
    };
    p.f_sigma = if id == SigId::Beta { 4.0 * crlb } else { crlb };

    // walk (station 0, fundamental), unit-variance steps
    let sc = (sig2(0, 1).max(1e-12) * CHUNK as f64 / 2.0).sqrt();
    p.walk = cum
        .w
        .iter()
        .map(|w| (w[0][0].re / sc, w[0][0].im / sc))
        .collect();
    p.walk.insert(0, (0.0, 0.0));

    // waveform estimate at station 0
    p.harm = (0..nh)
        .map(|hh| {
            (
                2.0 * last[0][hh].abs() / nf,
                last[0][hh].arg(),
            )
        })
        .collect();

    // delays and sky
    p.delays = coherent::delays(&last, n, h.nd, f_eff, &sig2);
    p.sky = p.delays.as_ref().map(coherent::sky_from_delays);

    // fold
    let taus = match &p.delays {
        Some(d) => [0.0, d.tau[0], d.tau[1]],
        None => [0.0; 3],
    };
    let theta = last[0][0].arg();
    let rot = theta / TAU + 0.5;
    let (prof, err) = coherent::fold(rx, model, taus, rot, n, NB_FOLD);
    p.profile = prof;
    p.profile_err = err;
    p.waterfall = coherent::waterfall(rx, model, taus, rot, n, NB_WF, 2);
    p.rival = if id != SigId::Beta {
        let half = model.scaled(0.5);
        let rot_r = theta / (2.0 * TAU) + 0.25;
        let (rp, _) = coherent::fold(rx, half, taus, rot_r, n, 2 * NB_FOLD);
        Some(Rival {
            label: "2P",
            profile: rp,
            s_rival: hs_at(spec, model.f0 * 0.5, nh, cfg),
            s_best: hs_at(spec, model.f0, nh, cfg),
        })
    } else {
        None
    };

    // ---- lock state machine (history-dependent, hysteresis) ----
    let lf = h.log10fap;
    let stable = match (prev_model, prev_f.is_finite()) {
        (Some(_), true) => {
            let df = (p.f_now - prev_f).abs();
            let sig = if p.f_sigma.is_finite() { p.f_sigma } else { f64::INFINITY };
            // stable = moved by less than the larger of 3σ and 1.5 resolution cells
            df <= (3.0 * sig).max(1.5 / t_now.max(1.0))
        }
        _ => false,
    };
    let _ = h.f_assoc;
    match p.stage {
        Stage::Unseen => {
            if lf <= CAND_LF {
                p.stage = Stage::Candidate;
                p.cand_cp = Some(k);
                events.push(Event {
                    cp: k,
                    kind: EventKind::Candidate,
                    who: Who::Sig(id),
                });
            }
        }
        _ => {}
    }
    if p.stage == Stage::Candidate {
        // a candidate whose frequency jumps far outside its own error bar has been
        // *revised*: the structure it was built on turned out to be something else
        if prev_f.is_finite() && p.cand_cp != Some(k) {
            let sig = if p.f_sigma.is_finite() { p.f_sigma } else { 1.0 };
            if (p.f_now - prev_f).abs() > 12.0 * sig.max(0.01 / t_now.max(1.0)) + 0.02 {
                events.push(Event {
                    cp: k,
                    kind: EventKind::Revised,
                    who: Who::Sig(id),
                });
            }
        }
        if lf <= LOCK_LF && stable {
            p.lock_run += 1;
        } else if lf > LOCK_LF {
            p.lock_run = 0;
        }
        if p.lock_run >= 2 {
            p.stage = Stage::Locked;
            p.lock_cp = Some(k);
            events.push(Event {
                cp: k,
                kind: EventKind::Lock,
                who: Who::Sig(id),
            });
        } else if lf > -0.5 {
            // candidate evaporated: back to search (a misleading candidate)
            p.stage = Stage::Unseen;
            p.lock_run = 0;
        }
    }
    if p.stage >= Stage::Locked {
        if let Some(sky) = &p.sky {
            if p.stage == Stage::Locked && sky.physical && sky.sig_major <= LOCALIZED_SIG {
                p.stage = Stage::Localized;
                p.loc_cp = Some(k);
                events.push(Event {
                    cp: k,
                    kind: EventKind::Localized,
                    who: Who::Sig(id),
                });
            }
            if p.stage == Stage::Localized && sky.sig_major <= RESOLVED_SIG {
                p.stage = Stage::Resolved;
                p.res_cp = Some(k);
                events.push(Event {
                    cp: k,
                    kind: EventKind::Resolved,
                    who: Who::Sig(id),
                });
            }
        }
    }
    // Before a candidate exists the posterior is the prior (uniform sky): do not
    // expose a noise-driven position as a measurement.
    if p.stage == Stage::Unseen {
        p.sky = None;
    }
}

/// Coherent analysis of one spectral line (fixed frequency, no drift): growth curve,
/// phasor walk, inter-station amplitude uniformity and delays, classification.
fn analyze_line(eng: &Engine, spec: &Spectrum, n: usize, f: f64, w_peak: f64) -> LineProduct {
    let rx = &eng.rx;
    let model = Model { f0: f, fdot: 0.0 };
    let cum = coherent::cumulative(rx, model, 1, n);
    let last = *cum.last().expect("chunk");
    let sig2 = |s: usize, _h: usize| spec.sigma2(s, f);
    let mut growth = Vec::new();
    let mut s_last = 0.0;
    for (j, w) in cum.w.iter().enumerate() {
        let nj = ((j + 1) * CHUNK) as f64;
        let mut s_tot = 0.0;
        for s in 0..3 {
            s_tot += w[s][0].norm2() / (nj * sig2(s, 1).max(1e-12));
        }
        s_last = s_tot;
        growth.push((nj / FS, (s_tot - 3.0).max(0.0).sqrt()));
    }
    let trials = (spec.nbins() as f64 / 4.0).max(1.0);
    let lf = log10_fap(3, s_last, trials);
    let d = coherent::delays(&last, n, 1, f, &sig2);
    let (amp_z, amp_ratio, sky) = match &d {
        Some(d) => (d.amp_z, d.amp_ratio, Some(coherent::sky_from_delays(d))),
        None => (0.0, [1.0; 2], None),
    };
    let sc = (sig2(0, 1).max(1e-12) * CHUNK as f64 / 2.0).sqrt();
    let mut walk: Vec<(f64, f64)> = cum
        .w
        .iter()
        .map(|w| (w[0][0].re / sc, w[0][0].im / sc))
        .collect();
    walk.insert(0, (0.0, 0.0));
    let max_snr = growth.iter().map(|g| g.1).fold(0.0, f64::max);
    let cur = growth.last().map(|g| g.1).unwrap_or(0.0);
    let class = if amp_z >= 4.0 {
        LineClass::Terrestrial
    } else if max_snr >= 3.0 && cur < 0.85 * max_snr {
        LineClass::Transient
    } else {
        LineClass::Unassigned
    };
    LineProduct {
        label: format!("X{:.1}", f),
        f,
        w_peak,
        class,
        s_tot: s_last,
        log10fap: lf,
        amp_z,
        amp_ratio,
        growth,
        walk,
        sky,
    }
}

/// Helper namespace re-export so `analysis::FsHelper` is not needed by callers.
#[allow(unused)]
pub(crate) const _HMAX: usize = HMAX;
