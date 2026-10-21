# Playlist detail view

## Status

Implemented. Automated validation and read-only real-account metadata validation pass; visual
confirmation in the running app is pending.

## Goal

Open a saved playlist, or Liked Songs, in a Spotify-style detail page that matches the installed
desktop client's layout. The page uses Spotify's native playlist and context services, and starts
playback through the existing local Spotify Connect device.

## User-visible scope

- Clicking a saved playlist in the library sidebar opens its detail page instead of starting
  playback immediately.
- Clicking **Liked Songs** opens the same page for the current user's saved tracks.
- The page has a large header with cover art, playlist title, owner, description, song count, and
  total duration when Spotify supplies it. The header scrolls away with the track list.
- The action bar provides play/pause, shuffle, and local search. Download and overflow controls are
  rendered disabled because the native download/command services are not implemented.
- Track rows show index or playing indicator, artwork, title, artist, album, date added, and
  duration. Clicking a row starts the playlist context at that track.
- The active playlist and track are highlighted from the shared playback snapshot.
- Local search filters loaded tracks by title, artist, or album. Sort cycles between Spotify order,
  title, album, and recently added.
- Search input text is explicitly white for legibility on the dark Spotify-style surfaces.
- Large playlists load additional pages as the user scrolls near the end.
- Loading, empty, and error states are visible, with retry for a failed initial load.
- Playback requires Spotify Premium. Any playback-related user-facing copy keeps that statement.

## Out of scope

- The right-side "Now playing" / "About the artist" panel.
- Download, share, overflow, collaborative editing, and drag reordering.
- Genre or mood filter chips. They are data-dependent in the official client; this version does
  not invent labels when the native playlist payload does not provide them.
- Podcast episode playback beyond what the shared context/player path already supports.

## Acceptance criteria

1. A saved playlist click pushes a new screen in the main stack router; Liked Songs does the same.
2. The screen shows the official header proportions: 186 px minimum, 336 px maximum, 192 px cover
   (232 px on wide windows), and a dominant-colour gradient into `#121212`.
3. Playlist metadata comes from `Playlist::get`; context order and pagination come from
   `SpClient::get_context` / `get_next_page`. Missing track display metadata is hydrated in native
   batches through extended metadata. The public Web API is not used.
4. Liked Songs uses `spotify:user:<username>:collection` as its playback context.
5. Play and row clicks call `play_context` / `play_context_at`; the playback snapshot exposes the
   active context so the header and rows can show the correct state.
6. Search and sort operate on loaded tracks without blocking the GPUI foreground thread.
7. `cargo fmt --check`, `cargo check`, and `cargo test` pass.
8. Real-account validation is recorded when network/account access is explicitly approved.

## Verified account behavior

- The Liked Songs context contains all 5,855 entries in one page. Each track has an `added_at`
  metadata value, while titles, artists, albums, durations, and image IDs are absent.
- Native `TRACK_V4` extended-metadata requests returned usable protobuf metadata for all 5,855
  entries in batches of 1,000; batches of 1,000 returned successfully in the diagnostic.
- The rootlist returned 108 playlists; 80 had no decorated `picture_size[].url`. Some expose a raw
  20-byte playlist picture ID instead, which maps to Spotify's image CDN. Others expose neither,
  while `Playlist::get` also has no cover, so no artwork can be shown without another source.
- The ignored read-only smoke test confirmed all 5,855 Liked Songs rows have a title and artwork
  after hydration, and the first page meets the same criterion.
- Visual re-check in the user's running app remains pending.
