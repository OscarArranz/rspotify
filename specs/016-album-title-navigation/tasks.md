# Song-title album navigation — tasks

- [x] Fetch origin/main, compare local main, and create feat/album-detail-navigation from the latest main.
- [x] Inspect existing native detail/context loading, metadata, artist links and UI references; specify reuse boundaries.
- [x] Add native track-to-release resolution, authoritative album metadata, cache-compatible album identity and release details.
- [x] Add shared hover-underlined title navigation across song-title surfaces without bubbling
  playback clicks.
- [x] Adapt the existing detail page for album/single/EP metadata, ordered rows and release footer; retain playlist behavior.
- [x] Run targeted offline behavior proof and required cargo fmt --check, cargo check, cargo test; record real results.

## Evidence

- Latest fetched origin/main and local main both 8c61eb6; starting worktree clean.
- Temporary offline GPUI smoke passed: actual album detail rendering, final-page duration,
  footer placement after ten paginated songs, release-order sorting, duplicate-route suppression,
  title-click destination and Back/Forward. The fixture used an unauthenticated native session,
  isolated state storage and no network/audio. Temporary test/client scaffolds were removed.
- The smoke exposed and fixed a render-time ListState RefCell borrow; header rendering no longer
  queries the ListState while its item renderer holds a mutable borrow.
- Permanent regressions cover title-versus-row click propagation, non-song behavior, canonical
  album identity, old caches, release kind/date precision and native context ordering.
- Final `cargo fmt --check` and `cargo check` passed. `cargo test` passed: 90 tests, 13 ignored;
  test state was isolated with `RSPOTIFY_STATE_DIR` in a temporary directory.
- Cargo reports the existing future-incompatibility warning for dependency proc-macro-error2 2.0.1.
- Live Spotify/account requests, playback and actual desktop automation have not been authorized.

## Hover correction

- [x] User reports title navigation works but the hover underline is invisible.
- [x] Confirm latest main remains 8c61eb6 and preserve the existing feature work.
- [x] Identify GPUI's paint-time hover styling versus layout-time text shaping; use the
  existing artist-link border pattern instead of a hover-only text decoration.
- [x] Fix the shared title hover indicator and exercise hover/click interaction.
- [x] Run required Rust validation and record visual limitations.

### Hover-fix evidence

- The shared title now reserves a transparent 1 px bottom border, painted white on hover.
  Album routing and click propagation are unchanged.
- The GPUI interaction regression exercises hover enter/leave, stable rendered title bounds,
  title clicks versus surrounding row clicks, and non-song behavior.
- `cargo fmt --check`, `cargo check`, and `cargo test` passed: 90 passed, 13 ignored.
- No pixel screenshot or desktop visual check was performed; isolated display tooling is
  unavailable. Headless validation covers hover geometry and event dispatch, not pixel appearance.
## Playlist navigation correction

- [x] Restrict hover-underlined song-title navigation to the now-playing bar.
- [x] Make playlist Album column entries release links, using native track metadata when album
  identity is missing.
- [x] Keep song titles and surrounding row actions unchanged elsewhere; retain explicit track-menu
  “Go to album.”
- [x] Add targeted interaction coverage; run and record the required Rust validation.

### Correction evidence

- LSP references show the only production `player_track_title` call is in the now-playing bar.
  Playlist song titles and other non-playbar song titles are plain text.
- Album-column interaction coverage checks known album links, metadata fallback, non-song entries,
  row click propagation, and stable hover bounds.
- `cargo fmt --check`, `cargo check`, and `cargo test` passed: 108 passed, 13 ignored.
- Cargo reports the existing future-incompatibility warning for dependency proc-macro-error2 2.0.1.
- No live-account or desktop visual check was performed.
