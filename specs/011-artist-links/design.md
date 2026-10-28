# Artist links in track metadata — design

Track metadata must preserve artist identity (name and Spotify artist URI) in addition to its
existing display string. Render credits as individual interactive elements and push the existing
`ArtistScreen` through the global `StackRouterHandle`; use the existing artist preview data and
router source identity. Stop click propagation so a playlist track click cannot start playback.
Apply the same renderer/behavior in `PlayerBar`, using artist metadata carried by `NowPlaying`.
Malformed or URI-less artist entries display as non-interactive text. Do not derive URIs from names.
Convert binary artist IDs to canonical base-62 Spotify URIs, including migration of hexadecimal
artist IDs in existing playlist caches. Both entry points share the existing search artist navigation.
