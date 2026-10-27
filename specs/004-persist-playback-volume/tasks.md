# Tasks: persist playback volume

## Implementation

- [x] Add the feature specification, design, and implementation/validation task list.
- [x] Add backwards-compatible volume persistence to `SessionStore`.
- [x] Restore the saved preference at Connect startup and persist UI volume changes.
- [x] Apply the restored volume to the local mixer and initial playback snapshot.
- [x] Add tests for legacy state, default volume, saved volume, and clamping.

## Validation

- [x] `cargo fmt --check`
- [x] `cargo check`
- [x] `cargo test` (12 passed, 3 ignored)
- [ ] Real-account playback and visual validation are not run without explicit account/network approval.
