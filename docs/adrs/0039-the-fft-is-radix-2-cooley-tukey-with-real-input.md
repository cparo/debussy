# ADR-0039 — The FFT is radix-2 Cooley-Tukey with real-input packing and a Hann-windowed STFT

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** #31
- **Architecture:** [Patent policy](../architecture/patents.md#patent-policy)

## Context

The `dsp` crate needs an FFT (#31) for spectrogram data (`audio_analysis`) and future effects.
`docs/architecture/patents.md` "Patent policy" requires recorded evidence that no unexpired,
non-royalty-free patent covers an in-house algorithm. The plan was researched against primary
sources ([research note](../research/fft-patents.md)) and the open item put to the developer.

## Decision

The FFT is an iterative radix-2 decimation-in-time Cooley–Tukey transform on `f32`, for
power-of-two sizes from 64 to 16384, with precomputed twiddle and bit-reversal tables. Real input
is transformed as a half-length complex FFT plus a split step. Spectrograms use a Hann window and
overlapping frames (a short-time Fourier transform). The evidence:

- **Cooley–Tukey:** Cooley and Tukey, *Mathematics of Computation* 19 (1965). No covering patent
  was found; the FFT patents in force claim hardware, GPUs, multi-bank memories or special
  instructions.
- **Real-input packing:** real-input FFTs date from 1968 (Bergland; Edson, US 3,584,781, issued
  1971 and long expired). The in-force patents found claim GPU execution (Microsoft US 9,342,486)
  or hardware apparatus (Astel KR 10-2376492). Broadcom US 7,676,533 (estimated expiry
  2027-05-14) claims, in every independent claim, a processor executing a dedicated SIMD "FFT
  conversion instruction" selected by opcode or control bit. A portable software implementation
  executes no such instruction, so the developer judged it not to apply.
- **Hann window and STFT:** Blackman and Tukey (1958); Allen (1977); Allen and Rabiner (1977).
  No covering patent was found.
- **Tables:** no claim was found on a plain twiddle table or table-driven bit reversal; those found
  require hardware (Nvidia US 7,640,284) or mixed-radix twiddle units (Freescale US 8,386,552).

## Alternatives considered

- **A full complex FFT on real input until 2027** — would sidestep Broadcom '533 at twice the
  work.
- **Waiting on the patent's fee status** — would delay the FFT for an item judged not to apply.
- **A crate such as `rustfft`** — real-time core crates depend on no third-party crate other than
  `thiserror` (`docs/architecture/crates.md` "Dependency rules").

## Consequences

- If Broadcom '533 is ever read as covering software, the real-input path can fall back to a full
  complex transform until it expires in 2027.
- Split-radix is not used; the patents in force on it claim hardware datapaths.
- The search was not a professional freedom-to-operate search; the research note lists its gaps.
