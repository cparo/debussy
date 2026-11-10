# ADR-0095 — Rasters reach the GPU in strips under the renderer's synchronous upload limit, and the effect display redraws only when its inputs change

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-10
- **Issue:** —
- **Architecture:** [Effect displays](../architecture/effect-displays.md#effect-displays)

## Context

The EQ and filter display in a node dialog flickered while a parameter was dragged and was mostly
blank during playback. Two findings in the iced 0.14 sources explained it:

- `display_area` rasterised the response and wrapped it in a new `image::Handle::from_rgba` on
  every `view()`. Each handle has a fresh id, so the renderer saw a never-seen image every time.
- `iced_wgpu` 0.14 (`src/image/cache.rs`, `upload_raster`) uploads an image in the frame it is
  first drawn only if it is under `MAX_SYNC_SIZE`, 2 MiB. A larger one goes to a background
  worker, and nothing is drawn in its place until the upload finishes. The display is the dialog's
  width by 320 logical pixels; at a scale of 2, about 1800 × 640 × 4 ≈ 4.6 MB, so it always took
  the background path.

During playback every display frame rebuilds the view, so the display was a new, not yet uploaded
image almost every frame. Each drag step did the same, giving a blank frame per change. A wide lane
can also exceed 2 MiB, so it blanked for a frame after each page turn. And while the view pages
along with the playhead, every page recomputed all lanes (file reads, spectrogram FFTs, a
re-raster and blur) on the interface thread, though a node dialog covered them all.

## Decision

- A raster is drawn into a canvas as horizontal strips, each under the renderer's synchronous
  upload limit (`raster::draw_tiled`), so it is drawn in the frame it first appears. Lanes and
  effect displays both use it.
- The effect display is a canvas with a cache. It calls the registered `EffectDisplay` only when
  the effect's kind, parameter values, the tone, the sample rate or the display's physical size
  change; otherwise the same strips are drawn again and nothing is uploaded.
- While a node dialog is open the track pane holds empty space of the lanes' height in place of
  the lanes, and lane recomputation waits until the dialog closes. A change to the tracks or their
  channels still recomputes the lanes at once, since the patch board's ports follow them.

## Alternatives considered

- **Cache the `image::Handle` and keep the `image` widget** — stops the repeat upload during
  playback, but each change while dragging is still one image over 2 MiB, so it still blanks a
  frame per step.
- **Render the display at logical rather than physical size** — keeps it under 2 MiB on most
  windows, but blurs it on high-density displays and fails again on wide windows.
- **Patch or fork iced to raise the limit** — the limit guards the frame time against huge
  uploads; strips keep each upload bounded without owning a fork.
- **Keep refreshing the hidden lanes** — correct, but spends interface-thread time on file reads
  and FFTs nobody can see, during playback when it matters most.

## Consequences

- An image is now several draws instead of one; the strips meet on whole physical rows with
  nearest-neighbour filtering, so no seam shows.
- The limit is a private constant of iced; an iced upgrade must re-check it (cited in
  `crates/debussy_ui/src/raster.rs`).
- An `EffectDisplay` must depend only on what it is given: anything else it reads will not redraw
  it.
- Closing a node dialog after paging recomputes the lanes once, in that update.
