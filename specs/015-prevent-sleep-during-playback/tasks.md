# Tasks — prevent sleep during playback (spec 015)

- [x] Add Linux `systemd-inhibit --what=sleep` adapter; no display inhibitor is requested.
- [x] Drive inhibitor lifetime from `Playing`, `Paused`, `Stopped`, and `Unavailable` state in the player event loop. Resource drop releases the inhibitor if the event loop exits.
- [x] Playback state transition behavior uses the production event handler; loading, pause, stopped, and unavailable transitions are covered by existing offline player-state tests.
- [x] Run `cargo fmt --check`, `cargo check`, and `cargo test`.
- [x] Record platform validation limits.

## Validation notes

- `cargo fmt --check`, `cargo check`, and `cargo test` pass; 84 tests passed, 13 ignored.
- A two-second `systemd-inhibit --what=sleep --who=rspotify --why='Audio playback' --mode=block sleep infinity` smoke check ran until the intentional timeout (exit 124), confirming the host's systemd inhibitor invocation can acquire and hold the sleep lock.
- No full desktop playback scenario was run. Unsupported platforms currently use a no-op adapter; Linux systems without `systemd-inhibit` continue playback without inhibition. Manual suspend/display behavior remains unverified.
