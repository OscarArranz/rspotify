# Volume mute toggle

## Goal

Replace the player bar's volume increment/decrement buttons with the Spotify desktop-style mute
toggle beside the volume slider.

## Acceptance criteria

1. The `+`/`−` buttons and numeric volume percentage are removed; the volume slider remains.
2. A speaker icon appears for nonzero volume and a crossed speaker appears at zero volume; both use
   the Spotify desktop icon artwork and brighten on hover.
3. Clicking the speaker sets volume to zero. Clicking it again restores the most recent nonzero
   volume (or 50% if no prior nonzero volume is known).
4. Slider interaction continues to set volume, and moving it above zero exits the muted state.
5. `cargo fmt --check`, `cargo check`, and `cargo test` pass.
