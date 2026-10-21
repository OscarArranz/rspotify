# Native Spotify login, search, library, and playback

## Status

Implemented and verified in the native Spotify client feature (commit `add0abf`).

## Goal

Replace the public Spotify Web API client with Spotify's native desktop services so rspotify can
sign in with the same account flow as the official desktop client, search through Pathfinder, load
the user's library, and play audio locally through a Spotify Connect device.

## User-visible scope

- The sign-in screen offers **Log in with Spotify**. The first sign-in opens Spotify's OAuth page
  in the browser; after authorisation the loopback callback returns to the app.
- A stored refresh token lets a returning user sign in without visiting the browser again, unless
  Spotify rejects the stored session.
- After sign-in, the titlebar shows back/forward navigation, the home button, search, notification
  and friends placeholders, and the user's avatar. The library sidebar lists the user's saved
  playlists. The player bar shows the current track, progress, and playback controls.
- Search is grouped into All, Songs, Artists, Albums, Playlists, and Podcasts & Shows tabs.
  Clicking a track or episode starts single-track playback; clicking another result starts that
  context.
- The library sidebar and the **Liked Songs** row start playback on the local `rspotify` Connect
  device.
- Playback controls include play/pause, previous/next, ±10 second seek, and volume. The player bar
  updates the now-playing metadata and progress.
- Playback requires **Spotify Premium**. The README and sign-in screen state this requirement.

## Behavior and architecture requirements

### Authentication and session

- Use Spotify's desktop OAuth client id and loopback redirect against `accounts.spotify.com`.
- Persist only the OAuth refresh token and a stable device id in `~/.cache/rspotify/session.json`.
  Write the file with owner-only permissions (`0600`) and its directory with `0700` on Unix.
- Exchange the OAuth token through librespot's native `login5` session. Do not use the public Web
  API for search, library, or playback.
- Never persist the account password or log access/refresh tokens.
- Do not auto-login on launch: the user clicks **Log in with Spotify**, and a stored refresh token
  skips the browser step.
- Run interactive OAuth and session setup off the GPUI foreground thread. Show a loading state and
  surface errors on the sign-in screen.

### Search

- Use the Pathfinder GraphQL endpoint with the persisted `searchDesktop` query and the hash
  recorded in `docs/reverse-engineering.md`.
- Authenticate Pathfinder with the account access token from `session.login5().auth_token()`.
  Keymaster tokens restricted to `streaming` are not sufficient and must not be used as the
  Pathfinder bearer token.
- Send the same search variables as the official client: 20 items, 20 top results, English search
  term, and audiobook/author/pre-release flags disabled.
- Parse tracks, artists, albums, playlists, and podcast episodes from the `searchV2` response.
- Discard stale search responses when the query, tab, or request generation changes.
- Keep search network work off the GPUI foreground thread.

### Library and profile

- Fetch the user's saved playlists from the native SpClient rootlist endpoint. Decode the response
  as `spotify.playlist4.proto.SelectedListContent` and page until the reported list length.
- Use the decorated `contents.meta_items[]` entries for playlist names and artwork, parallel to
  `contents.items[]`.
- Fetch the user's display name and avatar through the native profile endpoint. Fall back to the
  username and no avatar when fields are missing.
- A library or profile failure must not crash the main layout; the affected area stays empty or
  falls back to placeholders.

### Playback

- Register a Spotify Connect device named `rspotify` through librespot and play through the local
  audio backend.
- Call `Spirc::activate()` before loading a track or context so the local device becomes active.
- Support single-track playback for tracks/episodes and context playback for albums, playlists,
  artists, shows, and Liked Songs.
- Keep a shared playback snapshot updated from librespot player events. Extrapolate the position
  while playing and poll it from the player bar without blocking on the player event channel.
- Support play/pause, previous/next, seek, and volume changes. Playback requires Premium; do not
  bypass DRM.
- Use the bundled SVG media icons rather than relying on font glyphs.

### UI and state

- `AppState` owns the authenticated `NativeSpotify` handle. It is set after successful login and
  observed by the titlebar so authenticated controls appear immediately.
- Keep navigation in the existing path and stack routers. The home button resets the stack to the
  home screen.
- Keep library and player state in their GPUI entities; do not introduce another global store.
- Do not wrap blocking network or session work in the GPUI foreground thread. Use worker threads
  and `oneshot` channels from `cx.spawn`.

## Scope boundaries

- Playlist detail, album/artist pages, queue, lyrics, and search pagination are not included.
- Shuffle/repeat controls are not exposed in the player bar in this iteration, even though the
  client exposes the underlying Connect methods.
- Auto-login, offline cache controls, audio-quality settings, keyboard shortcuts, system media
  keys, and packaging are follow-up work.
- No public Web API fallback and no Widevine/PlayPlay reimplementation. librespot owns the native
  protocol and DRM path.
- The OAuth scopes include write permissions for desktop parity, but no playlist-editing UI is
  included.

## Acceptance criteria

1. A first-time user can click **Log in with Spotify**, complete the browser flow, and land on the
   main layout with the titlebar reflecting the authenticated state.
2. A returning user with a valid stored refresh token can sign in without the browser flow; an
   invalid token falls back to the browser flow.
3. A search returns categorized results for tracks, artists, albums, playlists, and podcasts, and
   tab changes show the matching group.
4. Clicking a track or episode starts single-track playback; clicking an album, playlist, artist,
   show, library playlist, or Liked Songs starts context playback.
5. Play/pause, previous/next, ±10 second seek, and volume controls update playback, and the player
   bar shows now-playing metadata and advancing progress.
6. The library sidebar lists saved playlists with artwork where available, and the avatar resolves
   for the signed-in user.
7. Login, search, and library fetches do not block the GPUI foreground thread.
8. No tokens or personal account data are logged or committed, and the session file has `0600`
   permissions.
9. `cargo fmt --check`, `cargo check`, and `cargo test` pass.
10. Real-account smoke tests pass for login/search/library and for playback when run with a
    Premium account.

## References

- [`docs/reverse-engineering.md`](../../docs/reverse-engineering.md) — observed endpoints, client
  id, Pathfinder hashes, and evidence.
- [`AGENTS.md`](../../AGENTS.md) — SDD workflow and native-client constraints.
