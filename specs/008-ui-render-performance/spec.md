# UI rendering performance

## Goal

Reduce dropped frames during interaction without changing Spotify behavior or layout. User feedback
on spec 007 confirms perceptible smoothing but reports frame drops across the interface.

## Scope and acceptance

- Scroll animation invalidates only its owning view instead of forcing a full-window refresh.
- Library, routed content, playback bar, and titlebar have independently cached rendering boundaries.
  Updating one region must not rebuild unrelated regions; resizing, focus, navigation, and metadata
  updates must still invalidate correctly.
- Headless regression tests demonstrate cached sibling reuse and invalidation after updates/resize.
- Formatting, checking, and tests pass. Build an optimized executable for a meaningful manual
  comparison. Record user feedback; do not claim an FPS improvement without measurements.

## Out of scope

Native Spotify API changes, dependency upgrades, speculative renderer replacements, and DRM changes.
