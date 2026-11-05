# Virtual lists — design

## Inventory and ownership

- Playlist detail already uses a single variable-height `ListState`, including header and individual track rows. Library uses `ListState`; retain both implementations.
- Home uses one variable-height list for headers, shelves, panels, footer and responsive expanded-grid rows. Logical card anchors are remapped when grid columns change; returning from an expanded shelf restores the feed position.
- Search uses separate virtual headings/cards/track rows while preserving the current caps (eight horizontal cards, ten tracks).
- Artist flattens popular tracks, discography grid rows and concerts into the page list, retaining section boundaries, spacing, filters and toggles. Long horizontal shelves virtualize individual cards.
- `components/virtual_list.rs` owns fixed-width horizontal virtualization using actual clipped viewport bounds, lazy card callbacks and full content extent. Existing scrollable divs continue to own wheel behavior and scroll handles. Collections of at most 50 uniform cards retain their existing eager rendering.

## Invariants

UI state remains in GPUI entities. No native client or global state changes. No data fetching, account access, new dependencies or result-cap changes. Render callbacks build only visible rows/cards and bounded overscan; never prebuild all elements then discard invisible ones. Width-dependent grids invalidate cached measurements on resize. Data/shape transitions update list counts and cancel stale scrolling as appropriate. Existing spacing is represented at row boundaries rather than replaced by new layout conventions.

## Nested input ownership

All vertical surfaces now use list smoothing; obsolete vertical-div axis support is removed.
During prepaint, visible horizontal carousels register clipped bounds and their stable handles with
the parent scroll state. The list overlay swaps a reusable pending/painted region buffer each frame,
routes horizontal input before the native list listener, and chains to the vertical list at carousel
edges. Offscreen cached handles cannot intercept input. Home/Search retain easing and Shift behavior;
Artist retains native horizontal movement. No global registry or per-frame scan of the full feed.

## Verification

Use GPUI headless windows with offline synthetic collections to observe bounded render counts, distant item visibility, end geometry and responsive resizing. Exercise the actual changed rendering callbacks. Run `cargo fmt --check`, `cargo check`, `cargo test`. No real-account smoke or desktop automation is authorized by this request; record the resulting visual parity limit honestly.
