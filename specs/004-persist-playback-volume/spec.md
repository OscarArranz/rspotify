# Persist playback volume

## Goal

Restore the user's most recently selected playback volume when the native Spotify client is
connected, rather than resetting volume to 50% on every sign-in.

## User-visible scope

- The volume selected with the player controls is persisted in the existing private state file.
- The next native session starts with that volume.
- Existing state files without a saved volume continue to start at 50%.

## Acceptance criteria

1. A saved volume from 0% through 100% is used when a new Connect session starts.
2. Changing volume through the player controls updates persistent state immediately.
3. Existing state files without a volume remain readable and use the 50% default.
4. Invalid out-of-range stored values are clamped to 100%.
5. `cargo fmt --check`, `cargo check`, and `cargo test` pass.
