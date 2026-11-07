# Double-click playlist playback — tasks

- [x] Trace sidebar row activation, playlist detail loading, and native playback APIs.
- [x] Add playlist-only double-click playback from the first track while preserving single-click navigation.
- [x] Add offline regression coverage for the double-click dispatch and empty-playlist behavior.
- [x] Run formatting, compilation, and the full test suite; record exact evidence and limits.

## Evidence

- Offline tests in `view::components::library::tests` pass: `only_playlist_double_click_requests_first_track_playback` covers playlist-only and double-click gating; `first_track_request_uses_context_and_first_track_uri` verifies the exact context/first-track selection and empty-playlist no-op.
- `cargo fmt --check`: passed.
- `cargo check`: passed.
- `cargo test`: 84 passed, 13 ignored, 0 failed.
- Double-click lookup and `play_context_at` run in a worker thread; error results return to the Library entity's visible error state.

## Validation limits

No real-account/network smoke test or desktop visual comparison was run. Headless unit coverage verifies request selection and click gating, not actual GPUI pointer timing or rendered appearance.
