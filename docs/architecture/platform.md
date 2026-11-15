# Platform Integration

## Compressed audio codecs

MP3 decoding, and AAC and FLAC encoding and decoding, must go through Apple's AudioToolbox
framework, from inside the macOS platform crate. No crate implementing these codecs may be added.
MP3 is encoded by a host-installed `lame`, not AudioToolbox ([MP3 export](import-export.md#mp3-export))
([ADR-0024](../adrs/0024-compressed-codecs-go-through-audiotoolbox-and-mp3.md), [ADR-0129](../adrs/0129-mp3-export-runs-a-host-installed-lame-as-a.md)).

The codecs call AudioToolbox's `ExtAudioFile` through the `objc2-audio-toolbox` bindings, decoding
in the application process and never on the real-time thread. A decoded file is untrusted: one
whose format is not MP3, AAC or FLAC, or that reports more than 2 channels, a sample rate outside
8–384 kHz or more than 24 hours, is refused, and a decode that runs past 24 hours stops with an
error. Every AudioToolbox status is checked ([ADR-0051](../adrs/0051-audiotoolbox-codecs-use-objc2-bindings-unsafe.md)).

## Folders

The paths of the app's folders and files on macOS (the projects folder, the settings file, and the
folders searched for `lame`) are worked out in `macos_platform::folders`; no other crate names a
macOS folder. ([ADR-0098](../adrs/0098-projects-live-in-music-debussy-and-open-lists-them.md))

## Audio device I/O

Audio devices are opened, enumerated and streamed through `cpal`, used only inside the platform
crate and only behind Debussy's own audio I/O interface. No other crate may depend on `cpal` ([ADR-0009](../adrs/0009-audio-device-i-o-uses-cpal-behind-our-own.md)).

A duplex stream is a `cpal` input stream and output stream on the same device. The input callback
pushes samples into a lock-free ring of 4 buffers; the output callback takes one buffer's worth
(silence for any shortfall) and runs the audio callback with both. The input latency reported
includes the frames waiting in the ring ([ADR-0052](../adrs/0052-a-duplex-stream-is-two-cpal-streams-joined-by-a.md)).

## App bundle

Debussy ships as `Debussy.app`, assembled by `cargo xtask bundle` from the release `debussy`
binary and `crates/macos_platform/bundle/Info.plist`, with the licence notices in
`Contents/Resources` (see [Releases](#releases)), and signed ad hoc. Its bundle identifier is
`io.github.cparo.debussy`. The `Info.plist` exports the type `io.github.cparo.debussy.project`, the
[project bundle](project.md#project-bundle) with extension `debussy`, conforming to
`com.apple.package` and `public.composite-content`, and claims it as the type's owner and editor
([ADR-0131](../adrs/0131-debussy-ships-as-a-debussy-app-bundle-that.md)).

## Releases

A release is a `vX.Y.Z` git tag and a GitHub Release carrying the [app bundle](#app-bundle), zipped
as `Debussy-<version>-macos-arm64.zip`, with the third-party licence notices inside the bundle.
Release builds are signed ad hoc only: not with a Developer ID and not notarized
([ADR-0138](../adrs/0138-the-first-release-is-an-unsigned-github-build.md)).

- **Version:** the release version is `version` in the root `Cargo.toml`'s `[workspace.package]`;
  every crate sets `version.workspace = true`, and `cargo xtask check` reports a crate whose
  version differs.
- **Notices:** `Contents/Resources/THIRD-PARTY-NOTICES.txt` covers every crate reached from
  `debussy` along normal (not build or dev) dependency edges for `aarch64-apple-darwin`, sorted
  by name and version. Each entry is a header (name, version, licence expression) followed by the
  verbatim contents of the crate's files named `LICENSE*`, `LICENCE*`, `COPYING*`, `NOTICE*` or
  `COPYRIGHT*` and its `license-file`. A crate with none gets the standard text of a licence its
  expression offers: MIT with its authors as copyright holders, else Apache-2.0. A crate with
  neither fails the bundle unless `OVERRIDES` in `crates/xtask/src/notices.rs` gives the reason
  it needs no text. Debussy's `LICENSE-MIT` and `LICENSE-APACHE` sit beside it.
- **Zip:** `cargo xtask bundle --zip` writes `target/release/Debussy-<version>-macos-arm64.zip`
  with `ditto -c -k --sequesterRsrc --keepParent`, and fails on a host that is not Apple silicon.
- **Publishing:** `.github/workflows/release.yml` runs on a pushed `v*` tag. It fails unless the
  tag is `v` followed by the workspace version and `docs/releases/<tag>.md` exists, then runs the
  workspace tests, builds the zip, and creates the GitHub Release titled `Debussy <tag>` with the
  zip attached and that file as its notes. Only that job has `contents: write`
  ([ADR-0142](../adrs/0142-release-notices-are-generated-in-xtask-and.md)).

## Documents opened from the Finder

Projects opened from the Finder arrive as an `odoc` Apple Event, read in `macos_platform` by a
handler that `macos_platform::documents::on_open_documents` installs when the application posts
`NSApplicationWillFinishLaunchingNotification`; it must be called before the window's event loop
starts. Each file URL is passed on as a path, and no Apple type crosses the crate's interface.
The Finder sends the event to a running instance of `Debussy.app`; a `debussy` process started
from the binary outside the bundle (`cargo run`, `target/*/debussy`) is not one, so opening a
project from the Finder alongside it launches a separate `Debussy.app` process
([ADR-0132](../adrs/0132-finder-open-document-events-arrive-through-an.md)).
