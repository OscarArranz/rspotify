# Native Spotify client — design

## Goals and constraints

The native client must behave like Spotify's desktop app without using the public Web API for
search, library, or playback. Spotify's OAuth page supplies an account access token; librespot
supplies the native `login5` session, Connect device, and audio pipeline; Pathfinder supplies
search; SpClient supplies the rootlist and profile.

The design keeps the existing GPUI structure and replaces only the data and playback layer. The
main constraint is that GPUI owns the foreground thread, while OAuth, librespot startup, Pathfinder,
and rootlist calls are blocking. Those calls run on worker threads, and results return through
`oneshot` channels.

## Module boundaries

Linux desktop integration is owned by `data/rspotify.desktop` and
`scripts/install-dev-desktop-entry.sh`. The installer copies `assets/icons/rspotify.svg` to XDG
data storage and generates a desktop entry with explicit paths to that installed icon and the
development executable. GPUI sets `app_id` to `rspotify`. KDE's service database refresh and
`org.kde.KIconLoader.iconChanged` notification are separate: rebuilding the former alone does not
invalidate a running panel's icon-loader cache. No desktop restart or Flatpak resource is required.

| Module | Responsibility |
| --- | --- |
| `src/lib/spotify/client.rs` | OAuth with desktop client id, session store handoff, librespot `Session`/`Spirc`/`Player`, Connect device registration, rootlist/profile fetches, playback commands and snapshot |
| `src/lib/spotify/search.rs` | Pathfinder request and `searchV2` response parser |
| `src/lib/spotify/store.rs` | Refresh token, stable device id, and cached non-secret profile persistence with private permissions |
| `src/lib/spotify/error.rs` | Unified auth/session/api/playback/storage error type |
| `src/state/app_state.rs` | Global authenticated client plus optimistic cached profile/restoration state |
| `src/view/screens/sign_in/` | Sign-in screen and non-blocking login trigger |
| `src/view/components/titlebar/` | Authenticated controls, search entry point, avatar, navigation |
| `src/view/screens/search.rs` | Search tabs, result rendering, stale-request handling, click-to-play |
| `src/view/main_layout.rs` | Resizable library/content split, library fetch, home screen, player bar, polling |
| `src/router/` | Path routing between sign-in and main; stack navigation inside main |

`NativeSpotify` is a cloneable `Arc<NativeInner>` handle. `NativeInner` owns:

- the librespot `Session`,
- the `Spirc` controller,
- the shared playback snapshot and its update timestamp,
- the Tokio runtime used by librespot and the request helpers,
- the Spotify username.

`NativeSpotify` is the only Spotify type the UI stores.

## Authentication flow

1. At startup, `SessionStore::load()` reads `session.json`. If it contains a refresh token, the
   router starts at `/main`, seeds `AppState` with cached profile details when available, and starts
   `NativeSpotify::restore()` on a worker thread. This is an optimistic UI state, not an active
   authenticated client; API actions remain unavailable until restoration completes.
2. `ensure_device_id()` generates a 128-bit hex id on
   first run.
3. Startup restore refreshes the saved token without opening a browser. A confirmed invalid-grant
   response clears the refresh token and navigates to `/sign-in`; network/transport/session errors
   leave the cached profile and main layout intact; startup restoration is attempted again on the
   next app launch. Explicit sign-in retains the browser OAuth path.
4. On successful restore/sign-in, update the refresh token and hand the access token to
   `NativeSpotify::connect`:
   - build a two-worker Tokio runtime,
   - create the librespot cache under the state directory,
   - construct `Session` inside `runtime.enter()` because librespot captures the current runtime
     handle,
   - create `Player` with the selected audio backend and `SoftMixer`,
   - create `Spirc` with device name `rspotify` and device type `Computer`, then spawn its task.
5. Fetch the current profile off-thread when possible, persist its display name and avatar URL, and
   publish both the client and profile through `AppState`. `MainLayout` observes global state and
   initializes its library/player once the client becomes available. The titlebar displays cached
   profile details immediately and updates them after restoration.

The loopback listener and browser interaction stay inside `OAuthClientBuilder`; the app does not
implement its own HTTP callback server.

## Search flow

`SearchScreen` owns the query, selected tab, result groups, load status, and a monotonically
increasing request generation.

1. Entering a query or changing a tab increments the generation and starts `cx.spawn`.
2. A worker thread calls `NativeSpotify::search`, which runs the async Pathfinder request on the
   librespot runtime.
3. `search::search` obtains the login5 access token, builds the persisted `searchDesktop` request,
   and sends it with `App-Platform: WebPlayer`.
4. The parser reads `/data/searchV2` and fills top result plus tracks, artists, albums, playlists,
   and podcast groups.
5. The UI applies the result only if the query, tab, and generation still match. This prevents a
   slow response from overwriting a newer tab or query.
6. Result clicks call `play_track` for tracks/episodes and `play_context` for contexts.

Search failures remain local to the screen and render an error message; they do not tear down the
session.

## Library and profile flow

The library sidebar is created with the main layout and receives the `NativeSpotify` handle. It
starts one worker fetch and replaces its entries on success. Playlist rows use `play_context`.

`NativeSpotify::library_playlists` pages through `spclient().get_rootlist(from, Some(100))`,
decodes `SelectedListContent`, filters to `spotify:playlist:` URIs, and reads names/artwork from the
parallel `contents.meta_items[]`. It stops when the page is empty or the reported length is reached.

`NativeSpotify::user_profile` calls `spclient().get_user_profile(...)`, parses the JSON name and
image fields, and normalizes `spotify:image:` ids to `https://i.scdn.co/image/<id>`. Both calls are
synchronous wrappers around `runtime.block_on` and are only invoked from worker threads.

## Playback flow and state

```
search/library click
    -> NativeSpotify::play_track / play_context
    -> Spirc::activate()
    -> Spirc::load(track list or context URI)
    -> librespot Player
    -> PlayerEvent channel
    -> PlaybackInner { snapshot, updated_at }
    -> PlayerBar poll every 250 ms
```

`PlaybackInner` stores a `PlaybackSnapshot`: current item metadata, playing flag, last known
position, volume, shuffle, and repeat. A dedicated runtime task consumes `PlayerEvent` values:

- `TrackChanged` replaces now-playing metadata,
- `Playing`/`Paused`/`Stopped` update the playing flag,
- position events update the last known position,
- volume/shuffle/repeat events update their fields.

Each event records `updated_at`. `NativeSpotify::playback` clones the snapshot and extrapolates the
position by the elapsed time while playing, clamped to the track duration. This keeps the progress
bar smooth without querying Spotify.

`PlayerBar` polls the snapshot from a background timer and re-renders only on changes. It sends
play/pause, previous/next, seek, and volume commands directly to `NativeSpotify`.

## UI and navigation

- `RootLayout` selects the sign-in screen or main layout from `AppState`.
- `PathRouter` keeps `/` and `/main`; `StackRouter` holds search screens above the home screen.
- `MainLayout` creates the stack router, registers a `StackRouterHandle` global, and builds the
  library sidebar and player bar from `AppState`.
- The titlebar observes `AppState` so authenticated controls appear immediately after login. It
  observes `StackRouterHandle` and then the router entity so back/forward button state stays in
  sync.
- The home button clears the stack back to the home screen.
- Media controls use `assets/icons/{play,pause,previous,next}.svg`; text controls continue to use
  the bundled font for seek and volume labels.

## Dependencies and versions

- `librespot 0.8` with `native-tls` and `rodio-backend` provides OAuth helpers, `login5`, Connect,
  audio decoding, and PlayPlay through the native protocol.
- `protobuf 3.7` decodes `SelectedListContent`.
- `reqwest`/`serde_json` send and parse Pathfinder GraphQL and profile/rootlist JSON.
- `tokio` hosts librespot and the async client calls.
- `gpui`/`gpui-component` provide the UI and resizable layout.
- Vendored `ashpd` crates are patched to use `zvariant = "5"`; keep the patch entries in
  `Cargo.toml` when updating dependencies.

## Security and privacy

- `session.json` contains the refresh token, device id, display name, and avatar URL. It is written through a temporary file, chmod `0600`
  on Unix, then renamed; the state directory is chmod `0700`.
- Access tokens live only in memory and are never rendered or written to logs.
- Errors from OAuth/Pathfinder are displayed without dumping credential payloads.
- The desktop client id is public but using it with librespot carries Terms-of-Service and account
  risk; README and spec call this out.

## Risks and trade-offs

| Risk | Mitigation |
| --- | --- |
| Spotify changes Pathfinder hashes, headers, or schema | Keep hashes in one module and document provenance; add parser/response tests |
| Desktop client id or redirect changes | Allow environment overrides for client id and redirect |
| librespot protocol/API changes | Pin to `0.8` until a spec justifies an upgrade |
| Rootlist artwork missing for some entries | Render a neutral placeholder; do not fail the sidebar |
| Pathfinder rejects the token | Use login5 auth token, not keymaster; surface a clear error |
| Blocking work freezes the UI | Worker threads + `oneshot`; `Session` under `runtime.enter()` |
| Playback requires Premium | Document requirement; do not attempt a DRM or premium bypass |
| Startup refresh fails temporarily | Retain cached profile and main layout; don't treat transport failure as invalid credentials |

## Validation strategy

- Pure parser test for the Pathfinder response (`search::tests`).
- `cargo fmt --check`, `cargo check`, and non-ignored tests for every change.
- Ignored, real-account smoke tests for login/search/library and playback. These require network,
  a valid account, and Premium for playback, and must only run with user approval.
- Manual visual checks for authenticated titlebar, search tabs, library scrolling, and player bar
  icon rendering.
