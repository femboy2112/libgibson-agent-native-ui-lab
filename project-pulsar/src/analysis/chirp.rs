//! Search for a drifting carrier: causal band-pass/decimation to baseband, then a
//! Fourier-domain dechirp over a grid of drift rates (an "acceleration search").

use super::coherent::Model;
use crate::dsp::{median, next_pow2, Fft, C};
use crate::scenario::{Receiver, FS};
use std::f64::consts::TAU;

pub const BB_DECIM: usize = 8;
pub const BB_FC: f64 = 6.7;
pub const BB_TAPS: usize = 193;
pub const BB_FS: f64 = FS / BB_DECIM as f64;
/// Catalogue prior band for BETA (Hz).
pub const B_BAND: (f64, f64) = (5.0, 8.4);
pub const FDOT_MAX: f64 = 1.15e-3;
const B0: usize = 26;

/// Complex baseband around `BB_FC` for the whole stream. The FIR is causal, so
/// output `b` depends only on samples `≤ 8b`: prefixes of it are legitimate
/// "data so far".
pub struct Baseband {
    pub y: [Vec<C>; 3],
}

impl Baseband {
    pub fn new(rx: &Receiver) -> Baseband {
        let fc = 2.0 / FS;
        let mid = (BB_TAPS - 1) as f64 / 2.0;
        let mut h: Vec<f64> = (0..BB_TAPS)
            .map(|j| {
                let x = j as f64 - mid;
                let sinc = if x == 0.0 {
                    2.0 * fc
                } else {
                    (TAU * fc * x).sin() / (std::f64::consts::PI * x)
                };
                let w = 0.54 - 0.46 * (TAU * j as f64 / (BB_TAPS - 1) as f64).cos();
                sinc * w
            })
            .collect();
        let g: f64 = h.iter().sum();
        for v in h.iter_mut() {
            *v /= g;
        }
        let mut y: [Vec<C>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        for s in 0..3 {
            let x = rx.station(s);
            let z: Vec<C> = x
                .iter()
                .enumerate()
                .map(|(m, &v)| C::expi(-TAU * BB_FC * m as f64 / FS).scale(v as f64))
                .collect();
            let nb = x.len() / BB_DECIM;
            let mut out = Vec::with_capacity(nb);
            for b in 0..nb {
                let n0 = b * BB_DECIM;
                let mut acc = C::ZERO;
                for (j, hj) in h.iter().enumerate() {
                    if n0 >= j {
                        acc += z[n0 - j].scale(*hj);
                    }
                }
                out.push(acc);
            }
            y[s] = out;
        }
        Baseband { y }
    }

    /// Time stamp (s) of baseband sample `b` (compensating the FIR group delay).
    pub fn tau(b: usize) -> f64 {
        (b * BB_DECIM) as f64 / FS - ((BB_TAPS - 1) / 2) as f64 / FS
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ChirpFit {
    pub model: Model,
    pub f_ref: f64,
    pub tau_ref: f64,
    pub s: f64,
    pub trials: f64,
    pub span: f64,
}

/// Dechirp search on the first `n` samples' worth of baseband.
pub fn dechirp(bb: &Baseband, n: usize) -> Option<ChirpFit> {
    let b1 = (n - 1) / BB_DECIM;
    if b1 < B0 + 40 {
        return None;
    }
    let nb = b1 - B0 + 1;
    let taus: Vec<f64> = (B0..=b1).map(Baseband::tau).collect();
    let tau_ref = taus.iter().sum::<f64>() / nb as f64;
    let span = taus[nb - 1] - taus[0];
    let delta = 1.0 / (span * span);
    let half = ((FDOT_MAX / delta).ceil() as usize).min(1000);
    let ntr = 2 * half + 1;
    let p = next_pow2(nb);
    let fft = Fft::new(p);

    // valid FFT bins: those whose carrier frequency lies in the catalogue band
    let mut bins: Vec<(usize, f64)> = Vec::new();
    for j in 0..p {
        let jj = if j < p / 2 { j as f64 } else { j as f64 - p as f64 };
        let f = BB_FC + jj / p as f64 * BB_FS;
        if f >= B_BAND.0 && f <= B_BAND.1 {
            bins.push((j, f));
        }
    }
    let nv = bins.len();
    if nv < 3 {
        return None;
    }
    let mut plane: [Vec<f32>; 3] = [
        Vec::with_capacity(ntr * nv),
        Vec::with_capacity(ntr * nv),
        Vec::with_capacity(ntr * nv),
    ];
    let mut buf = vec![C::ZERO; p];
    let fdots: Vec<f64> = (0..ntr)
        .map(|i| (i as f64 - half as f64) * delta)
        .collect();
    for &fd in &fdots {
        let chirp: Vec<C> = taus
            .iter()
            .map(|&t| {
                let u = t - tau_ref;
                C::expi(-std::f64::consts::PI * fd * u * u)
            })
            .collect();
        for s in 0..3 {
            for (i, b) in buf.iter_mut().enumerate() {
                *b = if i < nb {
                    bb.y[s][B0 + i] * chirp[i]
                } else {
                    C::ZERO
                };
            }
            fft.forward(&mut buf);
            for &(j, _) in &bins {
                plane[s].push(buf[j].norm2() as f32);
            }
        }
    }
    let mut floors = [0.0f64; 3];
    for s in 0..3 {
        let mut tmp: Vec<f64> = plane[s].iter().map(|&v| v as f64).collect();
        floors[s] = median(&mut tmp) / std::f64::consts::LN_2;
    }
    let stat = |t: usize, i: usize| -> f64 {
        (0..3)
            .map(|s| plane[s][t * nv + i] as f64 / floors[s].max(1e-300))
            .sum::<f64>()
    };
    let (mut bt, mut bi, mut bs) = (0usize, 0usize, f64::NEG_INFINITY);
    for t in 0..ntr {
        for i in 0..nv {
            let v = stat(t, i);
            if v > bs {
                bs = v;
                bt = t;
                bi = i;
            }
        }
    }
    let par = |a: f64, b: f64, c: f64| -> f64 {
        let den = a - 2.0 * b + c;
        if den.abs() < 1e-12 {
            0.0
        } else {
            (0.5 * (a - c) / den).clamp(-1.0, 1.0)
        }
    };
    let mut df_off = 0.0;
    if bi > 0 && bi + 1 < nv {
        df_off = par(stat(bt, bi - 1), bs, stat(bt, bi + 1));
    }
    let mut fd_off = 0.0;
    if bt > 0 && bt + 1 < ntr {
        fd_off = par(stat(bt - 1, bi), bs, stat(bt + 1, bi));
    }
    let f_bin = BB_FS / p as f64;
    let f_ref = bins[bi].1 + df_off * f_bin;
    let fdot = fdots[bt] + fd_off * delta;
    let trials = ((B_BAND.1 - B_BAND.0) * span * (FDOT_MAX * span * span).max(1.0)).max(1.0);
    Some(ChirpFit {
        model: Model {
            f0: f_ref - fdot * tau_ref,
            fdot,
        },
        f_ref,
        tau_ref,
        s: bs,
        trials,
        span,
    })
}
