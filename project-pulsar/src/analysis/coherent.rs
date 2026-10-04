//! Coherent demodulation against a phase model: cumulative phasor sums (the
//! "phasor walk"), inter-station delay estimation, sky localisation and folding.

use crate::dsp::C;
use crate::scenario::{Receiver, CHUNK, FS, STATIONS};
use std::f64::consts::TAU;

/// Maximum harmonics tracked per signal.
pub const HMAX: usize = 6;

/// A phase model: `φ(t) = f0·t + ½·fdot·t²` (cycles).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Model {
    pub f0: f64,
    pub fdot: f64,
}

impl Model {
    pub fn phase(&self, t: f64) -> f64 {
        self.f0 * t + 0.5 * self.fdot * t * t
    }
    pub fn freq(&self, t: f64) -> f64 {
        self.f0 + self.fdot * t
    }
    pub fn scaled(&self, k: f64) -> Model {
        Model {
            f0: self.f0 * k,
            fdot: self.fdot * k,
        }
    }
}

/// Cumulative demodulated sums `Σ_{n<n_j} x_s[n]·e^{-2πi h φ(t_n)}` at every chunk
/// boundary `n_j = (j+1)·CHUNK`, for each station and harmonic.
#[derive(Clone, Debug)]
pub struct Cum {
    pub nh: usize,
    pub w: Vec<[[C; HMAX]; 3]>,
}

impl Cum {
    pub fn last(&self) -> Option<&[[C; HMAX]; 3]> {
        self.w.last()
    }
}

/// Demodulate the first `n` samples (a whole number of chunks) against `model`.
pub fn cumulative(rx: &Receiver, model: Model, nh: usize, n: usize) -> Cum {
    let nh = nh.min(HMAX);
    let xs = [rx.station(0), rx.station(1), rx.station(2)];
    let mut acc = [[C::ZERO; HMAX]; 3];
    let mut out = Vec::with_capacity(n / CHUNK);
    for i in 0..n {
        let t = i as f64 / FS;
        let e1 = C::expi(-TAU * model.phase(t));
        let mut pw = [C::ZERO; HMAX];
        pw[0] = e1;
        for h in 1..nh {
            pw[h] = pw[h - 1] * e1;
        }
        for s in 0..3 {
            let x = xs[s][i] as f64;
            for h in 0..nh {
                acc[s][h] += pw[h].scale(x);
            }
        }
        if (i + 1) % CHUNK == 0 {
            out.push(acc);
        }
    }
    Cum { nh, w: out }
}

/// Total coherent statistic `Σ_s Σ_h |W_{s,h}|² / (n σ²_{s,h})` of the first `n`
/// samples against `model` (null mean `3·nh`). No per-chunk storage.
pub fn coherent_stat(
    rx: &Receiver,
    model: Model,
    nh: usize,
    n: usize,
    sig2: &dyn Fn(usize, usize) -> f64,
) -> f64 {
    let nh = nh.min(HMAX);
    let xs = [rx.station(0), rx.station(1), rx.station(2)];
    let mut acc = [[C::ZERO; HMAX]; 3];
    for i in 0..n {
        let t = i as f64 / FS;
        let e1 = C::expi(-TAU * model.phase(t));
        let mut pw = e1;
        for h in 0..nh {
            for s in 0..3 {
                acc[s][h] += pw.scale(xs[s][i] as f64);
            }
            pw = pw * e1;
        }
    }
    let mut tot = 0.0;
    for s in 0..3 {
        for h in 0..nh {
            tot += acc[s][h].norm2() / (n as f64 * sig2(s, h + 1).max(1e-12));
        }
    }
    tot
}

/// Maximum-likelihood polish of a phase model by coordinate-wise parabolic search of
/// the coherent statistic, in the decorrelated coordinates `(f at mid-span, ḟ)`.
/// Starting steps are fractions of the Fourier cells `1/T` and `1/T²`; they shrink
/// each pass. Deterministic; a handful of O(n) evaluations.
pub fn refine_model(
    rx: &Receiver,
    start: Model,
    nh: usize,
    n: usize,
    with_fdot: bool,
    sig2: &dyn Fn(usize, usize) -> f64,
) -> Model {
    let t = n as f64 / FS;
    let mid = t / 2.0;
    let mut f = start.f0 + start.fdot * mid;
    let mut fd = start.fdot;
    let mk = |f: f64, fd: f64| Model {
        f0: f - fd * mid,
        fdot: fd,
    };
    let mut df = 0.25 / t;
    let mut dfd = 0.5 / (t * t);
    let parab = |a: f64, b: f64, c: f64| -> f64 {
        let den = a - 2.0 * b + c;
        if den >= -1e-12 {
            0.0 // not concave: stay
        } else {
            (0.5 * (a - c) / den).clamp(-1.0, 1.0)
        }
    };
    for _ in 0..5 {
        let (a, b, c) = (
            coherent_stat(rx, mk(f - df, fd), nh, n, sig2),
            coherent_stat(rx, mk(f, fd), nh, n, sig2),
            coherent_stat(rx, mk(f + df, fd), nh, n, sig2),
        );
        f += parab(a, b, c) * df;
        if with_fdot {
            let (a, b, c) = (
                coherent_stat(rx, mk(f, fd - dfd), nh, n, sig2),
                coherent_stat(rx, mk(f, fd), nh, n, sig2),
                coherent_stat(rx, mk(f, fd + dfd), nh, n, sig2),
            );
            fd += parab(a, b, c) * dfd;
        }
        df *= 0.5;
        dfd *= 0.5;
    }
    mk(f, fd)
}

/// Inter-station delay estimate relative to station 0 (seconds), with covariance.
#[derive(Clone, Copy, Debug)]
pub struct DelayFit {
    pub tau: [f64; 2],
    pub cov: [[f64; 2]; 2],
    /// Largest |z| of the amplitude-ratio test (far-field sources have equal
    /// amplitude at every station).
    pub amp_z: f64,
    pub amp_ratio: [f64; 2],
}

/// Estimate delays from the full-prefix phasors.
///
/// `sig2(s, h)` is the per-sample noise variance of station `s` at harmonic `h`'s
/// frequency; `f_eff` the model frequency (Hz) the phase relation refers to.
pub fn delays(
    last: &[[C; HMAX]; 3],
    n: usize,
    nh: usize,
    f_eff: f64,
    sig2: impl Fn(usize, usize) -> f64,
) -> Option<DelayFit> {
    if nh == 0 || f_eff <= 0.0 {
        return None;
    }
    let nf = n as f64;
    // debiased phasor S/N per (station, harmonic)
    let mut sn = [[0.0f64; HMAX]; 3];
    for s in 0..3 {
        for h in 0..nh {
            let p = last[s][h].norm2() / (nf * sig2(s, h + 1).max(1e-12));
            sn[s][h] = (p - 1.0).max(0.25);
        }
    }
    let mut tau = [0.0; 2];
    let mut w_sum = [0.0; 2];
    let mut w = [[0.0f64; HMAX]; 2];
    let mut tauh = [[0.0f64; HMAX]; 2];
    for si in 0..2 {
        let s = si + 1;
        for h in 0..nh {
            let hf = (h + 1) as f64 * f_eff;
            let d = (last[s][h] * last[0][h].conj()).arg();
            let t = d / (TAU * hf);
            let var = (0.5 / sn[s][h] + 0.5 / sn[0][h]) / (TAU * hf).powi(2);
            w[si][h] = 1.0 / var;
            tauh[si][h] = t;
            w_sum[si] += 1.0 / var;
            tau[si] += t / var;
        }
        tau[si] /= w_sum[si];
    }
    let mut cov = [[0.0f64; 2]; 2];
    cov[0][0] = 1.0 / w_sum[0];
    cov[1][1] = 1.0 / w_sum[1];
    let mut c01 = 0.0;
    for h in 0..nh {
        let hf = (h + 1) as f64 * f_eff;
        let var0 = (0.5 / sn[0][h]) / (TAU * hf).powi(2);
        c01 += (w[0][h] / w_sum[0]) * (w[1][h] / w_sum[1]) * var0;
    }
    cov[0][1] = c01;
    cov[1][0] = c01;

    // amplitude-ratio uniformity test on the strongest harmonic
    let mut hbest = 0;
    let mut sbest = -1.0;
    for h in 0..nh {
        let t: f64 = (0..3).map(|s| sn[s][h]).sum();
        if t > sbest {
            sbest = t;
            hbest = h;
        }
    }
    let a: Vec<f64> = (0..3).map(|s| last[s][hbest].abs()).collect();
    let mut amp_z: f64 = 0.0;
    let mut amp_ratio = [1.0; 2];
    for si in 0..2 {
        let s = si + 1;
        let ratio = a[s] / a[0].max(1e-12);
        amp_ratio[si] = ratio;
        let var = 0.5 / sn[s][hbest] + 0.5 / sn[0][hbest];
        let z = ratio.max(1e-9).ln() / var.sqrt();
        amp_z = amp_z.max(z.abs());
    }
    Some(DelayFit {
        tau,
        cov,
        amp_z,
        amp_ratio,
    })
}

/// Posterior of the source direction cosines `(l, m)` from two delays.
#[derive(Clone, Copy, Debug)]
pub struct SkyFit {
    pub lm: (f64, f64),
    pub cov: [[f64; 2]; 2],
    /// 1σ semi-axes of the covariance ellipse and the major-axis angle (rad).
    pub sig_major: f64,
    pub sig_minor: f64,
    pub angle: f64,
    /// Mean lies inside the unit disc, i.e. corresponds to a real far-field direction.
    pub physical: bool,
}

pub fn sky_from_delays(d: &DelayFit) -> SkyFit {
    // τ_s = (x_s·l + y_s·m)·1e-3  ⇒  [l m]ᵀ = A⁻¹ τ
    let a = [
        [STATIONS[1].0 * 1e-3, STATIONS[1].1 * 1e-3],
        [STATIONS[2].0 * 1e-3, STATIONS[2].1 * 1e-3],
    ];
    let det = a[0][0] * a[1][1] - a[0][1] * a[1][0];
    let inv = [
        [a[1][1] / det, -a[0][1] / det],
        [-a[1][0] / det, a[0][0] / det],
    ];
    let l = inv[0][0] * d.tau[0] + inv[0][1] * d.tau[1];
    let m = inv[1][0] * d.tau[0] + inv[1][1] * d.tau[1];
    // C_lm = inv · C_τ · invᵀ
    let mut t = [[0.0; 2]; 2];
    for i in 0..2 {
        for j in 0..2 {
            for k in 0..2 {
                t[i][j] += inv[i][k] * d.cov[k][j];
            }
        }
    }
    let mut c = [[0.0; 2]; 2];
    for i in 0..2 {
        for j in 0..2 {
            for k in 0..2 {
                c[i][j] += t[i][k] * inv[j][k];
            }
        }
    }
    let (maj, min, ang) = ellipse(c);
    SkyFit {
        lm: (l, m),
        cov: c,
        sig_major: maj,
        sig_minor: min,
        angle: ang,
        physical: l * l + m * m <= 1.0,
    }
}

/// Eigen-decomposition of a symmetric 2×2 covariance: (σ_major, σ_minor, angle).
pub fn ellipse(c: [[f64; 2]; 2]) -> (f64, f64, f64) {
    let (a, b, d) = (c[0][0], c[0][1], c[1][1]);
    let tr = a + d;
    let det = a * d - b * b;
    let disc = ((tr * tr) / 4.0 - det).max(0.0).sqrt();
    let l1 = (tr / 2.0 + disc).max(0.0);
    let l2 = (tr / 2.0 - disc).max(0.0);
    let ang = 0.5 * (2.0 * b).atan2(a - d);
    (l1.sqrt(), l2.sqrt(), ang)
}

/// Cholesky factor `L` (lower) of a 2×2 covariance, regularised if needed.
pub fn chol2(c: [[f64; 2]; 2]) -> [[f64; 2]; 2] {
    let l00 = c[0][0].max(1e-18).sqrt();
    let l10 = c[1][0] / l00;
    let l11 = (c[1][1] - l10 * l10).max(1e-18).sqrt();
    [[l00, 0.0], [l10, l11]]
}

/// Fold the first `n` samples at `model`, aligning stations by `taus` (seconds, per
/// station, to remove geometric delay). `rot` rotates the phase so a feature of
/// interest sits at a fixed place. Returns `nb` bin means averaged over stations
/// and the 1σ error of one bin.
pub fn fold(
    rx: &Receiver,
    model: Model,
    taus: [f64; 3],
    rot: f64,
    n: usize,
    nb: usize,
) -> (Vec<f32>, f32) {
    let mut sum = vec![0.0f64; nb];
    let mut cnt = vec![0u32; nb];
    let mut tot2 = 0.0;
    for s in 0..3 {
        let x = rx.station(s);
        let mean: f64 = x[..n].iter().map(|&v| v as f64).sum::<f64>() / n as f64;
        for i in 0..n {
            let t = i as f64 / FS + taus[s];
            let ph = model.phase(t) + rot;
            let u = ph - ph.floor();
            let b = ((u * nb as f64) as usize).min(nb - 1);
            let v = x[i] as f64 - mean;
            sum[b] += v;
            cnt[b] += 1;
            tot2 += v * v;
        }
    }
    let sd = (tot2 / (3 * n) as f64).sqrt();
    let prof: Vec<f32> = (0..nb)
        .map(|b| {
            if cnt[b] > 0 {
                (sum[b] / cnt[b] as f64) as f32
            } else {
                0.0
            }
        })
        .collect();
    let per_bin = (3 * n) as f64 / nb as f64;
    (prof, (sd / per_bin.sqrt()) as f32)
}

/// Phase-vs-time stack: one row per `rows_chunks` chunks, `nb` phase bins, values in
/// units of the per-bin noise.
pub fn waterfall(
    rx: &Receiver,
    model: Model,
    taus: [f64; 3],
    rot: f64,
    n: usize,
    nb: usize,
    rows_chunks: usize,
) -> Vec<Vec<f32>> {
    let rows = n / (CHUNK * rows_chunks);
    let per = CHUNK * rows_chunks;
    let mut out = Vec::with_capacity(rows);
    let means: Vec<f64> = (0..3)
        .map(|s| rx.station(s)[..n].iter().map(|&v| v as f64).sum::<f64>() / n as f64)
        .collect();
    for r in 0..rows {
        let mut sum = vec![0.0f64; nb];
        let mut cnt = vec![0u32; nb];
        let mut tot2 = 0.0;
        for s in 0..3 {
            let x = rx.station(s);
            let mean = means[s];
            for i in r * per..(r + 1) * per {
                let t = i as f64 / FS + taus[s];
                let ph = model.phase(t) + rot;
                let u = ph - ph.floor();
                let b = ((u * nb as f64) as usize).min(nb - 1);
                let v = x[i] as f64 - mean;
                sum[b] += v;
                cnt[b] += 1;
                tot2 += v * v;
            }
        }
        let sd = (tot2 / (3 * per) as f64).sqrt();
        let row: Vec<f32> = (0..nb)
            .map(|b| {
                if cnt[b] > 0 {
                    let m = sum[b] / cnt[b] as f64;
                    (m / (sd / (cnt[b] as f64).sqrt())) as f32
                } else {
                    0.0
                }
            })
            .collect();
        out.push(row);
    }
    out
}
