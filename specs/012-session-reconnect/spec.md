# Session reconnect (spec 012)

## Goal

Keep an open rspotify window usable after the librespot connection is lost, instead of requiring a
restart. When the network connection is reset, the machine suspends, or Spotify closes the idle
connection, rspotify must re-establish the native session in the background and resume the
playback that was interrupted.

## Problem

librespot invalidates its `Session` when the access-point connection is lost or the keep-alive
handshake times out. Its `Spirc` task then exits and closes its command channel. A later play
action fails with "channel closed", but the Home screen reports every play failure as
"Spotify Premium is required", which is misleading. Closing and reopening the app works because
startup refreshes the OAuth token and builds a new session.

The OAuth access token obtained at startup is never refreshed while the app remains open, so even a
manual reconnect would need the stored refresh token again.

## User-visible scope

- After the connection drops, rspotify reconnects automatically without opening the browser or
  losing the window, library, search results, or current page.
- Track, context, position and volume survive the reconnect. If audio was playing when the
  connection dropped, playback resumes from the last known position once the session is ready.
- A play action issued while a reconnect is in progress is queued and starts once the new session
  is active.
- Network actions that fail during a reconnect report that Spotify is reconnecting instead of a
  Premium error. Requests that can wait briefly wait for the reconnect before failing.
- If Spotify confirms that the stored refresh token is no longer valid, the saved token is cleared
  and the app returns to the sign-in screen.
- The genuine Premium requirement remains documented and visible; it is no longer used as the
  catch-all playback error.

## Acceptance criteria

1. Loss of the librespot session while the app is open triggers an automatic reconnect using the
   stored OAuth refresh token. The user is not asked to sign in for transient network failures.
2. A rotated refresh token returned by the token exchange replaces the stored token. No access
   token, refresh token, or session file content is logged.
3. The stable `NativeSpotify` handle held by every view remains valid across reconnects; views do
   not need to be recreated and `AppState` keeps the same client.
4. A new `Session`, `Spirc`, and (when required) `Player` are built after the old session is shut
   down. Stale polling tasks stop writing to the shared playback snapshot.
5. If the session was playing when it was lost, the same context (or track) and position are
   loaded and playback resumes after reconnect. If playback was paused, it stays paused.
6. An explicit play request made during a reconnect takes precedence over the automatic resume and
   is applied once connected.
7. Playback commands issued while no session is available return a retryable "reconnecting" error;
   worker reads wait a bounded time for the reconnect, then return the same retryable error. The
   best-effort per-entry library hydration skips instead of stalling the refresh loop.
8. A confirmed invalid-grant response clears the saved refresh token and routes the app to
   `/sign-in`. Transient OAuth/network failures back off and retry.
9. Existing offline tests keep passing, and new offline tests cover the playback freeze/resume
   state and reconnect backoff. An ignored live smoke test exercises a forced session shutdown and
   is not part of the default suite.
10. `cargo fmt --check`, `cargo check`, and `cargo test` pass.

## Boundaries

- No new network endpoints, public Web API usage, dependencies, or DRM changes.
- No automatic sign-in flow. A revoked session requires the normal interactive sign-in.
- Startup restore failures are unchanged by this spec; the reconnect supervisor only runs while a
  client exists.
- Playback still requires Spotify Premium.

## Status

Implemented; offline validation recorded in `tasks.md`. The ignored live reconnect test is added
but was not executed (no account/network approval for this iteration).
