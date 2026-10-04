//! Signal-processing primitives owned by the application.
//!
//! FFT, windows, robust statistics and detection-significance helpers. None of this
//! lives in the plotting library, by design: `gibson::plot` displays observables, it
//! never manufactures them.

use std::f64::consts::{PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct C {
    pub re: f64,
    pub im: f64,
}

impl C {
    pub const ZERO: C = C { re: 0.0, im: 0.0 };
    #[inline]
    pub fn new(re: f64, im: f64) -> C {
        C { re, im }
    }
    #[inline]
    pub fn expi(theta: f64) -> C {
        C {
            re: theta.cos(),
            im: theta.sin(),
        }
    }
    #[inline]
    pub fn norm2(self) -> f64 {
        self.re * self.re + self.im * self.im
    }
    #[inline]
    pub fn abs(self) -> f64 {
        self.norm2().sqrt()
    }
    #[inline]
    pub fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }
    #[inline]
    pub fn conj(self) -> C {
        C::new(self.re, -self.im)
    }
    #[inline]
    pub fn scale(self, k: f64) -> C {
        C::new(self.re * k, self.im * k)
    }
}

impl std::ops::Add for C {
    type Output = C;
    #[inline]
    fn add(self, o: C) -> C {
        C::new(self.re + o.re, self.im + o.im)
    }
}
impl std::ops::Sub for C {
    type Output = C;
    #[inline]
    fn sub(self, o: C) -> C {
        C::new(self.re - o.re, self.im - o.im)
    }
}
impl std::ops::Mul for C {
    type Output = C;
    #[inline]
    fn mul(self, o: C) -> C {
        C::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
}
impl std::ops::AddAssign for C {
    #[inline]
    fn add_assign(&mut self, o: C) {
        self.re += o.re;
        self.im += o.im;
    }
}

/// Radix-2 FFT plan (power-of-two lengths only).
pub struct Fft {
    n: usize,
    tw: Vec<C>,
    rev: Vec<u32>,
}

impl Fft {
    pub fn new(n: usize) -> Fft {
        assert!(n.is_power_of_two() && n >= 2, "FFT length must be 2^k");
        let tw = (0..n / 2).map(|k| C::expi(-TAU * k as f64 / n as f64)).collect();
        let bits = n.trailing_zeros();
        let rev = (0..n as u32)
            .map(|i| i.reverse_bits() >> (32 - bits))
            .collect();
        Fft { n, tw, rev }
    }

    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    /// In-place forward DFT: `X[k] = Σ x[n] e^{-2πi kn/N}`.
    pub fn forward(&self, a: &mut [C]) {
        assert_eq!(a.len(), self.n);
        for i in 0..self.n {
            let j = self.rev[i] as usize;
            if i < j {
                a.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= self.n {
            let half = len / 2;
            let step = self.n / len;
            for start in (0..self.n).step_by(len) {
                for k in 0..half {
                    let w = self.tw[k * step];
                    let u = a[start + k];
                    let v = a[start + k + half] * w;
                    a[start + k] = u + v;
                    a[start + k + half] = u - v;
                }
            }
            len <<= 1;
        }
    }
}

/// Hann window of length `n`.
pub fn hann(n: usize) -> Vec<f64> {
    if n <= 1 {
        return vec![1.0; n];
    }
    (0..n)
        .map(|i| 0.5 - 0.5 * (TAU * i as f64 / n as f64).cos())
        .collect()
}

/// Median (consumes ordering of the slice). `NaN`-free input assumed.
pub fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mid = v.len() / 2;
    v.select_nth_unstable_by(mid, |a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v[mid]
}

/// Smoothly varying noise floor under a spectrum: block medians (robust to sparse
/// lines), log-linearly interpolated between block centres. Returns the *mean*
/// level of an exponential-distributed power (median / ln 2).
pub fn block_floor(power: &[f64], block: usize) -> Vec<f64> {
    let n = power.len();
    if n == 0 {
        return Vec::new();
    }
    let block = block.max(8).min(n);
    let nb = n.div_ceil(block);
    let mut centers = Vec::with_capacity(nb);
    let mut levels = Vec::with_capacity(nb);
    for b in 0..nb {
        let lo = b * block;
        let hi = ((b + 1) * block).min(n);
        let mut tmp: Vec<f64> = power[lo..hi].to_vec();
        let m = median(&mut tmp) / std::f64::consts::LN_2;
        centers.push((lo + hi) as f64 * 0.5);
        levels.push(m.max(1e-300).ln());
    }
    let mut out = Vec::with_capacity(n);
    let mut b = 0usize;
    for i in 0..n {
        let x = i as f64 + 0.5;
        while b + 1 < nb && centers[b + 1] <= x {
            b += 1;
        }
        let v = if nb == 1 || x <= centers[0] {
            levels[0]
        } else if b + 1 >= nb {
            levels[nb - 1]
        } else {
            let t = (x - centers[b]) / (centers[b + 1] - centers[b]);
            levels[b] * (1.0 - t) + levels[b + 1] * t
        };
        out.push(v.exp());
    }
    out
}

/// `ln Q(m, x)` for integer shape `m ≥ 1`: the log upper tail of a Gamma(m, 1)
/// variable (the null distribution of a sum of `m` unit-exponential powers).
pub fn ln_gamma_q(m: usize, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    // Q = e^{-x} Σ_{i<m} x^i / i!  — accumulate in log space.
    let mut terms = Vec::with_capacity(m);
    let mut lt = 0.0f64; // ln(x^0/0!)
    terms.push(lt);
    for i in 1..m {
        lt += x.ln() - (i as f64).ln();
        terms.push(lt);
    }
    let mx = terms.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let s: f64 = terms.iter().map(|t| (t - mx).exp()).sum();
    -x + mx + s.ln()
}

/// log10 of the false-alarm probability after `trials` independent trials of a
/// Gamma(m) statistic reaching `x`. Capped at 0 (a probability ≤ 1).
pub fn log10_fap(m: usize, x: f64, trials: f64) -> f64 {
    ((ln_gamma_q(m, x) + trials.max(1.0).ln()) / std::f64::consts::LN_10).min(0.0)
}

/// Gaussian-equivalent significance (σ) of a one-sided tail probability given as
/// log10(p). Monotone, finite, and accurate to ~0.05σ for 1e-300 < p < 0.5.
pub fn sigma_from_log10p(log10p: f64) -> f64 {
    let lnp = log10p * std::f64::consts::LN_10;
    if lnp >= -0.693 {
        return 0.0;
    }
    // Invert ln Q(z) ≈ -z²/2 - ln(z√(2π)) by a few Newton steps.
    let mut z = (-2.0 * lnp).sqrt().max(1.0);
    for _ in 0..30 {
        let f = -0.5 * z * z - (z * (TAU).sqrt()).ln() - lnp;
        let d = -z - 1.0 / z;
        let nz = z - f / d;
        if (nz - z).abs() < 1e-9 {
            z = nz;
            break;
        }
        z = nz;
    }
    z.max(0.0)
}

/// Wrap an angle into `(-π, π]`.
#[inline]
pub fn wrap_pi(mut a: f64) -> f64 {
    a = (a + PI).rem_euclid(TAU) - PI;
    a
}

pub fn next_pow2(n: usize) -> usize {
    n.next_power_of_two().max(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fft_matches_naive_dft() {
        let n = 64;
        let x: Vec<C> = (0..n)
            .map(|i| C::new((i as f64 * 0.37).sin(), (i as f64 * 0.11).cos()))
            .collect();
        let mut a = x.clone();
        Fft::new(n).forward(&mut a);
        for k in 0..n {
            let mut s = C::ZERO;
            for (j, v) in x.iter().enumerate() {
                s += *v * C::expi(-TAU * (k * j) as f64 / n as f64);
            }
            assert!((s - a[k]).abs() < 1e-9, "bin {k}");
        }
    }

    #[test]
    fn fft_recovers_a_tone() {
        let n = 256;
        let mut a: Vec<C> = (0..n)
            .map(|i| C::new((TAU * 10.0 * i as f64 / n as f64).cos(), 0.0))
            .collect();
        Fft::new(n).forward(&mut a);
        assert!((a[10].abs() - n as f64 / 2.0).abs() < 1e-6);
        assert!(a[11].abs() < 1e-6);
    }

    #[test]
    fn gamma_tail_known_values() {
        // Q(1, x) = e^{-x}
        assert!((ln_gamma_q(1, 3.0) + 3.0).abs() < 1e-12);
        // Q(2, 2) = e^{-2}(1+2) = 3e^{-2}
        assert!((ln_gamma_q(2, 2.0) - (3.0f64).ln() + 2.0).abs() < 1e-12);
    }

    #[test]
    fn sigma_conversion_known_points() {
        // one-sided p = 0.0013499 ≈ 3σ ; p = 2.867e-7 ≈ 5σ
        assert!((sigma_from_log10p((1.3499e-3f64).log10()) - 3.0).abs() < 0.08);
        assert!((sigma_from_log10p((2.867e-7f64).log10()) - 5.0).abs() < 0.08);
        assert_eq!(sigma_from_log10p(0.0), 0.0);
    }

    #[test]
    fn block_floor_ignores_sparse_lines() {
        let mut p = vec![1.0; 1024];
        p[500] = 1e4;
        let f = block_floor(&p, 64);
        assert!((f[500] - 1.0 / std::f64::consts::LN_2).abs() < 1e-9);
    }
}
