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
helper will preserve precise touchpad deltas while easing coarse line-based wheel movement over a
short animation. It must not block GPUI's foreground thread. Horizontal/vertical direction and
existing parent/child scroll ownership are preserved; the helper must not consume gestures that the
child cannot handle.

The playlist remains one variable-height virtualized GPUI list. Smoothing must preserve the
`ListScrollEvent` pagination threshold and existing programmatic top resets.

## State ownership

Each scroll surface owns its animation state through a stable GPUI scroll handle or entity state.
Animation tasks are cancelled/replaced when a newer wheel event arrives and end at the latest
requested offset. No global application state or new dependency is introduced.

## Validation

Run Rust formatting, check, and test commands. Visually exercise vertical wheel and touchpad
scrolling, horizontal carousels, nested edge behavior, and playlist pagination when a running GUI is
available.
