//! Deterministic pseudo-random numbers (SplitMix64 + Box-Muller). No `rand`
//! crate: the observatory must be byte-reproducible from a seed alone.

#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
    spare: Option<f64>,
}

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
            spare: None,
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Standard normal.
    pub fn normal(&mut self) -> f64 {
        if let Some(s) = self.spare.take() {
            return s;
        }
        loop {
            let u = self.uniform();
            if u <= 1e-300 {
                continue;
            }
            let v = self.uniform();
            let r = (-2.0 * u.ln()).sqrt();
            let th = std::f64::consts::TAU * v;
            self.spare = Some(r * th.sin());
            return r * th.cos();
        }
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.uniform() * n as f64) as usize % n.max(1)
    }
}

/// FNV-1a 64 over bytes: the digest used for determinism checks.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..1000 {
            assert_eq!(a.normal().to_bits(), b.normal().to_bits());
        }
    }

    #[test]
    fn normal_moments_are_calibrated() {
        let mut r = Rng::new(1);
        let n = 200_000;
        let (mut s, mut s2) = (0.0, 0.0);
        for _ in 0..n {
            let x = r.normal();
            s += x;
            s2 += x * x;
        }
        let mean = s / n as f64;
        let var = s2 / n as f64 - mean * mean;
        assert!(mean.abs() < 0.01, "mean {mean}");
        assert!((var - 1.0).abs() < 0.02, "var {var}");
    }
}
