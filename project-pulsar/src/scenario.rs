//! The synthetic universe: ground truth and the seeded receiver stream.
//!
//! Three persistent sources (ALPHA, BETA, GAMMA) are buried far below the noise in a
//! three-station receiver array. Two interferers lie in wait: a persistent
//! terrestrial line (RFI) and a transient that looks like a fourth source for a
//! while and then fades (the GHOST). Nothing here is real astrophysics; every
//! number is invented, deterministic, and a pure function of the seed.
//!
//! The analysis (`crate::analysis`) never reads the truth in this module — it only
//! reads the stream. Truth is consulted by the *tests* and by the optional "truth
//! overlay" so that the final interpretation can be checked against what was injected.

use crate::rng::{gauss, Rng};
use std::f64::consts::TAU;

/// Receiver sample rate (Hz).
pub const FS: f64 = 32.0;
/// Observation length (s).
pub const T_END: f64 = 480.0;
/// Samples per station for the whole observation.
pub const N_TOTAL: usize = 15_360;
/// Analysis cadence: one checkpoint per chunk of data (8 s).
pub const CHUNK: usize = 256;
/// Number of checkpoints.
pub const N_CP: usize = N_TOTAL / CHUNK;
/// Station positions in light-milliseconds (east, north). A source in direction
/// cosines `(l, m)` arrives at station `s` `(x·l + y·m)` ms earlier than at the origin.
pub const STATIONS: [(f64, f64); 3] = [(0.0, 0.0), (56.0, 0.0), (18.0, 48.0)];
/// The default seed ("PULSAR" in ASCII, folded).
pub const DEFAULT_SEED: u64 = 62;

/// The three persistent identities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SigId {
    Alpha,
    Beta,
    Gamma,
}

impl SigId {
    pub const ALL: [SigId; 3] = [SigId::Alpha, SigId::Beta, SigId::Gamma];
    pub fn idx(self) -> usize {
        self as usize
    }
    pub fn from_idx(i: usize) -> SigId {
        SigId::ALL[i % 3]
    }
    pub fn name(self) -> &'static str {
        ["ALPHA", "BETA", "GAMMA"][self.idx()]
    }
    /// One-cell signature glyph (the shape channel of the identity).
    pub fn glyph(self) -> &'static str {
        ["◆", "●", "▲"][self.idx()]
    }
    /// Letter used in direct labels; survives Mono and ASCII.
    pub fn letter(self) -> char {
        ['A', 'B', 'C'][self.idx()]
    }
    /// Accent colour (the hue channel of the identity).
    pub fn color(self) -> (u8, u8, u8) {
        [(76, 218, 244), (249, 193, 95), (226, 120, 214)][self.idx()]
    }
    /// What kind of thing this source is (the catalogue prior, not truth about the
    /// seed): ALPHA is a pulse-train, BETA a drifting carrier, GAMMA a slow broad pulse.
    pub fn kind(self) -> &'static str {
        [
            "fast pulse train",
            "drifting carrier",
            "slow broad-pulse train",
        ][self.idx()]
    }
}

/// A periodic pulse train.
#[derive(Clone, Copy, Debug)]
pub struct CombTruth {
    pub f: f64,
    pub phi0: f64,
    /// Gaussian pulse width in phase cycles.
    pub sigma: f64,
    pub amp: f64,
    pub lm: (f64, f64),
}

/// A carrier with a linear frequency drift.
#[derive(Clone, Copy, Debug)]
pub struct ChirpTruth {
    pub f0: f64,
    pub fdot: f64,
    pub phi0: f64,
    pub amp: f64,
    pub lm: (f64, f64),
}

/// Persistent terrestrial interference: a bright line whose *amplitude differs from
/// station to station* and whose delays correspond to no far-field direction.
#[derive(Clone, Copy, Debug)]
pub struct RfiTruth {
    pub f: f64,
    pub phi0: f64,
    pub amp: [f64; 3],
    pub tau: [f64; 3],
}

/// A transient line: present for a while, then gone.
#[derive(Clone, Copy, Debug)]
pub struct GhostTruth {
    pub f: f64,
    pub phi0: f64,
    pub amp: f64,
    pub t_on: f64,
    pub t_off: f64,
    pub lm: (f64, f64),
}

/// The complete ground truth of one universe.
#[derive(Clone, Debug)]
pub struct Scenario {
    pub seed: u64,
    pub alpha: CombTruth,
    pub beta: ChirpTruth,
    pub gamma: CombTruth,
    pub rfi: RfiTruth,
    pub ghost: GhostTruth,
    /// White receiver noise (per-sample standard deviation).
    pub noise_sigma: f64,
    /// Slow receiver wander (AR(1)) standard deviation and pole.
    pub red_sigma: f64,
    pub red_rho: f64,
}

/// Geometric delay (s) of direction `lm` at station `s`.
pub fn delay_s(lm: (f64, f64), s: usize) -> f64 {
    (STATIONS[s].0 * lm.0 + STATIONS[s].1 * lm.1) * 1e-3
}

fn in_disc(lm: (f64, f64), r: f64) -> bool {
    lm.0 * lm.0 + lm.1 * lm.1 <= r * r
}

impl Scenario {
    /// Build a universe from a seed. Constraint-rejection keeps the sources
    /// resolvable in principle (separated in sky and frequency); it never looks at
    /// the analysis.
    pub fn from_seed(seed: u64) -> Scenario {
        let mut rng = Rng::new(seed);
        loop {
            let fa = rng.range(1.10, 2.30);
            let fb0 = rng.range(5.4, 7.0);
            let fdot = (if rng.coin() { 1.0 } else { -1.0 }) * rng.range(0.6e-3, 1.0e-3);
            let fc = rng.range(0.35, 0.65);
            let f_rfi = rng.range(11.0, 13.5);
            let f_ghost = rng.range(3.9, 4.6);

            // Frequency-separation constraints (comb harmonics vs everything else).
            let near = |a: f64, b: f64, tol: f64| (a - b).abs() < tol;
            let mut ok = true;
            for k in 1..=10 {
                let h = k as f64 * fa;
                if near(h, f_ghost, 0.12) || near(h, f_rfi, 0.12) {
                    ok = false;
                }
                for step in 0..=24 {
                    let t = step as f64 * 20.0;
                    if near(h, fb0 + fdot * t, 0.12) {
                        ok = false;
                    }
                }
            }
            for k in 1..=4 {
                for h in 1..=3 {
                    if near(k as f64 * fa, h as f64 * fc, 0.06) {
                        ok = false;
                    }
                }
            }
            if !ok {
                continue;
            }

            // Sky positions: inside the disc, mutually well separated.
            let mut pos = Vec::new();
            let mut tries = 0;
            while pos.len() < 4 && tries < 2000 {
                tries += 1;
                let cand = (rng.range(-0.8, 0.8), rng.range(-0.8, 0.8));
                if !in_disc(cand, 0.78) {
                    continue;
                }
                if pos.iter().all(|p: &(f64, f64)| {
                    ((p.0 - cand.0).powi(2) + (p.1 - cand.1).powi(2)).sqrt() > 0.38
                }) {
                    pos.push(cand);
                }
            }
            if pos.len() < 4 {
                continue;
            }

            let phase = |r: &mut Rng| r.range(0.0, 1.0);
            let alpha = CombTruth {
                f: fa,
                phi0: phase(&mut rng),
                sigma: 0.05,
                amp: 0.45,
                lm: pos[0],
            };
            let beta = ChirpTruth {
                f0: fb0,
                fdot,
                phi0: phase(&mut rng),
                amp: 0.13,
                lm: pos[1],
            };
            let gamma = CombTruth {
                f: fc,
                phi0: phase(&mut rng),
                sigma: 0.11,
                amp: 0.45,
                lm: pos[2],
            };
            // Terrestrial line: station-dependent amplitude, delays that no far-field
            // direction can produce (|l| > 1 on the long baseline).
            let rfi = RfiTruth {
                f: f_rfi,
                phi0: phase(&mut rng),
                amp: [0.34, 0.12, 0.05],
                tau: [0.0, 0.066, -0.043],
            };
            let t_on = rng.range(18.0, 34.0);
            let ghost = GhostTruth {
                f: f_ghost,
                phi0: phase(&mut rng),
                amp: 0.16,
                t_on,
                t_off: t_on + rng.range(120.0, 150.0),
                lm: pos[3],
            };
            return Scenario {
                seed,
                alpha,
                beta,
                gamma,
                rfi,
                ghost,
                noise_sigma: 1.0,
                red_sigma: 0.6,
                red_rho: (-TAU * 0.12 / FS).exp(),
            };
        }
    }

    /// Truth direction of a persistent identity.
    pub fn lm(&self, id: SigId) -> (f64, f64) {
        match id {
            SigId::Alpha => self.alpha.lm,
            SigId::Beta => self.beta.lm,
            SigId::Gamma => self.gamma.lm,
        }
    }

    /// Truth carrier / fundamental frequency of an identity at time `t`.
    pub fn freq_at(&self, id: SigId, t: f64) -> f64 {
        match id {
            SigId::Alpha => self.alpha.f,
            SigId::Beta => self.beta.f0 + self.beta.fdot * t,
            SigId::Gamma => self.gamma.f,
        }
    }

    /// The noiseless signal part at station `s`, time `t` (s).
    pub fn signal(&self, s: usize, t: f64) -> f64 {
        let comb = |c: &CombTruth| -> f64 {
            let ts = t + delay_s(c.lm, s);
            let ph = c.f * ts + c.phi0;
            let u = ph - (ph + 0.5).floor(); // wrap into [-0.5, 0.5)
            let dc = c.sigma * TAU.sqrt();
            c.amp * ((-0.5 * (u / c.sigma).powi(2)).exp() - dc)
        };
        let b = &self.beta;
        let tb = t + delay_s(b.lm, s);
        let chirp = b.amp * (TAU * (b.f0 * tb + 0.5 * b.fdot * tb * tb) + TAU * b.phi0).cos();
        let r = &self.rfi;
        let rfi = r.amp[s] * (TAU * (r.f * (t + r.tau[s]) + r.phi0)).cos();
        let g = &self.ghost;
        let w = |x: f64| {
            let x = x.clamp(0.0, 1.0);
            x * x * (3.0 - 2.0 * x)
        };
        let ramp = 6.0;
        let env = w((t - g.t_on) / ramp) * (1.0 - w((t - g.t_off) / ramp));
        let tg = t + delay_s(g.lm, s);
        let ghost = g.amp * env * (TAU * (g.f * tg + g.phi0)).cos();
        comb(&self.alpha) + chirp + comb(&self.gamma) + rfi + ghost
    }
}

/// The receiver: three precomputed, deterministic sample streams.
///
/// Generating the whole stream eagerly (≈46k samples) makes seeking trivial — a
/// seek is an index — and keeps the AR(1) wander a pure function of the seed.
#[derive(Clone, Debug)]
pub struct Receiver {
    pub scn: Scenario,
    x: [Vec<f32>; 3],
}

impl Receiver {
    pub fn new(seed: u64) -> Receiver {
        Receiver::from_scenario(Scenario::from_seed(seed))
    }

    pub fn from_scenario(scn: Scenario) -> Receiver {
        let mut x = [
            Vec::with_capacity(N_TOTAL),
            Vec::with_capacity(N_TOTAL),
            Vec::with_capacity(N_TOTAL),
        ];
        for (s, xs) in x.iter_mut().enumerate() {
            let mut red = 0.0f64;
            let innov = (1.0 - scn.red_rho * scn.red_rho).sqrt() * scn.red_sigma;
            // burn-in so the AR(1) state is stationary at n = 0
            red += gauss(scn.seed, 100 + s as u64, u64::MAX / 3) * scn.red_sigma;
            for n in 0..N_TOTAL {
                red = scn.red_rho * red + innov * gauss(scn.seed, 100 + s as u64, n as u64);
                let white = scn.noise_sigma * gauss(scn.seed, s as u64, n as u64);
                let t = n as f64 / FS;
                xs.push((white + red + scn.signal(s, t)) as f32);
            }
        }
        Receiver { scn, x }
    }

    /// A noise-only receiver (all signal amplitudes zero): the null for calibration.
    pub fn null(seed: u64) -> Receiver {
        let mut scn = Scenario::from_seed(seed);
        scn.alpha.amp = 0.0;
        scn.beta.amp = 0.0;
        scn.gamma.amp = 0.0;
        scn.rfi.amp = [0.0; 3];
        scn.ghost.amp = 0.0;
        Receiver::from_scenario(scn)
    }

    /// Station `s` samples (all `N_TOTAL`; callers must only read the prefix that
    /// has "arrived" — see `Checkpoint::n`).
    pub fn station(&self, s: usize) -> &[f32] {
        &self.x[s]
    }

    pub fn sample(&self, s: usize, n: usize) -> f64 {
        self.x[s][n] as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_is_pure_function_of_seed() {
        let a = Scenario::from_seed(42);
        let b = Scenario::from_seed(42);
        assert_eq!(a.alpha.f, b.alpha.f);
        assert_eq!(a.beta.fdot, b.beta.fdot);
        let c = Scenario::from_seed(43);
        assert_ne!(a.alpha.f, c.alpha.f);
    }

    #[test]
    fn receiver_deterministic() {
        let r1 = Receiver::new(DEFAULT_SEED);
        let r2 = Receiver::new(DEFAULT_SEED);
        assert_eq!(r1.station(1), r2.station(1));
        assert_eq!(r1.station(0).len(), N_TOTAL);
    }

    #[test]
    fn delays_within_unambiguous_window() {
        for seed in 0..50u64 {
            let s = Scenario::from_seed(seed);
            for id in SigId::ALL {
                for st in 0..3 {
                    let tau = delay_s(s.lm(id), st).abs();
                    assert!(tau < 0.056 + 1e-9, "tau {tau}");
                }
            }
        }
    }
}
