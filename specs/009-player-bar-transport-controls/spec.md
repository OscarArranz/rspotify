# Spotify-style transport controls

## Goal

Match the Spotify desktop player bar's transport buttons: shuffle, previous, play/pause, next, and
repeat, replacing the current 10-second rewind/forward buttons.

## User-visible scope

- The center transport row shows shuffle, previous, play/pause, next, and repeat in that order.
- Shuffle toggles the local Connect device's shuffle mode and is highlighted in Spotify green while
  active.
- Repeat cycles through off, repeat context, and repeat track. The icon is highlighted in green when
  active and switches to the repeat-one artwork in track mode.
- Previous, play/pause, and next keep their current behavior.
- Playback still requires Spotify Premium.

## Acceptance criteria

1. The 10-second rewind and forward buttons are removed; shuffle and repeat replace them.
2. Clicking shuffle toggles shuffle and the button reflects the playback snapshot (green when on).
3. Clicking repeat advances off → repeat context → repeat track → off, with distinct active
   artwork/color for context and track modes.
4. Shuffle and repeat state changes survive the 250 ms playback snapshot poll without flickering
   back to the previous value.
5. Previous, play/pause, and next behave as before.
6. No new dependencies or blocking work on the GPUI foreground thread.
7. `cargo fmt --check`, `cargo check`, and `cargo test` pass.
8. Visual and real-account checks are recorded only when explicitly approved.
