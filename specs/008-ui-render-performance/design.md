# UI rendering performance — design

## Findings and boundaries

GPUI 0.2.2's `AnyView::cached` reuses layout/paint when a view is clean and bounds, clipping, and
text style match. `Window::refresh` explicitly bypasses this cache. The existing app embeds sibling
entities without these cache boundaries and the scroll helper calls `refresh` every animation frame.
These are code-level findings, not measured frame-rate results.

- `src/view/components/smooth_scroll.rs`: retain the owner entity ID in persistent animation state
  and use `App::notify(owner)` from input/frame callbacks. Do not use `Window::current_view` outside
  rendering (it panics). Tests bind state to real headless test entities.
  Optional `RSPOTIFY_SCROLL_METRICS=1` collects frame-callback intervals during active scrolling
  and logs only aggregate count/mean/p95/max after settling. These include display pacing and are
  not CPU/GPU render timings; compare on the same display. No account or content data is logged.
- `src/view/components/render_cache.rs`: small sized cache-boundary helpers with headless render
  counter tests. Fill-sized views require explicit size styles because GPUI does not infer cached
  root size from their rendered children. Fixed-height regions retain existing heights.
- `src/view/main_layout.rs`: independent cached library, content router, and 76px playback bar.
- `src/view/root_layout.rs`: independent cached titlebar, retaining its existing height.

All UI state remains GPUI-owned. GPUI notifications and descendant dependencies invalidate cached
regions; changed bounds invalidate on resize. No cache is introduced for network/account data.
No unrelated control behavior changes are included.

## Validation

Use headless GPUI rendering counters to confirm reuse/invalidation. Run Rust validation and build
release. Manual comparison must use the release binary in this worktree, not an older desktop
launcher. Record real frame timing only if collected; debug-build experience alone is not a release
performance baseline. No account smoke tests or desktop automation are needed.
