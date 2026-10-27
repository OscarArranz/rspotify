# Library sidebar parity — design (spec 009)

## Reference and evidence

Follow-up runtime discovery: `ev_44cc6297659070266cdb3fd9e6c5beffb5e880f58932f2e26bcb7a1cd0d3d1d5`;
passive script capture `ev_4115702056b17bc78c41cc68338e5d12c799dff82b84ea5f4249f9f1a1e6cfaa`;
verified script export `ev_f883d8bdd7109eeac96b49c75f20eb6f1cccf18716bacd8b95ed95b8d138f75e`.
The selected target is Spotify 1.2.95.453 at loopback CDP port 9222. Supplemental caller-authored
read-only CDP probes recover snapshot module source and compute styles; no credentials/storage
values or account identifiers are retained in repository evidence. The official LibraryAPI calls
the native Esperanto YourLibraryService (local aggregation), so its read results are reference
evidence, not a standalone SpClient endpoint to copy or a runtime dependency for rspotify.

Image 1 is authoritative for appearance. Installed `xpui.spa` has SHA-256
`7ce05c0001be22c7b5124d9dd59dd1ab1e0c188640967836b996f0d32a62f74e`.
REA inspection `ev_594535c909364f9da23da669ddec0ce21afef8e706ee5fad149ad60b6390e970`
establishes package identity/inventory only. Read-only inspection of
`xpui-routes-your-library-x.{js,css}` establishes static support for list/compact/grid views,
folders, pinning, creation, drag organization and collapse/expand. The bundle delegates service
calls to shared modules; request contracts and runtime availability remain separate questions.

## Module boundaries and ownership

- `src/lib/spotify/library.rs`: native rootlist decoding, folder hierarchy, entry model, metadata
  hydration, cover fallbacks, pure filtering/sorting and tests. Preserve rootlist order rather than
  calling it Recents without server recency metadata.
- `NativeSpotify`: blocking worker-only library methods using the existing session/runtime.
- `src/lib/spotify/collection.rs`: native collection-v2 paging and membership-write wire codecs,
  forward-compatible unknown-field handling and read-after-write pin verification.
- `src/lib/spotify/library_catalog.rs`: independent saved category membership and batched
  ALBUM_V4 / ARTIST_V4 / SHOW_V4 decoration; merge actual pin state into the row model.
- `src/view/components/library/`: dedicated GPUI library entity; owns query, category, view,
  selection, open folder, menus, load status, virtual list and playback snapshot.
- `MainLayout`: owns shared resizable panel state and handles library size-mode events.
- Main integration retains spec 001's optimistic client attachment and spec 008's independent
  cached library/router/player regions. The existing sidebar receives the authenticated handle
  without recreating its inputs or losing its size-event subscription.
- Library scrolling and horizontal chips reuse spec 007/008's owner-scoped smoothing; list resets
  cancel in-flight motion and preserve the shared scroll helper's click/input behavior.
- Toolbar geometry: the search field and view/sort picker live in the first virtualized list row so
  they scroll away with the rows. That row must declare `w_full()` explicitly; a list row is laid
  out at its intrinsic width otherwise, which collapses `flex_1` children and strands the picker
  next to the search button instead of the panel edge.
- Toolbar states: the resting normal layout shows the compact search button at the leading edge and
  the active sort label plus the current view glyph at the trailing edge. Opening search focuses the
  full field (magnifier, placeholder, clear affordance) up to 300 px wide and hides the sort label;
  the expanded layout keeps the label beside the field. An empty search collapses again on blur,
  and Escape clears and collapses it.
- Chip overflow controls paint a solid panel-colored backdrop plus a gradient under the arrow, so a
  clipped chip fades into the panel instead of showing cut text beside the button.
- Failure policy: item-level hydration misses (403/404/timeout for one playlist's cover or owner)
  keep the rootlist name, owner and `meta_items` artwork and must not raise a panel-wide error.
  Only catalog-level failures (collection sets, pins, recents, decoration batches) show the retry
  banner. `RSPOTIFY_LIBRARY_DIAGNOSTICS=1` emits failure counts and classes without titles or URIs.
- Router: retains source identity alongside history so selection follows back/forward navigation.

All network and hydration work stays on workers. Results return via oneshot/mpsc; refreshes use
generation guards. Item-level metadata misses leave the row usable with its rootlist fallback, and
catalog-level failures add an explicit partial-data warning.
No credentials, account identifiers or library titles are added to checked-in fixtures or diagnostic logs.

## Presentation

Use logical GPUI pixels (not scaled screenshot pixels), explicit 16/14 px row text, 48 px artwork,
64 px normal rows, 32 px compact rows with inline type, 8 px row inset and 12 px artwork/text gap. The panel is
`#191919`, with an 8 px radius. Header uses bold white text. Filter chips never wrap: a horizontal
scroll handle and edge navigation expose hidden chips. Search opens an actual input; sort/view
choices share a popup with radio sort rows and four segmented view buttons. Liked Songs uses the purple-to-mint gradient and SVG heart.
Rows truncate inside a flex child with `min_w(0)` so long titles cannot push indicators out.

The actual body family observed in the official runtime is `SpotifyMixUI`. Regular and Bold
WOFF2 assets from the installed package were losslessly decompressed using the installed FreeType
library, verified with `fc-scan`, and bundled as TTF under their family `Spotify Mix UI`. This avoids
the previous title-font/fallback approximation. Toolbar height is 48 px. Normal sidebar width is 308 px,
collapsed width 72 px and an 8 px inter-panel gutter. Expanded mode hides the main routed content
without destroying its state and gives the library the available width. The collapsed rail retains
its add action and restores the previous uncollapsed mode.

One virtual list contains the toolbar followed by Liked Songs and typed library entries; only the
heading/chips stay fixed. Liked Songs is a fixed first row: it leads every sort order, survives
reverse sort and account pin state, and remains visible under the Playlists filter regardless of
the ownership chip. Sort mode reorders only the entries below it. Switching
filters/search/sort/folders
rebuilds visible identities and resets scroll; playback updates redraw without resetting scroll.
Images use rounded squares and artists use circles. Expanded and collapsed modes share
the same state and entries; normal width is retained when temporarily collapsed/expanded.

Search uses a single filled input with leading search icon and clear affordance. Pure byte-range
matching produces UTF-8-safe highlighted text runs. Owner username is retained separately from
display name so By you / By Spotify filtering never depends on a decorated name. Grid columns
derive from a 144 px minimum cell width; compact grid hides labels while regular grid adds them.

## Service gates

Use decorated rootlist owner fields, folder start/end markers and playlist metadata/track covers.
Rootlist pages are timeout-bounded, reject repeated pages and reject inconsistent revisions.
Send entries before hydration, then hydrate one entry at a time with a per-refresh owner-name
cache. Missing covers inspect at most eight native tracks and retain up to four distinct album
covers. Per-entry/profile timeouts retain usable fallback data and report partial metadata failures.

Saved albums are direct album memberships from the native `collection` set, not albums inferred
from liked tracks. Followed artists, saved podcasts and pins use `artist`, `show` and `ylpin` sets.
Paging uses POST `/collection/v2/paging`, 300 items per page, vendor-protobuf headers and i64 dates.
Recents uses GET `/recently-played/v3/recently-played` (limit 50) and its protobuf context timestamps.
Rootlist rows appear first; categories are fetched and decorated in bounded batches on the worker,
then merged while preserving the visible scroll anchor. Membership tombstones/deduplication and
repeated pagination tokens are handled explicitly.

Pin/unpin runs only from an explicit context-menu click. POST `/collection/v2/write` carries one
ylpin item with is_removed, Unix-second timestamp and native-format 16-lowercase-hex update ID.
The write acknowledgment is followed by bounded membership reads; UI pin state changes only after
verification. Failures remain visible and ask for refresh when the outcome is uncertain. A pin
epoch prevents older in-flight category snapshots from overwriting newer pin state. The installed
native default is four pins; its configurable maximum and native pin rank are not fully reproduced.
The protocol is covered statically and by offline codecs, but account writes have not been tested.

Native read contracts and schema evidence are recorded in `docs/reverse-engineering.md` with
artifact identity and disassembly locations. Pin writes are never invoked by ignored read-only tests.
Do not invent endpoints, persisted-query hashes, recency, mixed-playlist labels or pin status.
Where account-wide service contracts cannot be verified from available artifacts, expose a clear
unavailable explanation and record remaining work rather than substituting public APIs or local
fake account mutations. Full parity remains open until those gates and visual checks are complete.

## Validation

Pure tests cover sparse/malformed rootlist records, nested folders spanning pages, owner/artwork
mapping, case-insensitive search, folder filtering, stable sorting and artwork normalization. Run cargo fmt
--check, cargo check, cargo test. No ignored account smoke tests without explicit approval.
Visual acceptance requires matching scale, panel width and reference state and is not implied by
a successful build.
