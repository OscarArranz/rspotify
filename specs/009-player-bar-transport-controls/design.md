# Spotify-style transport controls — design

## Module boundaries

- `src/view/main_layout.rs` renders the transport row and owns optimistic shuffle/repeat display
  state.
- `src/lib/spotify/client.rs` owns the Connect command boundary and exposes the repeat model in the
  playback snapshot.
- `assets/icons/shuffle.svg`, `repeat.svg`, and `repeat-one.svg` provide the control artwork.

## Behavior

`PlaybackSnapshot.repeat` is a `RepeatMode` (`Off`, `Context`, `Track`) derived from librespot's
`PlayerEvent::RepeatChanged { context, track }`; track repeat wins when both flags are set.
`RepeatMode::next` implements the Spotify cycle off → context → track → off.
`NativeSpotify::set_repeat_mode` sends the matching `Spirc::repeat` and `Spirc::repeat_track`
commands, so a single UI action always converges on exactly one active repeat mode.

The shuffle and repeat buttons keep a short-lived optimistic value, mirroring the existing seek and
volume optimistic state. A click updates the displayed value immediately, and the override is
cleared as soon as the next playback snapshot reports the requested state, or after three seconds if
the command did not take effect. Repeated clicks read the optimistic value, so rapid toggles cycle
from what the user last saw instead of the stale snapshot.

Buttons use the existing `shuffle.svg` and new Material-style `repeat.svg`/`repeat-one.svg` artwork.
Inactive controls render in `#b3b3b3`, brighten to white on hover, and active controls render in
Spotify green (`#1ed760`) and brighten on hover. Previous, play/pause, and next keep their existing
handlers and sizing.

All commands are non-blocking `Spirc` sends; no GPUI foreground work blocks on the network.
