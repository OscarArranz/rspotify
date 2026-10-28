# Session reconnect — design (spec 012)

## Native contract

librespot treats a lost access-point connection as terminal: `Session::shutdown()` marks the
session invalid, the Spirc task's `while !session.is_invalid()` loop ends, and every Spirc command
send fails. librespot does not rebuild the session itself. Its own CLI (`librespot/src/main.rs`)
handles this by creating a new `Session`, calling `player.set_session`, and building a new `Spirc`
when the previous Spirc task ends. rspotify adopts the same pattern.

Reconnect credentials come from `~/.cache/rspotify/session.json`:

- `oauth_client(false).refresh_token_async(stored_refresh)` obtains a fresh access token and
  possibly a rotated refresh token.
- The rotated token is persisted through `SessionStore`.
- A confirmed `invalid_grant`/`invalid_token` clears the token and marks the client signed out.

The librespot `login5` token used by Pathfinder and SpClient is renewed inside librespot on demand;
it is not the cause of the failure and needs no separate handling.

## Module boundaries

| Module | Responsibility |
| --- | --- |
| `src/lib/spotify/client.rs` | Replaceable session connection, supervisor, reconnect, playback freeze/resume, accessors |
| `src/lib/spotify/error.rs` | Retryable `Reconnecting` error |
| `src/view/screens/home.rs` | Real playback failure copy instead of the Premium message |
| `src/view/root_layout.rs` | Signed-out polling and navigation back to `/sign-in` |

`NativeSpotify` stays a cloneable `Arc<NativeInner>` handle. Its identity never changes, so views,
`AppState`, and background workers keep a valid handle across reconnects.

## Connection ownership

```rust
struct SessionConnection {
    session: Session,
    spirc: Spirc,
    alive: Arc<AtomicBool>,
}

struct NativeInner {
    connection: RwLock<Option<SessionConnection>>,
    reconnect_now: Notify,
    signed_out: AtomicBool,
    pending_load: Mutex<Option<LoadRequest>>,
    player: Mutex<Arc<Player>>,
    mixer: Arc<SoftMixer>,
    playback: Arc<Mutex<PlaybackInner>>,
    runtime: Arc<Runtime>,
    username: String,
    liked_songs_detail: Mutex<Option<PlaylistDetail>>,
    liked: Arc<Mutex<LikedCache>>,
}
```

- `connection` is empty while reconnecting. `None` means every command gets the retryable error.
- The `Player` and `SoftMixer` are created once. A reconnect calls `Player::set_session` so the
  audio thread and its player-event loop are reused. If the player thread died (for example after
  an audio-device change), a new `Player` is created and a new event loop is spawned.
- `SessionConnection::alive` stops the previous connect-state polling task before it can overwrite
  playback state from a dead Spirc.
- `pending_load` holds a play/load request issued during reconnection. The latest request wins.

## Startup and reconnect flow

1. `connect` builds the runtime, cache directory, mixer, session, player, playback snapshot and
   Spirc. It then spawns the player-event loop and the session supervisor.
2. The supervisor waits on a 2 s timer or on `reconnect_now` (notified by failed commands). It
   reconnects when any of these is true:
   - the current session is invalid,
   - the current Spirc task has finished,
   - the player thread has finished.
3. Reconnect first freezes playback (see below), removes and invalidates the old connection, stops
   its polling task, aborts its Spirc task and shuts the old session down.
4. It refreshes the OAuth token, rebuilds `Session` and `Spirc`, restores the player session, swaps
   the new connection into `NativeInner`, and records the new Spirc task handle.
5. Failures retry with 2 s → 30 s exponential backoff. A rejected refresh token signs the user out
   and stops the supervisor.
6. After success it applies a queued `pending_load`, otherwise it applies the automatic resume.

`Session::new` runs inside the runtime context. `Spirc::new`'s returned future is spawned on the
same runtime and its `JoinHandle` is kept by the supervisor so a dead task can be aborted and
replaced.

## Playback freeze and resume

The supervisor snapshots playback at `Instant::now()` before destroying the connection:

- `position_ms` is frozen (no further extrapolation), `is_playing` becomes false and
  `local_player_active` is cleared;
- `now_playing`, `context_uri` and volume are preserved so the player bar keeps showing the track;
- if the snapshot was playing and has a track, a `ResumeTarget { context_uri, track_uri,
  position_ms }` is returned.

After the new session is active, `ResumeTarget` becomes a `LoadRequest` with `start_playing`,
`seek_to` and `playing_track` set, then `activate()` + `load()` are sent. A `pending_load` from an
explicit user action replaces the automatic resume.

## Request behavior

- Transport commands (`play`, `pause`, `next`, `previous`, `seek`, volume, shuffle, repeat) never
  block the GPUI thread. If no live connection exists they notify the supervisor and return
  `SpotifyError::Reconnecting`. A failed command send is treated the same way.
- Play and load commands store their `LoadRequest` in `pending_load` when the session is not
  available, or when the send fails, and return the retryable error.
- Worker methods (`search`, library/rootlist/catalog, Home, playlist, artist, profile, liked writes)
  poll for a live connection for up to 8 s, waking the supervisor, then return
  `SpotifyError::Reconnecting`. These already run off the foreground thread.
- Per-entry library hydration is best-effort: with no live connection it reports a failure for the
  batch instead of waiting, so a long reconnect cannot stall the refresh loop entry by entry.
- Liked membership loading and its debounced refresh resolve the session when the request starts;
  if none is live they defer to the next poll.

## UI behavior

- `SpotifyError::Reconnecting` renders as "Spotify is reconnecting. Try again in a moment."
- Home playback failures display the actual error; the Premium sentence is no longer used for
  transport failures. The static Premium note remains on the sign-in screen and player bar.
- `RootLayout` polls `NativeSpotify::is_signed_out()` every 2 s. When true it logs out `AppState`
  and navigates to `/sign-in`, matching the startup invalid-refresh-token path.

## Security and privacy

- Only lifecycle/class messages are logged. Access tokens, refresh tokens, session JSON, track
  titles and account identifiers are never logged.
- The refresh token remains only in the existing private state file; it is never placed in logs or
  specs.

## Validation

- Offline unit tests for the playback freeze/resume state and for the reconnect backoff helper.
- Ignored live smoke test: restore a session, force `session.shutdown()`, wait for the automatic
  reconnect, then verify a read-only Pathfinder request succeeds. Not run in this iteration.
- `cargo fmt --check`, `cargo check`, `cargo test`.
