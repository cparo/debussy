# Routing and effects

The board on the right shows how tracks reach the output. Click **+** at its top right to add a
mixer, bus, feedback delay, effect or instrument, drag from an output to an input to connect, and
click a node to open its controls. Changes are heard while you drag; letting go makes one undo step.
Effect and instrument settings are saved with the project.

## High-pass and low-pass filters

**High-pass filter** removes what is below its cutoff, such as rumble; **Low-pass filter** removes
what is above it, such as hiss.

| Control | Range | Default | What it does |
|---|---|---|---|
| **Cutoff** | 20 Hz – 20 kHz | 80 Hz (high-pass), 12 kHz (low-pass) | Where the filter starts cutting. |
| **Order*** | 1 – 8 | 2 | How steeply: each step adds 6 dB per octave, so 8 is 48 dB per octave. |

## Parametric EQ

Shapes the tone with four bands and an optional filter at each end.

- **Band 1** to **Band 4** — each is **Disabled** until you click it to **Enabled**.

  | Control | Range | Default | What it does |
  |---|---|---|---|
  | **Type** | bell, low shelf, high shelf, notch | bell | **Bell** boosts or cuts around the frequency, the shelves everything below or above it, and **notch** removes a narrow range. |
  | **Frequency** | 20 Hz – 20 kHz | 100 Hz, 500 Hz, 2 kHz, 8 kHz | Where the band works. |
  | **Gain** | −24 – +24 dB | 0 dB | The boost or cut. |
  | **Q** | 0.1 – 18 | 0.71 | The width: higher is narrower. |

- **High-pass**, before the bands, and **Low-pass**, after them — each **Disabled** until you turn
  it on, then a **Cutoff** and **Order*** like the
  [high-pass and low-pass filters](#high-pass-and-low-pass-filters).

## Dynamics

A compressor, expander, gate or limiter. Pick one from **Preset** (Compressor, Expander, Gate,
Limiter) to set everything at once (one undo step), then adjust; **Preset** shows **Custom** once
you change a setting.

| Control | Range | Default | What it does |
|---|---|---|---|
| **Threshold** | −80 – 0 dB | −18 dB | The level the curve bends at. |
| **Ratio above** | 1 – ∞ | 4 | Turns the level down above the threshold; ∞ holds it there. |
| **Ratio below** | 1 – ∞ | 1 | Turns the level further down below the threshold… |
| **Range** | 0 – 96 dB | 96 dB | …by no more than this. |
| **Knee** | 0 – 24 dB | 6 dB | Softens the bend. |
| **Ceiling** | on / off; −24 – 0 dB | off; −1 dB | Keeps the output under this level. It is a soft limit and can pass it by a fraction of a dB. **Enabled** turns it on. |
| **Knee** (under Ceiling) | 0 – 12 dB | 0 dB | Softens the ceiling. |
| **Attack** | 0.01 – 200 ms | 10 ms | How fast it reacts to a rise. |
| **Release** | 1 – 3000 ms | 150 ms | How fast it lets go. |
| **Hold** | 0 – 500 ms | 0 ms | How long it waits before letting go. |
| **Detector** | peak, RMS | peak | Follows peaks or the average level. |
| **Lookahead*** | 0 – 10 ms | 0 ms | Reacts before a peak arrives, delaying the sound by as much. |
| **Makeup** | −24 – +24 dB | 0 dB | Adds gain back. |

## Amplifier

Adds distortion: **Drive** pushes the sound into the **Shape**.

| Control | Range | Default | What it does |
|---|---|---|---|
| **Drive** | −12 – +36 dB | 0 dB | How hard the sound hits the shape. |
| **Bias** | −1 – 1 | 0 | Makes the distortion lopsided, for a richer, tube-like tone. |
| **Shape** | warm, crunched, harsh | warm | **Warm** rounds the peaks off gently, **crunched** a little harder, **harsh** clips them flat. |
| **Makeup** | −36 – +12 dB | 0 dB | The output level. |
| **Oversampling*** | off, 2×, 4× | off | Reduces harsh, out-of-tune overtones at high drive, at the cost of more CPU and a short delay. |

The picture shows how input becomes output, with red lines at full scale.

## Delay

Repeats the sound. Presets: Slapback, Ping-pong and Dub echo.

- **Time** — the gap between repeats, set one of two ways by **Sync**; only the controls for your
  choice are shown:
  - **Sync** off: **Time**, 1 – 4000 ms (default 375 ms).
  - **Sync** on: **Note**, 1/32 – 1/1 (default 1/8), and **Feel**, straight, dotted or triplet
    (default straight), following the project tempo.
- **Stereo**
  - **Mode** — **stereo** (default), or **ping-pong** to bounce the repeats between left and
    right.
  - **Offset**, ±50 ms (default 0 ms) — delays the right side a little more or less than the left.
- **Feedback**, 0 – 100 % (default 35 %) — how much of each repeat comes back again.
  - **Low-pass enabled** (default off) turns on the **Low-pass**, 200 Hz – 20 kHz (default
    6 kHz), which makes each repeat darker than the last.
  - **Saturation** (default off) lets loud repeats distort gently instead of growing.
- **Mix**, 0 – 100 % (default 30 %) — how loud the repeats are against the original.

The picture shows what you would hear after a single click: its level over time, left channel on
top and right below, from full level down to −96 dB. The spike at the start is the original
sound and the repeats are spikes after it. The time across fits the settings.

## Reverb

Adds a sense of space.

| Control | Range | Default | What it does |
|---|---|---|---|
| **Algorithm*** | room, plate | room | **Room** is dense and natural, **plate** smooth and bright. A change starts the new one silent. |
| **Pre-delay** | 0 – 200 ms | 10 ms | Holds the reverb back a moment after the sound. |
| **Decay** | 0.2 – 20 s | 2 s | How long the reverb takes to die away. |
| **Damping** | 1 – 20 kHz | 8 kHz | Makes high frequencies die sooner: lower it for a darker, softer space. |
| **Mix** | 0 – 100 % | 25 % | How loud the reverb is against the original. |

The picture works like the [delay's](#delay); the reverb's tail is a falling slope after the
spike.

## Chorus

Thickens the sound with copies of it whose pitch wavers slowly. Presets: Subtle and Wide.

| Control | Range | Default | What it does |
|---|---|---|---|
| **Sync** | on / off | off | Times the wavering to the project tempo: shows **Note** and **Feel** in place of **Rate**. |
| **Rate** | 0.05 – 5 Hz | 0.8 Hz | How fast the copies waver. |
| **Note** | 1/16 – 4/1 | 1/1 | One waver per this note length, with Sync on. |
| **Feel** | straight, dotted, triplet | straight | Lengthens or shortens the note. |
| **Shape** | sine, triangle | sine | How the wavering moves: **sine** smoothly, **triangle** evenly. |
| **Delay** | 5 – 30 ms | 15 ms | How far behind the copies sit. |
| **Depth** | 0 – 10 ms | 3 ms | How far they swing. |
| **Voices** | 1 – 4 | 2 | How many copies. |
| **Spread** | 0 – 180° | 90° | Moves the left and right copies apart, from none (mono), for a wider sound. |
| **Mix** | 0 – 100 % | 50 % | How loud the copies are against the original. |

## Flanger

A sweeping, jet-like whoosh. Presets: Jet and Metallic. **Sync**, **Rate**, **Note**, **Feel**,
**Shape** and **Spread** work as in the [chorus](#chorus), with these defaults: Rate 0.25 Hz,
Note 2/1, Shape triangle.

| Control | Range | Default | What it does |
|---|---|---|---|
| **Delay** | 0.1 – 10 ms | 1 ms | A very short time behind the original. |
| **Depth** | 0 – 100 % | 70 % | How far the sweep swings around the delay. |
| **Feedback** | −95 – +95 % | 50 % | Makes the sweep ring: positive values sound hollow and bright, negative ones more metallic. |
| **Mix** | 0 – 100 % | 50 % | How loud the swept copy is against the original. |

## Synthesizer

Under **Instruments** in the **+** menu; turns notes into sound. Wire a MIDI track's output to its
input (the only input a MIDI track can reach) and its output to a mixer or the outputs. Each voice
mixes two oscillators, then a filter, shaped by two envelopes and an LFO. The sustain pedal holds
notes.

- **Oscillator 1** and **Oscillator 2** — the same controls, labelled **Osc 1** or **Osc 2**:

  | Control | Range | Default | What it does |
  |---|---|---|---|
  | **Wave** | saw, pulse, triangle | saw | The tone. |
  | **Octave** | −3 – +3 | 0 | Shifts by whole octaves. |
  | **Detune** | ±100 cents | 0 (osc 1), 7 cents (osc 2) | Fine pitch; a little thickens the sound. |
  | **Pulse width** | 5 – 95 % | 50 % | The pulse wave's shape; only heard with **pulse**. |
  | **Level** | 0 – 1 | 1 (osc 1), 0.5 (osc 2) | How loud it is in the mix. |

- **Filter** — removes what is above the cutoff.

  | Control | Range | Default | What it does |
  |---|---|---|---|
  | **Cutoff** | 20 Hz – 20 kHz | 2 kHz | Where it starts cutting. |
  | **Resonance** | 0.5 – 12 | 0.71 | Makes it ring at the cutoff. |
  | **Envelope amount** | ±5 oct | 2 oct | How far the filter envelope moves the cutoff when a note starts. |
  | **Velocity amount** | 0 – 4 oct | 0 oct | How much brighter harder playing is. |

- **Filter envelope** and **Amp envelope** — the first moves the cutoff, the second shapes each
  note's loudness. Labels start **Filter** or **Amp**:

  | Control | Range | Default (filter; amp) | What it does |
  |---|---|---|---|
  | **Attack** | 0.001 – 10 s | 0.005 s; 0.005 s | The rise when a note starts. |
  | **Decay** | 0.001 – 10 s | 0.3 s; 0.2 s | The fall to the sustain level. |
  | **Sustain** | 0 – 1 | 0.3; 0.8 | The level while the key is held. |
  | **Release** | 0.001 – 10 s | 0.3 s; 0.25 s | The fade after the key is let go. |
  | **Amp velocity** | 0 – 1 | 0.7 (amp only) | How much louder harder playing is. |

- **LFO** — wobbles the sound.

  | Control | Range | Default | What it does |
  |---|---|---|---|
  | **Rate** | 0.01 – 20 Hz | 5 Hz | How fast. |
  | **Shape** | sine, triangle, saw up, saw down, square, sample and hold | sine | How it moves. |
  | **Destination** | pitch, cutoff, amplitude | pitch | What it wobbles. |
  | **Depth** | 0 – 1 | 0 | How far; at 0 it does nothing. |

- **Bend range**, 0 – 24 st (default 2) — how far the pitch wheel bends, in semitones.
- **Voices**, 1 – 16 (default 8) — how many notes sound at once; a note beyond that takes over the
  quietest or oldest one.
- **Gain**, −48 – +12 dB (default −12 dB) — the output level.

## Sampler

Also under **Instruments**; plays your own audio files from notes: a recorded instrument across the
keyboard, or a drum kit from a folder of samples. See [The sampler](sampler.md) for how to set it
up.

## MIDI Out

Plays a MIDI track on a hardware synth (or another app, through the IAC bus).

- **Adding it** — choose **MIDI Out: <port>** from the track's ≡ menu, or add **MIDI Out** from the
  **+** menu and wire the track to it.
- **Port** — click the node to pick another port. If the port is unplugged its name turns grey and
  notes are dropped until it is back.
- **Latency (ms)** — how long your synth takes to sound a note. Notes are sent that much early, so
  the synth's audio, recorded back on an audio track, lines up with the notes. Remembered on this
  computer for that port, not in the project.

Stop and pause silence the synth. Export doesn't include hardware synths: record them back to audio
first.

---

\* Can only be changed while stopped or paused.
