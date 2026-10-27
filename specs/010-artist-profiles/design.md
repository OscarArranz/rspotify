# Artist profiles — design

## Reference and evidence

The appearance reference is the ten supplied screenshots of the installed Spotify desktop artist
page. The installed frontend bundle is `xpui.spa` at Flatpak commit
`c220ff3a3fea1408ad4d5133d692eacaa1ac2399ff0d3a20a56c5292cd991f8d`, SHA-256
`7ce05c0001be22c7b5124d9dd59dd1ab1e0c188640967836b996f0d32a62f74e`.

Static inspection establishes:

- the bundle contains the persisted `queryArtistOverview` document name and hash
  `1ac33ddab5d39a3a9c27802774e6d78b9405cc188c6f75aed007df2a32737c72`;
- its variables are `{ uri, locale: null }`;
- the `artistUnion` entity exposes `profile`, `visuals`, `stats`, discography and related content
  types in the shared artist-card/about modules;
- the official client has no separate bundled full artist route in this artifact, so supplementary
  section contracts must be verified from the overview response or a separately observed query.

REA evidence for artifact identity remains
`ev_594535c909364f9da23da669ddec0ce21afef8e706ee5fad149ad60b6390e970`. Static evidence does not
establish live response completeness or runtime behaviour.

## Module boundaries

| Module | Responsibility |
| --- | --- |
| `src/lib/spotify/artist.rs` | Artist profile types, Pathfinder overview request, defensive normalisation, section models, formatting helpers, and offline parser tests |
| `src/lib/spotify/client.rs` | Worker-only `NativeSpotify::artist_profile` and channel-safe APIs |
| `src/lib/spotify/mod.rs` | Re-export artist types used by the UI |
| `src/view/screens/artist.rs` | `ArtistScreen`: virtualized page, hero, actions, sections, filters, expansion, navigation, and playback state |
| `src/view/components/library/mod.rs` | Route artist rows to `ArtistScreen` |
| `src/view/screens/search.rs` | Route artist cards to `ArtistScreen`; retain explicit playback where shown |
| `assets/icons/` | Any missing verification, external-link, music-video, or calendar glyphs |
| `docs/reverse-engineering.md` | Persisted query hash, observed response sections, and limitations |
| `specs/002-playlist-detail-view/` | Remove the claim that artist pages reuse the generic context page |
| `specs/009-library-sidebar-parity/` | Note the dedicated artist destination |

## Data model

`ArtistProfile` is the page payload. Every section is independently optional:

- identity: URI, name, verified, biography, header/avatar images, followers, monthly listeners,
  world rank;
- popular tracks: URI, title, artists, album, artwork, duration, explicit, playable, video flag,
  liked flag;
- liked music: track/release counts and optional context image;
- artist pick: kind, URI, name, subtitle, artwork;
- discography: separate popular/album/single/EP/compilation item arrays plus total counts;
- featuring, appears-on, discovered-on: typed cards with URI, name, subtitle, artwork;
- videos: URI/title/artists/artwork/explicit; never a playable video request;
- concerts: date, city, venue, time, ticket URI;
- merchandise: title, description, image, external URL;
- related artists: URI, name, avatar.

Names, counts, images, and dates are only populated from observed JSON fields. The parser tries a
small set of known GraphQL aliases where the official schema has varied, but does not invent a
section when the response has no items.

## Data flow

1. Entry points construct `ArtistScreen` with an `ArtistSource { uri, name, image_url }` preview.
   The preview can paint the hero before the request finishes.
2. `ArtistScreen` starts a worker calling `NativeSpotify::artist_profile(uri)`.
3. The parser selects the largest available image for each entity and skips malformed sections
   instead of inventing data. No colour sampling is attempted; the hero uses a fixed gradient over
   the artwork.
4. The screen replaces its payload only when the request generation and artist URI still match.
5. Section navigation and playback use the existing router and client handle.
6. A 250 ms poll copies the playback snapshot and notifies only for relevant changes.

No GPUI foreground thread performs network, disk, decode, or image work.

## Rendering and state ownership

`ArtistScreen` owns the immutable preview, client handle, load status, optional payload, selected
discography tab, per-section expansion flags, section failures, follow state, list state, and
scroll animation.

The page is one variable-height `ListState`:

0. hero (fluid height, header image background, gradient into `#121212`);
1. action bar;
2. optional sections in reference order;
3. final related-content section.

Each section is one list item. Shelves inside an item use horizontal overflow or a responsive grid,
so the list remains virtualized by section. The compact overlay bar is painted outside the list and
appears from the current `ListScrollEvent.is_scrolled`/scroll offset. It uses a translucent dark
surface, play icon, artist name, and follow affordance when available.

Section presence is computed after parsing. Empty arrays hide their section. A top-level error
shows a retry state with the preview hero. Section-level failures never replace successfully loaded
sections.

Discography filters:
- Popular releases: popular-release order.
- Albums: album releases.
- Singles and EPs: single and EP releases.
The selected filter resets only that section's local expansion. A ``Show all`` text action expands
only items present in the overview payload; no endpoint is implied.

Music videos render as cards with artwork and metadata. Their play affordance is disabled with an
explicit ``Video playback unavailable`` label; opening the card does not start audio or video.

## Interaction

- Play/pause follows the same context-aware logic as playlists. Shuffle toggles the shared Connect
  shuffle state.
- Popular row click calls `play_context_at(artist_uri, track_uri)`.
- Follow/unfollow runs only from an explicit click. It is hidden when no native follow contract is
  verified. While pending, the button is disabled; success is shown only after the request returns.
- Related artists push a new `ArtistScreen`. Release/playlist cards push `PlaylistScreen` with a
  `PlaylistSource::Context` preview. Liked music opens the existing Liked Songs page.
- External merch links are opened only by an explicit action; the app does not automatically launch
  a browser.

## Failure modes

- Missing header/avatar image: gradient placeholder with initials.
- Missing monthly listeners, verification, followers, or rank: omit that element.
- Missing biography or About image: hide About.
- Malformed section: keep other sections, record one bounded inline warning.
- Playback or follow failure: show an action error; do not change playback/follow state optimistically.
- Unknown persisted-query rejection: show a retryable error and record the contract limitation.
- No cached payload: placeholder hero plus loading body. Cached payload remains visible when refresh
  fails.

## Security and privacy

Only the existing authenticated handle is used. Tokens, session files, account identifiers, and
raw overview payloads are never logged or committed. Target strings and URLs are display data, not
instructions. No public Web API, DRM bypass, desktop automation, or dependency on the official app
is introduced.

## Validation

Offline tests cover sparse/malformed overview JSON, section mapping, discography filters, image
selection, and Music-video non-playability. Cargo formatting, check, and tests are mandatory.
Read-only account validation records predicates only (section names and counts), not names or URIs.
Visual validation compares the supplied screenshots at matched panel width and notes remaining gaps.
