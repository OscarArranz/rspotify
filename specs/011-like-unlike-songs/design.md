# Like and unlike songs — design (spec 011)

## Native contract

Liked songs are track memberships in the existing native collection-v2 set `collection`.

- Read: `POST /collection/v2/paging` with `application/vnd.collection-v2.spotify.proto`.
  `collection::snapshot` already merges duplicate URIs, keeps the newest timestamp, and exposes
  tombstones. Membership keeps only items where `!removed` and the URI starts with
  `spotify:track:`.
- Write: `POST /collection/v2/write` with the same vendor-protobuf headers. The existing
  `WriteRequest` encoding is reused:
  - username = 1
  - set = 2 (`collection`)
  - item = 3 (`uri=1`, `added_at=2` int64 Unix seconds, `is_removed=3` bool)
  - client update id = 4 (16 lowercase hex characters)
- Liking sets `is_removed = false`; unliking writes a tombstone with `is_removed = true` and a
  newer timestamp. The spclient bearer token comes from the authenticated librespot session.
- The native client only checks HTTP 200 for this write. rspotify treats that as the write
  acknowledgment, updates local state, then reconciles with a later snapshot.

No per-track membership read endpoint is used. A full `collection` snapshot is the authoritative
membership read, so it is loaded once per session and refreshed after writes instead of on every
render.

## Module boundaries

| Module | Responsibility |
| --- | --- |
| `src/lib/spotify/collection.rs` | Shared membership write, `set_liked`, live-track extraction, serialization tests |
| `src/lib/spotify/client.rs` | Liked membership cache, status/revision API, background load and reconciliation, blocking `set_liked` |
| `src/lib/spotify/playlist.rs` | Pure detail removal helper used by the screen and cache pruning |
| `src/view/components/liked.rs` | Shared plus/check visual used by the player bar and rows |
| `src/view/main_layout.rs` | Now-playing like control and feedback |
| `src/view/screens/playlist.rs` | Liked column, row toggles, Liked Songs pruning and cache updates |

`NativeSpotify` remains the only owner of the authenticated session. Liked membership lives next
to the session inside the client's `Arc<NativeInner>`; it is not global UI state. Screens keep only
per-view pending/optimistic state and poll the client's revision.

## Membership cache

```rust
enum LikedStatus { Unknown, Liked, NotLiked }

struct LikedCache {
    tracks: HashSet<String>,
    loaded: bool,
    loading: bool,
    revision: u64,
    last_attempt: Option<Instant>,
    refresh_generation: u64,
}
```

- `ensure_liked_tracks_loaded()` starts one background snapshot and throttles retries. It never
  blocks the GPUI foreground thread.
- `liked_status(uri)` returns `Unknown` until the first snapshot succeeds; `Unknown` is also
  returned for non-track URIs, and the UI hides those controls.
- Successful writes update `tracks` and increment `revision` immediately.
- After a write, a debounced task waits briefly and performs one authoritative snapshot. A
  generation counter discards stale refreshes; if a newer write happens, that write schedules the
  winning refresh. The refresh only increments `revision` when membership actually changes.
- On a successful unlike, the cached and in-memory Liked Songs detail is pruned through the same
  pure helper used by the screen.

## Player-bar behavior

- The control appears after the title/artist block only for `spotify:track:` URIs.
- Rendering uses the client status plus a per-track optimistic override while a write is pending.
- Busy and unknown states are dimmed and non-interactive. A brief inline caption error replaces
  the artist line for five seconds on failure.
- The existing 250 ms poll also compares `liked_revision()`, so changes from a playlist view
  refresh the icon. The track-info block truncates long titles so the control does not move.

## Playlist-page behavior

- A fixed 28 px liked column is inserted before the duration column; its header is blank.
- Liked rows render a green check circle. Unliked rows render a dim plus inside the row's
  `group_hover` scope. Non-track rows leave the cell empty.
- The click handler calls `cx.stop_propagation()` so the row's play action does not run.
- Writes run on a worker and return through a oneshot. A per-URI pending set prevents duplicate
  writes. A per-URI optimistic override makes the icon respond immediately; failure removes the
  override and uses the existing action-error banner.
- In Liked Songs, the revision poll prunes visible tracks whose authoritative status is
  `NotLiked`. Rows are removed from the list state with descending `splice` calls so the scroll
  position is retained. The detail track count, total duration, added-at map, in-memory snapshot,
  and disk cache are updated in the same path.
- Newly appended Liked Songs pages also drop tracks that the loaded membership says are unliked.

## Failure handling

- Initial membership failure leaves controls disabled and retries at a bounded interval; it never
  guesses a liked state.
- Write failure reverts the optimistic icon, clears the busy state, and surfaces the existing
  playlist error banner or a short player-bar caption.
- Reconciliation failure after a successful write keeps the acknowledged local state; a later
  session load or write burst corrects it.

## Security and privacy

- Only counts and failure classes may be logged. Track titles, URIs, and tokens are never logged.
- The write uses the same authenticated spclient session as the rest of the native client.
- No credentials or account data are added to fixtures.

## Validation

- Offline serialization test for a `collection` tombstone write.
- Pure membership filtering test (albums, local files, tombstones ignored).
- Pure Liked Songs removal test covering duplicate URIs, count and duration updates.
- `cargo fmt --check`, `cargo check`, `cargo test`.
- Optional ignored read-only smoke check that loads the membership snapshot and reports counts
  only. No account mutations run in this iteration.
