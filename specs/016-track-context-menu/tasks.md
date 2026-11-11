# Track context menu — tasks

- [x] Create `feat/track-context-menu` from fetched `origin/main`, preserving unrelated untracked files.
- [x] Record requested behavior and module/state boundaries before implementation.
- [x] Establish native transport/schema evidence and explicitly resolve unavailable-service scope.
- [x] Implement native actions and shared sleep timer behavior.
- [x] Implement the reference-style shared track menu and integrate track surfaces.
- [x] Exercise menu interactions and behavioral boundaries offline.
- [x] Run `cargo fmt --check`, `cargo check`, `cargo test` and record evidence.

## Evidence

- `cargo fmt --check` and `cargo check`: passed. An upstream `proc-macro-error2` future-incompatibility notice remains.
- `cargo test`: 101 passed, 13 ignored.
- `cargo test -p librespot-connect --lib`: 6 passed.
- Final `cargo fmt --check`, `cargo check`, and `cargo test track_menu` after popup icon styling: passed; 6 focused GPUI tests passed.
- Production GPUI popup checks exercise clicked duplicate identity after row reorder, unchanged playback on right-click, Escape/outside/action dismissal, enabled keyboard navigation, disabled-entry safety, clipped targets and left-flipping edge submenus.
- Native offline regressions cover single-occurrence/revision/permission safety, ambiguous UID rejection, playlist cache invalidation, real credits/radio response shapes, queue order/capacity and timer replacement/cancellation.
- The user explicitly approved visibly disabled Jam and taste-profile entries. All other pictured actions are implemented against native/local paths.
- Native artifact evidence and limitations are recorded in `docs/reverse-engineering.md`. Full request-specific optional nonce/base-version policy remains unverified; playlist writes, radio/credits responses and authenticated desktop visuals were not exercised against a real account.
- No Spotify account requests, mutations, audio playback or other-application automation were performed. GPUI tests draw the actual popup with synthetic targets; they are not a claim of desktop screenshot/pixel parity.

## Main integration and release navigation

- [x] Fetch and integrate `origin/main` at `5ef15af`, preserving track menus, song-title links and unrelated local files.
- [x] Route Go to album through the shared containing-release entry point; remove obsolete menu-only album metadata.
- [x] Verify context-menu navigation and Album/Single/EP metadata behavior offline, then run the required Rust checks.

### Release-navigation evidence

- Temporary `album_menu_navigation_smoke` passed against the actual GPUI popup and dispatcher: choosing Go to album immediately pushes the clicked song's `album-for:` route, closes the menu, preserves the playing-track snapshot, deduplicates repeated navigation and supports Back/Forward.
- The smoke used an unauthenticated, disconnected native client and isolated state storage; no Spotify traffic or audio was started. Temporary client/test scaffolds were removed after proof.
- Final integrated `cargo fmt --check`, `cargo check` and `cargo test` passed: 107 passed, 13 ignored. Existing native release-kind/date/ordering regressions and all menu/title GPUI interaction regressions passed.
- Local `main` and fetched `origin/main` match at `5ef15af`. Existing user stashes and unrelated local files were preserved; only this integration's redundant autostash was removed after verification.
