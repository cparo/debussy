# Startup

## Application startup

With a project on its command line, `debussy` opens it, creating it if missing, imports any
`--import` files as new tracks, builds missing waveform summaries, and opens the window, playing
through the chosen output device, which must run at the project rate. With `--export` it renders
the project to a file and exits instead. Command-line import and export stand in until the
interface offers them ([ADR-0057](../adrs/0057-the-first-binary-imports-and-exports-from-the.md)).

With no arguments, the window opens on a start screen: a sample rate picker (44.1, 48, 88.2 or
96 kHz, 48 kHz by default) and New project, with the Open browser's list beneath. With a project
open, the ≡ menu's New project (⌘N) shows the same panel over the window, with Cancel, and Open
(⌘O) shows the Open browser over it; both need the transport stopped, no track armed and no take
still being saved. New project asks where to save through a save dialog, opened in the
[projects folder](project.md#projects-folder) with the first free `Untitled` name, adds the
`.debussy` extension if missing, and refuses a path that exists.

The Open browser lists the projects folder's projects by name and age ("just now", "5 min ago",
"3 h ago", "12 days ago"), or says the folder has none, and offers Browse…, which asks for a
bundle anywhere through a folder dialog; as a modal it also has Cancel, and Escape or a press
outside it cancels. Opening never creates a project
([ADR-0098](../adrs/0098-projects-live-in-music-debussy-and-open-lists-them.md)).

Switching saves and closes the open project, then replaces the whole `App`, with its services and
audio output; the tone and scale factor carry over. Opening goes through `debussy_ui`'s `Launcher`
trait, which `debussy` implements, in two steps: everything that can fail happens first, while the
open project still runs, and a failure shows in the status line and leaves it open; only then is
the old `App` dropped and the new one's audio output started
([ADR-0097](../adrs/0097-debussy-starts-without-a-project-and-switches.md)).
