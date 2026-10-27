# Artist profiles

## Status

Implemented. The native overview and supplementary sections were validated against the installed
desktop contract with bounded, read-only account requests. Follow/unfollow stays hidden because no
verified native write contract was established; music videos remain metadata-only. Visual comparison
against the reference screenshots is still pending.

## Goal

Open a dedicated Spotify-style artist profile from library and search results. The page presents
the artist's identity, popular music, releases, related content, concerts, merchandise, and
biography using Spotify's native desktop services. Playback uses the existing local Spotify Connect
device. Music videos are metadata-only because protected video playback is outside rspotify's
librespot audio path.

## User-visible scope

- Clicking a library artist or a search artist opens the artist profile instead of starting playback.
  Play controls inside the profile remain explicit.
- The hero shows the largest available header image, artist name, a Spotify verification badge,
  and monthly listeners. An avatar is used when no header image exists.
- Once the hero scrolls away, a compact sticky bar shows play/pause and the artist name.
- The action row provides play/pause, shuffle, follow/unfollow when the native follow contract is
  available, and an overflow placeholder.
- Optional sections appear only when Spotify supplies data:
  - Popular tracks with rank, artwork, title, optional music-video/explicit labels, duration,
    like state, and an expandable ``See more`` list.
  - ``You Liked`` summary linking to the artist's saved/liked music.
  - Artist pick release card.
  - Discography with ``Popular releases``, ``Albums``, and ``Singles and EPs`` filters, plus Show
    all where additional loaded items exist.
  - Featuring playlists, appeared-on releases, discovered-on playlists.
  - Music-video cards. They show artwork and credit metadata but do not attempt video playback;
    unsupported playback is communicated rather than silently failing.
  - On Tour concerts with date, city, venue, and time when available.
  - About with biography, monthly listeners, world rank, gallery/header artwork, and followers when
    available.
  - Merch items with title, description, image, and an explicit external-link affordance.
  - Fans also like artists, which open their profiles.
- Related artists, releases, and playlists navigate to existing artist/context screens without
  losing history. Back/forward preserves the artist screen.
- Missing sections, fields, or artwork degrade gracefully. A failed section keeps the rest of the
  profile usable and reports a bounded inline error; genuinely absent data is not replaced with
  invented placeholders.
- Playback requires Spotify Premium. The page keeps that requirement visible where relevant.

## Behavior and architecture requirements

### Native data

- Use the installed desktop client's persisted Pathfinder `queryArtistOverview` document with
  variables `{ uri, locale: null }` and the account login5 token. Do not use the public Web API.
- Validate and parse only fields observed in the response. Preserve section-level optionality so a
  missing section is distinct from an empty one.
- Use native playback contexts (`spotify:artist:<id>`) for play/shuffle and popular-row starts.
- Do not depend on the official Spotify application running, and do not scrape its UI.
- Music-video cards never invoke a DRM or protected-video path.

### Threading and state

- All network, decode, and image-colour work runs on workers; results return through channels.
- `ArtistScreen` owns its load state, profile payload, selected discography filter, expanded
  sections, section errors, follow-pending state, playback snapshot, and list scroll state.
- Stale overview responses must not replace a newer artist request.
- Global `AppState` remains unchanged; it owns only the authenticated client handle.

### Rendering

- Use one variable-height virtualized list so the hero scrolls away and long pages remain efficient.
- A compact overlay bar appears after the hero leaves the viewport.
- Use responsive card widths and horizontal shelves. Hover states and play overlays match the
  Spotify desktop visual language.
- Artist avatars are circular; release/playlist artwork is square.
- Use bundled SVG assets for controls rather than font glyphs.

### Navigation

- Library artist rows push a new `ArtistScreen` with the artist URI as the router source identity.
- Search artist cards open the same screen. Their separate play affordance, if any, must not
  navigate accidentally.
- Related artist cards push another profile; back returns to the previous artist.

## Out of scope

- Protected music-video playback, karaoke, lyrics, and Canvas.
- Ticket purchasing, merchandise checkout, and external browser automation.
- Account writes other than a follow/unfollow interaction backed by a verified native contract.
- A complete separate discography/catalog explorer if the native overview response does not expose
  enough data. ``Show all`` may reveal all items present in the loaded payload, but must not imply
  that an unverified page was fetched.
- Pixel-perfect recreation of every official transition or dynamic colour treatment.

## Acceptance criteria

1. Library and search artist entry points open `ArtistScreen` at `spotify:artist:<id>`; playback is
   not started merely by opening the profile.
2. The hero renders the best available image, name, verification badge when true, and monthly
   listeners when supplied. A compact bar appears while scrolled.
3. Popular, liked, pick, discography, featuring, video, tour, about, merch, discovered-on, related
   artists, and appears-on sections render when their native data is present. Absent data hides the
   section; request failures surface section-specific status without blanking the profile.
4. Discography filter changes and local ``See more``/Show all expansion work without a foreground
   block or scroll reset unrelated to the changed section.
5. Artist play/shuffle and popular-row playback use native artist contexts and reflect playback
   snapshot state. Video cards do not attempt unsupported playback.
6. Follow/unfollow is offered only when a verified native contract exists; pending and failure
   states are visible, and no optimistic success is shown after a failed request.
7. Related artist, release, and playlist navigation pushes the appropriate screen and preserves
   back/forward history.
8. No tokens, account identifiers, personal library names, or response payloads are logged or
   committed. Public Web API fallback is not added.
9. `cargo fmt --check`, `cargo check`, and `cargo test` pass, including offline parser tests for
   sparse and malformed overview responses.
10. Bounded read-only account validation records which optional sections the installed native
    overview response actually supplies. Visual comparison remains a separate check.

## References

- [`design.md`](design.md)
- [`tasks.md`](tasks.md)
- [`../../docs/reverse-engineering.md`](../../docs/reverse-engineering.md)
- [`../001-native-spotify-client/spec.md`](../001-native-spotify-client/spec.md)
- [`../002-playlist-detail-view/spec.md`](../002-playlist-detail-view/spec.md)
