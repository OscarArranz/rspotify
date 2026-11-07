# Double-click playlist playback

## Goal

Let users start a playlist from its first track by double-clicking its row in the library sidebar, without removing the existing single-click navigation behavior.

## Acceptance criteria

- [x] Double-clicking a playlist row starts playback at that playlist's first track, using its native Spotify context.
- [x] Single-clicking a playlist row continues to open playlist detail; double-click behavior is limited to playlist entries.
- [x] Playlist detail lookup and playback dispatch do not block the GPUI foreground thread.
- [x] Empty playlists do not issue a playback request; lookup/playback failures use the library's existing visible error state.
- [x] Offline verification exercises the click distinction and first-track request path; formatting, compilation and tests pass. Real-account and desktop visual checks are recorded separately.
