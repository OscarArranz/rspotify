# Library sidebar parity (spec 009)

## Goal

Match the library panel in the user's installed Spotify Flatpak (Image 1), not the newer
Create-button variant. Preserve native services and Premium-only librespot playback.

## Screenshot follow-up (2026-10-23)

The user's detailed interaction references supersede the initial approximations:

- Four view modes: compact inline text list, two-line artwork list, artwork-only compact grid,
  and a grid with titles/subtitles. The picker uses a four-button segmented control.
- Header collapse affordance appears on heading hover; add/expand use the exact descriptive
  tooltips. The collapsed rail retains the add button and uses rich, right-side tooltip cards.
- Search replaces its circular button with a single filled input containing a search icon and
  clear action; sort text hides while searching. Matching title/creator substrings are highlighted.
- Selecting Playlists exposes By you / By Spotify subfilters; selected nested chips form a joined
  group. Edge arrows advance through real overflow without leaving hidden end chips.
- Header and chips stay sticky; the search/sort toolbar scrolls away with the entries. Artwork
  exposes a play/pause overlay on hover without intercepting ordinary row navigation.
- Expansion uses the available main area, not an arbitrary wider sidebar. Collapse restores the
  prior uncollapsed mode. The inter-panel gutter is 8 logical pixels and highlights on hover.
- Context menus use the observed grouped actions and capability gates; do not imply that a disabled
  native editing operation works. Currently-playing tooltip titles/indicators are green.

Read-only inspection of the user-started official client is explicitly approved. No original-app
playback or account mutation controls may be invoked during inspection.

## Acceptance criteria

- [ ] At matched scale and panel width, the surface, heading, circular add/expand controls,
  single-line overflow chips, search/sort toolbar, artwork, row spacing and truncation match Image 1.
- [x] Liked Songs and saved entries share a virtualized scrolling list. Headers remain fixed.
  Liked Songs is the fixed first row under every sort and under the Playlists filter, including
  the ownership chips; account pin state never moves it.
- [x] Search, category filters, sort/view picker, refresh, resizing, collapse/expand and folder
  navigation work; loading, empty, partial-data and error states are distinct.
- [x] Rows show real owner metadata and available artwork, including album-art collages when a
  playlist has no custom cover. Active context and playing state come from the playback snapshot.
- [ ] Native service coverage is extended to saved albums, artists and shows, server recency,
  pin/unpin, playlist/folder creation and applicable context actions/drag organization.
- [x] Unverified service operations are visibly unavailable, never silent no-ops or fabricated
  successes. Local presentation choices must not be described as synchronized account state.
- [ ] Existing playlist/Liked Songs navigation and player behavior remain intact.
- [x] Formatting, compilation and tests pass. Account and visual validation are recorded separately.

## Boundaries

The titlebar, player bar and unrelated playlist-page controls are not redesigned. Album/artist/show
rows may reuse the native context track page; dedicated entity-page redesigns are outside scope.
Do not use the public Web API, modify credentials, automate unapproved applications, or start playback
during validation. Read-only inspection of the user-started Spotify client is approved.
Network/account checks and real mutations require explicit user approval. Shipped static code is
evidence of implementation, not evidence that a service currently accepts a request.

## Status

Partial parity implementation. The screenshot-driven UI pass, native saved categories, native
Recents and actual pin membership are implemented and read-only account checks pass. Pin/unpin
commands are wired to a statically evidenced native write contract with read-after-write
verification; no mutation was executed during validation. Creation, Jam, queue, other editing and
drag organization remain disabled. Recents coverage is bounded to 50 contexts, native pin ordering
is not recovered, and some rich playlist labels remain unavailable. Pixel-level visual acceptance
and account-changing validation are pending. The user's running official client was inspected
without clicking playback or mutation controls.
