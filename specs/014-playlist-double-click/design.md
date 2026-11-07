# Double-click playlist playback — design

## Ownership and boundary

The Library entity in `src/view/components/library/mod.rs` owns row interaction and its existing error presentation. `NativeSpotify` owns native playlist detail lookup and Connect playback; the sidebar does not add global state or a second playback implementation.

## Behavior

A library-row click whose GPUI `ClickEvent::click_count()` is 2 and whose entry kind is `Playlist` requests playback of that playlist's first track. The source is built from the row's playlist URI and metadata. On a worker thread, fetch its first native playlist detail page with `NativeSpotify::playlist_detail`; when that page contains a track, call `play_context_at(context_uri, track_uri)`. Send the result back to the library entity for its established error display. An empty first page performs no playback request. The artwork play overlay applies the same double-click distinction because its click handler stops propagation. Other entry kinds and non-double-click row activation retain existing navigation or play behavior.

The first click in the normal row click sequence still opens the detail view; playback is added on the double-click event. Playlist metadata fetching and Connect dispatch remain off the GPUI foreground thread. No public Web API, new dependency, track-list prefetch beyond the existing first page, or playback fallback is introduced.

## Verification

Use deterministic offline interaction/client-path tests where possible, then run `cargo fmt --check`, `cargo check`, and `cargo test`. No real account, network, or desktop automation is implied; record those limits in `tasks.md`.
