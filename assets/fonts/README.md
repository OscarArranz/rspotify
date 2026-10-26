# Font provenance

`SpotifyMixUI-Regular.ttf` and `SpotifyMixUI-Bold.ttf` are SFNT decompressions of
`fonts/SpotifyMixUI-{Regular,Bold}.woff2` in the locally installed Spotify Flatpak's `xpui.spa`
(SHA-256 `7ce05c0001be22c7b5124d9dd59dd1ab1e0c188640967836b996f0d32a62f74e`).
The conversion used the installed FreeType library (`FT_Load_Sfnt_Table`); no glyphs, outlines,
names or metrics were redesigned. `fc-scan` reports the family `Spotify Mix UI`, Regular/Bold.

Decoded file SHA-256 values:

- Regular: `b0bc34fa612cb2b9c0baf3a1868bf8183dd7d196cb860f93665bdac5a8b187ac`
- Bold: `f9fcefaabfb351fddcd147d1d0904a28ff7ece5bbbd4b3b19c914037d7d3645f`

These are upstream Spotify font assets; retain the applicable upstream copyright/license terms
when distributing them. The pre-existing `spotify_mix.ttf` title font is unchanged.
