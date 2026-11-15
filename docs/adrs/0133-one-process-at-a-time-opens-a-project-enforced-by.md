# ADR-0133 — One process at a time opens a project, enforced by an advisory lock on a lock file inside the bundle

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-14
- **Issue:** #161
- **Architecture:** [Project bundle](../architecture/project.md#project-bundle)

## Context

Two processes could open the same bundle. Each one saves its own state over `project.json` after
every edit, and each one's open and close delete audio its own session does not reference, so one
process could wipe out the other's edits and freshly recorded takes. Since
[ADR-0132](0132-finder-open-document-events-arrive-through-an.md) this is easy to hit: a Finder
double-click beside a `debussy` started from the command line starts a second process.

## Decision

- A bundle holds an empty file named `lock`. `Bundle::open` and `Bundle::create` open it (creating
  it if missing) and take an exclusive advisory lock on it without waiting, using std's
  `File::try_lock` (`flock` on macOS); the `Bundle` holds the lock until it is dropped.
- A bundle already locked is refused. The window's status line says "Cannot open <name> because
  it is open in another process.", beside the New project and Open choices; the command line
  exits with "<path> is open in another Debussy process".
- The Open browser leaves out projects that are open, in this process or another, judged by
  trying a shared lock on each `lock` file and releasing it at once.
- A lock that cannot be taken for any other reason (a file system that does not support locking,
  a read-only bundle) also refuses the open.
- Command-line runs with `--import` or `--export` take the same exclusive lock.
- Asking for the project already open in this process, from any open path (the Open browser,
  Browse…, the Finder), does nothing but close the browser.
- The persisted format gains `lock` without a `format_version` bump: it holds no data, and older
  builds ignore it.

## Alternatives considered

- **Create a lock directory (`mkdir` is atomic)** — a crash leaves it behind, so a stale lock
  would need detection and a way for the user to break it.
- **A PID file** — the same staleness problem, plus PID reuse makes "is the holder alive?"
  unreliable; checking needs platform-specific process queries.
- **A lock-file crate (`fs2`, `fd-lock`)** — std has offered `File::try_lock` since 1.89, under
  the 1.90 MSRV, so a crate would add a dependency for nothing.
- **A shared, read-only open for `--export`** — every open deletes leftover `.tmp` files and
  cleans up unreferenced audio, so even an export run writes to the bundle; a read-only open path
  would add a second way to load for little gain.
- **Open anyway when locking is unsupported** — this would silently drop the protection exactly
  where it cannot be checked; refusing is the safe default, and a project on such a file system
  can be copied to a local disk.
- **List open projects and refuse only when one is chosen** — offering a choice that is bound to
  fail is worse than leaving it out.
- **Bump `format_version`** — the file carries no data and older builds read the bundle unchanged,
  so a bump would only stop older builds opening new projects.

## Consequences

- No stale locks: the OS releases a `flock` when the process exits or crashes.
- The lock is advisory and per-host. Some network file systems do not propagate `flock` between
  machines, so two machines can still open one project on a share; on those that reject locking,
  the project cannot be opened at all.
- Older builds neither take nor respect the lock, so an older build beside a newer one is not
  protected.
- `lock` travels with the bundle when it is copied, AirDropped or mailed, harmlessly.
- Listing briefly holds a shared lock on each project's `lock` file; a process opening that
  project in the same instant is refused and can simply try again. A listed project can still be
  opened elsewhere before it is chosen, and is then refused when chosen.
- Reopening the open project in place (to discard edits, say) is not possible; it does nothing.
