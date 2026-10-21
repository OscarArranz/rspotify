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
- [x] Hydrate missing track/episode display metadata using bounded native extended-metadata batches.
- [x] Read `added_at` directly from collection context metadata.
- [x] Set playlist-local search input text color explicitly to white.
- [x] Force the GPUI component theme to dark mode so both search editors use a light foreground
  regardless of the operating-system appearance.
- [x] Fall back to raw 20-byte playlist picture IDs when rootlist/playlist cover URLs are absent.
- [x] Run the read-only real-account metadata check: 5,855/5,855 Liked Songs rows hydrated; 46/108
  rootlist playlists have artwork sources in the current account payload.

## Validation

- [x] `cargo fmt --check`
- [x] `cargo check`
- [x] `cargo test` (4 passed, 2 ignored)
- [ ] Visual check against the installed Flatpak client at the default and wide window sizes.
- [x] Read-only real-account check (explicitly approved): inspect context/rootlist structure and
  verify Liked Songs metadata hydration. No playback was started.
- [ ] Visual check in the running app: open Liked Songs and confirm titles, album artwork, and
  dates render; confirm image fallback is improved for playlists with raw picture IDs.
- [ ] Real-account interaction check (only with explicit network/account approval): open a playlist,
  play a row, pause, shuffle, search, sort, and scroll a multi-page playlist.
- [ ] Stop playback and close the app after any real-account check.
