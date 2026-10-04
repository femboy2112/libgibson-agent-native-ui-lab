//! Whitened multi-station power spectrum, harmonic-sum search and line finding.

use crate::dsp::{block_floor, hann, next_pow2, Fft, C};
use crate::scenario::{Receiver, FS};

/// Whitened, station-averaged power spectrum of the data prefix.
///
/// `w[j]` is the mean over the three stations of `P_s[j] / floor_s[j]`; under pure
/// noise each station contributes a unit-mean exponential, so `3·w[j]` is
/// Gamma(3)-distributed. `floors` keep the per-station absolute noise level so
/// coherent amplitudes can be normalised later.
#[derive(Clone, Debug)]
pub struct Spectrum {
    pub n: usize,
    pub df: f64,
    pub w: Vec<f32>,
    pub floors: [Vec<f64>; 3],
}

/// Σw² of a Hann window of length n, per sample (= 3/8).
pub const HANN_POWER: f64 = 0.375;

impl Spectrum {
    pub fn compute(rx: &Receiver, n: usize) -> Spectrum {
        let m = next_pow2(n) * 2;
        let fft = Fft::new(m);
        let win = hann(n);
        let nb = m / 2 + 1;
        let df = FS / m as f64;
        let block = ((0.35 / df).round() as usize).clamp(8, nb);
        let mut w = vec![0.0f32; nb];
        let mut floors: [Vec<f64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        let mut buf = vec![C::ZERO; m];
        for s in 0..3 {
            let x = rx.station(s);
            let mean: f64 = x[..n].iter().map(|&v| v as f64).sum::<f64>() / n as f64;
            for (i, b) in buf.iter_mut().enumerate() {
                *b = if i < n {
                    C::new((x[i] as f64 - mean) * win[i], 0.0)
                } else {
                    C::ZERO
                };
            }
            fft.forward(&mut buf);
            let p: Vec<f64> = (0..nb).map(|j| buf[j].norm2()).collect();
            let fl = block_floor(&p, block);
            for j in 0..nb {
                w[j] += (p[j] / fl[j].max(1e-300)) as f32 / 3.0;
            }
            floors[s] = fl;
        }
        Spectrum { n, df, w, floors }
    }

    pub fn nbins(&self) -> usize {
        self.w.len()
    }

    pub fn bin_of(&self, f: f64) -> usize {
        ((f / self.df).round() as isize).clamp(0, self.w.len() as isize - 1) as usize
    }

    /// Linear interpolation of the whitened spectrum at frequency `f` (0 outside).
    pub fn at(&self, f: f64) -> f64 {
        let x = f / self.df;
        if x < 0.0 || x >= (self.w.len() - 1) as f64 {
            return 0.0;
        }
        let i = x.floor() as usize;
        let t = x - i as f64;
        self.w[i] as f64 * (1.0 - t) + self.w[i + 1] as f64 * t
    }

    /// Local per-sample noise variance (white-equivalent) of station `s` at `f`,
    /// recovered from the periodogram floor: `E|X|² = σ² · Σw² = σ² · 0.375 n`.
    pub fn sigma2(&self, s: usize, f: f64) -> f64 {
        let j = self.bin_of(f.clamp(0.0, FS / 2.0));
        self.floors[s][j] / (HANN_POWER * self.n as f64)
    }
}

/// Result of a harmonic-sum search over a frequency band.
#[derive(Clone, Debug)]
pub struct HsPeak {
    pub f: f64,
    /// Gamma(3H)-distributed under noise.
    pub s: f64,
    pub h: usize,
    pub trials: f64,
}

/// Robust harmonic-sum configuration: frequencies inside `mask` (known interference)
/// count as noise, and no single harmonic may contribute more than `clip` — so one
/// brilliant line cannot masquerade as a comb.
#[derive(Clone, Copy)]
pub struct HsCfg<'a> {
    pub mask: &'a [(f64, f64)],
    pub clip: f64,
}

impl<'a> HsCfg<'a> {
    pub const NONE: HsCfg<'static> = HsCfg {
        mask: &[],
        clip: f64::INFINITY,
    };
    fn w(&self, sp: &Spectrum, f: f64) -> f64 {
        if f >= FS / 2.0 - 0.1 || self.mask.iter().any(|&(lo, hi)| f >= lo && f <= hi) {
            1.0
        } else {
            sp.at(f).min(self.clip)
        }
    }
}

/// Harmonic-sum statistic at one trial fundamental `f` (Gamma(3H) under noise).
pub fn hs_at(sp: &Spectrum, f: f64, hmax: usize, cfg: HsCfg) -> f64 {
    let mut s = 0.0;
    for h in 1..=hmax {
        s += cfg.w(sp, f * h as f64);
    }
    3.0 * s
}

/// Harmonic-sum statistic `3·Σ_{h=1..H} w(h f)` on the grid of the spectrum.
/// Returns the best fundamental in `band` and the full statistic curve.
pub fn harmonic_sum(
    sp: &Spectrum,
    band: (f64, f64),
    hmax: usize,
    cfg: HsCfg,
) -> (HsPeak, Vec<(f64, f64)>) {
    let j0 = sp.bin_of(band.0).max(1);
    let j1 = sp.bin_of(band.1);
    let mut curve = Vec::with_capacity(j1.saturating_sub(j0) + 1);
    let mut best = (0usize, f64::NEG_INFINITY);
    for j in j0..=j1 {
        let f = j as f64 * sp.df;
        let s = hs_at(sp, f, hmax, cfg);
        curve.push((f, s));
        if s > best.1 {
            best = (j - j0, s);
        }
    }
    // Parabolic refinement of the peak position on the curve.
    let (bi, bs) = best;
    let mut f = curve.get(bi).map(|c| c.0).unwrap_or(band.0);
    if bi > 0 && bi + 1 < curve.len() {
        let (a, b, c) = (curve[bi - 1].1, curve[bi].1, curve[bi + 1].1);
        let den = a - 2.0 * b + c;
        if den.abs() > 1e-12 {
            let off = (0.5 * (a - c) / den).clamp(-1.0, 1.0);
            f += off * sp.df;
        }
    }
    let t = sp.n as f64 / FS;
    let trials = ((band.1 - band.0) * t * hmax as f64).max(1.0);
    (
        HsPeak {
            f,
            s: bs.max(0.0),
            h: hmax,
            trials,
        },
        curve,
    )
}

/// A narrowband spectral line found in the whitened spectrum.
#[derive(Clone, Debug)]
pub struct SpecLine {
    pub f: f64,
    /// Whitened peak power (mean 1 under noise).
    pub w: f64,
}

/// Local maxima of the whitened spectrum above `thr`, strongest first, with a
/// minimum separation of `min_sep` Hz between reported lines.
pub fn find_lines(sp: &Spectrum, thr: f64, min_sep: f64, fmin: f64) -> Vec<SpecLine> {
    let mut cand = Vec::new();
    let j0 = sp.bin_of(fmin).max(1);
    for j in j0..sp.w.len() - 1 {
        let v = sp.w[j] as f64;
        if v >= thr && v >= sp.w[j - 1] as f64 && v > sp.w[j + 1] as f64 {
            // 3-point parabolic frequency
            let (a, b, c) = (sp.w[j - 1] as f64, v, sp.w[j + 1] as f64);
            let den = a - 2.0 * b + c;
            let off = if den.abs() > 1e-12 {
                (0.5 * (a - c) / den).clamp(-0.5, 0.5)
            } else {
                0.0
            };
            cand.push(SpecLine {
                f: (j as f64 + off) * sp.df,
                w: v,
            });
        }
    }
    cand.sort_by(|a, b| b.w.partial_cmp(&a.w).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<SpecLine> = Vec::new();
    for c in cand {
        if out.iter().all(|o| (o.f - c.f).abs() >= min_sep) {
            out.push(c);
        }
    }
    out
}
