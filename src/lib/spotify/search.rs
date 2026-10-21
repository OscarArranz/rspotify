//! Search through Spotify's own Pathfinder GraphQL endpoint.
//!
//! The official desktop/web client does not use the public Web API for search. It sends
//! persisted GraphQL queries to `api-partner.spotify.com/pathfinder/v1/query`. The query
//! hashes below were extracted from the installed Spotify desktop client
//! (`xpui-routes-search.js`).

use librespot::core::Session;
use serde_json::{Value, json};

use super::error::SpotifyError;

pub const PATHFINDER_URL: &str = "https://api-partner.spotify.com/pathfinder/v1/query";
pub const SEARCH_DESKTOP_HASH: &str =
    "db61238974d27839a136c9dc02bfdbe3fab7635f21cf85976ebff9a1ee281345";

/// Pathfinder accepts the account access token minted by login5. (Keymaster tokens are
/// limited to the `streaming` scope and are rejected by the GraphQL endpoint.)

#[derive(Clone, Debug, Default)]
pub struct SearchItem {
    pub uri: String,
    pub title: String,
    pub subtitle: String,
    pub image: Option<String>,
    pub kind: &'static str,
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct SearchResults {
    pub top_result: Option<SearchItem>,
    pub tracks: Vec<SearchItem>,
    pub artists: Vec<SearchItem>,
    pub albums: Vec<SearchItem>,
    pub playlists: Vec<SearchItem>,
    pub podcasts: Vec<SearchItem>,
}

impl SearchResults {
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
            && self.artists.is_empty()
            && self.albums.is_empty()
            && self.playlists.is_empty()
            && self.podcasts.is_empty()
    }
}

pub async fn search(session: &Session, query: &str) -> Result<SearchResults, SpotifyError> {
    let token = session
        .login5()
        .auth_token()
        .await
        .map_err(|error| SpotifyError::Auth(error.to_string()))?;

    let variables = json!({
        "searchTerm": query,
        "offset": 0,
        "limit": 20,
        "numberOfTopResults": 20,
        "includeAudiobooks": false,
        "includePreReleases": false,
        "includeAlbumPreReleases": false,
        "includeAuthors": false,
        "includeEpisodeContentRatingsV2": true,
    });
    let extensions = json!({
        "persistedQuery": {
            "version": 1,
            "sha256Hash": SEARCH_DESKTOP_HASH,
        }
    });

    let response = reqwest::Client::new()
        .get(PATHFINDER_URL)
        .query(&[
            ("operationName", "searchDesktop"),
            ("variables", &variables.to_string()),
            ("extensions", &extensions.to_string()),
        ])
        .header("Authorization", format!("Bearer {}", token.access_token))
        .header("App-Platform", "WebPlayer")
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("Spotify-App-Version", "1.2.46.428")
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(SpotifyError::Api(format!(
            "Pathfinder returned HTTP {status}: {}",
            truncate(&body, 300)
        )));
    }

    let value: Value = serde_json::from_str(&body)
        .map_err(|error| SpotifyError::Api(format!("invalid Pathfinder JSON: {error}")))?;

    if let Some(errors) = value.get("errors") {
        return Err(SpotifyError::Api(format!(
            "Pathfinder GraphQL error: {}",
            truncate(&errors.to_string(), 300)
        )));
    }

    let search = value
        .pointer("/data/searchV2")
        .ok_or_else(|| SpotifyError::Api("Pathfinder response is missing searchV2".into()))?;

    Ok(parse_search(search))
}

fn parse_search(search: &Value) -> SearchResults {
    let mut results = SearchResults::default();

    if let Some(items) = search
        .pointer("/topResultsV2/itemsV2")
        .and_then(Value::as_array)
    {
        results.top_result = items.iter().find_map(|item| {
            let data = unwrap_item(item);
            item_from_graphql(data, None)
        });
    }

    if let Some(items) = array_any(search, &["/tracksV2/items", "/tracksV2/itemsV2"]) {
        results.tracks = items
            .iter()
            .filter_map(|item| item_from_graphql(unwrap_item(item), Some("Song")))
            .collect();
    }

    if let Some(items) = array_any(search, &["/artists/items", "/artistsV2/items"]) {
        results.artists = items
            .iter()
            .filter_map(|item| item_from_graphql(unwrap_item(item), Some("Artist")))
            .collect();
    }

    if let Some(items) = array_any(search, &["/albumsV2/items", "/albums/items"]) {
        results.albums = items
            .iter()
            .filter_map(|item| item_from_graphql(unwrap_item(item), Some("Album")))
            .collect();
    }

    if let Some(items) = array_any(search, &["/playlists/items", "/playlistsV2/items"]) {
        results.playlists = items
            .iter()
            .filter_map(|item| item_from_graphql(unwrap_item(item), Some("Playlist")))
            .collect();
    }

    let mut podcasts: Vec<SearchItem> =
        array_any(search, &["/podcasts/items", "/podcastsV2/items"])
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item_from_graphql(unwrap_item(item), Some("Podcast")))
                    .collect()
            })
            .unwrap_or_default();

    if let Some(items) = array_any(search, &["/episodes/items", "/episodesV2/items"]) {
        podcasts.extend(
            items
                .iter()
                .filter_map(|item| item_from_graphql(unwrap_item(item), Some("Episode"))),
        );
    }
    results.podcasts = podcasts;

    results
}

fn array_any<'a>(value: &'a Value, pointers: &[&str]) -> Option<&'a Vec<Value>> {
    pointers
        .iter()
        .find_map(|pointer| value.pointer(pointer).and_then(Value::as_array))
}

/// Descends through the wrapper layers used by Pathfinder (`item`, `data`).
fn unwrap_item(value: &Value) -> &Value {
    let candidate = value.get("item").unwrap_or(value);
    let data = candidate.get("data").unwrap_or(candidate);
    if data.is_object() { data } else { candidate }
}

fn item_from_graphql(data: &Value, kind_hint: Option<&'static str>) -> Option<SearchItem> {
    let uri = data.get("uri").and_then(Value::as_str)?.to_string();
    let name = data
        .get("name")
        .or_else(|| data.get("title"))
        .or_else(|| data.pointer("/profile/name"))
        .and_then(Value::as_str)
        .unwrap_or("Unknown")
        .to_string();

    let typename = data
        .get("__typename")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let kind = kind_hint.unwrap_or_else(|| kind_from_uri(&uri, typename));

    let subtitle = subtitle_for(data, kind);
    let image = image_for(data, kind);
    let duration_ms = data
        .pointer("/duration/totalMilliseconds")
        .or_else(|| data.pointer("/duration/ms"))
        .and_then(Value::as_u64);

    Some(SearchItem {
        uri,
        title: name,
        subtitle,
        image,
        kind,
        duration_ms,
    })
}

fn kind_from_uri(uri: &str, typename: &str) -> &'static str {
    if uri.contains(":track:") || typename.contains("Track") {
        "Song"
    } else if uri.contains(":artist:") || typename.contains("Artist") {
        "Artist"
    } else if uri.contains(":album:") || typename.contains("Album") {
        "Album"
    } else if uri.contains(":playlist:") || typename.contains("Playlist") {
        "Playlist"
    } else if uri.contains(":episode:") || typename.contains("Episode") {
        "Episode"
    } else {
        "Show"
    }
}

fn subtitle_for(data: &Value, kind: &str) -> String {
    match kind {
        "Song" | "Album" => {
            let names = string_array_at(data, "/artists/items", "/profile/name")
                .or_else(|| string_array_at(data, "/artists/items", "/name"))
                .or_else(|| string_array_at(data, "/album/artists/items", "/profile/name"))
                .unwrap_or_default();
            if names.is_empty() {
                "Spotify".into()
            } else {
                names.join(", ")
            }
        }
        "Artist" => "Artist".into(),
        "Playlist" => data
            .pointer("/ownerV2/data/name")
            .or_else(|| data.pointer("/owner/displayName"))
            .or_else(|| data.pointer("/owner/name"))
            .and_then(Value::as_str)
            .unwrap_or("Playlist")
            .to_string(),
        "Podcast" => data
            .pointer("/publisher/name")
            .or_else(|| data.pointer("/publisher"))
            .and_then(Value::as_str)
            .unwrap_or("Podcast")
            .to_string(),
        "Episode" => data
            .pointer("/podcastV2/data/name")
            .or_else(|| data.pointer("/podcast/name"))
            .and_then(Value::as_str)
            .unwrap_or("Episode")
            .to_string(),
        _ => String::new(),
    }
}

fn string_array_at(data: &Value, items_pointer: &str, field_pointer: &str) -> Option<Vec<String>> {
    let items = data.pointer(items_pointer)?.as_array()?;
    let names: Vec<String> = items
        .iter()
        .filter_map(|item| item.pointer(field_pointer).and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    (!names.is_empty()).then_some(names)
}

fn image_for(data: &Value, kind: &str) -> Option<String> {
    let sources = match kind {
        "Artist" => first_array_at(
            data,
            &[
                "/visuals/avatarImage/sources",
                "/visuals/headerImage/sources",
            ],
        ),
        "Playlist" => first_array_at(data, &["/images/items/0/sources", "/images/0/sources"]),
        _ => first_array_at(
            data,
            &[
                "/albumOfTrack/coverArt/sources",
                "/coverArt/sources",
                "/images/items/0/sources",
                "/images/0/sources",
            ],
        ),
    }?;

    sources.iter().find_map(|source| {
        source
            .get("url")
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

fn first_array_at<'a>(data: &'a Value, pointers: &[&str]) -> Option<&'a Vec<Value>> {
    pointers
        .iter()
        .find_map(|pointer| data.pointer(pointer).and_then(Value::as_array))
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        format!("{}…", value.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search_desktop_response() {
        let json = serde_json::json!({
            "data": {
                "searchV2": {
                    "topResultsV2": {
                        "itemsV2": [{
                            "item": {
                                "data": {
                                    "__typename": "Track",
                                    "uri": "spotify:track:top",
                                    "name": "Top Track",
                                    "artists": { "items": [{ "profile": { "name": "Top Artist" } }] },
                                    "albumOfTrack": { "coverArt": { "sources": [{ "url": "https://img/top" }] } },
                                    "duration": { "totalMilliseconds": 123000 }
                                }
                            }
                        }]
                    },
                    "tracksV2": {
                        "items": [{
                            "item": {
                                "data": {
                                    "uri": "spotify:track:1",
                                    "name": "Song",
                                    "artists": { "items": [{ "profile": { "name": "Artist A" } }, { "profile": { "name": "Artist B" } }] },
                                    "albumOfTrack": { "coverArt": { "sources": [{ "url": "https://img/1" }] } },
                                    "duration": { "totalMilliseconds": 61000 }
                                }
                            }
                        }]
                    },
                    "artists": {
                        "items": [{
                            "data": {
                                "uri": "spotify:artist:1",
                                "profile": { "name": "An Artist" },
                                "visuals": { "avatarImage": { "sources": [{ "url": "https://img/artist" }] } }
                            }
                        }]
                    },
                    "albumsV2": {
                        "items": [{
                            "data": {
                                "uri": "spotify:album:1",
                                "name": "An Album",
                                "artists": { "items": [{ "profile": { "name": "Album Artist" } }] },
                                "coverArt": { "sources": [{ "url": "https://img/album" }] }
                            }
                        }]
                    },
                    "playlists": {
                        "items": [{
                            "data": {
                                "uri": "spotify:playlist:1",
                                "name": "A Playlist",
                                "ownerV2": { "data": { "name": "Owner" } },
                                "images": { "items": [{ "sources": [{ "url": "https://img/playlist" }] }] }
                            }
                        }]
                    },
                    "podcasts": {
                        "items": [{
                            "data": {
                                "uri": "spotify:show:1",
                                "name": "A Show",
                                "publisher": { "name": "Publisher" },
                                "coverArt": { "sources": [{ "url": "https://img/show" }] }
                            }
                        }]
                    }
                }
            }
        });

        let search = json.pointer("/data/searchV2").unwrap();
        let results = parse_search(search);

        let top = results.top_result.expect("top result");
        assert_eq!(top.title, "Top Track");
        assert_eq!(top.subtitle, "Top Artist");
        assert_eq!(top.image.as_deref(), Some("https://img/top"));

        assert_eq!(results.tracks.len(), 1);
        assert_eq!(results.tracks[0].subtitle, "Artist A, Artist B");
        assert_eq!(results.tracks[0].duration_ms, Some(61000));

        assert_eq!(results.artists[0].title, "An Artist");
        assert_eq!(
            results.artists[0].image.as_deref(),
            Some("https://img/artist")
        );
        assert_eq!(results.albums[0].subtitle, "Album Artist");
        assert_eq!(results.playlists[0].subtitle, "Owner");
        assert_eq!(results.podcasts[0].subtitle, "Publisher");
    }
}
