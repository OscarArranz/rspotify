# Smooth scrolling — tasks

- [x] Confirm the GPUI scroll-handle and wheel-event APIs used by the shared implementation.
- [x] Implement a reusable non-blocking smooth-scroll helper for vertical and horizontal divs.
- [x] Apply it to library and search scroll surfaces.
- [x] Apply compatible easing to the virtualized playlist list without disrupting pagination.
- [x] Run `cargo fmt --check`, `cargo check`, and `cargo test` (all passed; pre-existing compiler
  warnings remain).
- [ ] Record visual interaction validation; pending because no running GUI was used.

## Correction after user feedback

The first implementation passed compilation but was not visually validated and the user reports
no improvement. Pixel events were skipped, div input was handled after the native jump, and list
animation rewound already-clamped movement. Previous completion marks do not establish acceptance.

- [x] Replace rewind-after-native-scroll with owned offsets for both pixel and line input.
- [x] Share accumulation, reversal, clamping, frame timing, and cancellation across all surfaces.
- [x] Preserve playlist pagination callbacks using actual visible rows and cancel animation on
  programmatic resets. Real-account pagination remains untested in this change.
- [x] Add regression tests and rerun validation.

### Correction validation

- `cargo fmt --check`, `cargo check`, `cargo test`, and `cargo build` pass. Existing compiler
  warnings remain. Full suite: 19 passed, 3 ignored (account/network smoke tests).
- Six scroll regression tests cover pixel/line dispatch for both axes, no native offset jump,
  nested carousel boundary chaining, list overlay input dispatch and track clicks, incremental
  list movement with distant rows unrendered, accumulation, reversal, bounds, programmatic offset
  changes, and frame-rate-independent easing/settling.
- GPUI dispatch tests use the headless test platform. Easing time is tested deterministically;
  this platform does not deliver native display-frame callbacks. Running-app feel and real display
  frame cadence are NOT visually verified. No desktop automation or account access was performed.
- The built app is `target/debug/rspotify_client` in `worktree-smooth-scroll`. Existing desktop
  launchers or binaries from other worktrees do not include this branch's changes.
- First manual `cargo run` after this correction attempt exposed a GPUI frame-scheduling panic.
  Replaced the invalid `request_animation_frame()` call from a post-frame callback with
  `window.refresh()`; the application must be relaunched to verify this runtime fix.
- User visual feedback: scrolling is now perceptible but still laggy. Reduced the response time
  constant from 45 ms to 18 ms and added a ~100 ms settling regression assertion.
- Subsequent user feedback clarified that remaining lag is dropped frames across much of the UI,
  not just the easing response. Spec 008 now owns the broader rendering-performance work.
