# Tasks — session reconnect (spec 012)

- [x] Define the connection ownership, reconnect trigger, and playback resume contract.
- [x] Add `SpotifyError::Reconnecting`.
- [x] Make `NativeSpotify` own a replaceable `SessionConnection` and reuse the `Player`.
- [x] Extract the session/Spirc builder shared by startup and reconnect.
- [x] Add the session supervisor with backoff, rotation, and signed-out handling.
- [x] Freeze playback on loss and apply automatic resume or a queued user load.
- [x] Route worker reads through a bounded reconnect wait and transport calls through a live
      connection accessor.
- [x] Replace the Home Premium playback error with the real retryable error.
- [x] Route a revoked session back to `/sign-in`.
- [x] Add offline freeze/resume and backoff tests plus an ignored live reconnect smoke test.
- [x] Update the architecture docs, reverse-engineering notes, and `AGENTS.md` spec list.
- [x] Run `cargo fmt --check`, `cargo check`, and `cargo test`.
- [ ] Run the ignored live reconnect smoke test with explicitly approved account/network access.

## Idle command-channel recovery

- [x] Reject closed Spirc command channels in connection access and supervision.
- [x] Serialize play queuing with replacement publication; retain failed replays.
- [x] Exercise command receiver closure offline and run required Rust checks.
- [x] Record evidence and account/visual validation limits.

## Validation notes

- `cargo fmt --check`, `cargo check`, and `cargo test` pass. New offline tests:
  `freeze_for_reconnect_pauses_and_keeps_resume_target`,
  `freeze_for_reconnect_does_not_resume_paused_playback`,
  `resume_request_uses_context_or_track_and_position`, and
  `reconnect_backoff_doubles_and_caps`.
- The ignored live reconnect test (`smoke_native_session_reconnect`) was added but not run; this
  iteration had no account/network approval. It forces `Session::shutdown()` and verifies that a
  read-only Pathfinder request succeeds after the automatic reconnect.
- Interrupted playback is captured once, before the first rebuild, and kept across retry attempts so
  backoff retries cannot lose the resume position.
- Account read-only checks must still run sequentially because they share the stored refresh token.

### Idle command-channel recovery validation

- `cargo fmt --check`, vendored `cargo fmt --check`, `cargo check`, and `cargo test` pass:
  82 application tests passed, 13 ignored.
- `cargo test -p librespot-connect closed_command_receiver_invalidates_all_spirc_handles -- --nocapture`
  passes. It closes the actual command receiver while retaining it, verifies that all cloned Spirc
  handles become invalid, and confirms activation/load fail without needing a session or account.
- A throwaway offline Rust harness extracted the production `connection_now`, `send_load`,
  `live_connection`, and `resume_playback` methods, using simulated session/transport objects.
  It verified closed-channel rejection, latest queued play precedence over automatic resume,
  retention after failed replay, and delivery to the replacement followed by a new play request.
  This does not prove live Spotify recovery or UI behavior.
- No account access, audio playback, desktop automation, or live reconnect smoke test was run.
- A failed replay stops the unpublished replacement connection's connect-state poller explicitly:
  only published connections can be reached by the next `detach_connection()`.
- `cargo check` reports an existing future-compatibility warning in `proc-macro-error2` 2.0.1.
