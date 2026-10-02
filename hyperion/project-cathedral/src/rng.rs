//! A tiny deterministic PRNG owned by the application.
//!
//! The simulation must replay byte-identically from `fixture + seed + journal`, so
//! randomness is confined to a single explicit generator. Nothing here touches the
//! wall clock, the OS entropy pool, or the terminal.

/// SplitMix64 — a small, fast, well-distributed deterministic generator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng {
    state: u64,
    calls: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
            calls: 0,
        }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.calls += 1;
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    #[inline]
    pub fn f32(&mut self) -> f32 {
        // 24 mantissa bits, exactly representable in f32.
        ((self.next_u64() >> 40) as f32) / ((1u32 << 24) as f32)
    }

    /// Uniform integer in `[lo, hi)`.
    #[inline]
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next_u64() % (hi - lo) as u64) as u32
    }

    /// A stable per-index value in `[0,1)` that does not consume generator state.
    pub fn hash01(seed: u64, index: u64) -> f32 {
        let mut z = seed ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (((z ^ (z >> 31)) >> 40) as f32) / ((1u32 << 24) as f32)
    }

    pub fn calls(&self) -> u64 {
        self.calls
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_across_instances() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn f32_in_unit_interval() {
        let mut r = Rng::new(7);
        for _ in 0..10_000 {
            let x = r.f32();
            assert!((0.0..1.0).contains(&x), "out of range: {x}");
        }
    }

    #[test]
    fn hash01_is_stable_and_distinct() {
        let a = Rng::hash01(1, 5);
        assert_eq!(a, Rng::hash01(1, 5));
        assert_ne!(Rng::hash01(1, 5), Rng::hash01(1, 6));
        assert!((0.0..1.0).contains(&a));
    }
}
