# Smooth scrolling

## Goal

Make vertical and horizontal scrolling in rspotify feel fluid and consistent across the library,
search results, horizontal search carousels, and playlist detail screens.

## User-visible scope

- Mouse-wheel scrolling should ease into position instead of jumping in discrete steps.
- Both pixel-based and line-based events are smoothed; pixel events must not be assumed to be
  inherently smooth. Touchpad scrolling preserves direction and accumulated movement with short easing.
- Both vertical and horizontal scroll surfaces should use the same behavior.
- Horizontal carousels respond to wheel input only while Shift is held, so ordinary wheel input
  remains available for vertical page scrolling.
- Existing playlist virtualization, pagination, nested scrolling, and scroll boundaries must continue
  to work.

## Acceptance criteria

1. Library, search content, horizontal search carousels, and playlist detail scrolling use fluid
   wheel/touchpad behavior.
2. Nested horizontal carousels do not unexpectedly scroll the outer vertical page, and scroll
   gestures at an edge can continue to the parent when appropriate.
3. Large playlist virtualization and near-end pagination continue to work.
4. Horizontal wheel input is ignored unless Shift is held; unmodified input can scroll the parent
   vertically.
5. `cargo fmt --check`, `cargo check`, and `cargo test` pass.
6. Visual interaction is checked in the running app when available; otherwise this is recorded as
   pending.
