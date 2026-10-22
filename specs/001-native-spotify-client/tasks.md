# Tasks: native Spotify login, search, library, and playback

## Implementation

- [x] Add the feature specification, design, and implementation/validation task list.
- [x] Replace the public Web API authentication path with Spotify's desktop OAuth client id and
  loopback redirect.
- [x] Persist the refresh token and a stable device id, using a private state directory and
  owner-only file permissions.
- [x] Build a librespot native session, Connect device (`rspotify`), player, mixer, and player event
  loop; activate before loading tracks or contexts.
- [x] Implement Pathfinder `searchDesktop` with the observed persisted-query hash and login5 access
  token; parse tracks, artists, albums, playlists, and podcasts.
- [x] Fetch saved playlists from the SpClient rootlist protobuf and the user avatar/name from the
  native profile endpoint.
- [x] Rewire `AppState`, the sign-in screen, titlebar, tabbed search, library sidebar, and player
  bar to the native client.
- [x] Keep OAuth, search, library, and profile work off the GPUI foreground thread with worker
  threads and `oneshot` channels.
- [x] Refresh the titlebar after login and keep back/forward state synchronized with the stack
  router; reset the stack with the home button.
- [x] Replace missing Unicode media glyphs with SVG assets and verify previous, pause, and next
  render correctly.
- [x] Document the native endpoints, client id, Pathfinder hashes, Premium requirement, and legal
  caveats in `README.md` and `docs/reverse-engineering.md`.
- [x] Add `AGENTS.md` and this spec so future work follows spec-driven development.
- [x] Reorder modules in `src/view/components/mod.rs` so the existing code passes
  `cargo fmt --check`.

## Validation

- [x] `cargo fmt --check`
- [x] `cargo check`
- [x] `cargo test` (non-ignored tests)
- [x] Real-account login/search/library smoke test:
  `cargo test --bin rspotify_client smoke_native_login_and_search -- --ignored --nocapture`
  - Returned 20 tracks, 20 artists, and 20 albums for the test query.
  - Profile name and avatar resolved.
  - Library returned 108 playlists.
  - Playback snapshot was empty after login, confirming no accidental autoplay.
- [x] Real-account playback smoke test:
  `cargo test --bin rspotify_client smoke_native_playback -- --ignored --nocapture`
  - A test track reached roughly 2.3 seconds of playback and then paused.
  - The playback path was unchanged by later UI-only fixes.
- [x] Manual visual checks of the sign-in screen, authenticated titlebar, library sidebar, search
  tabs, and player bar media controls.

## Follow-ups

### Linux development icon fix

- [x] Inspect live KWin identity: `resourceClass=rspotify`, `desktopFileName=rspotify`.
- [x] Verify Qt can render the installed SVG and a fresh Plasma launcher model resolves it.
- [x] Generate a launcher with an explicit executable and installed bundled-icon path; notify
  KDE's icon loader independently of its service database.
- [x] Test installation in isolated XDG directories and validate the installed desktop entry.
  `python3 scripts/test_dev_desktop_entry.py`: two tests pass (paths with spaces, repeated install,
  missing executable). Installed desktop file passes `desktop-file-validate`.
- [x] Verify the existing KDE taskbar displays the icon (not just the window decoration).
  Captured the running KDE panel after setup: rspotify's active task button shows the green
  Spotify icon. Local evidence: `/tmp/opencode/rspotify-taskbar-check.png`. The temporary test
  app and KWin diagnostic script were closed/unloaded; no login or audio playback was performed.
- [x] `cargo fmt --check`, `cargo check`, `cargo test` (10 passed, 3 account tests ignored).

- [ ] Auto-login using the stored refresh token without an explicit button press (requires a
  separate spec and an explicit user decision).
- [ ] Playlist detail, album/artist pages, queue, and lyrics.
- [ ] Search pagination, debouncing, keyboard focus, and empty-result polish.
- [ ] Shuffle/repeat controls in the player bar.
- [ ] Library artist/album sections and missing-artwork fallback.
- [ ] Offline cache controls, audio-quality settings, and system media keys.
- [ ] Cross-platform packaging and installer work.
