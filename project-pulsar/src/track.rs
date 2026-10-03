//! The analysis pipeline: per-epoch candidate detection (harmonic-summed
//! periodogram), frequency tracking, coherence / persistence statistics,
//! lock acquisition, rejection of false candidates, inter-station delays and
//! the resulting sky localisation (including the mirror ambiguity of a
//! planar array and its resolution once station 4 is online).
//!
//! `Timeline::build` runs the whole pipeline once, causally: the solution
//! published at epoch `k` only ever reads samples `[0, k * EPOCH_SAMPLES)`.

use crate::dsp::{self, Clean};
use crate::sim::{
    EPOCH_SAMPLES, FS, N, N_EPOCHS, S4_ON_SAMPLE, SLOTS, STATIONS, STATION_POS, TAU_MAX,
};

/// Search slots: each owns a frequency band (a channelised search).
pub const SLOT_BANDS: [(f64, f64); SLOTS] = [
    (1.15, 1.45), // A
    (1.90, 2.50), // B
    (0.45, 0.80), // C
    (2.80, 3.30), // D
    (4.00, 5.00), // E
];

/// Detection threshold on the 4-term harmonic sum (noise mean ~ 4, tail tuned by
/// the noise-only false-alarm test).
pub const HS_THRESHOLD: f64 = 30.0;
const HARMONICS_SEARCH: usize = 4;
const HARMONICS_Z: usize = 4;
pub const Z_TRACK: f64 = 4.0;
pub const Z_LOCK: f64 = 8.0;
pub const PULSY_MIN: f64 = 0.12;
pub const PULSY_CW: f64 = 0.10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum State {
    /// No spectral evidence in this slot.
    Quiet,
    /// A spectral peak exists but has not been confirmed in the time domain.
    Candidate,
    /// Frequency is being followed and coherent power is building.
    Tracking,
    /// Lock acquired: coherent, pulsed, persistent and localisable.
    Locked,
    /// Rejected as continuous-wave interference (sinusoidal, common-mode).
    RejectedCw,
    /// Rejected as a transient that did not persist.
    RejectedTransient,
}

impl State {
    pub fn label(self) -> &'static str {
        match self {
            State::Quiet => "QUIET",
            State::Candidate => "CANDIDATE",
            State::Tracking => "TRACKING",
            State::Locked => "LOCKED",
            State::RejectedCw => "RFI",
            State::RejectedTransient => "TRANSIENT",
        }
    }
    pub fn is_active(self) -> bool {
        self != State::Quiet
    }
    pub fn is_rejected(self) -> bool {
        matches!(self, State::RejectedCw | State::RejectedTransient)
    }
}

/// Inter-station delay measurement for one slot at one epoch.
#[derive(Clone, Copy, Debug)]
pub struct DelayFix {
    /// tau_21, tau_31, tau_41 (s). `NaN` where not measurable.
    pub tau: [f64; 3],
    pub sigma: [f64; 3],
}

/// Sky localisation (array frame) derived from the delays.
#[derive(Clone, Copy, Debug)]
pub struct SkyFix {
    /// In-plane direction cosines.
    pub sx: f64,
    pub sy: f64,
    /// Covariance of (sx, sy): var_x, var_y, cov_xy.
    pub cov: [f64; 3],
    /// The measured in-plane solution lay outside the unit disc and was clamped.
    pub clamped: bool,
    /// Upper-hemisphere solution (degrees).
    pub az: f64,
    pub el: f64,
    /// Probability that the source is in the upper (z > 0) hemisphere.
    pub w_plus: f64,
    /// Whether station 4 contributed to `w_plus`.
    pub resolved_by_s4: bool,
    /// Approximate 1-sigma radius of the solution (deg).
    pub sigma_deg: f64,
}

impl SkyFix {
    /// Most probable (az, el) in degrees, honouring the mirror weights.
    pub fn best(&self) -> (f64, f64) {
        if self.w_plus >= 0.5 {
            (self.az, self.el)
        } else {
            (self.az, -self.el)
        }
    }
    pub fn confidence_of_best(&self) -> f64 {
        self.w_plus.max(1.0 - self.w_plus)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SlotEpoch {
    pub state: State,
    pub f: f64,
    pub f_sigma: f64,
    /// Gaussian-equivalent significance of the coherent harmonic power.
    pub z: f64,
    /// Fraction of excess power in harmonics 2..H (pulse-likeness, 0 for a sinusoid).
    pub pulsy: f64,
    /// Recent-window / expected power ratio (1 steady, 0 gone).
    pub persist: f64,
    /// Phase (cycles) of the fold peak at `f`.
    pub peak_phase: f64,
    /// The same peak measured on the most recent half of the data only, so that a
    /// small frequency error has not yet accumulated into a phase drift: this is
    /// the phase to extrapolate pulse arrival times from near "now".
    pub peak_phase_recent: f64,
    /// Boxcar fold significance.
    pub fold_snr: f64,
    pub delays: Option<DelayFix>,
    pub sky: Option<SkyFix>,
    /// Delays consistent with zero at useful precision (local origin).
    pub common_mode: bool,
    /// The coherent excess is shared by both halves of the data.
    pub stationary: bool,
    /// HS strength of the spectral detection this epoch (0 if none).
    pub detection: f64,
}

impl SlotEpoch {
    fn quiet() -> SlotEpoch {
        SlotEpoch {
            state: State::Quiet,
            f: 0.0,
            f_sigma: f64::INFINITY,
            z: 0.0,
            pulsy: 0.0,
            persist: 0.0,
            peak_phase: 0.0,
            peak_phase_recent: 0.0,
            fold_snr: 0.0,
            delays: None,
            sky: None,
            common_mode: false,
            stationary: false,
            detection: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Epoch {
    /// 1-based epoch index; the solution reads samples `[0, n)`.
    pub k: usize,
    pub n: usize,
    pub t: f64,
    pub slots: [SlotEpoch; SLOTS],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventKind {
    Detected,
    Tracking,
    Locked,
    RejectedCw,
    RejectedTransient,
    /// The mirror ambiguity of a locked source collapsed.
    MirrorResolved,
    /// Station 4 came online.
    StationOnline,
    /// A candidate turned out to be a harmonic / octave ghost of another one.
    Withdrawn,
}

#[derive(Clone, Copy, Debug)]
pub struct Event {
    pub epoch: usize,
    pub t: f64,
    pub slot: Option<usize>,
    pub kind: EventKind,
}

impl Event {
    pub fn describe(&self) -> &'static str {
        match self.kind {
            EventKind::Detected => "spectral candidate",
            EventKind::Tracking => "tracking",
            EventKind::Locked => "LOCK acquired",
            EventKind::RejectedCw => "rejected: CW interference",
            EventKind::RejectedTransient => "rejected: transient",
            EventKind::MirrorResolved => "mirror ambiguity resolved",
            EventKind::StationOnline => "station S4 online",
            EventKind::Withdrawn => "withdrawn: harmonic / octave ghost",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Timeline {
    pub epochs: Vec<Epoch>,
    pub events: Vec<Event>,
}

// ───────────────────────────── sky geometry ──────────────────────────────────

/// Solve the in-plane direction cosines from (tau_21, tau_31):
/// `-tau_k = b_k . s` for stations 2, 3 (both in the z = 0 plane).
pub fn solve_inplane(tau2: f64, tau3: f64) -> (f64, f64) {
    let b2 = STATION_POS[1];
    let b3 = STATION_POS[2];
    let det = b2[0] * b3[1] - b2[1] * b3[0];
    let (r2, r3) = (-tau2, -tau3);
    let sx = (r2 * b3[1] - r3 * b2[1]) / det;
    let sy = (b2[0] * r3 - b3[0] * r2) / det;
    (sx, sy)
}

/// `B^-1` for the in-plane baselines (rows map (r2, r3) -> (sx, sy)).
fn inplane_inverse() -> [[f64; 2]; 2] {
    let b2 = STATION_POS[1];
    let b3 = STATION_POS[2];
    let det = b2[0] * b3[1] - b2[1] * b3[0];
    [[b3[1] / det, -b2[1] / det], [-b3[0] / det, b2[0] / det]]
}

fn sky_fix(delays: &DelayFix) -> Option<SkyFix> {
    let (t2, t3) = (delays.tau[0], delays.tau[1]);
    if !t2.is_finite() || !t3.is_finite() {
        return None;
    }
    let (mut sx, mut sy) = solve_inplane(t2, t3);
    let mut clamped = false;
    let r = sx.hypot(sy);
    if r > 0.999 {
        sx *= 0.999 / r;
        sy *= 0.999 / r;
        clamped = true;
    }
    let sz = (1.0 - sx * sx - sy * sy).max(1e-6).sqrt();
    let inv = inplane_inverse();
    let (v2, v3) = (delays.sigma[0].powi(2), delays.sigma[1].powi(2));
    let c23 = 0.5 * (v2 * v3).sqrt();
    // Sigma_s = Binv Sigma_tau Binv^T
    let a = inv;
    let var_x = a[0][0] * a[0][0] * v2 + 2.0 * a[0][0] * a[0][1] * c23 + a[0][1] * a[0][1] * v3;
    let var_y = a[1][0] * a[1][0] * v2 + 2.0 * a[1][0] * a[1][1] * c23 + a[1][1] * a[1][1] * v3;
    let cov = a[0][0] * a[1][0] * v2
        + (a[0][0] * a[1][1] + a[0][1] * a[1][0]) * c23
        + a[0][1] * a[1][1] * v3;
    let az = sy.atan2(sx).to_degrees().rem_euclid(360.0);
    let el = sz.asin().to_degrees();
    // Mirror weights from station 4.
    let mut w_plus = 0.5;
    let mut resolved = false;
    if delays.tau[2].is_finite() {
        let b4 = STATION_POS[3];
        let tau4_m = delays.tau[2];
        let inplane = -(b4[0] * sx + b4[1] * sy);
        let pred_p = inplane - b4[2] * sz;
        let pred_m = inplane + b4[2] * sz;
        let var_xy = b4[0] * b4[0] * var_x + b4[1] * b4[1] * var_y + 2.0 * b4[0] * b4[1] * cov;
        let var_z = (b4[2] * (sx * var_x.sqrt() + sy * var_y.sqrt()) / sz).powi(2);
        let var = delays.sigma[2].powi(2) + var_xy + var_z;
        let ll_p = -(tau4_m - pred_p).powi(2) / (2.0 * var);
        let ll_m = -(tau4_m - pred_m).powi(2) / (2.0 * var);
        let d = (ll_m - ll_p).clamp(-40.0, 40.0);
        w_plus = 1.0 / (1.0 + d.exp());
        resolved = true;
    }
    // 1-sigma sky radius: the largest angular distance from the best fix to the
    // boundary of the 1-sigma ellipse of direction cosines, mapped onto the sphere
    // (so a source near the array plane honestly reports a long, thin posterior).
    let sigma_deg = {
        let a = var_x.max(1e-14).sqrt();
        let b = cov / a;
        let c = (var_y - b * b).max(1e-14).sqrt();
        let mut worst = 0.0f64;
        for i in 0..24 {
            let th = std::f64::consts::TAU * i as f64 / 24.0;
            let (mut qx, mut qy) = (sx + a * th.cos(), sy + b * th.cos() + c * th.sin());
            let r2 = qx * qx + qy * qy;
            if r2 > 0.998 {
                let k = (0.998 / r2).sqrt();
                qx *= k;
                qy *= k;
            }
            let qz = (1.0 - qx * qx - qy * qy).max(1e-6).sqrt();
            let qaz = qy.atan2(qx).to_degrees().rem_euclid(360.0);
            let qel = qz.asin().to_degrees();
            worst = worst.max(crate::sim::separation_deg((az, el), (qaz, qel)));
        }
        worst
    };
    Some(SkyFix {
        sx,
        sy,
        cov: [var_x, var_y, cov],
        clamped,
        az,
        el,
        w_plus,
        resolved_by_s4: resolved,
        sigma_deg,
    })
}

// ──────────────────────────── delay measurement ─────────────────────────────

/// Estimate `tau_k1` at frequency `f` from the harmonic phases of stream `k`
/// against stream 1 over samples `[start, end)`. Returns `(tau, sigma)`.
fn measure_delay(
    ref_clean: &Clean,
    other: &Clean,
    start: usize,
    end: usize,
    f: f64,
) -> Option<(f64, f64)> {
    let hmax = (((0.45 / (f * TAU_MAX)).floor()) as usize).clamp(1, 12);
    let (xr, nr) = dsp::fourier_sums(ref_clean, start, end, f, hmax);
    let (xo, no) = dsp::fourier_sums(other, start, end, f, hmax);
    if nr < 64 || no < 64 {
        return None;
    }
    let n_eff = nr.min(no) as f64;
    let sigma = 0.5 * (ref_clean.sigma + other.sigma);
    let (mut wsum, mut tsum) = (0.0, 0.0);
    for h in 1..=hmax {
        let (ar, ai) = xr[h - 1];
        let (br, bi) = xo[h - 1];
        // arg(X_k conj(X_1)) = -2 pi h f tau
        let cr = br * ar + bi * ai;
        let ci = bi * ar - br * ai;
        let dphi = ci.atan2(cr);
        let tau_h = -dphi / (std::f64::consts::TAU * h as f64 * f);
        let mag2 = (ar * ar + ai * ai) * 4.0 / (n_eff * n_eff) - 2.0 * sigma * sigma / n_eff;
        let amp2 = mag2.max(0.05 * 2.0 * sigma * sigma / n_eff);
        // var(phi_h) per station = sigma^2 * 2 / (N a^2); difference of two stations doubles it.
        let var_phi = 2.0 * 2.0 * sigma * sigma / (n_eff * amp2);
        let var_t = var_phi / (std::f64::consts::TAU * h as f64 * f).powi(2);
        let w = 1.0 / var_t;
        wsum += w;
        tsum += w * tau_h;
    }
    Some((tsum / wsum, (1.0 / wsum).sqrt()))
}

// ───────────────────────────── the pipeline ─────────────────────────────────

fn in_band(slot: usize, f: f64) -> bool {
    f >= SLOT_BANDS[slot].0 && f <= SLOT_BANDS[slot].1
}

/// A spectral detection (harmonic-sum strength `hs` at frequency `f`).
#[derive(Clone, Copy, Debug)]
pub struct Peak {
    pub f: f64,
    pub hs: f64,
    /// Power of the single spectral line at `f` (multiples of the floor).
    pub s1: f64,
}

/// A line this strong at `f` itself proves `f` is a real component (not an
/// octave ghost that only exists because its multiples carry power).
pub const S1_LINE: f64 = 8.0;

/// Harmonic-summed detections with an octave / harmonic sieve.
pub fn detect(clean: &Clean, n: usize) -> Vec<Peak> {
    detect_with(clean, n, HS_THRESHOLD)
}

/// [`detect`] with an explicit threshold (used by the false-alarm calibration).
pub fn detect_with(clean: &Clean, n: usize, threshold: f64) -> Vec<Peak> {
    let spec = dsp::periodogram(clean, n, N);
    detect_spec(&spec, n, threshold)
}

/// Single-line power (multiples of the floor) around frequency `f`.
pub fn line_power(spec: &dsp::Spectrum, f: f64) -> f64 {
    let k = spec.bin(f).clamp(1, spec.power.len() - 2);
    spec.power[k - 1..=k + 1]
        .iter()
        .cloned()
        .fold(0.0, f64::max)
}

/// Detections from an already computed periodogram.
pub fn detect_spec(spec: &dsp::Spectrum, n: usize, threshold: f64) -> Vec<Peak> {
    let df = spec.df;
    let k_lo = (0.40 / df) as usize;
    let k_hi = (5.2 / df) as usize;
    let hs = dsp::harmonic_sum(spec, HARMONICS_SEARCH, k_lo, k_hi);
    let t_obs = n as f64 / FS;
    let lobe = ((0.6 / t_obs) / df).round().max(1.0) as usize;
    let mut peaks: Vec<Peak> = Vec::new();
    for k in k_lo.max(lobe)..k_hi.min(hs.len().saturating_sub(lobe)) {
        let v = hs[k];
        if v < threshold {
            continue;
        }
        let lo = k.saturating_sub(lobe);
        let hi = (k + lobe).min(hs.len() - 1);
        let is_max = !hs[lo..=hi]
            .iter()
            .enumerate()
            .any(|(d, &hj)| hj > v || (hj == v && lo + d < k));
        if is_max {
            // parabolic refinement on the HS lobe
            let (a, b, c) = (hs[k - 1], hs[k], hs[k + 1]);
            let den = a - 2.0 * b + c;
            let off = if den.abs() > 1e-12 {
                (0.5 * (a - c) / den).clamp(-0.5, 0.5)
            } else {
                0.0
            };
            let s1 = line_power(spec, (k as f64 + off) * df);
            peaks.push(Peak {
                f: (k as f64 + off) * df,
                hs: v,
                s1,
            });
        }
    }
    // Harmonic / octave sieve. Strongest first; a candidate related to an accepted
    // one by a factor 2..4 is either a harmonic (dropped), an octave GHOST (dropped)
    // or the true fundamental (it replaces the member that only existed because
    // its multiples carry power). The deciding evidence is the single spectral
    // line at the candidate's own frequency: ghosts have none.
    peaks.sort_by(|a, b| b.hs.total_cmp(&a.hs));
    let tol = (2.5 / t_obs).max(0.004);
    let mut accepted: Vec<Peak> = Vec::new();
    for p in peaks {
        enum Act {
            Accept,
            Reject,
            Replace(usize),
        }
        let mut act = Act::Accept;
        'rel: for (idx, a) in accepted.iter().enumerate() {
            for h in 2..=4 {
                let hf = h as f64;
                let sub = (p.f - a.f / hf).abs() <= tol / hf.sqrt() + 1e-9; // p = a / h
                let sup = (p.f - a.f * hf).abs() <= tol * hf; // p = a * h
                if sub {
                    act = if p.s1 >= S1_LINE && p.hs >= 0.5 * a.hs {
                        Act::Replace(idx)
                    } else {
                        Act::Reject
                    };
                    break 'rel;
                }
                if sup {
                    act = if a.s1 < S1_LINE && p.s1 >= S1_LINE {
                        Act::Replace(idx)
                    } else if p.hs <= 1.15 * a.hs {
                        Act::Reject
                    } else {
                        Act::Accept
                    };
                    break 'rel;
                }
            }
            // Non-integer rational ratios (3/2, 4/3, ...): a candidate with no line of
            // its own that is a small-integer ratio of a stronger one is a cross-
            // harmonic ghost (its 2nd / 4th multiples land on the other's harmonics).
            for (i, j) in [(3.0, 2.0), (2.0, 3.0), (4.0, 3.0), (3.0, 4.0)] {
                if (p.f - a.f * i / j).abs() <= tol * i / j && p.s1 < S1_LINE && p.hs <= 1.25 * a.hs
                {
                    act = Act::Reject;
                    break 'rel;
                }
            }
        }
        match act {
            Act::Accept => accepted.push(p),
            Act::Reject => {}
            Act::Replace(i) => accepted[i] = p,
        }
    }
    accepted
}

#[derive(Clone, Copy)]
struct Tracker {
    state: State,
    f: f64,
    c_track: u32,
    c_lock: u32,
    c_cw: u32,
    c_fade: u32,
    z_max: f64,
    resolved: bool,
    announced: bool,
}

impl Timeline {
    pub fn build(cleans: &[Clean]) -> Timeline {
        assert_eq!(cleans.len(), STATIONS);
        let mut epochs = Vec::with_capacity(N_EPOCHS);
        let mut events: Vec<Event> = Vec::new();
        let mut trackers = [Tracker {
            state: State::Quiet,
            f: 0.0,
            c_track: 0,
            c_lock: 0,
            c_cw: 0,
            c_fade: 0,
            z_max: 0.0,
            resolved: false,
            announced: false,
        }; SLOTS];
        let mut s4_announced = false;

        for k in 1..=N_EPOCHS {
            let n = k * EPOCH_SAMPLES;
            let t = n as f64 / FS;
            let t_obs = t;
            let spec = dsp::periodogram(&cleans[0], n, N);
            let peaks = if k >= 3 {
                detect_spec(&spec, n, HS_THRESHOLD)
            } else {
                Vec::new()
            };
            // Identity hygiene: a tracker that is a harmonic of a lower tracker with a
            // real line of its own (or an octave ghost of a higher one that has none)
            // is the same physical signal seen twice. Withdraw it.
            let live = |st: State| matches!(st, State::Candidate | State::Tracking | State::Locked);
            let snapshot: Vec<(State, f64)> = trackers.iter().map(|t| (t.state, t.f)).collect();
            for i in 0..SLOTS {
                if !live(snapshot[i].0) {
                    continue;
                }
                let fi = snapshot[i].1;
                let tol_i = (2.5 / t_obs).max(0.004);
                let mut dup = false;
                for (j, &(sj, fj)) in snapshot.iter().enumerate() {
                    if j == i || !live(sj) {
                        continue;
                    }
                    for m in 2..=4 {
                        let mf = m as f64;
                        if (fi - fj * mf).abs() <= tol_i * mf && line_power(&spec, fj) >= S1_LINE {
                            dup = true; // i is a harmonic of a genuine lower line j
                        }
                        if (fi - fj / mf).abs() <= tol_i / mf.sqrt()
                            && line_power(&spec, fi) < S1_LINE
                        {
                            dup = true; // i is an octave ghost of a higher line j
                        }
                    }
                }
                if dup {
                    let announce = matches!(trackers[i].state, State::Tracking | State::Locked);
                    trackers[i] = Tracker {
                        state: State::Quiet,
                        f: 0.0,
                        c_track: 0,
                        c_lock: 0,
                        c_cw: 0,
                        c_fade: 0,
                        z_max: 0.0,
                        resolved: false,
                        announced: trackers[i].announced,
                    };
                    if announce {
                        events.push(Event {
                            epoch: k,
                            t,
                            slot: Some(i),
                            kind: EventKind::Withdrawn,
                        });
                    }
                }
            }
            let mut slots = [SlotEpoch::quiet(); SLOTS];

            if !s4_announced && n > S4_ON_SAMPLE {
                s4_announced = true;
                events.push(Event {
                    epoch: k,
                    t,
                    slot: None,
                    kind: EventKind::StationOnline,
                });
            }

            for s in 0..SLOTS {
                let tr = &mut trackers[s];
                // best detection in this slot's band
                let det = peaks
                    .iter()
                    .filter(|p| in_band(s, p.f))
                    .max_by(|a, b| a.hs.total_cmp(&b.hs))
                    .copied();
                let seeded = matches!(tr.state, State::Quiet | State::Candidate);
                let prev_state = tr.state;
                // choose the working frequency
                let (f_seed, half_w) = if seeded {
                    match det {
                        Some(p) => (p.f, 1.0 / t_obs),
                        None => {
                            tr.state = State::Quiet;
                            slots[s] = SlotEpoch::quiet();
                            continue;
                        }
                    }
                } else {
                    (tr.f, (0.5 / t_obs).max(2e-4))
                };
                let (f_est, _) = dsp::refine_frequency(&cleans[0], n, f_seed, half_w, 13, 2);
                tr.f = f_est;
                let c0 = &cleans[0];
                // coherent power over the whole prefix
                let (sums, cnt) = dsp::fourier_sums(c0, 0, n, f_est, 6);
                let zh = dsp::z2_per_harmonic(&sums, cnt, c0.sigma);
                let z2: f64 = zh.iter().take(HARMONICS_Z).sum();
                let z = dsp::chi2_sigma(z2, HARMONICS_Z);
                let ex: Vec<f64> = zh
                    .iter()
                    .take(HARMONICS_Z)
                    .map(|v| (v - 2.0).max(0.0))
                    .collect();
                let total_ex: f64 = ex.iter().sum();
                let pulsy = if total_ex > 1e-9 {
                    ex.iter().skip(1).sum::<f64>() / total_ex
                } else {
                    0.0
                };
                // persistence: power in the most recent 40 % against the expectation
                let start_recent = n - (n as f64 * 0.4) as usize;
                let (rs, rc) = dsp::fourier_sums(c0, start_recent, n, f_est, HARMONICS_Z);
                let rz2: f64 = dsp::z2_per_harmonic(&rs, rc, c0.sigma).iter().sum();
                let expect = 0.4 * (z2 - 2.0 * HARMONICS_Z as f64);
                let persist = if expect > 4.0 {
                    ((rz2 - 2.0 * HARMONICS_Z as f64) / expect).clamp(0.0, 2.0)
                } else {
                    1.0
                };
                // stationarity: the excess must be shared by both halves of the data
                let half = n / 2;
                let (h1, c1) = dsp::fourier_sums(c0, 0, half, f_est, HARMONICS_Z);
                let (h2, c2) = dsp::fourier_sums(c0, half, n, f_est, HARMONICS_Z);
                let e1: f64 = dsp::z2_per_harmonic(&h1, c1, c0.sigma).iter().sum::<f64>()
                    - 2.0 * HARMONICS_Z as f64;
                let e2: f64 = dsp::z2_per_harmonic(&h2, c2, c0.sigma).iter().sum::<f64>()
                    - 2.0 * HARMONICS_Z as f64;
                let stationary = e1 > 0.0 && e2 > 0.0 && e1.min(e2) >= 0.2 * (e1 + e2);
                // fold
                let fo = dsp::fold(c0, 0, n, f_est, 64);
                let fold_snr = fo.snr();
                let peak_phase = fo.peak_phase();
                let peak_phase_recent = dsp::fold(c0, n / 2, n, f_est, 64).peak_phase();
                // delays
                let mut tau = [f64::NAN; 3];
                let mut sig = [f64::INFINITY; 3];
                let mut any = false;
                for (i, st) in [1usize, 2].iter().enumerate() {
                    if let Some((tv, sv)) = measure_delay(c0, &cleans[*st], 0, n, f_est) {
                        tau[i] = tv;
                        sig[i] = sv;
                        any = true;
                    }
                }
                if n > S4_ON_SAMPLE + 256 {
                    if let Some((tv, sv)) = measure_delay(c0, &cleans[3], S4_ON_SAMPLE, n, f_est) {
                        tau[2] = tv;
                        sig[2] = sv;
                    }
                }
                let delays = if any {
                    Some(DelayFix { tau, sigma: sig })
                } else {
                    None
                };
                let sky = delays.as_ref().and_then(sky_fix);
                let common_mode = delays
                    .map(|d| {
                        d.sigma[0] < 0.010
                            && d.sigma[1] < 0.010
                            && d.tau[0].abs() < 2.5 * d.sigma[0]
                            && d.tau[1].abs() < 2.5 * d.sigma[1]
                    })
                    .unwrap_or(false);

                // ── state machine ──
                tr.z_max = tr.z_max.max(z);
                match tr.state {
                    State::Quiet | State::Candidate => {
                        tr.state = State::Candidate;
                    }
                    _ => {}
                }
                if matches!(tr.state, State::Candidate | State::Tracking | State::Locked) {
                    if z >= Z_TRACK {
                        tr.c_track += 1;
                    } else if tr.state == State::Candidate {
                        tr.c_track = 0;
                    }
                    if z >= Z_LOCK
                        && pulsy >= PULSY_MIN
                        && persist >= 0.5
                        && stationary
                        && !common_mode
                    {
                        tr.c_lock += 1;
                    } else {
                        tr.c_lock = 0;
                    }
                    if z >= Z_LOCK && pulsy < PULSY_CW && n >= 40 * (FS as usize) {
                        tr.c_cw += 1;
                    } else {
                        tr.c_cw = 0;
                    }
                    if tr.z_max >= Z_TRACK
                        && persist < 0.3
                        && z < 0.85 * tr.z_max
                        && n >= 30 * (FS as usize)
                    {
                        tr.c_fade += 1;
                    } else {
                        tr.c_fade = 0;
                    }
                }
                match tr.state {
                    State::Candidate if tr.c_track >= 3 => tr.state = State::Tracking,
                    _ => {}
                }
                if tr.state == State::Tracking && tr.c_lock >= 3 {
                    tr.state = State::Locked;
                }
                if matches!(tr.state, State::Candidate | State::Tracking) && tr.c_cw >= 2 {
                    tr.state = State::RejectedCw;
                }
                if matches!(tr.state, State::Tracking | State::Locked) && tr.c_fade >= 4 {
                    tr.state = State::RejectedTransient;
                }

                if tr.state != prev_state
                    || (prev_state == State::Quiet && tr.state == State::Candidate)
                {
                    let kind = match tr.state {
                        State::Candidate => Some(EventKind::Detected),
                        State::Tracking => Some(EventKind::Tracking),
                        State::Locked => Some(EventKind::Locked),
                        State::RejectedCw => Some(EventKind::RejectedCw),
                        State::RejectedTransient => Some(EventKind::RejectedTransient),
                        State::Quiet => None,
                    };
                    let repeat = kind == Some(EventKind::Detected) && tr.announced;
                    if kind == Some(EventKind::Detected) {
                        tr.announced = true;
                    }
                    if let (Some(kind), false) = (kind, repeat) {
                        events.push(Event {
                            epoch: k,
                            t,
                            slot: Some(s),
                            kind,
                        });
                    }
                }
                if tr.state == State::Locked && !tr.resolved {
                    if let Some(sk) = &sky {
                        if sk.resolved_by_s4 && sk.confidence_of_best() >= 0.95 {
                            tr.resolved = true;
                            events.push(Event {
                                epoch: k,
                                t,
                                slot: Some(s),
                                kind: EventKind::MirrorResolved,
                            });
                        }
                    }
                }

                // frequency uncertainty: sigma_f ~ sqrt(6)/(pi T snr_amp)
                let f_sigma = if z > 0.5 {
                    (6.0f64).sqrt() / (std::f64::consts::PI * t_obs * z.max(1.0))
                } else {
                    f64::INFINITY
                };
                slots[s] = SlotEpoch {
                    state: tr.state,
                    f: f_est,
                    f_sigma,
                    z,
                    pulsy,
                    persist,
                    peak_phase,
                    peak_phase_recent,
                    fold_snr,
                    delays,
                    sky,
                    common_mode,
                    stationary,
                    detection: det.map(|p| p.hs).unwrap_or(0.0),
                };
            }
            epochs.push(Epoch { k, n, t, slots });
        }
        events.sort_by_key(|e| e.epoch);
        Timeline { epochs, events }
    }

    /// Epoch record for analysis index `ep` (1-based). `None` for `ep == 0`.
    pub fn epoch(&self, ep: usize) -> Option<&Epoch> {
        if ep == 0 {
            None
        } else {
            self.epochs.get(ep - 1)
        }
    }

    pub fn first_event(&self, slot: usize, kind: EventKind) -> Option<&Event> {
        self.events
            .iter()
            .find(|e| e.slot == Some(slot) && e.kind == kind)
    }
}

/// Convenience: analysis epoch index for display time `t` (seconds).
pub fn epoch_at(t: f64) -> usize {
    ((t / (EPOCH_SAMPLES as f64 / FS)).floor().max(0.0) as usize).min(N_EPOCHS)
}
