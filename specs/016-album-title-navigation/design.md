# Song-title album navigation — design

## Data and ownership

`playlist.rs` retains the shared detail model and native context pagination. Add
`PlaylistSource::TrackAlbum { track_uri: String }` for song-title links with no known album URI.
On the existing background loader, resolve that URI with native track metadata, then load the
canonical album through native album metadata plus context resolution. Decode the
`Album::request` protobuf directly: `Album::get` fills absent month/day with January 1 and
would invent release-date precision. Existing `PlaylistSource::Context` album URIs take the
same loading path, ignoring stale previews in favor of authoritative release metadata.

`PlaylistDetail` gains a serde-defaulted `album: Option<AlbumMetadata>`:
`kind: String` (Album/Single/EP), `release_year: Option<i32>`, `release_date: String`,
`artist_credits: Vec<ArtistCredit>`, and `copyrights: Vec<String>`.
`PlaylistTrack` and its native hydration gain serde-defaulted `album_uri: Option<String>`.
Missing identity is not inferred from title, artwork, or playback context. Old caches remain
readable. TrackAlbum caches are account/source-scoped like existing detail caches.

## Navigation and links

A shared `track_title(title: &str, track_uri: &str, album_uri: Option<&str>)` component renders
an inline-width, truncated, underlined-on-hover title for canonical song URIs only. It inherits
font/color from its parent, uses a pointer cursor, and stops click propagation. Known album URIs
push a Context source; otherwise TrackAlbum resolves on the destination's worker. Repeated
clicks on the same current source do not push duplicate pages. Use the global authenticated
client and StackRouterHandle, matching artist-credit navigation. Links do not issue play commands.
Use a reserved transparent 1 px bottom border that becomes white on hover, matching the
artist-credit link pattern. GPUI 0.2.2 applies hover styles at paint time, after text runs
are shaped; a hover-only `.underline()` therefore does not add a visible text decoration.
The border is painted independently of text shaping and does not change hover-time geometry.

Apply this component in the now-playing bar, shared detail rows, search song rows/top result,
artist track rows, and Home song card titles. Parent row/card/play-control handlers remain.

## Shared screen

`PlaylistScreen` remains the owner of loading/error/retry, playback polling, Like state,
pagination, search/sort, ListState and scroll animation. Release mode is identified from the
source (including TrackAlbum) before load, then populated from detail.album. It renders a
release-aware hero, artist credits and year. Album rows omit per-track cover, album and added-at
columns; duration/Like remain aligned. Keep one variable-height virtualized list with a final
release-information item. Album sorting only exposes release order and title, and search/error
wording says album rather than playlist. Sum durations only once all tracks are loaded; do not
present a partial total as the full release duration. No empty/fabricated Plays column.
Release titles wrap and use a 72 px maximum size rather than clipping long album names.
The item renderer never reads ListState geometry while GPUI holds its mutable render borrow.

## Verification

Offline protobuf/metadata fixtures cover release kind, canonical album identity and ordered
tracks. GPUI headless interaction verifies title clicks consume events while row clicks remain
playback targets; destination rendering and source selection are exercised without credentials.
Run cargo fmt --check, cargo check and cargo test after integration. Live native requests and
actual desktop reference parity require separately authorized account/desktop access.
