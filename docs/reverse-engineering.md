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
`picture_size[].url`), parallel to `contents.items[]`. The profile endpoint
(`/user-profile-view/v3/profile/<username>`) returns the account name and avatar
(`image_url`) but not the library.

## What is observed vs. inferred

Confirmed directly from the artifact:

- client id, redirect shape, login5 hosts, pathfinder host and query hashes
- search variable names and the `searchV2` response tree
- Connect and audio storage paths

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

Playback is implemented with librespot because it already implements Spotify's native
login5, keymaster, audio-key and Connect protocols. No DRM is bypassed or reimplemented.
A valid Premium account is required for playback.
