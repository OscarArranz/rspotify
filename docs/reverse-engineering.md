# Reverse-engineering notes

These notes record what was observed in the installed Spotify desktop client and what was
inferred or supplied by librespot. They explain why the client talks to Spotify's native
services instead of the public Web API.

## Artifact analysed

- Installed Flatpak app:
  `/var/lib/flatpak/app/com.spotify.Client/x86_64/stable/c220ff3a3fea1408ad4d5133d692eacaa1ac2399ff0d3a20a56c5292cd991f8d`
- Frontend bundle: `xpui.spa`
  - SHA-256 `7ce05c0001be22c7b5124d9dd59dd1ab1e0c188640967836b996f0d32a62f74e`
  - Extracted with REA, evidence id
    `ev_a1e7008c30443054deb6dbc8476817a52e9d4814102d495df520173b04716ec1`
- Native ELF: `files/extra/share/spotify/spotify`
- Login bundle `login.spa` was also extracted separately.

## Endpoints observed in the official client

### Authentication / session

- `https://accounts.spotify.com/authorize`
- `https://accounts.spotify.com/api/token`
- `https://login5.spotify.com/v3/login`
- `https://login5.spotify.com/v4/login`
- `https://accounts.spotify.com/login/ott/v2#token=`
- Protobuf packages `spotify.login5.v3.*` and `spotify.login5.v4.*`
- Keymaster-related strings and paths: `keystore/`, `episodekeystore/`,
  `mediakeystore/`, `keymaster_error`

### Web / partner APIs

- `https://api.spotify.com` (`WEB_API_URL`)
- `https://api-partner.spotify.com` (`PARTNER_API_ENDPOINT`)
- `https://spclient.wg.spotify.com` (`SPCLIENT_WG_URL`)
- `https://exp.wg.spotify.com` (`EXP_WG_ENDPOINT`)
- `https://wgint.spotify.net` (`INT_WG_ENDPOINT`)
- `https://ap.spotify.com`
- `https://apresolve.spotify.com/?type=dealer-g2&type=spclient`
- `https://spclient.wg.spotify.com/gabo-receiver-service`

### Playback / search

- Search: `searchview/v3/search?query=` and `&request_id=`
- Audio files: `storage-resolve/v2/files/audio/`
- Connect: `connect-state/v1/...`
- Pathfinder GraphQL: `https://api-partner.spotify.com/pathfinder/v1/query`

### Connect player state on startup

The librespot 0.8 Connect implementation subscribes to the dealer topic
`hm://connect-state/v1/cluster`. On connection setup, it registers the device through
`PUT /connect-state/v1/devices/<device-id>`; the response is a Connect `Cluster` containing
`player_state` (track URI, context URI, position, duration, and play/pause flags). This response is
available before calling `Spirc::activate()`, so reading it does not require transferring playback
to the local device. This behavior is evidenced by the pinned librespot 0.8 source used by this
project; it is not a claim that the public Web API exposes the same state.

## Client id and redirect

The desktop login UI and librespot both use:

```text
client id: 65b708073fc0480ea92a077233ca87bd
redirect:  http://127.0.0.1:8898/login
```

`login.spa` also contains `de32132...`, which was checked and is a Sentry DSN, not a client id.
The client id above is the one used by librespot's OAuth examples and its default keymaster
configuration.

## Pathfinder search

The official search route uses persisted GraphQL queries. The client sends a GET request:

```text
GET https://api-partner.spotify.com/pathfinder/v1/query
  ?operationName=searchDesktop
  &variables={...}
  &extensions={"persistedQuery":{"version":1,"sha256Hash":"<hash>"}}
Authorization: Bearer <access token>
App-Platform: WebPlayer
```

Extracted from `xpui-routes-search.js`:

| Operation | SHA-256 |
| --- | --- |
| `searchDesktop` | `db61238974d27839a136c9dc02bfdbe3fab7635f21cf85976ebff9a1ee281345` |
| `searchTracks` | `59ee4a659c32e9ad894a71308207594a65ba67bb6b632b183abe97303a51fa55` |
| `searchAlbums` | `64ae1fe6df380b038c0a65a2606d3361bc270de6870b2fdc99cf0848b1efa6d3` |
| `searchArtists` | `270905851ba5c7faca81cfe053c2dbd8ceb4f156a0e0ef4b385af75ab69ffd13` |
| `searchPlaylists` | `d520014e748f9ea44f7707d8df1819867ac1205e8b7f3e28f22fe5fc858921b1` |
| `searchFullEpisodes` | `d54e35fafe7520cb53883b86d012911cbad75c14ac079a917951c24cdb07c60f` |
| `searchTopResultsOnly` | `559f8bda5efce252945d20e0993e46e6d2d42dbb7f462abcd9a368668dcaf03e` |
| `searchTopResultsList` | `337d8b1b4f911fb12c60996623391703c2807550baccb51d95f5eabc8c8bdacd` |
| `searchGenres` | `9e1c0e056c46239dd1956ea915b988913c87c04ce3dadccdb537774490266f46` |
| `searchAudiobooks` | `e05ac765d02c084f8783d3c1572b23d57761c43f47eb8b87ce2f9ccced3fa068` |
| `searchAuthors` | `4a9d403a7cbc7e19da5520d619a865472b35382b043bfa458154e73a5c6f46bd` |
| `searchUsers` | `8f358dd82e62f61dd4ceaa9f8cd0889e644c9b707f1b724fbfb356a757cb7e5a` |

The `searchDesktop` variables observed in the UI are:

```json
{
  "searchTerm": "...",
  "offset": 0,
  "limit": 20,
  "numberOfTopResults": 20,
  "includeAudiobooks": false,
  "includePreReleases": false,
  "includeAlbumPreReleases": false,
  "includeAuthors": false,
  "includeEpisodeContentRatingsV2": true
}
```

## Token used for Pathfinder

Keymaster tokens (`hm://keymaster/token/authenticated`) are not usable for Pathfinder with
the desktop client id: requesting any scope other than `streaming` returns HTTP 403 with
`{"code":4,"errorDescription":"Invalid request"}`. Even a `streaming` token is rejected by
the GraphQL endpoint. Pathfinder accepts the account access token returned by login5
(`Session::login5().auth_token()`), which the client caches and refreshes automatically.
The same access token also worked against Pathfinder in testing.

## Library / rootlist

The user's library is fetched through the native SpClient rootlist endpoint:

```text
GET https://spclient.wg.spotify.com/playlist/v2/user/<username>/rootlist
  ?decorate=revision,attributes,length,owner,capabilities,status_code
  &from=<offset>&length=<page size>
```

It returns a protobuf `spotify.playlist4.proto.SelectedListContent`. Playlist names and
artwork are in the decorated `contents.meta_items[].attributes` (`name` and
`picture_size[].url`), parallel to `contents.items[]`. Some entries omit `picture_size` but include
the raw 20-byte `attributes.picture` image ID; hex-encoding that ID produces a working
`https://i.scdn.co/image/<id>` URL. Some playlists have neither a URL nor an image ID in the
rootlist metadata or playlist detail record. The profile endpoint
(`/user-profile-view/v3/profile/<username>`) returns the account name and avatar
(`image_url`) but not the library.

## Playlist detail and context pages

The playlist detail page uses the same native services as librespot rather than the public
Web API:

- Playlist header metadata: `GET https://spclient.wg.spotify.com/playlist/v2/playlist/<id>`
  through librespot's `Playlist::get`. It returns `playlist4_external.SelectedListContent`
  with the name, description, cover sizes, owner username, length, item order, and per-item
  added-at timestamps.
- First track page: `GET https://spclient.wg.spotify.com/context-resolve/v1/<context-uri>`
  through `SpClient::get_context`. It returns protobuf-JSON `Context` pages containing
  `tracks[].uri`, sparse `tracks[].metadata`, and a `next_page_url`. Runtime inspection of this
  account's Liked Songs context found 5,855 tracks, each with only `added_at`; the context metadata
  itself only included `context_owner`.
- Missing display fields are resolved by POSTing bounded batches to
  `/extended-metadata/v0/extended-metadata` with `TRACK_V4` / `EPISODE_V4` queries. Runtime
  validation returned track protobufs with names, artists, album names, cover groups, duration, and
  explicit flags. A 1,000-entity batch succeeded; a single 5,855-entity request was rejected with
  HTTP 413, so requests are capped at 500 entities.
- Additional track pages: `next_page_url` values begin with `hm:/`; librespot strips that
  prefix and requests the remaining path from the SpClient host.
- Liked Songs context: `spotify:user:<username>:collection`.

The UI reads context metadata defensively (`title`, `artist_name`, `album_name`, `image_url`,
`duration`, `is_explicit`, and `is_playable`) and falls back to episode fields when present.
Missing optional fields are omitted instead of guessed.

### Playlist visual metrics

Extracted from `xpui-routes-playlist.js` (module `43396`):

| Constant | Value |
| --- | --- |
| `MINIMUM_HEADER_HEIGHT` | `186px` |
| `MAXIMUM_HEADER_HEIGHT` | `336px` |
| `BACKGROUND_HEIGHT_PERCENTAGE` | `40vh` |
| `IMAGE_SIZE` | `192px` |
| `IMAGE_SIZE_BIG` | `232px` at `min-width: 1280px` |

The official header blends a dominant artwork colour into `#121212` with a radial gradient
around `14% 12%`. GPUI exposes linear gradients, so rspotify uses a vertical linear
approximation. Track rows are `56px` high with a `#ffffff1a` hover and a `#1ed760` play
button.

The official playlist route can render genre/mood filter chips when its payload supplies
them. The native playlist/context payload used here does not provide a dependable chip list,
so rspotify deliberately omits chips rather than inventing labels.

## What is observed vs. inferred

Confirmed directly from the artifact:

- client id, redirect shape, login5 hosts, pathfinder host and query hashes
- search variable names and the `searchV2` response tree
- Connect and audio storage paths
- playlist header metrics, cover sizes, and row styling in `xpui-routes-playlist.js`

Confirmed at runtime (against a valid account):

- OAuth login with the desktop client id and loopback redirect
- login5 session establishment
- Pathfinder `searchDesktop` returning tracks, artists, albums and playlists
- local Connect playback (load, progress, pause) through librespot

Inferred or delegated to librespot:

- the exact Pathfinder transport implementation (the relevant vendor chunk is not in
  `xpui.spa`); request shape was cross-checked against public documentation and client
  behaviour
- login5 protobuf exchange, keymaster, audio keys and PlayPlay decryption
- Connect state machine / dealer protocol
- context pagination behavior beyond the first page; first-page context shape and extended metadata
  hydration are now confirmed at runtime

Not yet verified in the running UI: pagination and row playback. A read-only real-account check
confirmed Liked Songs hydration for all 5,855 entries and confirmed the rootlist artwork behavior;
no playback was started during that check.

Playback is implemented with librespot because it already implements Spotify's native
login5, keymaster, audio-key and Connect protocols. No DRM is bypassed or reimplemented.
A valid Premium account is required for playback.
