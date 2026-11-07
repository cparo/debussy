//! Fast Fourier transforms (ADR-0039): an iterative radix-2 decimation-in-time Cooley–Tukey FFT,
//! a real-input FFT computed as a half-length complex FFT plus a split step, and a Hann-windowed
//! short-time spectrum for spectrograms.
//!
//! Constructors precompute twiddle and bit-reversal tables and allocate; call them off the
//! real-time thread. Transforms allocate nothing.

use std::f64::consts::PI;
use std::ops::{Add, Mul, Sub};

use crate::error::Error;

/// The smallest supported transform size.
pub const MIN_SIZE: usize = 64;
/// The largest supported transform size.
pub const MAX_SIZE: usize = 16_384;

/// A complex number of `f32`s.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Complex {
    /// The real part.
    pub re: f32,
    /// The imaginary part.
    pub im: f32,
}

impl Complex {
    /// `re + i·im`.
    pub const fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    /// The complex conjugate.
    pub fn conj(self) -> Self {
        Self::new(self.re, -self.im)
    }

    /// The magnitude.
    pub fn abs(self) -> f32 {
        self.re.hypot(self.im)
    }

    fn scale(self, k: f32) -> Self {
        Self::new(self.re * k, self.im * k)
    }
}

impl Add for Complex {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.re + o.re, self.im + o.im)
    }
}

impl Sub for Complex {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.re - o.re, self.im - o.im)
    }
}

impl Mul for Complex {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        Self::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
}

/// `e^(−2πi·k/n)`, computed in `f64`.
fn twiddle(k: usize, n: usize) -> Complex {
    let angle = -2.0 * PI * k as f64 / n as f64;
    Complex::new(angle.cos() as f32, angle.sin() as f32)
}

fn check_size(size: usize) -> Result<(), Error> {
    if size.is_power_of_two() && (MIN_SIZE..=MAX_SIZE).contains(&size) {
        Ok(())
    } else {
        Err(Error::InvalidFftSize(size))
    }
}

fn check_len(expected: usize, found: usize) -> Result<(), Error> {
    if expected == found {
        Ok(())
    } else {
        Err(Error::LengthMismatch { expected, found })
    }
}

/// A complex FFT of one power-of-two size.
#[derive(Debug, Clone)]
pub struct Fft {
    size: usize,
    /// `e^(−2πi·k/size)` for `k < size / 2`.
    twiddles: Box<[Complex]>,
    /// For each index, its bit reversal, where that is larger (pairs to swap).
    swaps: Box<[(u32, u32)]>,
}

impl Fft {
    /// An FFT of `size` points: a power of two from [`MIN_SIZE`] to [`MAX_SIZE`]. Allocates.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidFftSize`].
    pub fn new(size: usize) -> Result<Self, Error> {
        check_size(size)?;
        Ok(Self::unchecked(size))
    }

    /// Any power of two from 2 up; the real FFT uses half its own size.
    fn unchecked(size: usize) -> Self {
        let bits = size.trailing_zeros();
        let swaps = (0..size)
            .filter_map(|i| {
                let j = i.reverse_bits() >> (usize::BITS - bits);
                // Sizes are at most MAX_SIZE, so indices fit in u32.
                (j > i).then_some((i as u32, j as u32))
            })
            .collect();
        Self {
            size,
            twiddles: (0..size / 2).map(|k| twiddle(k, size)).collect(),
            swaps,
        }
    }

    /// The number of points.
    pub fn size(&self) -> usize {
        self.size
    }

    /// The forward transform of `data`, in place: `X[k] = Σ x[n]·e^(−2πi·kn/N)`, unscaled.
    ///
    /// # Errors
    ///
    /// [`Error::LengthMismatch`] unless `data` has [`Fft::size`] values.
    pub fn forward(&self, data: &mut [Complex]) -> Result<(), Error> {
        check_len(self.size, data.len())?;
        self.transform(data);
        Ok(())
    }

    /// The inverse transform of `data`, in place, scaled by `1/N` so that it undoes
    /// [`Fft::forward`].
    ///
    /// # Errors
    ///
    /// [`Error::LengthMismatch`] unless `data` has [`Fft::size`] values.
    pub fn inverse(&self, data: &mut [Complex]) -> Result<(), Error> {
        check_len(self.size, data.len())?;
        // inverse(x) = conj(forward(conj(x))) / N
        for x in data.iter_mut() {
            *x = x.conj();
        }
        self.transform(data);
        let scale = 1.0 / self.size as f32;
        for x in data.iter_mut() {
            *x = x.conj().scale(scale);
        }
        Ok(())
    }

    fn transform(&self, data: &mut [Complex]) {
        for &(i, j) in self.swaps.iter() {
            data.swap(i as usize, j as usize);
        }
        let mut span = 2;
        while span <= self.size {
            let half = span / 2;
            let stride = self.size / span;
            for block in data.chunks_exact_mut(span) {
                let (low, high) = block.split_at_mut(half);
                for (j, (a, b)) in low.iter_mut().zip(high.iter_mut()).enumerate() {
                    let t = self.twiddles[j * stride] * *b;
                    *b = *a - t;
                    *a = *a + t;
                }
            }
            span *= 2;
        }
    }
}

/// An FFT of real input, computed as a half-length complex FFT plus a split step. Produces the
/// `size / 2 + 1` non-negative-frequency bins; the rest are their conjugates.
#[derive(Debug, Clone)]
pub struct RealFft {
    size: usize,
    half: Fft,
    /// `e^(−2πi·k/size)` for `k ≤ size / 2`.
    split: Box<[Complex]>,
    scratch: Box<[Complex]>,
}

impl RealFft {
    /// A real FFT of `size` points: a power of two from [`MIN_SIZE`] to [`MAX_SIZE`]. Allocates.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidFftSize`].
    pub fn new(size: usize) -> Result<Self, Error> {
        check_size(size)?;
        let half = size / 2;
        Ok(Self {
            size,
            half: Fft::unchecked(half),
            split: (0..=half).map(|k| twiddle(k, size)).collect(),
            scratch: vec![Complex::default(); half].into_boxed_slice(),
        })
    }

    /// The number of real input points.
    pub fn size(&self) -> usize {
        self.size
    }

    /// The number of output bins: `size / 2 + 1`.
    pub fn bins(&self) -> usize {
        self.size / 2 + 1
    }

    /// Transforms `input` (`size` real values) into `output` (`size / 2 + 1` bins), unscaled.
    ///
    /// # Errors
    ///
    /// [`Error::LengthMismatch`] for buffers of the wrong length.
    pub fn forward(&mut self, input: &[f32], output: &mut [Complex]) -> Result<(), Error> {
        check_len(self.size, input.len())?;
        check_len(self.bins(), output.len())?;
        let half = self.size / 2;
        // Pack even samples as real parts and odd samples as imaginary parts.
        for (z, pair) in self.scratch.iter_mut().zip(input.as_chunks::<2>().0) {
            *z = Complex::new(pair[0], pair[1]);
        }
        self.half.transform(&mut self.scratch);
        // Split: X[k] = E[k] + W^k·O[k], with E and O the spectra of the even and odd samples.
        for (k, out) in output.iter_mut().enumerate() {
            let z = self.scratch[k % half];
            let mirror = self.scratch[(half - k % half) % half].conj();
            let even = (z + mirror).scale(0.5);
            // (z − mirror) / 2i
            let d = (z - mirror).scale(0.5);
            let odd = Complex::new(d.im, -d.re);
            *out = even + self.split[k] * odd;
        }
        Ok(())
    }

    /// Inverts [`RealFft::forward`]: `input` (`size / 2 + 1` bins) back to `output` (`size`
    /// real values), scaled by `1/size`. The imaginary parts of the first and last bins are
    /// ignored, as for any real signal's spectrum.
    ///
    /// # Errors
    ///
    /// [`Error::LengthMismatch`] for buffers of the wrong length.
    pub fn inverse(&mut self, input: &[Complex], output: &mut [f32]) -> Result<(), Error> {
        check_len(self.bins(), input.len())?;
        check_len(self.size, output.len())?;
        let half = self.size / 2;
        for k in 0..half {
            let x = input[k];
            let mirror = input[half - k].conj();
            let even = (x + mirror).scale(0.5);
            let odd = (x - mirror).scale(0.5) * self.split[k].conj();
            // Z[k] = E[k] + i·O[k]
            self.scratch[k] = even + Complex::new(-odd.im, odd.re);
        }
        // z = inverse half-length FFT of Z, as conj(forward(conj(Z))) / half; its real and
        // imaginary parts are the even and odd samples.
        for z in self.scratch.iter_mut() {
            *z = z.conj();
        }
        self.half.transform(&mut self.scratch);
        let scale = 1.0 / half as f32;
        for (pair, z) in output
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(self.scratch.iter())
        {
            let z = z.conj().scale(scale);
            pair[0] = z.re;
            pair[1] = z.im;
        }
        Ok(())
    }
}

/// The periodic Hann window of `size` points, `0.5 − 0.5·cos(2πn/size)`, as used for
/// overlapping analysis frames. Allocates.
pub fn hann(size: usize) -> Box<[f32]> {
    (0..size)
        .map(|n| (0.5 - 0.5 * (2.0 * PI * n as f64 / size as f64).cos()) as f32)
        .collect()
}

/// One frame of a short-time Fourier transform: Hann-windows a frame and returns its magnitude
/// spectrum. The caller chooses the hop between frames.
#[derive(Debug, Clone)]
pub struct Stft {
    fft: RealFft,
    window: Box<[f32]>,
    windowed: Box<[f32]>,
    spectrum: Box<[Complex]>,
    /// Scales magnitudes so a full-scale sine at a bin centre reads 1.0.
    norm: f32,
}

impl Stft {
    /// Frames of `size` points: a power of two from [`MIN_SIZE`] to [`MAX_SIZE`]. Allocates.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidFftSize`].
    pub fn new(size: usize) -> Result<Self, Error> {
        let fft = RealFft::new(size)?;
        let window = hann(size);
        let sum: f32 = window.iter().sum();
        Ok(Self {
            spectrum: vec![Complex::default(); fft.bins()].into_boxed_slice(),
            windowed: vec![0.0; size].into_boxed_slice(),
            fft,
            window,
            norm: 2.0 / sum,
        })
    }

    /// The frame size.
    pub fn size(&self) -> usize {
        self.fft.size()
    }

    /// The number of magnitude bins: `size / 2 + 1`.
    pub fn bins(&self) -> usize {
        self.fft.bins()
    }

    /// Writes the magnitudes of the Hann-windowed `frame` (`size` samples) into `magnitudes`
    /// (`size / 2 + 1` bins), scaled so a sine of amplitude `A` centred on a bin reads `A`.
    ///
    /// # Errors
    ///
    /// [`Error::LengthMismatch`] for buffers of the wrong length.
    pub fn magnitudes(&mut self, frame: &[f32], magnitudes: &mut [f32]) -> Result<(), Error> {
        check_len(self.size(), frame.len())?;
        check_len(self.bins(), magnitudes.len())?;
        for ((w, &x), &h) in self.windowed.iter_mut().zip(frame).zip(self.window.iter()) {
            *w = x * h;
        }
        self.fft.forward(&self.windowed, &mut self.spectrum)?;
        for (m, z) in magnitudes.iter_mut().zip(self.spectrum.iter()) {
            *m = z.abs() * self.norm;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_rejects_bad_sizes() {
        for size in [0, 1, 32, 63, 100, 32_768] {
            assert!(Fft::new(size).is_err(), "{size}");
            assert!(RealFft::new(size).is_err(), "{size}");
        }
        assert!(Fft::new(64).is_ok() && Fft::new(16_384).is_ok());
    }

    #[test]
    fn test_hann_is_periodic_and_symmetric() {
        let w = hann(8);
        assert_eq!(w[0], 0.0);
        assert!((w[4] - 1.0).abs() < 1e-7);
        for n in 1..8 {
            assert!((w[n] - w[8 - n]).abs() < 1e-7);
        }
    }
}
