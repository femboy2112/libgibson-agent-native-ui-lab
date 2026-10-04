//! Counter-based deterministic randomness.
//!
//! Everything random in Pulsar is a pure function of `(seed, stream, index)`; there
//! is no hidden sequential state, so any sample can be recomputed in any order and
//! "same seed ⇒ same universe" holds by construction.

/// SplitMix64 finaliser: a strong 64-bit mixer.
#[inline]
pub fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Hash three words into one.
#[inline]
pub fn hash3(a: u64, b: u64, c: u64) -> u64 {
    mix(mix(mix(a) ^ b.rotate_left(21)) ^ c.rotate_left(42))
}

/// Uniform in `[0, 1)` from a hash (53 random bits).
#[inline]
pub fn unit(h: u64) -> f64 {
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Standard normal deviate from `(seed, stream, index)` via Box–Muller.
#[inline]
pub fn gauss(seed: u64, stream: u64, index: u64) -> f64 {
    let h1 = hash3(seed, stream, index.wrapping_mul(2));
    let h2 = hash3(seed, stream, index.wrapping_mul(2).wrapping_add(1));
    let u1 = unit(h1).max(1e-300);
    let u2 = unit(h2);
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

/// A tiny sequential generator for *scenario construction only* (the stream itself
/// uses the counter-based functions above). Deterministic from its seed.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(mix(seed ^ 0x5055_4C53_4152_5F31))
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }
    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * unit(self.next_u64())
    }
    pub fn coin(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gauss_is_pure_and_roughly_standard() {
        assert_eq!(gauss(1, 2, 3), gauss(1, 2, 3));
        assert_ne!(gauss(1, 2, 3), gauss(1, 2, 4));
        let n = 20_000;
        let (mut s, mut s2) = (0.0, 0.0);
        for i in 0..n {
            let g = gauss(7, 1, i);
            s += g;
            s2 += g * g;
        }
        let mean = s / n as f64;
        let var = s2 / n as f64 - mean * mean;
        assert!(mean.abs() < 0.03, "mean {mean}");
        assert!((var - 1.0).abs() < 0.05, "var {var}");
    }
}
