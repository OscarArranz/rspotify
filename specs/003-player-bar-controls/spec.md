# Interactive player bar

## Goal

Make playback progress and volume directly interactive and closer to Spotify desktop behavior.

## User-visible scope

- Clicking or dragging the progress track seeks to the corresponding position in the current track.
- Hovering the progress track shows the time at the pointer position; fill and thumb respond to hover
  and dragging.
- Clicking or dragging the volume track sets volume to the corresponding percentage.
- Displayed volume stays synchronized with the playback snapshot.
- Existing transport controls remain available. Playback still requires Spotify Premium.

## Acceptance criteria

1. Seeking maps the pointer location to a clamped position from zero through track duration.
2. The seek tooltip displays the pointer's time, not current playback time.
3. Volume maps the pointer location to a clamped 0–100% value.
4. Both sliders support click and drag, with visible hover/active feedback.
5. Seeking is safely disabled with no track or zero duration.
6. `cargo fmt --check`, `cargo check`, and `cargo test` pass.
7. Visual and real-account checks are recorded when explicitly approved.
