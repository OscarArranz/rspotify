# Track context menu — design

## Boundaries

- One `view/components/track_menu` component builds the track popup. The existing `gpui-component` menu hardcodes 26px submenu rows and cannot express the reference's uniform 40px rows; use a scoped GPUI popup rather than vendoring the entire component crate or accepting inconsistent row sizing. Existing library menus remain unchanged. Screens supply immutable clicked-track identity and optional source occurrence.
- `NativeSpotify` owns authenticated operations, revision/cache consistency and shared timer state. New native transport and decoding live in `lib/spotify/track_actions.rs`; playlist loading retains UID/item/revision identity in `playlist.rs`, with indexed matching rather than scanning the whole playlist per row.
- Existing `StackRouterHandle`, `ArtistScreen` and the release-aware `PlaylistScreen` handle navigation. “Go to album” uses the same `track_title::open_track_album` entry point as song-title links: immediately push `PlaylistSource::TrackAlbum` and resolve the native album identity and actual Album/Single/EP kind on the destination's worker, not through a second menu-specific metadata fetch.
- UI entities own pending/error/presentation state. Worker threads send results through oneshot channels and `cx.spawn`; no foreground blocking reads.

## Interaction

Preserve ordinary left-click playback and child artist/Like click propagation. Capture the clicked track in the context-menu builder so a later virtual-list rerender cannot retarget an action. Read current membership and pending status at menu-open time. Source removal requires an editable playlist and stable native occurrence identity; duplicate URIs are not an occurrence key.

Use bundled SVG icons and the reference's charcoal surface, uniform 40px rows, green liked check and 14px labels. The shared popup owns keyboard focus, disabled-action guards, Escape/outside/action dismissal, submenu edge flipping and viewport snapping. Missing metadata is fetched off-thread; never fabricate artist, album or credits metadata.

`RootLayout` owns the persistent `TrackMenuHost` entity; a global handle only locates that entity.
The host owns menu focus, target scope, pending actions, feedback and credits presentation.
Editable playlist destinations load off-thread into a leaf submenu with loading/error/retry states.
Removal refreshes the captured source only after acknowledgment; errors leave its rows intact.

## Native operations

Reuse collection-v2 Like/Unlike and its revision reconciliation. Queue append uses the vendored Connect state machine, preserving current/context/next queue and rejecting inactive/full queues without takeover or item loss. Playlist changes use native playlist4 revision-checked operations and edit capabilities. Shared timer generation/cancellation invalidates superseded tasks; reconnect cancels timers. The one-shot end-of-track flag is consumed inside Spirc after its current-request check, before repeat/advance; the ended song is retained and reloaded paused at its beginning so Play remains usable without consuming the queue.

The user explicitly approved visibly disabled Jam and taste-profile entries. Credits, radio and playlist transport evidence, including the unverified request-specific optional nonce/base-version policy, are recorded in `docs/reverse-engineering.md`. Static evidence is not account-write validation. No direct account traffic or desktop automation was performed.

## Verification

Exercise the actual shared menu with GPUI test-support where no safe account-free desktop run is possible. Cover action targeting, right-click non-playback, dismissal/submenu behavior and uncertain timer/queue/playlist boundaries. Run repository-required `cargo fmt --check`, `cargo check`, `cargo test` after implementation, not during delegated work. Record real results and visual/account limits in tasks.md.
