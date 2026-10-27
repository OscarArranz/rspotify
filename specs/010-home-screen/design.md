# Design

Investigate the installed desktop artifact before choosing a feed endpoint or video protocol; distinguish static observations from account-validated behavior. Use login5 and existing native service patterns, without public Web API or DRM bypass. Normalize feed data at the Spotify boundary and keep request and presentation state in a GPUI entity. Reuse existing playlist navigation, playback, artwork and scrolling components where possible. Treat unknown feed entries as optional and use artwork fallback when previews cannot be played. Do not expose unsupported actions as active controls.

The initial interim library shortcut grid has been removed in favor of the native
personalized feed described below.

## Feed-backed implementation

`spotify/home.rs` owns the desktop `home` persisted query and normalized chips,
shortcuts and shelves. The main query was recovered from UTF-16 source in the
installed V8 snapshot; read-only account validation confirmed its response shape.
Use server section order and labels, never synthetic personalized recommendations.
The screen moves to `view/screens/home.rs`, with request generations preventing
stale category responses and entity-owned scroll handles. Keep category chips
outside the vertical scroller. Horizontal shelves use the existing smooth-scroll
component, hover playback and a full shelf view fetched with `homeSection`, with
explicit Load more pagination. Playlist cards open existing detail screens;
album/artist/podcast cards reuse the native context page. Track/episode items can play directly.

Canvas previews are separate from protected music-video playback. The observed
`feedBaselineLookup` operation accepts `uris` and returns `previewItems`, whose
tracks may carry `canvas.url`, `canvas.thumbnail`, and audio previews. Investigate
and validate that response before presenting video controls. Unsupported DRM
manifest IDs must not be treated as directly playable videos. Optional local
FFmpeg decoding renders muted, bounded Canvas frames without adding a required
system dependency; missing decoder or media falls back to artwork. Decode/network
work runs off-thread. A hover renders a GPUI animated image; leaving the card or
switching category stops rendering those frames. Per-category generation checks
discard stale decodes, and only three bounded panel animations are retained.
No audio preview mixing is implemented. Without baseline sections, select up to
three actual returned album/playlist recommendations for panels; never invent content.

Supported spotlight/promotion content renders only when supplied by the service.
Do not replace absent announcements with hardcoded advertisements. Saving or
feedback mutations remain disabled until their native protocols are established.
Separate announcement-service integration and its large banner/feedback menus are
still outstanding; a featured shelf is not claimed to be equivalent to that banner.
