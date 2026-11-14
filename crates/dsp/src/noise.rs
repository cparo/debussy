//! A small, fast, seeded pseudo-random generator: xorshift64* (Marsaglia 2003, Vigna 2016),
//! seeded through SplitMix64 so every seed, including 0, gives a good state. Both are public,
//! textbook techniques. Not for cryptography.
//!
//! [`Pink`] filters it to pink (1/f) noise through a cascade of first-order sections whose real
//! poles and zeros alternate along the frequency axis (J. O. Smith, *Spectral Audio Signal
//! Processing*, and his spectral-tilt filters; ADR-0122).

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

/// The lowest pole of a [`Pink`] filter, in Hz.
const PINK_LOWEST_POLE_HZ: f64 = 5.0;

/// The most sections a [`Pink`] filter has: one per octave from [`PINK_LOWEST_POLE_HZ`], enough
/// for rates up to about 650 kHz.
const PINK_MAX_SECTIONS: usize = 16;

/// The frequency at which a [`Pink`] filter's gain is 1, in Hz.
const PINK_UNITY_HZ: f64 = 1_000.0;

/// One first-order section, `(1 − q·z⁻¹) / (1 − p·z⁻¹)`, and its state.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Section {
    /// The pole.
    p: f64,
    /// The zero.
    q: f64,
    /// The last input.
    x1: f64,
    /// The last output.
    y1: f64,
}

impl Section {
    /// The next output for input `x`.
    fn step(&mut self, x: f64) -> f64 {
        let y = x - self.q * self.x1 + self.p * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }

    /// The magnitude of the response at `omega` radians per sample.
    fn magnitude(&self, omega: f64) -> f64 {
        let (c, s) = (omega.cos(), omega.sin());
        let numerator = (1.0 - self.q * c).hypot(self.q * s);
        let denominator = (1.0 - self.p * c).hypot(self.p * s);
        numerator / denominator
    }
}

/// Seeded pink noise: [`Noise::uniform`] values through a cascade of first-order sections, one
/// pole per octave from 5 Hz up to the Nyquist frequency and a zero half an octave above each,
/// at the given sample rate. Its power falls 3 dB per octave; its magnitude at 1 kHz is that of
/// the uniform values. The same seed and rate always give the same sequence.
#[derive(Debug, Clone, PartialEq)]
pub struct Pink {
    noise: Noise,
    sections: [Section; PINK_MAX_SECTIONS],
    /// How many of `sections` are used.
    count: usize,
    /// Scales the cascade to unity at [`PINK_UNITY_HZ`].
    gain: f64,
}

impl Pink {
    /// Pink noise from `seed` at `sample_rate` Hz. A rate too low for any section gives the
    /// uniform values unfiltered.
    pub fn new(seed: u64, sample_rate: u32) -> Self {
        let rate = f64::from(sample_rate);
        let nyquist = rate / 2.0;
        let radius = |hz: f64| (-std::f64::consts::TAU * hz / rate).exp();
        let mut sections = [Section::default(); PINK_MAX_SECTIONS];
        let mut count = 0;
        let mut pole = PINK_LOWEST_POLE_HZ;
        while count < PINK_MAX_SECTIONS && pole < nyquist {
            sections[count] = Section {
                p: radius(pole),
                q: radius(pole * std::f64::consts::SQRT_2),
                ..Section::default()
            };
            count += 1;
            pole *= 2.0;
        }
        let omega = std::f64::consts::TAU * PINK_UNITY_HZ.min(nyquist / 2.0) / rate.max(1.0);
        let magnitude: f64 = sections[..count]
            .iter()
            .map(|s| s.magnitude(omega))
            .product();
        let gain = if magnitude > 0.0 && magnitude.is_finite() {
            magnitude.recip()
        } else {
            1.0
        };
        Self {
            noise: Noise::new(seed),
            sections,
            count,
            gain,
        }
    }

    /// The next sample.
    pub fn sample(&mut self) -> f32 {
        let x = f64::from(self.noise.uniform());
        let y = self.sections[..self.count]
            .iter_mut()
            .fold(x, |x, section| section.step(x));
        // Pink noise of a few times the uniform values' level: well within f32.
        #[allow(clippy::cast_possible_truncation)]
        let y = (y * self.gain) as f32;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Stft;

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

    #[test]
    fn test_pink_new_same_seed_same_sequence() {
        let (mut a, mut b) = (Pink::new(3, 48_000), Pink::new(3, 48_000));
        for _ in 0..1_000 {
            assert_eq!(a.sample(), b.sample());
        }
        assert_ne!(Pink::new(3, 48_000).sample(), Pink::new(4, 48_000).sample());
    }

    #[test]
    fn test_pink_new_without_sections_is_white() {
        let (mut pink, mut white) = (Pink::new(5, 0), Noise::new(5));
        assert_eq!(pink.count, 0);
        assert_eq!(pink.sample(), white.uniform());
    }

    /// The mean power per bin of `pink` in each of `bands` (low and high edges in Hz), over
    /// `frames` Hann-windowed frames of `size` at `rate`.
    fn band_powers(
        pink: &mut Pink,
        rate: u32,
        size: usize,
        frames: usize,
        bands: &[(f32, f32)],
    ) -> Result<Vec<f64>, crate::Error> {
        let mut stft = Stft::new(size)?;
        let mut frame = vec![0.0_f32; size];
        let mut magnitudes = vec![0.0_f32; stft.bins()];
        let mut power = vec![0.0_f64; stft.bins()];
        for _ in 0..frames {
            frame.iter_mut().for_each(|x| *x = pink.sample());
            stft.magnitudes(&frame, &mut magnitudes)?;
            for (p, &m) in power.iter_mut().zip(&magnitudes) {
                *p += f64::from(m) * f64::from(m);
            }
        }
        let bin_hz = rate as f32 / size as f32;
        Ok(bands
            .iter()
            .map(|&(low, high)| {
                let (start, end) = ((low / bin_hz) as usize, (high / bin_hz) as usize);
                power[start..end].iter().sum::<f64>() / (end - start) as f64
            })
            .collect())
    }

    #[test]
    fn test_pink_falls_three_db_per_octave() -> Result<(), crate::Error> {
        for (rate, size, top) in [(48_000, 4096, 12_800.0), (192_000, 16_384, 51_200.0)] {
            let mut pink = Pink::new(9, rate);
            // Settle the low sections.
            for _ in 0..rate {
                pink.sample();
            }
            // Third-octave bands an octave apart, from 100 Hz up to `top`.
            let mut bands = Vec::new();
            let mut centre = 100.0_f32;
            while centre <= top {
                bands.push((centre / 1.122, centre * 1.122));
                centre *= 2.0;
            }
            let powers = band_powers(&mut pink, rate, size, 300, &bands)?;
            let db: Vec<f64> = powers.iter().map(|p| 10.0 * p.log10()).collect();
            for (octave, pair) in db.windows(2).enumerate() {
                let step = pair[1] - pair[0];
                assert!(
                    (step + 3.01).abs() < 1.0,
                    "{rate} Hz, octave {octave}: {step} dB"
                );
            }
            let total = db[db.len() - 1] - db[0];
            let expected = -3.01 * (db.len() - 1) as f64;
            assert!((total - expected).abs() < 1.5, "{rate} Hz: {total} dB");
        }
        Ok(())
    }
}
