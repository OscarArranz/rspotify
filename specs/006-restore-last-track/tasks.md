# Tasks: restore the last-known track

## Implementation

- [x] Replace the local playback cache approach with server-side Connect cluster state.
- [x] Expose read-only cluster player state from librespot-connect without activating playback.
- [x] Resolve current server track metadata through Spotify's native extended-metadata endpoint.
- [x] Keep playback updates off the GPUI foreground thread and preserve local player event handling.
- [x] Make explicit Play activate rspotify and resume the displayed track/context at the server
  position.
- [x] Bind Space to the player toggle when text input does not own focus.

## Validation

- [x] `cargo fmt --check`
- [x] `cargo check`
- [x] `cargo test`
- [x] `cargo test --manifest-path vendor/librespot-connect/Cargo.toml`
- [ ] Real-account validation that the server track appears without transferring/starting playback.
