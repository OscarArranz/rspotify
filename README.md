# rspotify — native Spotify client in Rust + GPUI

A lightweight Spotify desktop client written in Rust with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui). It uses Spotify's own desktop login and API endpoints — not the public Web API — and plays audio locally through Spotify Connect.

## Features

- **Native login**: OAuth against `accounts.spotify.com` using Spotify's desktop client id, then a native session over `login5.spotify.com` via librespot.
- **Native search**: persisted GraphQL queries against Spotify's Pathfinder endpoint (`api-partner.spotify.com/pathfinder/v1/query`), the same service the official desktop and web clients use.
- **Local playback**: registers as a Spotify Connect device (`rspotify`) and streams through librespot to the default audio output.
- **Persistent session**: the refresh token and a stable device id are stored in `~/.cache/rspotify/` so the browser login is only needed once.
- **Playlist detail**: saved playlists and Liked Songs open in a Spotify-style page with a large header, virtualized track list, local search, sort, and context playback.
- **Spotify-style UI**: custom titlebar, library sidebar, tabbed search results, playlist pages and a now-playing bar, built with GPUI.

> **Spotify Premium is required for playback.** Free accounts can sign in and search, but Spotify does not permit playback through this kind of client.

## Current status

| Area | Status |
| --- | --- |
| Native OAuth login + refresh | Implemented |
| Native session (login5/keymaster) | Implemented via librespot |
| Search (all / songs / artists / albums / playlists / podcasts) | Implemented |
| Local playback (play / pause / next / previous / seek / volume) | Implemented |
| User library sidebar | Implemented |
| Playlist detail + Liked Songs page | Implemented; visual and real-account checks pending |
| Albums, queue, lyrics | Not yet |

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

Then press **Log in with Spotify**. A browser window opens at Spotify's login page; after
authorising, the loopback callback returns to the app automatically.

No Spotify Developer app is required. The client uses the same public desktop client id as
the official client and librespot.

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

`~/.cache/rspotify/session.json` is written with owner-only permissions and contains the
OAuth refresh token. Delete it to sign out.

## Architecture

```text
src/
├── main.rs                     # GPUI app bootstrap
├── lib/spotify/
│   ├── client.rs               # OAuth, librespot session, Connect/player control
│   ├── playlist.rs             # native playlist detail, context pages, cover colours
│   ├── search.rs               # Pathfinder persisted-query search
│   ├── store.rs                # device id + refresh token persistence
│   └── error.rs                # unified error type
├── router/                     # path + stack navigation
├── state/                      # global AppState (client handle)
└── view/
    ├── main_layout.rs          # library sidebar + content + player bar
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

1. The app opens Spotify's OAuth page with the desktop client id and a loopback redirect.
2. The refresh token is exchanged for an access token and cached.
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
