//! Signal processing, owned by the application (none of this lives in
//! LibGibson): glitch masking, a causal high-pass, a Hann-windowed FFT
//! periodogram, harmonic summing, coherent Fourier sums, epoch folding and a
//! little robust statistics.

use crate::sim::FS;
use std::f64::consts::TAU;

// ───────────────────────────── robust statistics ─────────────────────────────

pub fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let m = values.len() / 2;
    if values.len() % 2 == 1 {
        values[m]
    } else {
        0.5 * (values[m - 1] + values[m])
    }
}

/// Robust sigma (1.4826 x MAD) and median of the finite values.
pub fn robust_sigma(values: &[f64]) -> (f64, f64) {
    let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return (0.0, 1.0);
    }
    let med = median(&mut v);
    let mut dev: Vec<f64> = v.iter().map(|x| (x - med).abs()).collect();
    let mad = median(&mut dev);
    (med, (1.4826 * mad).max(1e-12))
}

/// `ln Q(x; k)`: log survival function of a chi-square variable with an EVEN
/// number of degrees of freedom `k = 2m` (exact: `e^{-x/2} sum_{j<m} (x/2)^j / j!`).
pub fn ln_chi2_sf_even(x: f64, m: usize) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let half = 0.5 * x;
    let (mut term, mut sum) = (1.0f64, 1.0f64);
    for j in 1..m {
        term *= half / j as f64;
        sum += term;
    }
    -half + sum.ln()
}

/// `ln` of the standard-normal upper tail, `ln Q(z)`, accurate in the far tail
/// (Numerical-Recipes `erfcc` rational form evaluated in log space).
pub fn ln_normal_sf(z: f64) -> f64 {
    let x = z.abs() / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.5 * x);
    let poly = -1.26551223
        + t * (1.00002368
            + t * (0.37409196
                + t * (0.09678418
                    + t * (-0.18628806
                        + t * (0.27886807
                            + t * (-1.13520398
                                + t * (1.48851587 + t * (-0.82215223 + t * 0.17087277))))))));
    // erfc(x) = t * exp(-x^2 + poly)
    let ln_erfc = t.ln() - x * x + poly;
    if z >= 0.0 {
        ln_erfc - std::f64::consts::LN_2
    } else {
        (1.0 - 0.5 * ln_erfc.exp()).ln()
    }
}

/// Gaussian-equivalent significance ("n sigma") of a chi-square statistic with
/// `2m` degrees of freedom: the `z` whose normal upper tail equals the exact
/// chi-square survival probability.
pub fn chi2_sigma(chi2: f64, m: usize) -> f64 {
    let target = ln_chi2_sf_even(chi2, m);
    let (mut lo, mut hi) = (-8.0f64, 40.0f64);
    for _ in 0..64 {
        let mid = 0.5 * (lo + hi);
        if ln_normal_sf(mid) > target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

// ─────────────────────────────── preprocessing ───────────────────────────────

/// A cleaned stream: invalid / glitched samples are masked (value 0, `ok=false`)
/// and a causal trailing-mean high-pass removes the receiver wander.
#[derive(Clone, Debug)]
pub struct Clean {
    pub x: Vec<f64>,
    pub ok: Vec<bool>,
    /// Robust per-sample noise sigma of the cleaned stream.
    pub sigma: f64,
    pub masked: usize,
}

/// Trailing-window length of the high-pass (samples) = 6 s.
pub const HP_WINDOW: usize = 384;
/// Samples further than this many robust sigmas from the median are masked.
pub const GLITCH_SIGMA: f64 = 6.5;

/// The receiver is calibrated on its first 20 s: the noise floor, median and
/// glitch threshold are measured there and held fixed afterwards, so nothing
/// the pipeline publishes at epoch `k` can depend on samples after `k`.
pub const CALIBRATION_SAMPLES: usize = 1280;

pub fn preprocess(raw: &[f64]) -> Clean {
    let n = raw.len();
    let cal = &raw[..CALIBRATION_SAMPLES.min(n)];
    let (med, sig) = robust_sigma(cal);
    let mut ok = vec![true; n];
    let mut masked = 0;
    for i in 0..n {
        let v = raw[i];
        if !v.is_finite() || (v - med).abs() > GLITCH_SIGMA * sig {
            ok[i] = false;
            masked += 1;
        }
    }
    // causal trailing mean over valid samples
    let mut x = vec![0.0; n];
    let mut ring_sum = 0.0;
    let mut ring_cnt = 0usize;
    for i in 0..n {
        // window = samples [i-W, i)
        if i >= HP_WINDOW {
            let j = i - HP_WINDOW;
            if ok[j] {
                ring_sum -= raw[j];
                ring_cnt -= 1;
            }
        }
        let mean = if ring_cnt > 0 {
            ring_sum / ring_cnt as f64
        } else {
            med
        };
        if ok[i] {
            x[i] = raw[i] - mean;
        }
        if ok[i] {
            ring_sum += raw[i];
            ring_cnt += 1;
        }
    }
    // noise floor of the cleaned stream, from the calibration window past the warm-up
    let lo = HP_WINDOW.min(n);
    let hi = CALIBRATION_SAMPLES.min(n).max(lo);
    let valid: Vec<f64> = (lo..hi).filter(|i| ok[*i]).map(|i| x[i]).collect();
    let (_, sigma) = robust_sigma(&valid);
    Clean {
        x,
        ok,
        sigma,
        masked,
    }
}

// ───────────────────────────────── FFT ─────────────────────────────────────

/// In-place iterative radix-2 FFT (forward, e^{-i...}). Length must be a power of two.
pub fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    debug_assert!(n.is_power_of_two() && im.len() == n);
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -TAU / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        let half = len / 2;
        let mut i = 0;
        while i < n {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..half {
                let (a, b) = (i + k, i + k + half);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
            i += len;
        }
        len <<= 1;
    }
}

/// A one-sided normalised periodogram. `power[k]` has mean 1 for white noise of
/// the stream's measured sigma, so values read as "multiples of the noise floor".
#[derive(Clone, Debug)]
pub struct Spectrum {
    pub df: f64,
    /// Power in multiples of the local noise floor.
    pub power: Vec<f64>,
    /// Local noise floor that was divided out (relative to white sigma).
    pub floor: Vec<f64>,
    /// Number of samples integrated.
    pub n: usize,
}

impl Spectrum {
    pub fn freq(&self, k: usize) -> f64 {
        k as f64 * self.df
    }
    pub fn bin(&self, f: f64) -> usize {
        ((f / self.df).round().max(0.0) as usize).min(self.power.len().saturating_sub(1))
    }
}

/// Fraction of the window length occupied by each cosine taper.
pub const TUKEY_TAPER: f64 = 0.125;

/// Tukey (tapered-cosine) window. Unlike a Hann window it keeps the newest
/// samples at full weight, so a signal that just switched on is not suppressed
/// while the integration is still growing.
pub fn tukey(i: usize, n: usize) -> f64 {
    let x = (i as f64 + 0.5) / n as f64;
    if x < TUKEY_TAPER {
        0.5 - 0.5 * (std::f64::consts::PI * x / TUKEY_TAPER).cos()
    } else if x > 1.0 - TUKEY_TAPER {
        0.5 - 0.5 * (std::f64::consts::PI * (1.0 - x) / TUKEY_TAPER).cos()
    } else {
        1.0
    }
}

/// Periodogram of the first `n` cleaned samples, Tukey-windowed, zero-padded to `n_fft`.
pub fn periodogram(clean: &Clean, n: usize, n_fft: usize) -> Spectrum {
    let n = n.min(clean.x.len());
    let mut re = vec![0.0; n_fft];
    let mut im = vec![0.0; n_fft];
    let mut norm = 0.0;
    for (i, ((r, &ok), &x)) in re
        .iter_mut()
        .zip(&clean.ok)
        .zip(&clean.x)
        .take(n)
        .enumerate()
    {
        let w = tukey(i, n);
        if ok {
            *r = x * w;
            norm += w * w;
        }
    }
    fft(&mut re, &mut im);
    let denom = (norm * clean.sigma * clean.sigma).max(1e-12);
    let half = n_fft / 2;
    let raw: Vec<f64> = (0..half)
        .map(|k| (re[k] * re[k] + im[k] * im[k]) / denom)
        .collect();
    let df = FS / n_fft as f64;
    let floor = local_floor(&raw, df, n as f64 / FS);
    let power: Vec<f64> = raw.iter().zip(&floor).map(|(p, f)| p / f).collect();
    Spectrum {
        df,
        power,
        floor,
        n,
    }
}

/// Local noise floor of a periodogram: sliding-window median (immune to narrow
/// lines) divided by ln 2 (the median of a unit-mean exponential), interpolated
/// between block centres. The window spans ~0.5 Hz but never fewer than 48
/// independent resolution cells.
pub fn local_floor(p: &[f64], df: f64, t_obs: f64) -> Vec<f64> {
    let len = p.len();
    let cells_per_bin = (1.0 / (t_obs * df)).max(1e-9);
    let half_w = ((0.25 / df).max(24.0 / cells_per_bin * 1.0)).round() as usize;
    let half_w = half_w.clamp(8, len / 4);
    let step = (half_w / 4).max(1);
    let mut centres = Vec::new();
    let mut vals = Vec::new();
    let mut c = 0usize;
    loop {
        let lo = c.saturating_sub(half_w);
        let hi = (c + half_w + 1).min(len);
        let mut w: Vec<f64> = p[lo..hi].to_vec();
        let m = median(&mut w) / std::f64::consts::LN_2;
        centres.push(c as f64);
        vals.push(m.max(1e-12));
        if c + step >= len {
            if c != len - 1 {
                centres.push((len - 1) as f64);
                let lo = len.saturating_sub(2 * half_w + 1);
                let mut w: Vec<f64> = p[lo..].to_vec();
                vals.push((median(&mut w) / std::f64::consts::LN_2).max(1e-12));
            }
            break;
        }
        c += step;
    }
    let mut out = vec![1.0; len];
    let mut seg = 0usize;
    for (k, o) in out.iter_mut().enumerate() {
        while seg + 1 < centres.len() - 1 && centres[seg + 1] < k as f64 {
            seg += 1;
        }
        let (c0, c1) = (centres[seg], centres[(seg + 1).min(centres.len() - 1)]);
        let (v0, v1) = (vals[seg], vals[(seg + 1).min(vals.len() - 1)]);
        let a = if c1 > c0 {
            ((k as f64 - c0) / (c1 - c0)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        *o = v0 + (v1 - v0) * a;
    }
    out
}

/// Harmonic sum with `harmonics` terms: `HS(k) = sum_h max(S[h*k-1 ..= h*k+1])`.
pub fn harmonic_sum(spec: &Spectrum, harmonics: usize, k_lo: usize, k_hi: usize) -> Vec<f64> {
    let len = spec.power.len();
    let mut out = vec![0.0; len];
    for (k, o) in out.iter_mut().enumerate().take(k_hi.min(len)).skip(k_lo) {
        let mut s = 0.0;
        for h in 1..=harmonics {
            let c = h * k;
            if c >= len {
                break;
            }
            let lo = c.saturating_sub(1);
            let hi = (c + 1).min(len - 1);
            let mut m = 0.0f64;
            for v in &spec.power[lo..=hi] {
                m = m.max(*v);
            }
            s += m;
        }
        *o = s;
    }
    out
}

// ─────────────────────────── coherent Fourier sums ───────────────────────────

/// `sum_{j in [start, end)} x_j e^{-i 2 pi h f j / FS}` for h = 1..=hmax over
/// the valid samples. Uses a unit-rotation recurrence (one complex multiply per
/// sample per harmonic). Returns `(re, im)` per harmonic and the valid count.
pub fn fourier_sums(
    clean: &Clean,
    start: usize,
    end: usize,
    f: f64,
    hmax: usize,
) -> (Vec<(f64, f64)>, usize) {
    let end = end.min(clean.x.len());
    let mut out = Vec::with_capacity(hmax);
    let mut count = 0usize;
    for j in start..end {
        if clean.ok[j] {
            count += 1;
        }
    }
    for h in 1..=hmax {
        let w = -TAU * h as f64 * f / FS;
        let (wr, wi) = (w.cos(), w.sin());
        // starting phasor e^{i w start}
        let a0 = w * start as f64;
        let (mut cr, mut ci) = (a0.cos(), a0.sin());
        let (mut sr, mut si) = (0.0, 0.0);
        for j in start..end {
            if clean.ok[j] {
                let v = clean.x[j];
                sr += v * cr;
                si += v * ci;
            }
            let ncr = cr * wr - ci * wi;
            ci = cr * wi + ci * wr;
            cr = ncr;
        }
        out.push((sr, si));
    }
    (out, count)
}

/// Per-harmonic `Z_h^2 = 2 |X_h|^2 / (N sigma^2)`; each is chi-square(2) under noise.
pub fn z2_per_harmonic(sums: &[(f64, f64)], count: usize, sigma: f64) -> Vec<f64> {
    let denom = (count.max(1) as f64) * sigma * sigma;
    sums.iter()
        .map(|(r, i)| 2.0 * (r * r + i * i) / denom)
        .collect()
}

/// `Z_H^2` over the first `h` harmonics at trial frequency `f` (prefix `[0, n)`).
pub fn z2_total(clean: &Clean, n: usize, f: f64, h: usize) -> f64 {
    let (s, c) = fourier_sums(clean, 0, n, f, h);
    z2_per_harmonic(&s, c, clean.sigma).iter().sum()
}

/// Locate the maximum of `Z_h^2(f)` around `f0` (+- `half_width`): coarse grid
/// scan then golden-section polish. Returns `(f, z2)`.
pub fn refine_frequency(
    clean: &Clean,
    n: usize,
    f0: f64,
    half_width: f64,
    grid: usize,
    harmonics: usize,
) -> (f64, f64) {
    let grid = grid.max(3);
    let step = 2.0 * half_width / (grid - 1) as f64;
    let mut best = (f0, f64::NEG_INFINITY);
    for g in 0..grid {
        let f = f0 - half_width + g as f64 * step;
        let z = z2_total(clean, n, f, harmonics);
        if z > best.1 {
            best = (f, z);
        }
    }
    let (mut a, mut b) = (best.0 - step, best.0 + step);
    let gr = 0.618_033_988_749_894_9;
    let mut c = b - gr * (b - a);
    let mut d = a + gr * (b - a);
    let mut zc = z2_total(clean, n, c, harmonics);
    let mut zd = z2_total(clean, n, d, harmonics);
    for _ in 0..14 {
        if zc > zd {
            b = d;
            d = c;
            zd = zc;
            c = b - gr * (b - a);
            zc = z2_total(clean, n, c, harmonics);
        } else {
            a = c;
            c = d;
            zc = zd;
            d = a + gr * (b - a);
            zd = z2_total(clean, n, d, harmonics);
        }
    }
    let f = 0.5 * (a + b);
    let z = z2_total(clean, n, f, harmonics);
    if z >= best.1 {
        (f, z)
    } else {
        best
    }
}

// ───────────────────────────────── folding ───────────────────────────────────

/// A folded profile: per phase bin the mean cleaned amplitude (in units of the
/// stream sigma) and the sample count.
#[derive(Clone, Debug)]
pub struct Fold {
    pub mean: Vec<f64>,
    pub count: Vec<u32>,
    pub sigma: f64,
}

/// Fold samples `[start, end)` at frequency `f` into `bins` phase bins.
pub fn fold(clean: &Clean, start: usize, end: usize, f: f64, bins: usize) -> Fold {
    let end = end.min(clean.x.len());
    let mut sum = vec![0.0; bins];
    let mut count = vec![0u32; bins];
    for j in start..end {
        if !clean.ok[j] {
            continue;
        }
        let ph = f * j as f64 / FS;
        let frac = ph - ph.floor();
        let b = ((frac * bins as f64) as usize).min(bins - 1);
        sum[b] += clean.x[j];
        count[b] += 1;
    }
    let mean = sum
        .iter()
        .zip(&count)
        .map(|(s, c)| if *c > 0 { s / *c as f64 } else { 0.0 })
        .collect();
    Fold {
        mean,
        count,
        sigma: clean.sigma,
    }
}

impl Fold {
    /// Index of the highest bin after a 3-bin circular boxcar.
    pub fn peak_bin(&self) -> usize {
        let n = self.mean.len();
        let mut best = (0, f64::NEG_INFINITY);
        for i in 0..n {
            let v = self.mean[(i + n - 1) % n] + self.mean[i] + self.mean[(i + 1) % n];
            if v > best.1 {
                best = (i, v);
            }
        }
        best.0
    }
    /// Phase (cycles) of the profile peak, sub-bin refined.
    pub fn peak_phase(&self) -> f64 {
        crate::sim::boxcar_peak_phase(&self.mean)
    }
    /// Boxcar matched-filter significance of the strongest feature (sigma).
    pub fn snr(&self) -> f64 {
        let n = self.mean.len();
        let total: f64 = self.count.iter().map(|c| *c as f64).sum();
        let mean_all: f64 = self
            .mean
            .iter()
            .zip(&self.count)
            .map(|(m, c)| m * *c as f64)
            .sum::<f64>()
            / total.max(1.0);
        let mut best = f64::NEG_INFINITY;
        for width in [1usize, 2, 4, 8] {
            if width >= n {
                continue;
            }
            for i in 0..n {
                let (mut s, mut cnt) = (0.0, 0.0);
                for k in 0..width {
                    let b = (i + k) % n;
                    s += (self.mean[b] - mean_all) * self.count[b] as f64;
                    cnt += self.count[b] as f64;
                }
                if cnt > 0.0 {
                    let z = s / (self.sigma * cnt.sqrt());
                    best = best.max(z);
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    fn tone(f: f64, amp: f64, n: usize, noise: f64, seed: u64) -> Clean {
        let mut r = Rng::new(seed);
        let raw: Vec<f64> = (0..n)
            .map(|j| amp * (TAU * f * j as f64 / FS).cos() + noise * r.normal())
            .collect();
        preprocess(&raw)
    }

    #[test]
    fn fft_matches_direct_dft() {
        let n = 64;
        let mut r = Rng::new(3);
        let x: Vec<f64> = (0..n).map(|_| r.normal()).collect();
        let (mut re, mut im) = (x.clone(), vec![0.0; n]);
        fft(&mut re, &mut im);
        for k in [0usize, 1, 5, 17, 31, 32, 63] {
            let (mut dr, mut di) = (0.0, 0.0);
            for (j, v) in x.iter().enumerate() {
                let a = -TAU * (k * j) as f64 / n as f64;
                dr += v * a.cos();
                di += v * a.sin();
            }
            assert!(
                (re[k] - dr).abs() < 1e-9 && (im[k] - di).abs() < 1e-9,
                "bin {k}"
            );
        }
    }

    #[test]
    fn periodogram_is_calibrated_on_white_noise() {
        // Instrument rule: on known noise the normalised power must average ~1.
        let c = tone(1.0, 0.0, 8192, 1.0, 5);
        let s = periodogram(&c, 8192, 8192);
        let lo = s.bin(2.0);
        let hi = s.bin(28.0);
        let mean: f64 = s.power[lo..hi].iter().sum::<f64>() / (hi - lo) as f64;
        assert!((mean - 1.0).abs() < 0.06, "mean power {mean}");
    }

    #[test]
    fn periodogram_recovers_a_known_tone() {
        let c = tone(3.1, 0.4, 8192, 1.0, 9);
        let s = periodogram(&c, 8192, 8192);
        let k = (0..s.power.len())
            .max_by(|a, b| s.power[*a].total_cmp(&s.power[*b]))
            .unwrap();
        assert!((s.freq(k) - 3.1).abs() < 0.02, "peak at {}", s.freq(k));
        assert!(s.power[k] > 30.0);
    }

    #[test]
    fn refinement_beats_the_bin_grid() {
        let c = tone(2.0371, 0.5, 16384, 1.0, 11);
        let (f, _) = refine_frequency(&c, 16384, 2.04, 0.004, 15, 1);
        assert!((f - 2.0371).abs() < 2e-5, "refined {f}");
    }

    #[test]
    fn fourier_sum_recurrence_matches_direct_evaluation() {
        let c = tone(1.7, 0.5, 2048, 1.0, 13);
        let (s, _) = fourier_sums(&c, 100, 2000, 1.7, 2);
        for h in 1..=2usize {
            let (mut dr, mut di) = (0.0, 0.0);
            for j in 100..2000 {
                let a = -TAU * h as f64 * 1.7 * j as f64 / FS;
                dr += c.x[j] * a.cos();
                di += c.x[j] * a.sin();
            }
            assert!((s[h - 1].0 - dr).abs() < 1e-6 && (s[h - 1].1 - di).abs() < 1e-6);
        }
    }

    #[test]
    fn invalid_samples_are_masked_not_propagated() {
        let mut raw = vec![0.0; 4096];
        let mut r = Rng::new(2);
        for v in raw.iter_mut() {
            *v = r.normal();
        }
        raw[10] = f64::NAN;
        raw[11] = f64::INFINITY;
        raw[12] = f64::NEG_INFINITY;
        raw[2000] = 60.0;
        let c = preprocess(&raw);
        assert_eq!(c.masked, 4);
        assert!(c.x.iter().all(|v| v.is_finite()));
        assert!(!c.ok[10] && !c.ok[11] && !c.ok[12] && !c.ok[2000]);
        let s = periodogram(&c, 4096, 4096);
        assert!(s.power.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn chi2_sigma_is_monotone_and_centred() {
        assert!(
            chi2_sigma(7.34, 4).abs() < 0.3,
            "median of chi2(8) maps near 0 sigma"
        );
        assert!(chi2_sigma(48.0, 4) > chi2_sigma(20.0, 4));
        // exact tail: P(chi2_8 > 30) ~ 2.1e-4 -> ~3.5 sigma
        let z = chi2_sigma(26.12, 4);
        assert!((z - 3.0).abs() < 0.2, "z {z}");
    }

    #[test]
    fn fold_finds_an_injected_period_and_phase() {
        let f = 1.2345;
        let mut r = Rng::new(21);
        let raw: Vec<f64> = (0..16384)
            .map(|j| {
                let ph = f * j as f64 / FS;
                let fr = ph - ph.floor();
                let d = (fr - 0.4) / 0.03;
                1.4 * (-0.5 * d * d).exp() + r.normal()
            })
            .collect();
        let c = preprocess(&raw);
        let (fr, _) = refine_frequency(&c, 16384, f + 0.002, 0.004, 21, 2);
        assert!((fr - f).abs() < 1.5e-4, "recovered {fr}");
        let fo = fold(&c, 0, 16384, fr, 64);
        assert!(
            (fo.peak_phase() - 0.4).abs() < 0.05,
            "phase {}",
            fo.peak_phase()
        );
        let detuned = fold(&c, 0, 16384, f + 0.01, 64);
        assert!(fo.snr() > detuned.snr() + 3.0);
    }
}
