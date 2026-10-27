# Artist profiles — tasks

## Specification and evidence

- [x] Create the numbered spec, design, and task list.
- [x] Record the installed bundle identity, `queryArtistOverview` hash and variables in
      `docs/reverse-engineering.md`.
- [x] Use REA and bounded static inspection to identify supplementary artist sections, follow
      behaviour, and unsupported video/ticket paths.
- [x] Run an ignored, read-only account probe and record which sections the live overview supplies
      without retaining names, URIs, tokens, or payloads.

## Native data layer

- [x] Add `src/lib/spotify/artist.rs` with the overview request and defensive section parser.
- [x] Model identity, popular tracks, liked music, pick, discography, featuring, videos, concerts,
      about, merch, discovered-on, related artists and appears-on independently.
- [x] Add bounded image selection that never blocks the UI thread. The hero uses a fixed gradient
      rather than artwork colour sampling.
- [x] Add `NativeSpotify::artist_profile` with a request-generation guard and an ignored read-only
      smoke test. No cross-screen overview cache is kept yet.
- [x] Implement and validate follow/unfollow only if a native contract is established; otherwise
      hide the action and document the gap. **Gap:** `profile.saved` is parsed but no verified native
      write contract was established, so the action stays hidden.

## Screen and navigation

- [x] Add `ArtistScreen` with one variable-height virtualized list and compact scrolled bar.
- [x] Render hero, action bar, popular, liked, pick, discography filters, featuring, videos, tour,
      about, merch, discovered-on, related artists and appears-on sections.
- [x] Implement play/shuffle/row playback, discography filters, section expansion and profile
      navigation.
- [x] Route library artists and search artists to the profile; preserve explicit playback affordances.
- [x] Add missing SVG assets and hover styling.
- [x] Update specs 002/009 and user-facing documentation to describe the dedicated page.

## Validation

- [x] Add offline unit tests for sparse and malformed JSON, all section mappings, filters, image
      fallbacks and video non-playability.
- [x] Run `cargo fmt --check`, `cargo check`, and `cargo test`.
- [x] Run the read-only artist smoke test with explicit approval; no playback or follow writes.
- [ ] Compare against the supplied screenshots at matched panel widths; record gaps.
- [ ] Confirm no secrets, personal data, or untrusted payload text is committed.

## Evidence

- `cargo fmt --check`, `cargo check`, and `cargo test`: pass. Default tests include four offline
  artist-parser tests (sparse overview, observed section mapping, non-artist union, missing name).
- `cargo test --bin rspotify_client smoke_native_artist_profile -- --ignored --nocapture` with
  `RSPOTIFY_ARTIST_PROBE_LIMIT=12`: read-only overview requests succeeded against the saved library.
  No playback, follow, or other account mutation was sent.
- Probe sections observed: popular 2–10, popular releases up to 10, singles up to 10, albums 0–6,
  related artists up to 20, discovered-on up to 12, appears-on up to 20, featuring up to 5,
  one artist pick, header images on most artists, three merch items for two artists, eight concert
  dates for one artist, and a `watchFeedEntrypoint` card for every probed artist.
  `profile.likedMusic` was absent for every probed artist, so that section stayed hidden. Section
  availability is recorded without names or URIs.
- `RSPOTIFY_ARTIST_DIAGNOSTICS` prints JSON paths and array lengths only; it never prints values.
- Visual comparison at matched panel widths and a final secrets/untrusted-text review remain open.
