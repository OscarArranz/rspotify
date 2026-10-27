# Tasks

- [x] Define Home outcomes and boundaries before implementation.
- [x] Investigate desktop feed, categories, pagination and Canvas sources.
- [x] Replace the interim library-only grid with native personalized feed models and UI.
- [x] Add sticky category chips, responsive eight-item shortcuts, ordered horizontal shelves,
  hover playback controls, supported featured layouts, detail navigation and full-shelf paging.
- [x] Keep Home state on root navigation; reject stale category/shelf responses.
- [x] Add video-backed recommendation panels with bounded muted Canvas decoding and artwork fallback.
- [x] Add synthetic parser/security tests and headless GPUI scroll/resize coverage.
- [x] Run `cargo fmt --check`, `cargo check`, and `cargo test`: 54 passed, 11 ignored.
- [x] With explicit read-only account permission, run `smoke_native_home_schema`:
  feed, all returned categories and full-shelf lookup passed. The sampled full shelf
  had no subsequent page; further page requests are supported but not account-verified.
- [x] Run `smoke_native_home_canvas_decode`: 48 decoded frames, no audio/desktop playback.
  Run account tests sequentially to avoid refresh-token rotation races.
- [x] Update README, design and endpoint evidence.
- [ ] Validate real-desktop visual parity at different window/sidebar sizes.
- [ ] Validate playback actions (starts audio; not covered by read-only permission).
- [ ] Complete separate announcement banner/service integration and feedback/save menus.

## Boundaries and remaining gaps

The original library-only interim grid is removed. No recommendations or account-specific
names are hardcoded. Main Home and full-shelf data come from the observed native queries.
Albums/artists/podcasts reuse native context pages rather than new dedicated entity layouts.
When no baseline panels are returned, the screen previews up to three actual returned
album/playlist recommendations; this is an explicit presentation choice, not an observation
that Spotify always chooses those same contexts.

FFmpeg on PATH is optional. Downloads are HTTPS CDN-only and size/time bounded; decoding
uses local bytes, no network protocols, at most 48 BGRA frames at 256×320. Frames animate
only while the panel is hovered/rendered. No protected manifest IDs or audio previews
are played. Decoder absence/failure leaves artwork and an unavailable message.

Promotions with supported feed content use a featured layout; the screenshot's separate
announcement banner and its interested/not-interested/hide-announcements actions remain
unimplemented, as do save/feedback writes. Exact parity is not claimed.
