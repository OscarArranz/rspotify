//! Native playlist and context loading for the playlist detail screen.
//!
//! Playlist metadata comes from Spotify's `playlist4_external` protobuf endpoint
//! (`Playlist::get`). Track metadata and pagination come from the native
//! context-resolve endpoint (`SpClient::get_context` / `get_next_page`).

use std::collections::HashMap;
use std::time::Duration;

use librespot::core::{Session, SpotifyUri};
use librespot::metadata::{Metadata, Playlist};
use librespot::protocol::{context::Context, context_track::ContextTrack};
use serde_json::Value;

use super::error::SpotifyError;

const FALLBACK_ACCENT: u32 = 0x5c44c7;
const LIKED_ACCENT: u32 = 0x4b1fd0;

/// A screen source before the full playlist payload has loaded.
#[derive(Clone, Debug)]
pub enum PlaylistSource {
    Playlist {
        uri: String,
        name: String,
        owner: String,
        image_url: Option<String>,
    },
    LikedSongs,
}

impl PlaylistSource {
    pub fn preview_name(&self) -> &str {
        match self {
            Self::Playlist { name, .. } => name,
            Self::LikedSongs => "Liked Songs",
        }
    }

    pub fn preview_owner(&self) -> &str {
        match self {
            Self::Playlist { owner, .. } => owner,
            Self::LikedSongs => "",
        }
    }

    pub fn preview_image(&self) -> Option<&str> {
        match self {
            Self::Playlist { image_url, .. } => image_url.as_deref(),
            Self::LikedSongs => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PlaylistDetail {
    pub context_uri: String,
    pub name: String,
    pub description: String,
    pub owner_name: String,
    pub owner_image_url: Option<String>,
    pub image_url: Option<String>,
    pub accent: u32,
    pub track_count: Option<usize>,
    pub total_duration_ms: Option<u64>,
    pub is_liked_songs: bool,
    pub tracks: Vec<PlaylistTrack>,
    pub next_page_url: Option<String>,
    pub(crate) added_at: HashMap<String, i64>,
}

#[derive(Clone, Debug, Default)]
pub struct PlaylistTrack {
    pub uri: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub image_url: Option<String>,
    pub duration_ms: u64,
    pub added_at_ms: Option<i64>,
    pub explicit: bool,
    pub playable: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PlaylistPage {
    pub tracks: Vec<PlaylistTrack>,
    pub next_page_url: Option<String>,
}

/// Loads a playlist header plus the first context page.
pub async fn fetch_playlist_detail(
    session: &Session,
    source: PlaylistSource,
) -> Result<PlaylistDetail, SpotifyError> {
    match source {
        PlaylistSource::Playlist {
            uri,
            name,
            owner,
            image_url,
        } => fetch_saved_playlist(session, uri, name, owner, image_url).await,
        PlaylistSource::LikedSongs => fetch_liked_songs(session).await,
    }
}

/// Loads one additional context page for infinite scrolling.
pub async fn fetch_playlist_page(
    session: &Session,
    next_page_url: &str,
) -> Result<PlaylistPage, SpotifyError> {
    let bytes = session
        .spclient()
        .get_next_page(next_page_url)
        .await
        .map_err(|error| SpotifyError::Api(error.to_string()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| SpotifyError::Api(format!("context page JSON: {error}")))?;
    Ok(parse_json_page(&value))
}

async fn fetch_saved_playlist(
    session: &Session,
    uri: String,
    preview_name: String,
    preview_owner: String,
    preview_image: Option<String>,
) -> Result<PlaylistDetail, SpotifyError> {
    let parsed = SpotifyUri::from_uri(&uri)
        .map_err(|error| SpotifyError::Api(format!("playlist URI: {error}")))?;
    let playlist = Playlist::get(session, &parsed)
        .await
        .map_err(|error| SpotifyError::Api(error.to_string()))?;

    let owner_username = match &playlist.id {
        SpotifyUri::Playlist {
            user: Some(user), ..
        } if !user.is_empty() => user.clone(),
        _ if !preview_owner.is_empty() => preview_owner.clone(),
        _ => session.username(),
    };
    let (owner_name, owner_image_url) = fetch_profile(session, &owner_username).await;

    let image_url = playlist_image(&playlist)
        .or(preview_image)
        .filter(|url| !url.is_empty());
    let accent = if let Some(url) = image_url.as_deref() {
        sample_accent(url).await.unwrap_or(FALLBACK_ACCENT)
    } else {
        FALLBACK_ACCENT
    };

    let context = session
        .spclient()
        .get_context(&uri)
        .await
        .map_err(|error| SpotifyError::Api(error.to_string()))?;

    let added_at = added_at_by_uri(&playlist);
    let (tracks, next_page_url) = context_tracks(&context, &added_at);
    let name = non_empty(&playlist.attributes.name)
        .map(ToOwned::to_owned)
        .unwrap_or(preview_name);
    let description = html_to_text(&playlist.attributes.description);
    let total_duration_ms = context_duration_ms(&context);
    let track_count = if playlist.length >= 0 {
        Some((playlist.length as usize).max(tracks.len()))
    } else if next_page_url.is_none() {
        Some(tracks.len())
    } else {
        None
    };

    Ok(PlaylistDetail {
        context_uri: uri,
        name,
        description,
        owner_name,
        owner_image_url,
        image_url,
        accent,
        track_count,
        total_duration_ms,
        is_liked_songs: false,
        tracks,
        next_page_url,
        added_at,
    })
}

async fn fetch_liked_songs(session: &Session) -> Result<PlaylistDetail, SpotifyError> {
    let username = session.username();
    let context_uri = format!("spotify:user:{username}:collection");
    let context = session
        .spclient()
        .get_context(&context_uri)
        .await
        .map_err(|error| SpotifyError::Api(error.to_string()))?;
    let (owner_name, owner_image_url) = fetch_profile(session, &username).await;
    let (tracks, next_page_url) = context_tracks(&context, &HashMap::new());
    let track_count = context_track_count(&context)
        .map(|count| count.max(tracks.len()))
        .or_else(|| next_page_url.is_none().then_some(tracks.len()));

    Ok(PlaylistDetail {
        context_uri,
        name: "Liked Songs".to_string(),
        description: "Your saved tracks".to_string(),
        owner_name,
        owner_image_url,
        image_url: None,
        accent: LIKED_ACCENT,
        track_count,
        total_duration_ms: context_duration_ms(&context),
        is_liked_songs: true,
        tracks,
        next_page_url,
        added_at: HashMap::new(),
    })
}

fn playlist_image(playlist: &Playlist) -> Option<String> {
    playlist
        .attributes
        .picture_sizes
        .iter()
        .filter(|size| !size.url.is_empty())
        .max_by_key(|size| size.target_name.parse::<u32>().unwrap_or(0))
        .map(|size| image_url_from_id(&size.url))
        .filter(|url| !url.is_empty())
}

fn added_at_by_uri(playlist: &Playlist) -> HashMap<String, i64> {
    playlist
        .contents
        .items
        .iter()
        .filter_map(|item| {
            let uri = item.id.to_uri().ok()?;
            let timestamp = item.attributes.timestamp.as_timestamp_ms();
            (timestamp > 0).then_some((uri, timestamp))
        })
        .collect()
}

fn context_tracks(
    context: &Context,
    added_at: &HashMap<String, i64>,
) -> (Vec<PlaylistTrack>, Option<String>) {
    let mut tracks = Vec::new();
    let mut next_page_url = None;
    for page in &context.pages {
        tracks.extend(
            page.tracks
                .iter()
                .map(|track| track_from_context(track, added_at)),
        );
        if let Some(url) = page.next_page_url.as_deref().filter(|url| !url.is_empty()) {
            next_page_url = Some(url.to_string());
        }
    }
    (tracks, next_page_url)
}

fn track_from_context(track: &ContextTrack, added_at: &HashMap<String, i64>) -> PlaylistTrack {
    let uri = track.uri.clone().unwrap_or_default();
    let metadata = &track.metadata;
    track_from_metadata(&uri, metadata, added_at.get(&uri).copied())
}

fn track_from_metadata(
    uri: &str,
    metadata: &HashMap<String, String>,
    added_at_ms: Option<i64>,
) -> PlaylistTrack {
    let title = metadata_value(metadata, &["title", "name", "episode_name", "track_name"])
        .unwrap_or_else(|| "Unknown track".to_string());
    let artist = metadata_value(
        metadata,
        &["artist_name", "artist", "show_name", "album_artist_name"],
    )
    .unwrap_or_default();
    let album = metadata_value(metadata, &["album_name", "album", "show_name"]).unwrap_or_default();
    let image_url = metadata_value(
        metadata,
        &[
            "image_url",
            "image_large_url",
            "image_small_url",
            "album_image_url",
        ],
    )
    .map(|url| image_url_from_id(&url))
    .filter(|url| !url.is_empty());
    let duration_ms = metadata_value(metadata, &["duration", "duration_ms"])
        .and_then(|value| parse_millis(&value))
        .unwrap_or(0);
    let explicit = metadata_value(metadata, &["is_explicit", "explicit"])
        .is_some_and(|value| value == "true" || value == "1");
    let playable = !uri.is_empty()
        && !metadata_value(metadata, &["is_playable"])
            .is_some_and(|value| value == "false" || value == "0");

    PlaylistTrack {
        uri: uri.to_string(),
        title,
        artist,
        album,
        image_url,
        duration_ms,
        added_at_ms,
        explicit,
        playable,
    }
}

fn parse_json_page(value: &Value) -> PlaylistPage {
    let mut tracks = Vec::new();
    if let Some(items) = value.get("tracks").and_then(Value::as_array) {
        for item in items {
            let uri = item.get("uri").and_then(Value::as_str).unwrap_or_default();
            let metadata = item
                .get("metadata")
                .and_then(Value::as_object)
                .map(|metadata| {
                    metadata
                        .iter()
                        .filter_map(|(key, value)| {
                            value.as_str().map(|value| (key.clone(), value.to_string()))
                        })
                        .collect::<HashMap<_, _>>()
                })
                .unwrap_or_default();
            tracks.push(track_from_metadata(uri, &metadata, None));
        }
    }

    PlaylistPage {
        tracks,
        next_page_url: value
            .get("next_page_url")
            .and_then(Value::as_str)
            .filter(|url| !url.is_empty())
            .map(ToOwned::to_owned),
    }
}

fn context_duration_ms(context: &Context) -> Option<u64> {
    duration_from_map(&context.metadata)
}

fn context_track_count(context: &Context) -> Option<usize> {
    for key in ["total_tracks", "total_track_count", "length"] {
        if let Some(value) = context.metadata.get(key) {
            if let Ok(count) = value.parse::<usize>() {
                return Some(count);
            }
        }
    }
    None
}

fn duration_from_map(metadata: &HashMap<String, String>) -> Option<u64> {
    for key in ["duration", "total_duration", "total_duration_ms"] {
        if let Some(value) = metadata
            .get(key)
            .and_then(|value| parse_millis(value))
            .filter(|value| *value > 0)
        {
            return Some(value);
        }
    }
    None
}

fn parse_millis(value: &str) -> Option<u64> {
    value
        .parse::<u64>()
        .ok()
        .or_else(|| value.parse::<f64>().ok().map(|value| value.max(0.0) as u64))
}

fn metadata_value(metadata: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| metadata.get(*key))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn non_empty(value: &str) -> Option<&str> {
    (!value.trim().is_empty()).then_some(value.trim())
}

fn image_url_from_id(id: &str) -> String {
    let id = id.strip_prefix("spotify:image:").unwrap_or(id);
    if id.is_empty() {
        String::new()
    } else if id.starts_with("http://") || id.starts_with("https://") {
        id.to_string()
    } else {
        format!("https://i.scdn.co/image/{id}")
    }
}

async fn fetch_profile(session: &Session, username: &str) -> (String, Option<String>) {
    let fallback = if username.is_empty() {
        session.username()
    } else {
        username.to_string()
    };
    let Ok(bytes) = session
        .spclient()
        .get_user_profile(&fallback, None, None)
        .await
    else {
        return (fallback, None);
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return (fallback, None);
    };
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or(fallback);
    (name, image_from_json(&value))
}

fn image_from_json(value: &Value) -> Option<String> {
    for pointer in [
        "/image_uri",
        "/imageUri",
        "/image_url",
        "/imageUrl",
        "/images/0/url",
        "/images/0/sources/0/url",
        "/images/items/0/sources/0/url",
        "/avatarImage/sources/0/url",
    ] {
        if let Some(url) = value.pointer(pointer).and_then(Value::as_str) {
            if !url.is_empty() {
                let url = image_url_from_id(url);
                if !url.is_empty() {
                    return Some(url);
                }
            }
        }
    }
    None
}

async fn sample_accent(url: &str) -> Option<u32> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .ok()?;
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = response.bytes().await.ok()?;
    let image = image::load_from_memory(&bytes).ok()?;
    let image = image
        .resize_exact(8, 8, image::imageops::FilterType::Triangle)
        .to_rgba8();

    let mut sum_r = 0u64;
    let mut sum_g = 0u64;
    let mut sum_b = 0u64;
    let mut weight = 0u64;
    for pixel in image.pixels() {
        if pixel[3] < 32 {
            continue;
        }
        let r = pixel[0] as u64;
        let g = pixel[1] as u64;
        let b = pixel[2] as u64;
        let luma = (r * 3 + g * 6 + b) / 10;
        if luma < 12 {
            continue;
        }
        let saturation = r.max(g).max(b) - r.min(g).min(b);
        let w = 1 + saturation / 16;
        sum_r += r * w;
        sum_g += g * w;
        sum_b += b * w;
        weight += w;
    }
    if weight == 0 {
        return None;
    }

    let mut r = (sum_r / weight) as f32;
    let mut g = (sum_g / weight) as f32;
    let mut b = (sum_b / weight) as f32;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    if max > 0.0 {
        let boost = (max - min) / max;
        if boost < 0.35 {
            let gray = (r + g + b) / 3.0;
            let factor = 0.35 / boost.max(0.05);
            r = (gray + (r - gray) * factor).clamp(0.0, 255.0);
            g = (gray + (g - gray) * factor).clamp(0.0, 255.0);
            b = (gray + (b - gray) * factor).clamp(0.0, 255.0);
        }
    }
    let luma = (r * 0.299 + g * 0.587 + b * 0.114).clamp(0.0, 255.0);
    let scale = if luma < 58.0 {
        58.0 / luma.max(1.0)
    } else {
        1.0
    };
    r = (r * scale).clamp(0.0, 255.0);
    g = (g * scale).clamp(0.0, 255.0);
    b = (b * scale).clamp(0.0, 255.0);

    Some(((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
}

fn html_to_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_tag = false;
    for character in input.chars() {
        match character {
            '<' => {
                in_tag = true;
                if !output.ends_with(' ') {
                    output.push(' ');
                }
            }
            '>' => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }

    let decoded = output
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_context_track_metadata() {
        let metadata = HashMap::from([
            ("title".to_string(), "Get Lucky".to_string()),
            ("artist_name".to_string(), "Daft Punk".to_string()),
            (
                "album_name".to_string(),
                "Random Access Memories".to_string(),
            ),
            ("image_url".to_string(), "spotify:image:abc123".to_string()),
            ("duration".to_string(), "369000".to_string()),
            ("is_explicit".to_string(), "true".to_string()),
        ]);
        let track = track_from_metadata("spotify:track:abc123", &metadata, Some(1_700_000_000_000));
        assert_eq!(track.title, "Get Lucky");
        assert_eq!(track.artist, "Daft Punk");
        assert_eq!(track.album, "Random Access Memories");
        assert_eq!(
            track.image_url.as_deref(),
            Some("https://i.scdn.co/image/abc123")
        );
        assert_eq!(track.duration_ms, 369_000);
        assert_eq!(track.added_at_ms, Some(1_700_000_000_000));
        assert!(track.explicit);
        assert!(track.playable);
    }

    #[test]
    fn parses_context_page_json() {
        let value = serde_json::json!({
            "tracks": [
                {
                    "uri": "spotify:track:one",
                    "metadata": {
                        "title": "One",
                        "artist_name": "Artist",
                        "album_name": "Album",
                        "duration": "180000"
                    }
                },
                {
                    "uri": "spotify:episode:two",
                    "metadata": {
                        "episode_name": "Episode",
                        "show_name": "Show",
                        "duration": "60000",
                        "is_playable": "false"
                    }
                }
            ],
            "next_page_url": "hm:/context-resolve/v1/next"
        });
        let page = parse_json_page(&value);
        assert_eq!(page.tracks.len(), 2);
        assert_eq!(page.tracks[0].title, "One");
        assert_eq!(page.tracks[1].title, "Episode");
        assert_eq!(page.tracks[1].artist, "Show");
        assert!(!page.tracks[1].playable);
        assert_eq!(
            page.next_page_url.as_deref(),
            Some("hm:/context-resolve/v1/next")
        );
    }

    #[test]
    fn sanitizes_html_descriptions() {
        assert_eq!(
            html_to_text("<p>The <b>best</b> of 2024 &amp; more</p>"),
            "The best of 2024 & more"
        );
    }
}
