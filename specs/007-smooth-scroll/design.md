# Smooth scrolling — design

## Module boundaries

- `src/view/components/smooth_scroll.rs` owns the shared wheel-delta easing behavior for GPUI
  scrollable `div` elements.
- `src/view/components/mod.rs` exports the helper to view modules.
- `src/view/main_layout.rs` applies it to the library playlist list.
- `src/view/screens/search.rs` applies it to the search page and horizontal result carousels.
- `src/view/screens/playlist.rs` retains the GPUI virtualized `ListState` and applies compatible
  smoothing without replacing virtualization or pagination.

## Behavior

GPUI scrollable divs currently apply incoming wheel deltas directly to their offsets. The shared
helper owns the scroll offset and eases both pixel and line deltas: Linux wheel events can be
pixel-based too. Scrollable divs use clipped overflow plus a tracked handle, disabling GPUI's
immediate wheel movement; the helper consumes input only when the requested axis can move.
Repeated input accumulates toward a clamped target; reversing direction discards stale momentum.
Frames follow a time-based exponential curve and stop once settled. It must not block GPUI's
foreground thread. Horizontal/vertical direction and
existing parent/child scroll ownership are preserved; the helper must not consume gestures that the
child cannot handle.

The playlist remains one variable-height virtualized GPUI list. Smoothing must preserve the
pagination threshold and existing programmatic top resets. A scroll-only canvas above the list
handles bubble-phase input before the list's native wheel listener; clicks remain unaffected.
Animation updates notify pagination from the actual visible rows, not an intermediate wheel jump.

## State ownership

Each scroll surface owns its animation state through a stable GPUI scroll handle or entity state.
Animation tasks are cancelled/replaced when a newer wheel event arrives and end at the latest
requested offset. No global application state or runtime dependency is introduced. Enable the
existing GPUI dependency's `test-support` feature for headless wheel-dispatch regression tests.

## Validation

Run Rust formatting, check, and test commands. Visually exercise vertical wheel and touchpad
scrolling, horizontal carousels, nested edge behavior, and playlist pagination when a running GUI is
available.
