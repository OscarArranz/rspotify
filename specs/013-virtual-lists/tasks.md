# Virtual lists — tasks

- [x] Fetch latest main and create `feat/virtual-lists` from `origin/main`.
- [x] Inventory scrolling surfaces and document eligible collections and unchanged small collections.
- [x] Implement bounded horizontal card virtualization.
- [x] Migrate Home vertical feed and expanded grids.
- [x] Migrate Search vertical sections and rows.
- [x] Flatten eligible Artist inner collections and migrate long horizontal shelves.
- [x] Retain existing Playlist and Library virtualization; migrate the Library chip helper call to the horizontal-only smoothing API.
- [x] Preserve nested carousel input ownership and edge chaining under virtual-list overlays.
- [x] Run offline rendering scenarios, formatting, compilation and tests; record exact evidence and limitations.
- [x] Update README and the smooth-scroll design; remove the standalone smoke source after running it.

## Evidence

- `cargo fmt --check`: passed.
- `cargo check`: passed. Only the existing dependency future-incompatibility notice for `proc-macro-error2` remains.
- `cargo test`: 82 passed, 13 ignored, 0 failed.
- Actual HomeScreen headless fixtures exercise a 120-shelf/80-panel feed, a 2,000-card expanded shelf, and a 500-card horizontal shelf. Assertions cover bounded construction, distant content, final-row geometry, width-dependent reflow retaining card 800, and shrinking the expanded data to 25 cards.
- Artist's production row model and concert renderer are exercised with 10,000 concerts, distant scrolling, resizing and shrinking data. Row-model checks cover expansion/collapse, grid columns and semantic anchor remapping.
- Horizontal element scenarios exercise 1,000 cards, distant clicks, ancestor clipping, final-card alignment, resize, empty data and shrinking data.
- The nested-carousel scenario initially failed after migrating its parent to a real ListState. It now passes for eased and native horizontal surfaces, including edge chaining and scrolling the carousel offscreen without stale bounds stealing input.
- A standalone `cargo run --example virtual_list_smoke` ran the production horizontal element in a headless GPUI window with 10,000 cards. Initial construction: 5 cards, 1,099,740px scroll extent. Card 731 was reached and clicked. After resizing to 650px, final card 9,999 was reachable with 9 cards constructed. Temporary example source removed afterward.
- Existing cache-boundary and virtual playlist wheel/click regression scenarios pass. Playlist/library list implementations and native pagination were not changed.

## Validation limits

No account/network smoke tests, playback or other-application automation was run. Full offline ArtistScreen and SearchScreen rendering requires an authenticated NativeSpotify handle; their model behavior and shared list mechanics are covered, but authenticated desktop visuals were not compared. Headless layout/interaction checks do not establish pixel-for-pixel desktop parity or a measured release FPS improvement. Those remain manual checks, not claimed results.
