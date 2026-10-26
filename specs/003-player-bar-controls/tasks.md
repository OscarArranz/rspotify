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

## Playback-position jumping fix

- [x] Trace local player events and Connect polling; document clock ownership and acceptance criteria.
- [x] Prevent Connect polling from resetting or overwriting the local playback clock.
- [x] Add deterministic regression coverage for stale remote state, unchanged local polls, pause,
  backward seek, and release of local ownership.
- [x] Run `cargo fmt --check`, `cargo check`, and `cargo test` for this fix.
- [ ] Real-account restored-track playback and visual verification (not authorized for this fix).

Headless validation: `cargo fmt --check` and `cargo check` passed; `cargo test` passed
(25 passed, 3 account/audio smoke tests ignored). Five new clock regression tests use synthetic
events and explicit monotonic times; no account access, audio, or desktop automation was used.
Existing compiler warnings remain. The defect was two writers sharing one clock: Connect polling
reset the timestamp without supplying a local position, and stale previous-device state overwrote
the local position/playing flag. The fix gives local player events ownership until stop/unavailable
and leaves the position anchor unchanged on metadata-only updates.
