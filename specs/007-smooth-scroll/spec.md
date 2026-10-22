# Smooth scrolling

## Goal

Make vertical and horizontal scrolling in rspotify feel fluid and consistent across the library,
search results, horizontal search carousels, and playlist detail screens.

## User-visible scope

- Mouse-wheel scrolling should ease into position instead of jumping in discrete steps.
- Precise touchpad scrolling should remain responsive and preserve its direction and relative movement.
- Both vertical and horizontal scroll surfaces should use the same behavior.
- Existing playlist virtualization, pagination, nested scrolling, and scroll boundaries must continue
  to work.

## Acceptance criteria

1. Library, search content, horizontal search carousels, and playlist detail scrolling use fluid
   wheel/touchpad behavior.
2. Nested horizontal carousels do not unexpectedly scroll the outer vertical page, and scroll
   gestures at an edge can continue to the parent when appropriate.
3. Large playlist virtualization and near-end pagination continue to work.
4. `cargo fmt --check`, `cargo check`, and `cargo test` pass.
5. Visual interaction is checked in the running app when available; otherwise this is recorded as
   pending.
