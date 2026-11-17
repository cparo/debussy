# The sampler

The **Sampler** plays your own audio files when it receives notes. Use it to play a recorded
instrument from a keyboard, or to play a drum kit from a sample pack. Debussy doesn't come with
any samples, so you bring your own WAV, FLAC, AAC, M4A or MP3 files.

## Setting it up

1. Add a MIDI track, and choose **Sampler** as its instrument. You can also add one from
   **Instruments** in the **+** menu and wire the MIDI track's output to it.
2. Click the sampler on the patch board to open it. Its **Zones** list is empty at first.
3. With playback stopped, add samples:
   - **Add sample…** adds one file, for an instrument you play across keys.
   - **Add kit…** adds a whole folder, for drums.

Each file becomes a **zone**: one sample, the keys that play it, and how it plays. Each zone is a
row in the list. Every change you make to a row can be undone.

A sample can be at most 60 seconds long. A sampler holds at most 128 zones. If anything can't be
added, a message lists what was left out and why.

## A melodic instrument: Add sample…

**Add sample…** puts the file on one key, C3 (one octave below middle C, which is C4). It plays at
its own pitch there and stops when you let go. To play it across the keyboard:

- **Widen its range.** Use the first two key lists ("C3 to C3"), for example C2 to C5. Keys above
  the root play the sample faster and higher, and keys below play it slower and lower, like a tape
  sped up or slowed down. It also gets shorter or longer as it goes.
- **Set the root** to the note the recording actually plays. If you recorded an A, set the root
  to that A, so every key sounds in tune.
- **Use several zones for a wide range.** A sample pitched far from its root sounds unnatural:
  high notes thin out, and low notes get dull and slow. For a piano or a voice, record a few
  notes, add each as its own sample, and give each a range around its root, such as C2 to F2
  rooted on D2, then F#2 to B2 rooted on A2, and so on.

## A drum kit: Add kit…

**Add kit…** asks for a folder and adds every audio file in it and its subfolders. Pick the
folder of one kit, not a whole library. It places each file on the key that drum parts and pads
expect (the General MIDI drum map), by the words in the file and folder names:

| Name contains | Key |
|---|---|
| kick, BD, bass drum | 36 (C2) |
| rim, side stick | 37 |
| snare, SD | 38 (D2) |
| clap | 39 |
| hat, HH, closed hat | 42 (F#2) |
| pedal hat | 44 |
| open hat, OHH | 46 (A#2) |
| floor, low, mid or high tom (unnamed toms in order) | 41, 45, 47, 50 |
| crash (a second crash) | 49 (57) |
| ride, ride bell | 51, 53 |
| china, splash, cowbell, tambourine, shaker, claves, woodblock… | their General MIDI keys |

A pack laid out in folders such as `Kicks/`, `Snares/` and `Hats/` works too, because folder names
count. A second file for a drum that already has one, or a file whose name doesn't match, goes on
the keys from 82 (A#5) up, above the drum map, so nothing is lost. Move it from there by hand.
Kit samples play to the end even if you let go (**One-shot**). When the kit has more than one
hi-hat, they share choke group 1.

In a one-shot row the keys are named as drums (kick, snare, closed-hat…). If you have named the
pitches yourself in the MIDI track's **Key…** dialog, with its key set to Indefinite, your names
are used instead.

## The choices in each row

**Low and high key.** These set which keys play the zone. Use one key for a drum. Use a range to
play a sample melodically, such as a melodic instrument, an 808 bass, or tuned toms or congas
spread over a few keys. If two zones share a key, the one higher in the list plays.

**Root.** This is the key at which the sample plays at its own pitch. Each key above or below it
shifts the pitch a semitone. Set it to the note the recording plays. For a single-key drum, leave
it on that key.

**Tune.** This shifts this zone up or down, by up to 24 semitones, without moving its keys. Use it
to:

- fit a drum to the song's key: tune a snare or kick up or down until it sits with the bass;
- make a set from one sample: add the same tom three times, on three keys, and tune them 0, +3
  and +7 to get low, mid and high toms;
- fix a sample that is recorded a little off its name;
- deepen or brighten a sound for effect, such as a snare tuned down five semitones for a fatter,
  slower crack.

The sampler's own **Tune** setting, below the zones, shifts every zone at once.

**One-shot or Gated.** This button switches between the two:

- **One-shot** plays the whole sample however briefly you hold the key, which is what drums,
  hits and effects want. A drum pad's note is often only a few milliseconds long.
- **Gated** stops the sample when you let go, fading out over the **Amp release** time. Use it for
  sustained instruments, pads and anything you want to stop yourself. The sustain pedal holds
  gated notes.

**Choke.** A note in a choke group cuts off every other sound in the same group, fading it in
3 ms. Use it for:

- **hi-hats:** put the closed, pedal and open hats in one group, so a closed hat stops a ringing
  open one, as on a real kit (a kit does this for you);
- **a mono 808 bass:** give the 808 zone a group of its own, so each new note cuts off the last
  instead of piling up into mud;
- **cymbal chokes:** put a crash and a short "choke" sample in one group;
- **cutting off long effects:** for example, a reversed cymbal stopped by the snare it leads into.

Zones with no choke ring on over each other, which is right for most drums and for chords.

**Remove** takes the zone out. Its file stays in the project until it is no longer used anywhere,
including in saved versions.

## The sampler's settings

Below the zones are settings that apply to every zone:

- **Amp envelope** shapes each note's loudness: **Attack** (how fast it starts), **Decay** and
  **Sustain** (the level it falls to while held, for gated zones) and **Release** (the fade after
  you let go). Keep the attack short for drums, or their hits go soft. A longer release lets
  gated notes ring out.
- **Amp velocity** sets how much harder playing is louder. At 0 every hit plays at full level; at
  1 a soft hit is nearly silent. Lower it for programmed drums that should hit evenly.
- **Tune** shifts every zone, up to an octave either way.
- **Bend range** sets how far the pitch wheel bends, in semitones.
- **Voices** sets how many sounds can play at once (up to 16). When they run out, a new note takes
  over the quietest or oldest one.
- **Gain** sets the overall level.
