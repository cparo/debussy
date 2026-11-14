# ADR-0132 — Finder open-document events arrive through an Apple Event handler installed at will-finish-launching

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-14
- **Issue:** #128
- **Architecture:** [Documents opened from the Finder](../architecture/platform.md#documents-opened-from-the-finder)

## Context

With the project type claimed by `Debussy.app` ([ADR-0131](0131-debussy-ships-as-a-debussy-app-bundle-that.md)),
double-clicking a project launches Debussy, or activates it, and macOS delivers the file through
the `kAEOpenDocuments` (`odoc`) Apple Event, not on the command line. AppKit's default handler
forwards it to the application delegate's `application:openURLs:`. winit 0.30 installs its own
`NSApplicationDelegate` in `EventLoop::new`, without that method, and iced creates the event loop
inside `iced::application(..).run()` with no hook before it, so the delegate route is closed. The
plan raised the new dependencies, the new untrusted input and the new `unsafe` with the
`flag-risk` skill; the developer approved the decision below.

## Decision

Before starting the interface, `debussy` calls `macos_platform::documents::on_open_documents`
with a callback taking a `PathBuf`. It adds an observer for
`NSApplicationWillFinishLaunchingNotification`; when that fires, on the main thread and before
AppKit sends the launch's `odoc` event, the observer installs an `NSAppleEventManager` handler for
`kCoreEventClass`/`kAEOpenDocuments`, replacing AppKit's. The handler reads the event's
`keyDirectObject` list of file URLs and calls the callback with each path. `debussy_ui` takes the
paths through a channel and treats each as a request to switch projects.

- **Dependencies:** `objc2` 0.6, `objc2-foundation` 0.3 and `block2` 0.6, direct dependencies of
  `macos_platform` only. All three were already in `Cargo.lock` through winit, at the same
  versions, and are MIT licensed.
- **`unsafe`:** confined to `macos_platform::sys::open_documents`
  ([ADR-0051](0051-audiotoolbox-codecs-use-objc2-bindings-unsafe.md)), main thread only, enforced
  with `MainThreadMarker`; the handler object and observer are leaked, living for the process.
- **Security:** a path from an event is untrusted. It goes through the same `Launcher` open as
  Browse… and the command line, which validates the bundle and never creates one; a refusal shows
  in the status line. A switch is refused while recording is armed or the transport runs, as for
  Open.

## Alternatives considered

- **Implement `application:openURLs:` on winit's delegate** — impossible without replacing or
  swizzling winit's delegate class, which iced and winit do not expose.
- **Read the paths from `argv`** — the Finder does not pass documents as arguments (only a
  `-psn_` process serial number on older macOS), and a running app never sees later opens.
- **Install the handler directly before `run()`** — AppKit's `finishLaunching` installs its own
  `odoc` handler and would overwrite ours; the will-finish-launching notification is posted after
  that and before the launch event is dispatched.
- **Hand-written Objective-C runtime FFI** — avoids three direct dependencies but needs far more
  `unsafe`, for the same crates already compiled into the binary.
- **Declare the type without opening documents** — rejected: double-clicking a project would
  launch Debussy without opening it.

## Consequences

- The behaviour depends on AppKit's launch order (handlers installed in `finishLaunching`, then the
  notification, then the launch event), which is long-standing but undocumented as a contract.
- Opens that arrive before the window exists wait in the channel and apply once it starts.
- A multi-document `odoc` opens its projects one after another; the last one wins.
- `-psn_` arguments are ignored by the command-line parser.
