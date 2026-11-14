# ADR-0130 — Export asks for the format and its settings before the save dialog, and remembers them

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-14
- **Issue:** #8
- **Architecture:** [Files and devices](../architecture/user-interface.md#files-and-devices)

## Context

Export first opened the save dialog, and the file name's extension picked the format, with fixed
settings (24-bit WAV and FLAC, 256 kbit/s AAC); "Export mono…" was a second menu item, and MP3
([ADR-0129](0129-mp3-export-runs-a-host-installed-lame-as-a.md)) asked for its bit rate in a modal after the save dialog. The developer asked
that Export show the available formats and the chosen format's settings *before* asking where to
save, and chose to fold mono into it and to remember the choices between launches, which changes
the application settings' persisted format.

## Decision

- The ≡ menu has one **Export…** item. It opens an in-app modal with a row each for the format
  (WAV, FLAC, AAC, and MP3 only when `lame` was found), that format's setting (WAV: 16-bit, 24-bit
  or 32-bit float; FLAC: 16- or 24-bit; AAC: 128, 192 or 256 kbit/s; MP3: 128, 256 or 320 kbit/s)
  and the channels (stereo or mono), then Cancel and Export….
- Export… closes it and opens a save dialog offering only that format, named `Mix.<ext>`; the
  export uses the dialog's settings, not the extension.
- Each format keeps its own setting while the format changes. The settings of the last successful
  export are saved in the application settings, as format version 3, and the dialog opens on them;
  within a run it opens on the last confirmed choice.
- The command line keeps picking the format by extension, with `--mono` and `--bitrate`.

## Alternatives considered

- **Format by extension in the save dialog (as before)** — rejected by the developer: the settings
  are hidden or fixed, and are asked for, if at all, after the path.
- **Keep "Export mono…" as its own item** — rejected: channels are one more setting, best chosen
  with the rest.
- **Remember for the session only, or not at all** — rejected by the developer in favour of
  remembering between launches.
- **Settings in the save dialog's accessory view** — rejected: `rfd` offers no accessory view, and
  it would need platform code outside the platform crate.

## Consequences

- `settings.json` moves to format version 3, with an `export` object; versions 1 and 2 still load
  with the default export (24-bit stereo WAV). An older build reading a version 3 file falls back
  to all its defaults, losing the remembered device and click too.
- A remembered MP3 choice opens as WAV when `lame` is not found, keeping MP3's bit rate for later.
- AAC now offers 128 and 192 kbit/s besides 256.
