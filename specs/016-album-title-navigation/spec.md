# Song-title album navigation

## User outcome

The now-playing title links to the song's actual album, single, or EP without starting or
interrupting playback. Playlist and playlist-like detail rows keep song titles as plain text;
the Album column entry links to that song's album, single, or EP instead. Other song-title
surfaces remain non-links.

The destination uses the supplied Spotify reference: prominent cover, release kind/title/artist/year/count/duration, green play
control, ordered song rows with Like controls and durations, and release information below.

## Acceptance criteria

- [x] Only the now-playing song title underlines on hover and navigates to its containing release.
  Playlist song titles remain plain text; their Album column entries underline and navigate to
  the containing release without playback. Other song-title surfaces remain non-links.
- [x] Playbar title and Album column link clicks do not bubble into surrounding playback actions;
  other row/play controls retain playback behavior. Missing album identity in the Album column is
  resolved from native track metadata, never names. The explicit track-menu “Go to album” action
  remains available.
- [x] Albums, singles and EPs load native release metadata and all native context pages in order;
  existing album entry points use the same release-aware detail page.
- [x] Release pages show the correct kind, artwork, title, artists, year, song count and duration
  when available; numbered rows omit redundant artwork/album/date-added columns. Release date
  and copyright text appear below the list when provided. Unavailable play counts are not invented.
- [x] Navigation supports Back/Forward; loading/failure/retry and unavailable metadata degrade
  honestly. Native network work never blocks GPUI foreground rendering.
- [x] Required offline Rust checks and targeted interaction/data checks pass; live-account and
  actual desktop visual checks are recorded separately when not authorized/available.

## Boundaries

Reuse the existing virtualized detail screen rather than introduce a parallel album screen.
No public Web API, new dependencies, DRM changes, downloads, album-save writes, music-video
playback, or fabricated play counts. Spotify Premium remains required for playback.
