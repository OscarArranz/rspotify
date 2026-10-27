# Like and unlike songs (spec 011)

## Goal

Let the user add or remove a song from Liked Songs from the now-playing bar and from playlist or
context detail pages, using Spotify's native collection service rather than the public Web API.

## User-visible scope

- The now-playing bar shows a like control next to the current track:
  - an outlined plus when the song is not in Liked Songs,
  - a green circle with a white check when it is,
  - no control for episodes, local files, or when nothing is playing.
- Playlist and context detail rows show a liked column before the duration:
  - a green check for liked songs,
  - a plus revealed on row hover for unliked songs,
  - no control for non-track rows.
- Clicking either control toggles membership without starting playback.
- Unliking a song from the Liked Songs page removes it from the list after the write succeeds and
  keeps the virtualized list, track count, total duration, and cached page consistent.
- A like or unlike made in one view appears in the other without reopening the page.
- Write failures leave the previous state in place and report an error in the visible page or the
  now-playing track caption.

## Acceptance criteria

1. The native `collection` set is the membership source for liked tracks. Albums, shows, local
   files, and tombstones are not treated as liked songs.
2. Membership is loaded once per session in the background and exposed as an explicit
   `Unknown`, `Liked`, or `NotLiked` state. Controls stay disabled while membership is unknown.
3. A successful write updates the local membership cache and every open view. A debounced
   authoritative read reconciles the cache after a burst of writes.
4. A failed write reverts any optimistic state and reports an error. It never reports success
   for a request that did not complete.
5. Liked Songs row removal updates the detail model and cache, and preserves scroll position
   using the virtualized list's splice path rather than a full reset.
6. The player-bar control is hidden for unsupported item kinds and exposes a busy state while a
   write is in flight.
7. No access tokens, refresh tokens, session file contents, or account titles/URIs are logged.
8. Offline tests cover write encoding, liked-track filtering, and Liked Songs removal.
9. `cargo fmt --check`, `cargo check`, and `cargo test` pass.
10. Real-account write validation and a live visual check are recorded separately and only run
    with explicit approval. This iteration explicitly excludes live account writes.

## Boundaries

- No playlist editing, queue changes, download state, "added from" attribution, or library
  sidebar redesign.
- No public Web API calls. Playback remains the existing librespot Connect path and still
  requires Spotify Premium.
- No optimistic success for a failed write, and no fabricated liked membership before the
  native snapshot has loaded.
- Live like/unlike writes are not performed during this iteration's validation.

## Status

Implementation complete in code; offline validation pending. Backend, player-bar, and
playlist-page controls are implemented. Live account-write and live visual validation remain
pending and excluded from this iteration.
