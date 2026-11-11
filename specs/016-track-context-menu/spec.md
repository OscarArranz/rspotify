# Track context menu — spec 016

## Outcome

Right-clicking a Spotify track opens a Spotify-style context menu at the pointer, matching the supplied reference: charcoal surface, compact icon/label rows, hover highlighting, grouped navigation actions and nested submenus. Opening or dismissing it must not start playback.

## Requested actions

- Add to playlist: choose an editable playlist and append the clicked track.
- Remove from this playlist: remove the clicked occurrence only when its source is editable.
- Add to / Remove from your Liked Songs: reflect authoritative membership and reuse native collection writes.
- Add to queue: append the track without interrupting the current song or replacing the context.
- Exclude from your taste profile and Start a Jam: retain the pictured entries visibly disabled. The user explicitly approved this boundary because native contracts are not established.
- Sleep timer: bounded minute choices, end of track, cancellation; pause playback when due.
- Go to song radio and artist: navigate using actual Spotify identities. Go to album opens the clicked song's containing album, single or EP through the shared release-aware route, without starting or interrupting playback.
- View credits: display returned song credits, not artists relabeled as full credits.
- Share: copy a canonical song link.

## Acceptance

- [x] Track rows in playlist/context pages, search, artist pages and the now-playing bar share one menu implementation.
- [x] Menu order, icons, separator and submenu arrows implement the reference; available actions are functional. Desktop pixel parity remains unverified.
- [x] Right-click preserves playback; Escape, outside click and choosing an action dismiss the menu. Menus remain within the viewport.
- [x] Like state stays consistent across the player bar and Liked Songs; unknown membership never guesses a destructive action.
- [x] Playlist edits respect permissions, revisions and duplicate occurrence identity; errors are visible and failed writes do not remove rows.
- [x] Queue additions preserve current playback and existing queued order.
- [x] Timer replacement/cancellation cannot fire a superseded timer.
- [x] Native-service actions use established transport/schema evidence. Jam and taste-profile commands are visibly disabled as agreed; no public Web API, invented service requests or fake success. Live mutation validation is separate.
- [x] No foreground network or disk work; no account/network smoke checks without explicit authorization.

## Constraints

Playback requires Spotify Premium. Native service use may violate Spotify's Terms of Service and risk the account. Runtime/account validation is separate from deterministic offline validation. Any native capability that cannot be established must be called out and its treatment agreed before narrowing the requested behavior.
