# rspotify — native Spotify client in Rust + GPUI

A lightweight Spotify desktop client written in Rust with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui). It uses Spotify's own desktop login and API endpoints — not the public Web API — and plays audio locally through Spotify Connect.

## Features

- **Native login**: OAuth against `accounts.spotify.com` using Spotify's desktop client id, then a native session over `login5.spotify.com` via librespot.
- **Native search**: persisted GraphQL queries against Spotify's Pathfinder endpoint (`api-partner.spotify.com/pathfinder/v1/query`), the same service the official desktop and web clients use.
- **Personalized Home**: native category chips, recent shortcuts, ordered recommendation shelves,
  full-shelf pagination, hover play controls and recommendation panels with muted Canvas previews.
  Playlist, album, artist and podcast cards open native detail/context pages.
- **Local playback**: registers as a Spotify Connect device (`rspotify`) and streams through librespot to the default audio output.
- **Persistent session**: the refresh token, stable device id, and cached profile details are stored in `~/.cache/rspotify/`. Returning users see the app immediately while the session refreshes in the background; the browser login is only needed once.
- **Playlist detail**: saved playlists and Liked Songs open in a Spotify-style page with a large header, virtualized track list, local search, sort, and context playback.
- **Library panel**: saved playlists, albums, followed artists and podcasts from native services;
  owner names, artwork fallbacks, Recents, pin indicators, search with highlighted matches,
  category/ownership filters and four list/grid layouts. Liked Songs scrolls with the library.
  Expansion uses the main area; the collapsed rail keeps rich hover cards and playback controls.
- **Library pin/unpin**: explicit context-menu actions use native collection writes and verify the
  resulting membership. Account-changing validation is still pending; no writes run on startup.
- **Spotify-style UI**: custom titlebar, library sidebar, tabbed search results, playlist pages and a now-playing bar, built with GPUI.
- **Smooth scrolling**: shared easing for wheel and touchpad input in the library, search results,
  horizontal carousels, and virtualized playlists.

> **Spotify Premium is required for playback.** Free accounts can sign in and search, but Spotify does not permit playback through this kind of client.

## Current status

| Area | Status |
| --- | --- |
| Native OAuth login + refresh | Implemented |
| Native session (login5/keymaster) | Implemented via librespot |
| Search (all / songs / artists / albums / playlists / podcasts) | Implemented |
| Personalized Home | Feed, category filters, shelves and muted Canvas panels implemented; visual parity pending |
| Local playback (play / pause / next / previous / seek / volume) | Implemented |
| User library sidebar | Native categories, Recents, filters, four layouts and pin commands; full parity pending |
| Playlist detail + Liked Songs page | Implemented; visual and real-account checks pending |
| Album/artist/podcast library navigation | Native context track page; dedicated entity layouts not yet |
| Queue, lyrics | Not yet |

The library redesign is **not yet a complete 1:1 implementation**: playlist/folder creation,
Jam, queue, other editing and drag organization remain disabled. Recents uses the native endpoint's
latest 50 contexts; older ordering and pin ordering may differ from Spotify's local library index.
Pinning currently follows the installed desktop client's default four-item limit, not a claimed
universal backend limit. Pin/unpin writes and exact visual parity still need validation. Some rich
playlist labels (such as "Made for" / mixed playlists) remain unavailable. See
[`specs/009-library-sidebar-parity/`](specs/009-library-sidebar-parity/).

Home uses the sections available to the account, so its content and order can differ from
Spotify screenshots. Canvas panels preview returned recommendations; when Spotify doesn't
provide baseline panels, up to three returned album/playlist recommendations are used.
Hover previews are muted, six-second bounded loops and use an **optional `ffmpeg` executable**
on `PATH`; without it, artwork remains visible. No protected music-video/episode manifests,
audio preview mixing, save/feedback mutations or announcement-service integration are implemented.
Spotlight/promotion sections with supported content have a featured layout, but full announcement
banner/menu parity and real-desktop visual checks remain pending. See
[`specs/010-home-screen/`](specs/010-home-screen/).

## Requirements

- Rust 1.85+ (edition 2024)
- A desktop session with a working audio output (PulseAudio/PipeWire/ALSA)
- A Spotify account
- Spotify Premium for playback

On Linux you may need ALSA development headers for the audio backend:

```bash
# Fedora
sudo dnf install alsa-lib-devel

# Debian/Ubuntu
sudo apt install libasound2-dev
```

## Build and run

```bash
cargo run --release
```

Use the release build when evaluating UI responsiveness; the default `cargo run` uses unoptimized
application and dependency code. For optional scroll frame-callback timing summaries, run
`RSPOTIFY_SCROLL_METRICS=1 cargo run --release`. These report aggregate timing only, not account data.

Then press **Log in with Spotify**. A browser window opens at Spotify's login page; after
authorising, the loopback callback returns to the app automatically.

No Spotify Developer app is required. The client uses the same public desktop client id as
the official client and librespot.

### Linux desktop icon during development

After building, register the project-owned icon and development launcher once:

```sh
cargo build
scripts/install-dev-desktop-entry.sh
cargo run
```

This installs `rspotify.desktop` and the bundled SVG under `${XDG_DATA_HOME:-~/.local/share}`.
It uses the built executable's absolute path and refreshes KDE's service and icon-loader caches.
Spotify's Flatpak is not required. Rerun setup if you move the checkout; pass an executable path
as the first argument for release builds or a custom Cargo target directory. The installer uses
Python 3 to escape paths in the desktop entry. It does not launch the app or access your account.

### Configuration

All configuration is optional. Put overrides in `.env` or export them:

```env
# Override the native client id (only if you know its registered redirect URI)
RSPOTIFY_NATIVE_CLIENT_ID=65b708073fc0480ea92a077233ca87bd

# Loopback redirect used by the login flow
RSPOTIFY_REDIRECT_URI=http://127.0.0.1:8898/login

# Where the session token, device id and librespot cache are stored
RSPOTIFY_STATE_DIR=/home/you/.cache/rspotify
```

`~/.cache/rspotify/session.json` is written with owner-only permissions and contains the OAuth
refresh token, stable device id, and cached display name/avatar URL. It does not contain an access
token or account password. Delete it to sign out.

## Architecture

```text
src/
├── main.rs                     # GPUI app bootstrap
├── lib/spotify/
│   ├── client.rs               # OAuth, librespot session, Connect/player control
│   ├── playlist.rs             # native playlist detail, context pages, cover colours
│   ├── library.rs              # native rootlist, folders, owners and artwork fallback
│   ├── library_catalog.rs      # saved categories, batched decoration and pins
│   ├── collection.rs           # native collection-v2 paging and explicit pin writes
│   ├── search.rs               # Pathfinder persisted-query search
│   ├── home.rs                 # native Home feed, shelf paging and Canvas metadata
│   ├── store.rs                # device id + refresh token persistence
│   └── error.rs                # unified error type
├── router/                     # path + stack navigation
├── state/                      # global AppState (client handle)
└── view/
    ├── main_layout.rs          # resizable library/content split + player bar
    ├── components/library/    # virtualized library panel and interactions
    ├── screens/playlist.rs     # playlist / Liked Songs detail page
    ├── screens/search.rs       # search tabs and results
    ├── screens/sign_in/        # login screen
    └── components/titlebar/    # Spotify-style titlebar
```

Key dependencies:

- **gpui / gpui-component** — UI
- **librespot 0.8** — native Spotify session, Connect protocol and audio playback
- **reqwest / serde_json** — Pathfinder search and context pagination
- **image** — samples playlist artwork for the header gradient

## Development workflow

This project uses spec-driven development. Every feature or architectural change starts with a
numbered directory under [`specs/`](specs/), containing `spec.md`, `design.md`, and `tasks.md`.
The specification is the source of truth for intended behavior. See [`AGENTS.md`](AGENTS.md) for
the agent workflow, native-client constraints, validation commands, and security rules.

## How it works

1. On first sign-in, the app opens Spotify's OAuth page with the desktop client id and a loopback
   redirect. Returning users see their cached profile immediately while the refresh token is
   exchanged for an access token in the background.
2. The refresh token, stable device id, and latest non-secret profile details are cached.
3. librespot connects to Spotify using the native access-token flow (login5, keymaster,
   AP resolver), and registers a Connect device named `rspotify`.
4. Search is sent to Pathfinder with query hashes extracted from the installed official
   client. Results are rendered natively.
5. Opening a playlist requests header metadata from the native `playlist/v2` endpoint and
   track pages from `context-resolve/v1`; Liked Songs uses the collection context.
6. Pressing play activates the local Connect device and loads the selected track/context.
   librespot resolves audio keys and streams to the local audio backend.
7. Playback events update the now-playing bar, progress, and active playlist state.

See [`docs/reverse-engineering.md`](docs/reverse-engineering.md) for the observed endpoints,
client id, Pathfinder query hashes and which parts are confirmed versus inferred.

## Roadmap

- [x] Playlist detail view
- [ ] Queue
- [ ] Album/artist pages
- [x] Liked Songs detail and playback
- [ ] Lyrics
- [ ] Keyboard shortcuts and system media controls
- [ ] Offline cache controls and audio quality settings

## Legal

This is an unofficial client and is not affiliated with Spotify.

- Using a client id that belongs to Spotify's official client, and using librespot, may
  violate Spotify's Terms of Service and can put your account at risk.
- A Spotify Premium subscription is required for playback.
- No DRM circumvention or premium bypass is implemented; playback uses librespot's
  implementation of Spotify's native protocol with a valid account.
- The project is provided for educational and personal use.

## Acknowledgements

- [librespot](https://github.com/librespot-org/librespot)
- [GPUI](https://github.com/zed-industries/zed)
- [gpui-component](https://github.com/longbridge/gpui-component)
- The Rust community

## License

MIT — see [LICENSE.md](LICENSE.md).
