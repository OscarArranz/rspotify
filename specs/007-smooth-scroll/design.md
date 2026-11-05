# Smooth scrolling — design

## Module boundaries

- `src/view/components/smooth_scroll.rs` owns wheel-delta easing for horizontal scrollable divs
  and vertical virtual lists, including routing input to nested horizontal carousels.
- `src/view/components/mod.rs` exports the helper to view modules.
- `src/view/components/library/` applies it to the virtual library list and category chips.
- Home, Search and Artist use vertical virtual lists (spec 013); Home and Search retain eased
  horizontal carousels, while Artist retains native horizontal movement.
- Playlist retains its GPUI `ListState`, compatible easing and pagination.

## Behavior

Horizontal scrollable divs use clipped overflow plus a tracked handle, disabling GPUI's immediate
wheel movement; the helper consumes input only when the requested axis can move.
Horizontal divs additionally require the Shift modifier; without it they leave the event
unconsumed so the vertical parent can handle it.
Repeated input accumulates toward a clamped target; reversing direction discards stale momentum.
Frames follow a responsive 18 ms time-based exponential curve and stop once within sub-pixel
tolerance of the destination, so ordinary wheel steps settle in roughly 100 ms. It must not block GPUI's
foreground thread. Horizontal/vertical direction and
existing parent/child scroll ownership are preserved; the helper must not consume gestures that the
child cannot handle.

Vertical surfaces use variable-height virtualized GPUI lists. Smoothing preserves pagination and
programmatic resets. A scroll-only canvas above each list handles bubble-phase input before the
list's native wheel listener; clicks remain unaffected. Painted horizontal carousels register their
clipped bounds during prepaint. The overlay routes input to those carousels first and chains at their
edges, without letting stale bounds from offscreen rows intercept input.

## State ownership

Each scroll surface owns its animation state through a stable GPUI scroll handle or entity state.
Animation tasks are cancelled/replaced when a newer wheel event arrives and end at the latest
requested offset. No global application state or runtime dependency is introduced. Enable the
existing GPUI dependency's `test-support` feature for headless wheel-dispatch regression tests.

## Validation

Run Rust formatting, check, and test commands. Visually exercise vertical wheel and touchpad
scrolling, horizontal carousels, nested edge behavior, and playlist pagination when a running GUI is
available.
