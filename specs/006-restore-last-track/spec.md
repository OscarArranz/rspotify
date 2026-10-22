# Restore the last-known track

## Goal

After signing in, show the last track in Spotify's server-side Connect player state in the player
bar, as the desktop client does, without automatically activating this client or starting audio.

## Acceptance criteria

- The player bar reads the most recently received server-side Connect cluster/player state; it does
  not use a local playback cache or public Web API.
- If the account's server-side player state has a track, its metadata and position appear after
  sign-in without activating rspotify or starting audio.
- Playback on another Connect device is displayed as paused locally so local controls do not imply
  that audio is playing on rspotify.
- Pressing the player bar's play button or Space activates rspotify and starts the displayed track
  at the server-reported position; playback starts only after this explicit user action.
- Space toggles playback when focus is not in a text input and is not intercepted while typing.
- Live local player events remain authoritative when rspotify is the active Connect device.
- Missing or invalid server state does not prevent sign-in.
- `cargo fmt --check`, `cargo check`, and `cargo test` pass.
