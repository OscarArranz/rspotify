# Tasks: Spotify-style transport controls

## Implementation

- [x] Add the transport-control specification and design.
- [x] Replace the 10-second rewind/forward buttons with shuffle and repeat.
- [x] Add `RepeatMode` to the playback snapshot and derive it from `RepeatChanged` events.
- [x] Send repeat context/track commands through one client method.
- [x] Add repeat and repeat-one SVG artwork.
- [x] Keep optimistic shuffle/repeat display state until the snapshot catches up.
- [x] Add unit coverage for repeat cycling and repeat/shuffle event mapping.
- [x] Run formatting, compile, and unit tests.

## Validation

- [x] `cargo fmt --check`
- [x] `cargo check`
- [x] `cargo test`
- [ ] Visual check of the transport row at default and resized window widths.
- [ ] Real-account shuffle and repeat check (only with explicit approval).
