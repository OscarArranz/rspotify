# Library sidebar parity — tasks (spec 009)

## Interaction-reference follow-up (2026-10-23)

- [x] Connect to user-started Spotify CDP, retain passive script evidence and identify the native
  LibraryAPI/YourLibraryService boundary without playback or account changes.
- [x] Correct hover affordances, rich collapsed tooltips, artwork play overlays and capability-gated
  context menu layout. Exact screenshot comparison remains pending.
- [x] Implement four view modes and segmented picker, inline compact subtitles, filled search and
  Unicode-safe match highlighting, plus nested ownership filters.
- [x] Put the toolbar into the virtual scroll region; implement real expansion and the 8 px gutter.
- [x] Probe observed native read contracts for independent recency/category support; do not depend
  on the running official app for rspotify's normal operation.
- [x] Re-run offline and user-approved read-only checks, and record remaining parity gaps.
- [x] Bundle the original Regular/Bold body font from the installed package without adding a
  project dependency; retain font provenance under `assets/fonts/README.md`.
- [x] Implement explicit pin/unpin commands with native-format writes, read-back verification,
  pending/error state and protection against stale catalog responses. Do not execute account writes.

- [x] Inspect existing specs/sidebar and identify installed Flatpak reference without account access.
- [x] Define boundaries, appearance target, state ownership and native-service evidence gates.
- [x] Replace playlist-only rootlist decoding with typed entries and folder hierarchy; hydrate owners
  and missing artwork using native metadata; add fixture tests.
- [x] Extract library entity and rebuild header, chips, toolbar, virtualized rows and artwork.
- [x] Implement search, playlist filtering, sort/view selection, folder navigation and explicit
  load/error states. Category failures explain their service limitation instead of masquerading
  as an empty library.
- [x] Implement layout size modes, router-aware selection and playback indicators.
- [x] Investigate/implement native saved albums/artists/shows, bounded native Recents and pin state.
- [ ] Investigate/implement native create/pin/context mutations and drag organization.
- [x] Update README and reverse-engineering notes with actual support and limitations.
- [x] Run cargo fmt --check, cargo check and cargo test; record results.
- [ ] Matched-scale visual comparison (pending; not performed by static analysis).
- [x] User-approved read-only account sample (`smoke_native_library_sidebar`) passed on
  2026-10-22: rootlist identity/hierarchy checks and sampled owner/artwork hydration, with zero
  sampled metadata failures. No account names, library titles or URIs retained here.
- [ ] Native editing validation (not approved or performed; no mutation/playback commands sent).

## Validation evidence (2026-10-22)

- `cargo fmt --check`: passed.
- `cargo check`: passed; existing dead-code/vendor lifetime and future-compatibility warnings remain.
- `cargo test`: passed, 21 tests passed and 4 account/audio tests ignored by default.
- `cargo test --bin rspotify_client smoke_native_library_sidebar -- --ignored --nocapture`:
  passed with the user's explicit read-only approval. The test validates native rootlist structure,
  owner hydration and bounded artwork hydration without sending playback or mutation commands.
- `git diff --check`: passed. Existing untracked `qq` was not touched.
- Added offline tests for folder markers spanning pages, sparse protobuf metadata, owner/picture
  mapping, URI/image normalization, search and stable sorting, Liked Songs visibility, and panel
  constraints during collapse/expand/window resize.
- No desktop automation or application launch was performed. Matched-scale screenshots and UI
  interaction checks are still pending; passing compilation/tests does not establish visual parity.

## Remaining parity gaps

The initial missing-CDP blocker was resolved when the user started the official app with debugging.
Native collection reads, category decoration, Recents and pin state are now verified independently
of the running official app. Pin/unpin write code has offline/static coverage only; account-changing
validation has not been approved/performed. Creation's complete request contract remains unresolved,
so creation, Jam, queue, other editing and drag organization are disabled. Native drag pin rank,
nondefault pin limits, rich mixed/Made-for labels, older-than-50 recency ordering and pixel-level
visual acceptance remain incomplete. Full 1:1 parity must not be claimed.

## Main integration (2026-10-26)

- [x] Restore the normal-view toolbar to the reference layout: the row now fills the virtualized
  list width, so the picker sits at the trailing edge. The resting state is the compact search
  button plus the active sort label and view glyph; opening search shows the focused field with
  its "Search in Your Library" placeholder and clear affordance, and hides the sort label.
  An empty search collapses on blur and Escape clears it.
- [x] Show the library heading glyph at rest and swap it for the collapse glyph on hover, matching
  the reference header.
- [x] Fade overflowing category chips into a solid panel-colored backdrop beneath the overflow
  arrows instead of leaving hard-clipped labels.
- [x] Diagnose the metadata banner: catalog paging, recents, pins and decoration were healthy; one
  playlist returns 403 from `get_playlist`, and a single item-level miss was promoted to the global
  banner. Item-level misses now keep the rootlist fallback instead of raising it.
- [ ] User re-check of the normal-view toolbar at matched scale.

- [x] Merge origin/main without discarding the pending library work or existing untracked `qq`.
- [x] Renumber this pending spec from 006 to 009 because main already uses 006–008.
- [x] Preserve main's playback-clock/track-transition fixes, optimistic authentication, sized
  rendering caches and shared smooth-scroll helpers when resolving the sidebar overlap.
- [x] Re-run formatting, compilation and headless tests after integration; no account checks needed.
  `cargo fmt --check`, `cargo check`, `cargo test` and `git diff --check` pass; 46 tests passed and
  8 account/audio tests stayed ignored.

## Follow-up validation (2026-10-26)

- `cargo fmt --check`, `cargo check`, `cargo test`, `cargo build`, and `git diff --check`: passed.
- Default tests: **30 passed, 8 ignored**. New coverage includes Unicode expansion-safe highlight
  ranges, all four view modes, identity-based ownership filters, native recency decoding, pin
  priority/membership normalization, int64 collection dates, tombstones and offline write encoding.
- `cargo test --bin rspotify_client smoke_native_library -- --ignored --nocapture --test-threads=1`:
  **5 passed** with the user's existing read-only approval. This verified rootlist/profile/artwork,
  native recency timestamps, category/pin collection contracts, batched category decoration and
  native album/artist/podcast context pages. No pin writes, creation, library editing, playback or
  transfer commands were invoked.
- SVG XML, all library icon paths and the bundled TTF SFNT headers were checked successfully.
- Remaining compiler warnings are the existing unused project scaffolding/vendor lifetime warnings.
- No official-client UI actions were automated. A side-by-side rendered comparison of the new GPUI
  build at matched scale is still required before claiming visual 1:1 parity.
