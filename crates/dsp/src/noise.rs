//! A small, fast, seeded pseudo-random generator: xorshift64* (Marsaglia 2003, Vigna 2016),
//! seeded through SplitMix64 so every seed, including 0, gives a good state. Both are public,
//! textbook techniques. Not for cryptography.

/// A deterministic noise source: the same seed always gives the same sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Noise {
    state: u64,
}

impl Noise {
    /// A generator for `seed`.
    pub fn new(seed: u64) -> Self {
        // The SplitMix64 finaliser spreads nearby seeds over unrelated states.
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // xorshift must not start at 0, which it would never leave.
        Self {
            state: if z == 0 { 0x9E37_79B9_7F4A_7C15 } else { z },
        }
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        self.state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A uniform value in `[-0.5, 0.5)`, from the top 24 bits so it is exact in `f32`.
    pub fn uniform(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 * (1.0 / 16_777_216.0) - 0.5
    }

    /// A triangular (TPDF) value in `(-1, 1)`: the sum of two independent uniforms.
    pub fn triangular(&mut self) -> f32 {
        self.uniform() + self.uniform()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_same_seed_same_sequence() {
        let (mut a, mut b) = (Noise::new(7), Noise::new(7));
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_ne!(Noise::new(7).next_u64(), Noise::new(8).next_u64());
    }

    #[test]
    fn test_new_seed_zero_is_usable() {
        let mut n = Noise::new(0);
        let first = n.next_u64();
        assert_ne!(first, 0);
        assert_ne!(first, n.next_u64());
    }

    #[test]
    fn test_triangular_has_tpdf_statistics() {
        // Triangular on (-1, 1): mean 0, variance 1/6, and 3/4 of values within ±0.5.
        let mut n = Noise::new(2);
        let count = 400_000;
        let (mut sum, mut sum_sq, mut inner) = (0.0_f64, 0.0_f64, 0_u32);
        for _ in 0..count {
            let t = n.triangular();
            assert!(t > -1.0 && t < 1.0, "{t}");
            sum += f64::from(t);
            sum_sq += f64::from(t) * f64::from(t);
            if t.abs() < 0.5 {
                inner += 1;
            }
        }
        let count = f64::from(count);
        assert!((sum / count).abs() < 0.005, "mean");
        assert!((sum_sq / count - 1.0 / 6.0).abs() < 0.002, "variance");
        assert!((f64::from(inner) / count - 0.75).abs() < 0.005, "shape");
    }

    #[test]
    fn test_uniform_stays_in_range_with_zero_mean() {
        let mut n = Noise::new(1);
        let count = 200_000;
        let mut sum = 0.0_f64;
        for _ in 0..count {
            let u = n.uniform();
            assert!((-0.5..0.5).contains(&u), "{u}");
            sum += f64::from(u);
        }
        assert!((sum / f64::from(count)).abs() < 0.005);
    }
}
