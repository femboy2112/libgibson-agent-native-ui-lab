//! The synthetic world: five source "slots" (three persistent periodic sources
//! and two false candidates), a four-station receiver array, and the raw noisy
//! data streams they produce. Everything is a pure function of the seed.
//!
//! Units are SYNTHETIC. Seconds are mission seconds, the baseline is expressed
//! directly in light-seconds, and nothing here claims to describe any real
//! astrophysical object.

use crate::rng::Rng;

/// Receiver sample rate (Hz).
pub const FS: f64 = 64.0;
/// Samples per station stream (a power of two: the periodogram is one FFT).
pub const N: usize = 16384;
/// Total mission length (s).
pub const DURATION: f64 = N as f64 / FS;
/// The analysis pipeline publishes a solution every `EPOCH_S` seconds.
pub const EPOCH_S: f64 = 4.0;
pub const N_EPOCHS: usize = 64;
/// Samples integrated per epoch.
pub const EPOCH_SAMPLES: usize = N / N_EPOCHS;
/// Station 4 (the out-of-plane element) comes online at this time (s).
pub const T_S4_ON: f64 = 140.0;
pub const S4_ON_SAMPLE: usize = (T_S4_ON * FS) as usize;
/// Number of candidate slots.
pub const SLOTS: usize = 5;
/// Number of stations.
pub const STATIONS: usize = 4;

/// Station positions (light-seconds). Stations 1-3 lie in the z = 0 plane (so a
/// 3-station solution has a mirror ambiguity); station 4 sits out of plane.
pub const STATION_POS: [[f64; 3]; STATIONS] = [
    [0.0, 0.0, 0.0],
    [0.060, 0.0, 0.0],
    [0.030, 0.051962, 0.0],
    [0.030, 0.017321, 0.048],
];
/// Largest in-plane |delay| any source can have (s): bounds harmonic unwrapping.
pub const TAU_MAX: f64 = 0.062;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Persistent periodic pulsed source with a sky position.
    Pulsed,
    /// Continuous-wave interference, identical at every station.
    Interference,
    /// A pulse train that switches on and off.
    Burst,
}

#[derive(Clone, Copy, Debug)]
pub struct Component {
    pub phase: f64,
    pub sigma: f64,
    pub amp: f64,
}

#[derive(Clone, Debug)]
pub struct Truth {
    pub slot: usize,
    pub kind: Kind,
    pub f: f64,
    pub comps: Vec<Component>,
    /// Sinusoid amplitude (interference only).
    pub sine_amp: f64,
    /// Direction in the array frame (degrees); azimuth from +x, elevation from the z = 0 plane.
    pub az: f64,
    pub el: f64,
    pub t_on: f64,
    pub t_off: f64,
}

impl Truth {
    pub fn period(&self) -> f64 {
        1.0 / self.f
    }
    pub fn direction(&self) -> [f64; 3] {
        direction(self.az, self.el)
    }
    /// Time-of-arrival delay of this source at `station` relative to station 1.
    pub fn delay(&self, station: usize) -> f64 {
        if self.kind == Kind::Interference {
            return 0.0;
        }
        station_delay(station, self.direction())
    }
    /// Phase (cycles) of the strongest profile component.
    pub fn peak_phase(&self) -> f64 {
        if self.kind == Kind::Interference {
            return 0.0;
        }
        self.comps
            .iter()
            .max_by(|a, b| a.amp.total_cmp(&b.amp))
            .map(|c| c.phase)
            .unwrap_or(0.0)
    }
    /// Noise-free pulse profile at rotation phase `phase` (cycles), without the
    /// on/off envelope. A sinusoid for interference.
    pub fn profile_at(&self, phase: f64) -> f64 {
        let frac = phase - phase.floor();
        match self.kind {
            Kind::Interference => self.sine_amp * (std::f64::consts::TAU * frac).cos(),
            _ => self
                .comps
                .iter()
                .map(|c| {
                    [-1.0, 0.0, 1.0]
                        .iter()
                        .map(|m| {
                            let d = (frac - c.phase + m) / c.sigma;
                            c.amp * (-0.5 * d * d).exp()
                        })
                        .sum::<f64>()
                })
                .sum(),
        }
    }

    /// Where a 64-bin fold with a 3-bin boxcar would put this profile's peak.
    pub fn fold_peak_phase(&self) -> f64 {
        let bins = 64usize;
        // a fold bin AVERAGES the profile over its width (a pulse narrower than a bin is smeared)
        let sub = 24usize;
        let v: Vec<f64> = (0..bins)
            .map(|b| {
                (0..sub)
                    .map(|k| {
                        self.profile_at((b as f64 + (k as f64 + 0.5) / sub as f64) / bins as f64)
                    })
                    .sum::<f64>()
                    / sub as f64
            })
            .collect();
        boxcar_peak_phase(&v)
    }

    fn envelope(&self, t: f64) -> f64 {
        const TAPER: f64 = 4.0;
        if t < self.t_on || t > self.t_off {
            return 0.0;
        }
        let a = ((t - self.t_on) / TAPER).clamp(0.0, 1.0);
        let b = ((self.t_off - t) / TAPER).clamp(0.0, 1.0);
        let s = |x: f64| x * x * (3.0 - 2.0 * x);
        s(a) * s(b)
    }
    /// Noise-free waveform at time `t` (already shifted for the station).
    pub fn waveform(&self, t: f64) -> f64 {
        let env = self.envelope(t);
        if env == 0.0 {
            return 0.0;
        }
        let ph = self.f * t;
        match self.kind {
            Kind::Interference => env * self.sine_amp * (std::f64::consts::TAU * ph).cos(),
            _ => {
                let frac = ph - ph.floor();
                let mut v = 0.0;
                for c in &self.comps {
                    for m in [-1.0, 0.0, 1.0] {
                        let d = (frac - c.phase + m) / c.sigma;
                        v += c.amp * (-0.5 * d * d).exp();
                    }
                }
                env * v
            }
        }
    }
}

/// Peak phase (cycles) of a circular profile: argmax of a 3-bin circular boxcar,
/// refined to sub-bin accuracy by a parabola through the three boxcar values.
pub fn boxcar_peak_phase(values: &[f64]) -> f64 {
    let n = values.len();
    if n < 3 {
        return 0.0;
    }
    let sm: Vec<f64> = (0..n)
        .map(|i| values[(i + n - 1) % n] + values[i] + values[(i + 1) % n])
        .collect();
    let mut best = (0, f64::NEG_INFINITY);
    for (i, v) in sm.iter().enumerate() {
        if *v > best.1 {
            best = (i, *v);
        }
    }
    let i = best.0;
    let (a, b, c) = (sm[(i + n - 1) % n], sm[i], sm[(i + 1) % n]);
    let den = a - 2.0 * b + c;
    let off = if den.abs() > 1e-12 {
        (0.5 * (a - c) / den).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    ((i as f64 + 0.5 + off) / n as f64).rem_euclid(1.0)
}

pub fn direction(az_deg: f64, el_deg: f64) -> [f64; 3] {
    let (az, el) = (az_deg.to_radians(), el_deg.to_radians());
    [el.cos() * az.cos(), el.cos() * az.sin(), el.sin()]
}

/// `tau_k = t_k - t_1 = -(b_k - b_1) . s` (stations further along `s` hear it first).
pub fn station_delay(station: usize, s: [f64; 3]) -> f64 {
    let b = STATION_POS[station];
    let b1 = STATION_POS[0];
    -((b[0] - b1[0]) * s[0] + (b[1] - b1[1]) * s[1] + (b[2] - b1[2]) * s[2])
}

/// Angular separation (deg) between two (az, el) directions.
pub fn separation_deg(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (u, v) = (direction(a.0, a.1), direction(b.0, b.1));
    let d = (u[0] * v[0] + u[1] * v[1] + u[2] * v[2]).clamp(-1.0, 1.0);
    d.acos().to_degrees()
}

/// The ground-truth catalogue. Slot order is identity order everywhere.
pub fn catalogue() -> Vec<Truth> {
    vec![
        // A: a narrow, single-peaked pulse. Bright harmonics.
        Truth {
            slot: 0,
            kind: Kind::Pulsed,
            f: 1.29731,
            comps: vec![Component {
                phase: 0.30,
                sigma: 0.018,
                amp: 1.15,
            }],
            sine_amp: 0.0,
            az: 40.0,
            el: 50.0,
            t_on: 0.0,
            t_off: DURATION,
        },
        // B: a double-peaked profile (an "interpulse").
        Truth {
            slot: 1,
            kind: Kind::Pulsed,
            f: 2.17184,
            comps: vec![
                Component {
                    phase: 0.20,
                    sigma: 0.022,
                    amp: 0.85,
                },
                Component {
                    phase: 0.58,
                    sigma: 0.030,
                    amp: 0.55,
                },
            ],
            sine_amp: 0.0,
            az: 215.0,
            el: -32.0,
            t_on: 0.0,
            t_off: DURATION,
        },
        // C: a slow, broad hump with a narrow spike.
        Truth {
            slot: 2,
            kind: Kind::Pulsed,
            f: 0.61307,
            comps: vec![
                Component {
                    phase: 0.70,
                    sigma: 0.040,
                    amp: 0.50,
                },
                Component {
                    phase: 0.80,
                    sigma: 0.010,
                    amp: 0.90,
                },
            ],
            sine_amp: 0.0,
            az: 125.0,
            el: 38.0,
            t_on: 0.0,
            t_off: DURATION,
        },
        // D: a continuous-wave interference line, common-mode at every station.
        Truth {
            slot: 3,
            kind: Kind::Interference,
            f: 3.00213,
            comps: vec![],
            sine_amp: 0.34,
            az: 0.0,
            el: 90.0,
            t_on: 0.0,
            t_off: DURATION,
        },
        // E: a transient burst that is only on for a while.
        Truth {
            slot: 4,
            kind: Kind::Burst,
            f: 4.41270,
            comps: vec![Component {
                phase: 0.45,
                sigma: 0.030,
                amp: 2.1,
            }],
            sine_amp: 0.0,
            az: 300.0,
            el: 22.0,
            t_on: 66.0,
            t_off: 118.0,
        },
    ]
}

/// Intervals (s) where station 1 loses its data (receiver dropouts).
pub const DROPOUTS: [(f64, f64); 2] = [(47.0, 49.5), (176.0, 177.4)];

/// The raw data: one stream per station. Invalid samples are NaN / +-inf / huge.
#[derive(Clone, Debug)]
pub struct RawData {
    pub streams: Vec<Vec<f64>>,
    pub truth: Vec<Truth>,
    /// Sample indices (station 1) that were deliberately corrupted.
    pub corrupted: Vec<usize>,
}

pub fn generate(seed: u64, with_sources: bool) -> RawData {
    let truth = catalogue();
    let mut corrupted = Vec::new();
    let mut streams = Vec::with_capacity(STATIONS);
    for k in 0..STATIONS {
        let mut rng = Rng::new(seed ^ (0xA5A5_0000 + k as u64 * 0x1_0001));
        // Slow receiver wander: AR(1) with a ~0.03 Hz corner.
        let rho = (-std::f64::consts::TAU * 0.03 / FS).exp();
        let drive = (1.0 - rho * rho).sqrt() * 1.0;
        let mut red = 0.0;
        let mut x = vec![0.0; N];
        for (n, xn) in x.iter_mut().enumerate() {
            red = rho * red + drive * rng.normal();
            let t = n as f64 / FS;
            let mut v = rng.normal() + red;
            if with_sources {
                for src in &truth {
                    v += src.waveform(t - src.delay(k));
                }
            }
            *xn = v;
        }
        streams.push(x);
    }
    // Corruptions are drawn from a dedicated generator so they never perturb the noise.
    let mut crng = Rng::new(seed ^ 0x00C0_FFEE_D00D);
    // Station 1: dropouts and a few glitches. Others: a handful of glitches.
    for &(a, b) in &DROPOUTS {
        let (lo, hi) = ((a * FS) as usize, (b * FS) as usize);
        streams[0][lo..hi].fill(f64::NAN);
        corrupted.extend(lo..hi);
    }
    for (k, stream) in streams.iter_mut().enumerate() {
        let count = if k == 0 { 16 } else { 6 };
        for _ in 0..count {
            let n = 200 + crng.below(N - 400);
            let amp = 9.0 + 5.0 * crng.uniform();
            let sign = if crng.uniform() < 0.5 { -1.0 } else { 1.0 };
            stream[n] = sign * amp;
            if k == 0 {
                corrupted.push(n);
            }
        }
    }
    // One saturated (infinite) sample and a 3-sample saturation burst on station 1.
    streams[0][9000] = f64::INFINITY;
    corrupted.push(9000);
    streams[0][12000..12003].fill(40.0);
    corrupted.extend(12000..12003);
    // Station 4 is offline until it comes up.
    streams[3][..S4_ON_SAMPLE].fill(f64::NAN);
    corrupted.sort_unstable();
    corrupted.dedup();
    RawData {
        streams,
        truth,
        corrupted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic() {
        let a = generate(42, true);
        let b = generate(42, true);
        for k in 0..STATIONS {
            for n in 0..N {
                assert_eq!(a.streams[k][n].to_bits(), b.streams[k][n].to_bits());
            }
        }
        let c = generate(43, true);
        assert!(a.streams[1] != c.streams[1]);
    }

    #[test]
    fn delays_follow_geometry() {
        let s = direction(40.0, 50.0);
        let tau2 = station_delay(1, s);
        assert!((tau2 + 0.060 * s[0]).abs() < 1e-12);
        const { assert!(TAU_MAX >= 0.0599) };
        // interference is common-mode
        let cat = catalogue();
        assert_eq!(cat[3].delay(1), 0.0);
        assert_eq!(cat[3].delay(2), 0.0);
    }

    #[test]
    fn corruption_is_present() {
        let d = generate(1, true);
        assert!(d.streams[0].iter().any(|v| v.is_nan()));
        assert!(d.streams[0].iter().any(|v| v.is_infinite()));
        assert!(d.streams[3][0].is_nan());
        assert!(d.streams[3][S4_ON_SAMPLE + 5].is_finite());
    }

    #[test]
    fn separation_is_a_metric() {
        assert!(separation_deg((10.0, 20.0), (10.0, 20.0)) < 1e-6);
        assert!((separation_deg((0.0, 0.0), (90.0, 0.0)) - 90.0).abs() < 1e-9);
        assert!((separation_deg((0.0, 90.0), (0.0, -90.0)) - 180.0).abs() < 1e-9);
    }
}
