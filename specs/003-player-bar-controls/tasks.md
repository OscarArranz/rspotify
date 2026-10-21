# Tasks: interactive player bar

## Implementation

- [x] Add the player-bar interaction specification and design.
- [x] Add responsive click/drag seeking with pointer-time tooltip and hover feedback.
- [x] Add responsive click/drag volume adjustment with hover feedback.
- [x] Share slider rendering and interaction feedback between seek and volume.
- [x] Keep the actual seek fill stationary while idle; show a lighter hover preview and commit seek
  on release. Track drag movement outside the slider track.
- [x] Run formatting, compile, and unit tests.

## Validation

- [x] `cargo fmt --check`
- [x] `cargo check`
- [x] `cargo test` (7 passed, 3 ignored)
- [ ] Visual check at default and resized window widths.
- [ ] Real-account seek and volume check (only with explicit approval).
