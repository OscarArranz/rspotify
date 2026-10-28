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
