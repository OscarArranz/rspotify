# Virtual lists

## Goal

Keep the current UI appearance and interactions while bounding rendering work for scrollable collections. Virtualize variable-height collections and collections displaying more than 50 items. Retain current display caps; do not expose additional results or change native Spotify APIs.

## Acceptance criteria

- [x] Home and Search use variable-height virtual vertical lists, retaining ordering, spacing, padding, status messages and actions in the rendering implementation.
- [x] Expanded Home shelves and Artist discography render only visible grid rows, preserving responsive column layout.
- [x] Artist popular tracks and concert rows do not hide unbounded collections inside a single virtual item.
- [x] Horizontal Home and Artist collections over 50 cards create only visible cards plus bounded overscan, retaining card geometry and existing scrolling controls.
- [x] Existing virtual playlist and library lists remain virtual, including pagination, selection, filtering and smooth scrolling.
- [x] Small fixed-size collections (including capped Search carousels, Artist gallery and category chips) retain their existing content and eager rendering.
- [x] Resizing, filtering, expansion, shrinking data and scrolling to the end retain reachable content without stale rows.
- [x] Offline GPUI scenarios exercise large fixtures, distant scrolling and resizing; Rust formatting, compilation and tests pass. Account and actual desktop visual validation are recorded separately and never implied by headless checks.

Desktop pixel parity and release performance measurements are not established by the offline checks;
see `tasks.md` for exact coverage and limits.
