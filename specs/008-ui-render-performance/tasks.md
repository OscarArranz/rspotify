# UI rendering performance — tasks

- [x] Inspect scroll scheduling and GPUI cache/invalidation behavior.
- [x] Notify scroll owner only; test owner invalidation via actual headless wheel dispatch.
- [x] Add independently sized cache boundaries for library, content, player, and titlebar.
- [x] Test clean sibling reuse, notified child updates, and resize invalidation headlessly.
- [x] Run `cargo fmt --check`, `cargo check`, and `cargo test`.
- [x] Build optimized app.
- [ ] Record manual comparison/remaining profiling needs, including focus/navigation/layout checks.

## Evidence

- User explicitly clarified that the remaining lag is dropped frames across much of the interface.
- Previous testing used default unoptimized `cargo run`. No native frame timings or profiler
  captures have been collected, so neither an FPS baseline nor a speedup is established.
- Formatting, checking, full tests, and `cargo build --release` pass. Full suite: 20 passed,
  3 account/network smoke tests ignored. Existing compiler warnings remain.
- Headless render-counter regression: notifying the root reuses both clean regions; notifying one
  child or scrolling it leaves its sibling's render count unchanged. Resizing redraws both.
- Added opt-in aggregate scroll frame-callback metrics for the next optimized manual test. These
  measure callback spacing (including display pacing), not the time spent rendering on CPU/GPU.
