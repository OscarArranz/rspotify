# Tasks: playlist detail view

## Implementation

- [x] Add the playlist detail feature specification and design.
- [x] Add `src/lib/spotify/playlist.rs` with playlist/context types and native fetches.
- [x] Add cover-colour sampling and the direct `image` dependency.
- [x] Extend `PlaybackSnapshot` with the active context and add `play_context_at`.
- [x] Add `liked_songs_context_uri` and `spotify:user:<username>:collection` handling.
- [x] Add `PlaylistScreen` with the official header/metrics, action bar, rows, search, sort,
  pagination, and loading/error/empty states.
- [x] Push `PlaylistScreen` for sidebar playlist and Liked Songs clicks.
- [x] Add shuffle, download, more, heart, clock, list, and sort SVG assets.
- [x] Update `README.md`, `docs/reverse-engineering.md`, and `AGENTS.md` for the new spec.
- [x] Add unit tests for context metadata mapping, context-page JSON, and description cleanup.

## Validation

- [x] `cargo fmt --check`
- [x] `cargo check`
- [x] `cargo test` (4 passed, 2 ignored)
- [ ] Visual check against the installed Flatpak client at the default and wide window sizes.
- [ ] Real-account check (only with explicit network/account approval): open a playlist, open
  Liked Songs, play a row, pause, shuffle, search, sort, and scroll a multi-page playlist.
- [ ] Stop playback and close the app after any real-account check.
