# Tasks — like and unlike songs (spec 011)

- [x] Define the native membership contract, cache ownership, and UI behavior.
- [x] Generalize the collection write path and add `set_liked` for the `collection` set.
- [x] Add liked-track filtering and offline serialization/filtering tests.
- [x] Add the client liked cache, status/revision API, background load, and debounced reconciliation.
- [x] Prune the cached and in-memory Liked Songs detail after a successful unlike.
- [x] Add the shared plus/check visual.
- [x] Wire the now-playing control with optimistic, busy, unknown, and error states.
- [x] Add the playlist liked column, row toggles, Liked Songs pruning, and cache updates.
- [x] Update the README, reverse-engineering notes, and AGENTS spec list.
- [x] Run `cargo fmt --check`, `cargo check`, and `cargo test`.
- [ ] Run the ignored read-only membership smoke check with explicitly approved account access.
- [x] Record live write and visual validation as pending; this iteration excludes live writes.

## Validation notes

- `cargo fmt --check`, `cargo check`, and `cargo test` pass: 57 passed, 11 ignored.
- No account-mutating test is added to the default suite.
- No live like/unlike request was sent during this iteration.
- The read-only membership smoke check was not added or run; it requires explicit account approval.
- Account read-only checks must run sequentially if they share the stored refresh token.
