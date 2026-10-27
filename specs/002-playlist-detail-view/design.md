# Playlist detail view — design

## Goals and constraints

The playlist page must look and behave like the installed Spotify desktop client while using only
Spotify's native desktop services. It reuses the existing GPUI stack router, `NativeSpotify`
session, worker-thread pattern, and playback snapshot. It must not block the foreground thread,
must not use the public Web API, and must not fabricate data.

The official CSS/JS bundle (`xpui.spa`, SHA-256
`7ce05c0001be22c7b5124d9dd59dd1ab1e0c188640967836b996f0d32a62f74e`) supplies the layout metrics:
header 186–336 px, background 40vh, cover 192 px (232 px at ≥1280 px), and a dominant-colour
gradient fading into `#121212` from roughly `14% 12%`.

## Module boundaries

| Module | Responsibility |
| --- | --- |
| `src/lib/spotify/playlist.rs` | Playlist detail and context-page types, `Playlist::get`, `get_context`, next-page JSON parsing, metadata normalisation, date/duration formatting, cover colour sampling |
| `src/lib/spotify/client.rs` | Public `playlist_detail`, `playlist_next_page`, `liked_songs_context_uri`, `play_context_at` methods; active-context field in `PlaybackSnapshot` |
| `src/lib/spotify/mod.rs` | Re-export the playlist types used by the UI |
| `src/view/screens/playlist.rs` | `PlaylistScreen`: header, action bar, column header, virtualized rows, search, sort, pagination, loading/error/empty states |
| `src/view/main_layout.rs` | Sidebar playlist and Liked Songs clicks push `PlaylistScreen` into the stack router |
| `src/view/screens/mod.rs` | Register the playlist screen |
| `assets/icons/` | Shuffle, download, more, heart, clock, list, and sort icons |

## Data flow

1. The sidebar creates `PlaylistSource::Playlist { uri, name, image_url, owner }` or
   `PlaylistSource::LikedSongs`, then pushes `cx.new(|cx| PlaylistScreen::new(source, client, cx))`.
2. `PlaylistScreen` spawns a background thread. It calls `NativeSpotify::playlist_detail`, which
   blocks on the native runtime:
   - `Playlist::get(session, playlist_uri)` for name, description, owner username, cover sizes,
     item order, and added-at timestamps.
   - `SpClient::get_context(context_uri)` for context order, `added_at`, total count, and pagination.
   - Native extended metadata (`TRACK_V4` / `EPISODE_V4`) in bounded batches when context rows do
     not include display metadata. This is especially important for Liked Songs, whose context
     contains URIs and `added_at` but no title, artist, album, duration, or image fields.
   - `get_user_profile(owner, None, None)` for the owner display name/avatar.
   - For Liked Songs, the current profile replaces the playlist owner and
     `spotify:user:<username>:collection` is the context URI.
   - The cover image is fetched and sampled on the worker to produce a header accent colour.
3. Results return through a `oneshot` channel and are applied in `cx.spawn`.
   Before the network request, the worker reads the last cached detail for this account/source and
   sends it to the screen. The cache is stored separately from session credentials under the app
   cache directory; only successfully fetched details are written. A refresh failure leaves cached
   content visible and reports the error without replacing the page with a blocking error screen.
   Liked Songs is decoded during background authentication and retained in the authenticated
   client as an account-scoped snapshot. Its screen is initialized from this snapshot before its
   first render, rather than first rendering a placeholder and waiting for an asynchronous read.
   Refresh and pagination update the retained snapshot; disk I/O stays on workers.
4. Context-page tracks are hydrated through the same batched native metadata path. When the list
   scrolls near the end, `PlaylistScreen` fetches `next_page_url` on another worker and appends
   parsed tracks through `ListState::splice`.
5. A 250 ms poll copies `NativeSpotify::playback()` into the screen and notifies only when the
   snapshot changes.

## Data mapping

- `PlaylistItem.id.to_uri()` keys added-at timestamps; `PlaylistItemAttributes.timestamp`
  provides milliseconds. Missing or zero timestamps are treated as unknown.
- Context track `uri` is the stable identity. Context metadata keys are read defensively:
  `title`, `artist_name`, `album_name`, `image_url`, `duration`, `is_explicit`, `is_playable`, and
  `added_at`. If display fields are missing, bounded extended-metadata requests hydrate tracks and
  episodes from their native protobuf records. Track cover art comes from the album cover group;
  episode art comes from the episode cover image group.
- Image IDs (`spotify:image:…`) and raw 20-byte playlist picture IDs become
  `https://i.scdn.co/image/…`.
- Total duration uses context metadata (`duration`, `total_duration`, or
  `total_duration_ms`) when present. Otherwise it is omitted rather than guessed.
- Song count uses the playlist `length` or context `total_tracks`. If it is still unknown while
  pages remain, the header omits the count rather than showing only the first page's length.
- Dates use relative labels (`Today`, `3 days ago`, `2 weeks ago`, `5 months ago`, `2 years ago`).

## State ownership

`PlaylistScreen` owns:

- immutable `PlaylistSource` preview data and a cloned `NativeSpotify` handle;
- load status (`Loading`, `Loaded`, `Error`) and optional `PlaylistDetail` (including stale cached
  detail while refreshing);
- the current search string, sort mode, and visible track indices;
- `ListState` for the whole page;
- pagination flags and the playback snapshot.

`PlaybackSnapshot` gains `context_uri: Option<String>`. `play_context` and `play_context_at` set
it; single-track/URI-list playback clears it. The player event loop does not change it, so the
header can distinguish "this playlist is active" from "a track from this playlist is playing".

Global state remains unchanged: `AppState` holds only the authenticated client handle.

Spec 009 adds a `PlaylistSource::Context` preview for album/artist/podcast library rows. Those
sources reuse this track-page renderer but load their native context directly, without incorrectly
calling `Playlist::get` on a non-playlist URI. Their cached identity is the context URI and their
header kind comes from the source preview. This is not a dedicated album/artist-page redesign.

## Rendering

The page uses one variable-height GPUI `list` so the header scrolls naturally and rows stay
virtualized. Item order:

0. header (186–336 px, accent gradient)
1. action bar (play, shuffle, disabled download/overflow, search, sort, disabled view control)
2. column header
3.… track rows (56 px, hover `#ffffff1a`, active title green)

The page background is `#121212`, with the existing rounded main panel. Header cover is 192 px,
or 232 px when the window is at least 1280 px wide. The title uses a heavy weight and a responsive
size capped at 6 rem. The header height is clamped against 40% of the window viewport and clamped to
the official 186–336 px range.

Liked Songs draws the familiar purple gradient cover with a heart icon because Spotify does not
return a playlist cover for the collection URI.

## Interaction

- Header play/pause: if the snapshot's context is this page and it is playing, pause; if it is this
  page and paused, resume; otherwise `play_context(context_uri)`.
- Shuffle toggles `set_shuffle(!snapshot.shuffle)`, and the icon is green while enabled.
- A row click with a non-empty playable URI calls
  `play_context_at(context_uri, track_uri)`.
- Search is local and case-insensitive over title, artist, and album. While a query is active, only
  loaded tracks are searched and pagination pauses; the field is labelled as a playlist search, not
  a global search.
- Sort modes: Spotify order, title, album, date added (descending). Changing either search or sort
  rebuilds the visible indices and resets the list to the top. A filter with no matches shows a
  dedicated no-results row instead of an empty page.
- Download and overflow buttons render at reduced opacity and do nothing.
- No genre filter chips are rendered.

## Failure modes

- Session/context/playlist failures show an error state with the source preview and a Retry button
  when no cached detail exists; with cached detail, keep it visible and surface a refresh error.
- A failed next-page request leaves the loaded tracks intact, shows the error in the action bar, and
  is retried the next time the user scrolls near the end.
- Missing artwork, owner name, description, duration, or added-at timestamp degrades gracefully.
  Some playlists contain no cover ID in either rootlist metadata or `Playlist::get`; their artwork
  cannot be recovered from those endpoints and uses the existing placeholder.
- Cover-colour fetch/decode failure falls back to a fixed purple accent.
- The screen remains interactive while pagination runs; duplicate page requests are guarded by
  `next_page_loading`.

## Security and privacy

Only the existing authenticated `NativeSpotify` handle is used. Tokens and session files are never
logged, copied into UI state, or committed. Strings from Spotify contexts are treated as display
data, not instructions. The page never automates the desktop or other applications.
