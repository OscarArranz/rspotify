//! Native playlist and context loading for the playlist detail screen.
//!
//! Playlist metadata comes from Spotify's `playlist4_external` protobuf endpoint
//! (`Playlist::get`). Track metadata and pagination come from the native
//! context-resolve endpoint (`SpClient::get_context` / `get_next_page`).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use librespot::core::{Session, SpotifyId, SpotifyUri};
use librespot::metadata::{Metadata, Playlist};
use librespot::protocol::{
    context::Context,
    context_track::ContextTrack,
    extended_metadata::{BatchedEntityRequest, EntityRequest, ExtensionQuery},
    extension_kind::ExtensionKind,
    metadata::{Episode as EpisodeMessage, Track as TrackMessage},
};
use protobuf::{EnumOrUnknown, Message};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::client::ArtistCredit;
use super::error::SpotifyError;

const FALLBACK_ACCENT: u32 = 0x5c44c7;
const LIKED_ACCENT: u32 = 0x4b1fd0;
/// Extended-metadata accepts at least 1,000 entities per request. Stay well below
/// that limit so large collections do not hit the endpoint's 413 response.
const METADATA_BATCH_SIZE: usize = 500;

/// A screen source before the full playlist payload has loaded.
#[derive(Clone, Debug)]
pub enum PlaylistSource {
    Playlist {
        uri: String,
        name: String,
        owner: String,
        image_url: Option<String>,
    },
    Context {
        uri: String,
        name: String,
        owner: String,
        image_url: Option<String>,
        kind: &'static str,
    },
    LikedSongs,
}

impl PlaylistSource {
    pub fn preview_name(&self) -> &str {
        match self {
            Self::Playlist { name, .. } | Self::Context { name, .. } => name,
            Self::LikedSongs => "Liked Songs",
        }
    }

    pub fn preview_owner(&self) -> &str {
        match self {
            Self::Playlist { owner, .. } | Self::Context { owner, .. } => owner,
            Self::LikedSongs => "",
        }
    }

    pub fn preview_image(&self) -> Option<&str> {
        match self {
            Self::Playlist { image_url, .. } | Self::Context { image_url, .. } => {
                image_url.as_deref()
            }
            Self::LikedSongs => None,
        }
    }

    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Context { kind, .. } => kind,
            _ => "Playlist",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlaylistTrack {
    pub uri: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub artist_credits: Vec<ArtistCredit>,
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

fn detail_cache_path(account: &str, source: &PlaylistSource) -> PathBuf {
    let identity = match source {
        PlaylistSource::Playlist { uri, .. } | PlaylistSource::Context { uri, .. } => uri.as_str(),
        // Use the same collection URI used by the native context request. Keep the
        // account in the key as an extra guard against sharing private library data.
        PlaylistSource::LikedSongs => {
            return detail_cache_path_for_identity(
                account,
                &format!("spotify:user:{account}:collection"),
            );
        }
    };
    detail_cache_path_for_identity(account, identity)
}

fn detail_cache_path_for_identity(account: &str, identity: &str) -> PathBuf {
    let key = format!("{account}\0{identity}");
    let filename = key
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let root = std::env::var_os("RSPOTIFY_STATE_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .map(|p| p.join("rspotify"))
        })
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|p| p.join(".cache/rspotify"))
        })
        .unwrap_or_else(|| std::env::temp_dir().join("rspotify"));
    root.join("playlists").join(format!("{filename}.json"))
}

pub fn read_cached_detail(account: &str, source: &PlaylistSource) -> Option<PlaylistDetail> {
    fs::read(detail_cache_path(account, source))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .or_else(|| {
            // Migrate caches created by the initial optimistic-loading implementation.
            let PlaylistSource::LikedSongs = source else {
                return None;
            };
            fs::read(detail_cache_path_for_identity(account, "liked-songs"))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        })
        .map(|mut detail: PlaylistDetail| {
            for track in &mut detail.tracks {
                for credit in &mut track.artist_credits {
                    if let Some(id) = credit.uri.strip_prefix("spotify:artist:") {
                        if id.len() == 32 {
                            credit.uri = SpotifyId::from_base16(id)
                                .and_then(|id| id.to_base62())
                                .map(|id| format!("spotify:artist:{id}"))
                                .unwrap_or_default();
                        }
                    }
                }
            }
            detail
        })
}

pub fn write_cached_detail(account: &str, source: &PlaylistSource, detail: &PlaylistDetail) {
    let path = detail_cache_path(account, source);
    let Some(parent) = path.parent() else { return };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }
    let Ok(bytes) = serde_json::to_vec(detail) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, bytes).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
        }
        let _ = fs::rename(tmp, path);
    }
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
        PlaylistSource::Context {
            uri,
            name,
            owner,
            image_url,
            ..
        } => fetch_catalog_context(session, uri, name, owner, image_url).await,
    }
}

async fn fetch_catalog_context(
    session: &Session,
    uri: String,
    name: String,
    owner_name: String,
    image_url: Option<String>,
) -> Result<PlaylistDetail, SpotifyError> {
    let context = session
        .spclient()
        .get_context(&uri)
        .await
        .map_err(|e| SpotifyError::Api(e.to_string()))?;
    let (mut tracks, next_page_url) = context_tracks(&context, &HashMap::new());
    hydrate_tracks(session, &mut tracks).await;
    let track_count =
        context_track_count(&context).or_else(|| next_page_url.is_none().then_some(tracks.len()));
    let accent = if let Some(url) = image_url.as_deref() {
        sample_accent(url).await.unwrap_or(FALLBACK_ACCENT)
    } else {
        FALLBACK_ACCENT
    };
    Ok(PlaylistDetail {
        context_uri: uri,
        name,
        owner_name,
        image_url,
        accent,
        track_count,
        tracks,
        next_page_url,
        total_duration_ms: context_duration_ms(&context),
        ..Default::default()
    })
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
    let mut page = parse_json_page(&value);
    hydrate_tracks(session, &mut page.tracks).await;
    Ok(page)
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
    let (mut tracks, next_page_url) = context_tracks(&context, &added_at);
    hydrate_tracks(session, &mut tracks).await;
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
    let (mut tracks, next_page_url) = context_tracks(&context, &HashMap::new());
    hydrate_tracks(session, &mut tracks).await;
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
    let from_sizes = playlist
        .attributes
        .picture_sizes
        .iter()
        .filter(|size| !size.url.is_empty())
        .max_by_key(|size| size.target_name.parse::<u32>().unwrap_or(0))
        .map(|size| image_url_from_id(&size.url))
        .filter(|url| !url.is_empty());
    from_sizes.or_else(|| image_url_from_picture(&playlist.attributes.picture))
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
        artist_credits: Vec::new(),
        album,
        image_url,
        duration_ms,
        added_at_ms,
        explicit,
        playable,
    }
}

fn track_from_context(track: &ContextTrack, added_at: &HashMap<String, i64>) -> PlaylistTrack {
    let uri = track.uri.clone().unwrap_or_default();
    let metadata = &track.metadata;
    let added_at_ms = added_at.get(&uri).copied().or_else(|| {
        metadata_value(metadata, &["added_at"]).and_then(|value| parse_timestamp_ms(&value))
    });
    track_from_metadata(&uri, metadata, added_at_ms)
}

#[derive(Clone, Debug, Default)]
pub(crate) struct HydratedTrack {
    pub(crate) title: String,
    pub(crate) artist: String,
    pub(crate) artist_credits: Vec<ArtistCredit>,
    pub(crate) album: String,
    pub(crate) image_url: Option<String>,
    pub(crate) duration_ms: u64,
    explicit: bool,
}

pub(crate) async fn fetch_track_metadata(
    session: &Session,
    uri: &str,
) -> Result<Option<HydratedTrack>, SpotifyError> {
    let kind = if uri.starts_with("spotify:episode:") {
        ExtensionKind::EPISODE_V4
    } else if uri.starts_with("spotify:track:") {
        ExtensionKind::TRACK_V4
    } else {
        return Ok(None);
    };
    let uris = [uri.to_string()];
    Ok(fetch_metadata_batch(session, &uris, kind)
        .await?
        .remove(uri))
}

/// Fills in titles, artists, albums, artwork, and durations for context tracks
/// that only carry URIs. Collection contexts and some playlist context pages
/// return `added_at` but no display metadata, so it is fetched in large batches
/// from the native extended-metadata endpoint.
async fn hydrate_tracks(session: &Session, tracks: &mut [PlaylistTrack]) {
    let mut track_uris = Vec::new();
    let mut episode_uris = Vec::new();
    let mut seen = HashSet::new();

    for track in tracks.iter() {
        if !needs_metadata(track) || !seen.insert(track.uri.clone()) {
            continue;
        }
        if track.uri.starts_with("spotify:track:") {
            track_uris.push(track.uri.clone());
        } else if track.uri.starts_with("spotify:episode:") {
            episode_uris.push(track.uri.clone());
        }
    }

    let mut hydrated = HashMap::new();
    for chunk in track_uris.chunks(METADATA_BATCH_SIZE) {
        match fetch_metadata_batch(session, chunk, ExtensionKind::TRACK_V4).await {
            Ok(entries) => hydrated.extend(entries),
            Err(error) => eprintln!("playlist track metadata batch failed: {error}"),
        }
    }
    for chunk in episode_uris.chunks(METADATA_BATCH_SIZE) {
        match fetch_metadata_batch(session, chunk, ExtensionKind::EPISODE_V4).await {
            Ok(entries) => hydrated.extend(entries),
            Err(error) => eprintln!("playlist episode metadata batch failed: {error}"),
        }
    }

    if hydrated.is_empty() {
        return;
    }
    for track in tracks.iter_mut() {
        if let Some(metadata) = hydrated.get(&track.uri) {
            apply_hydrated(track, metadata);
        }
    }
}

fn needs_metadata(track: &PlaylistTrack) -> bool {
    (track.uri.starts_with("spotify:track:") || track.uri.starts_with("spotify:episode:"))
        && (track.title.is_empty()
            || track.title == "Unknown track"
            || track.artist.is_empty()
            || (track.uri.starts_with("spotify:track:") && track.artist_credits.is_empty())
            || track.image_url.is_none()
            || track.duration_ms == 0)
}

async fn fetch_metadata_batch(
    session: &Session,
    uris: &[String],
    kind: ExtensionKind,
) -> Result<HashMap<String, HydratedTrack>, SpotifyError> {
    let request = BatchedEntityRequest {
        entity_request: uris
            .iter()
            .map(|uri| EntityRequest {
                entity_uri: uri.clone(),
                query: vec![ExtensionQuery {
                    extension_kind: EnumOrUnknown::new(kind),
                    ..Default::default()
                }],
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let response = session
        .spclient()
        .get_extended_metadata(request)
        .await
        .map_err(|error| SpotifyError::Api(error.to_string()))?;

    let mut hydrated = HashMap::new();
    for array in response.extended_metadata {
        let array_kind = array.extension_kind.enum_value().ok();
        for data in array.extension_data {
            if data
                .header
                .as_ref()
                .is_some_and(|header| header.status_code != 0 && header.status_code != 200)
            {
                continue;
            }
            let uri = data.entity_uri;
            let Some(any) = data.extension_data.as_ref() else {
                continue;
            };
            if uri.is_empty() {
                continue;
            }
            let metadata = match array_kind {
                Some(ExtensionKind::TRACK_V4) => TrackMessage::parse_from_bytes(&any.value)
                    .ok()
                    .and_then(|message| hydrated_track_from_message(&message)),
                Some(ExtensionKind::EPISODE_V4) => EpisodeMessage::parse_from_bytes(&any.value)
                    .ok()
                    .and_then(|message| hydrated_episode_from_message(&message)),
                _ => None,
            };
            if let Some(metadata) = metadata {
                hydrated.insert(uri, metadata);
            }
        }
    }
    Ok(hydrated)
}

fn hydrated_track_from_message(track: &TrackMessage) -> Option<HydratedTrack> {
    let title = track.name().trim().to_string();
    if title.is_empty() {
        return None;
    }
    let artist = track
        .artist
        .iter()
        .filter_map(|artist| non_empty(artist.name()))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>()
        .join(", ");
    let artist_credits = track
        .artist
        .iter()
        .filter_map(|artist| {
            let name = artist.name.as_deref()?.trim();
            if name.is_empty() {
                return None;
            }
            let uri = artist
                .gid
                .as_deref()
                .and_then(|gid| SpotifyId::from_raw(gid).ok())
                .and_then(|id| id.to_base62().ok())
                .map(|id| format!("spotify:artist:{id}"))
                .unwrap_or_default();
            Some(ArtistCredit {
                name: name.to_string(),
                uri,
            })
        })
        .collect();
    let album = track
        .album
        .as_ref()
        .map(|album| album.name().trim().to_string())
        .unwrap_or_default();
    let image_url = track.album.as_ref().and_then(album_image_url);

    Some(HydratedTrack {
        title,
        artist,
        artist_credits,
        album,
        image_url,
        duration_ms: track.duration().max(0) as u64,
        explicit: track.explicit(),
    })
}

fn hydrated_episode_from_message(episode: &EpisodeMessage) -> Option<HydratedTrack> {
    let title = episode.name().trim().to_string();
    if title.is_empty() {
        return None;
    }
    let show_name = episode
        .show
        .as_ref()
        .map(|show| show.name().trim().to_string())
        .unwrap_or_default();
    let image_url = episode.cover_image.as_ref().and_then(image_group_url);

    Some(HydratedTrack {
        title,
        artist: show_name.clone(),
        artist_credits: Vec::new(),
        album: show_name,
        image_url,
        duration_ms: episode.duration().max(0) as u64,
        explicit: episode.explicit(),
    })
}

fn album_image_url(album: &librespot::protocol::metadata::Album) -> Option<String> {
    let group_image = album
        .cover_group
        .as_ref()
        .map(|group| group.image.iter().max_by_key(image_area))
        .unwrap_or(None);
    group_image
        .or_else(|| album.cover.iter().max_by_key(image_area))
        .and_then(image_message_url)
}

fn image_group_url(group: &librespot::protocol::metadata::ImageGroup) -> Option<String> {
    group
        .image
        .iter()
        .max_by_key(image_area)
        .and_then(image_message_url)
}

fn image_area(image: &&librespot::protocol::metadata::Image) -> i32 {
    image.width().max(0).saturating_mul(image.height().max(0))
}

fn image_message_url(image: &librespot::protocol::metadata::Image) -> Option<String> {
    image.file_id.as_deref().and_then(image_url_from_picture)
}

fn apply_hydrated(track: &mut PlaylistTrack, metadata: &HydratedTrack) {
    if !metadata.title.is_empty() {
        track.title = metadata.title.clone();
    }
    if !metadata.artist.is_empty() {
        track.artist = metadata.artist.clone();
    }
    if !metadata.artist_credits.is_empty() {
        track.artist_credits = metadata.artist_credits.clone();
    }
    if !metadata.album.is_empty() {
        track.album = metadata.album.clone();
    }
    if metadata.image_url.is_some() {
        track.image_url = metadata.image_url.clone();
    }
    if metadata.duration_ms > 0 {
        track.duration_ms = metadata.duration_ms;
    }
    track.explicit |= metadata.explicit;
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
            let added_at_ms = metadata
                .get("added_at")
                .and_then(|value| parse_timestamp_ms(value));
            tracks.push(track_from_metadata(uri, &metadata, added_at_ms));
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

fn parse_timestamp_ms(value: &str) -> Option<i64> {
    let raw = value
        .trim()
        .parse::<i64>()
        .ok()
        .or_else(|| value.trim().parse::<f64>().ok().map(|value| value as i64))?;
    (raw > 0).then(|| {
        if raw < 10_000_000_000 {
            raw.saturating_mul(1000)
        } else {
            raw
        }
    })
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

fn image_url_from_picture(bytes: &[u8]) -> Option<String> {
    if bytes.len() != 20 {
        return None;
    }
    let id = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let url = image_url_from_id(&id);
    (!url.is_empty()).then_some(url)
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
    fn cached_detail_round_trips_and_corrupt_data_is_ignored() {
        let source = PlaylistSource::Playlist {
            uri: "spotify:playlist:cache-test".into(),
            name: "Preview".into(),
            owner: "owner".into(),
            image_url: None,
        };
        let detail = PlaylistDetail {
            context_uri: "spotify:playlist:cache-test".into(),
            name: "Cached playlist".into(),
            tracks: vec![PlaylistTrack {
                title: "Cached track".into(),
                artist_credits: vec![ArtistCredit {
                    name: "Artist".into(),
                    uri: format!("spotify:artist:{}", "12".repeat(16)),
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let path = detail_cache_path("cache-test-account", &source);
        write_cached_detail("cache-test-account", &source, &detail);
        let loaded = read_cached_detail("cache-test-account", &source).unwrap();
        assert_eq!(loaded.name, detail.name);
        assert_eq!(loaded.tracks[0].title, "Cached track");
        let id = SpotifyId::from_raw(&[0x12; 16])
            .unwrap()
            .to_base62()
            .unwrap();
        assert_eq!(
            loaded.tracks[0].artist_credits[0].uri,
            format!("spotify:artist:{id}")
        );
        fs::write(&path, b"not json").unwrap();
        assert!(read_cached_detail("cache-test-account", &source).is_none());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn liked_songs_cache_is_scoped_to_the_account_collection() {
        let detail = PlaylistDetail {
            context_uri: "spotify:user:cache-test-account:collection".into(),
            name: "Liked Songs".into(),
            tracks: vec![
                PlaylistTrack {
                    title: "Saved track".into(),
                    ..Default::default()
                };
                5855
            ],
            ..Default::default()
        };
        let source = PlaylistSource::LikedSongs;
        write_cached_detail("cache-test-account", &source, &detail);
        assert_eq!(
            read_cached_detail("cache-test-account", &source)
                .unwrap()
                .tracks
                .len(),
            5855
        );
        assert_eq!(
            read_cached_detail("cache-test-account", &source)
                .unwrap()
                .tracks[0]
                .title,
            "Saved track"
        );
        assert!(read_cached_detail("another-account", &source).is_none());
        let _ = fs::remove_file(detail_cache_path("cache-test-account", &source));
    }

    #[test]
    fn reads_legacy_liked_songs_cache_key() {
        let source = PlaylistSource::LikedSongs;
        let legacy_path = detail_cache_path_for_identity("legacy-cache-account", "liked-songs");
        fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
        fs::write(
            &legacy_path,
            serde_json::to_vec(&PlaylistDetail {
                name: "Legacy Liked Songs".into(),
                ..Default::default()
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            read_cached_detail("legacy-cache-account", &source)
                .unwrap()
                .name,
            "Legacy Liked Songs"
        );
        let _ = fs::remove_file(legacy_path);
    }

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
                        "duration": "180000",
                        "added_at": "1791840911000"
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
        assert_eq!(page.tracks[0].added_at_ms, Some(1_791_840_911_000));
        assert_eq!(page.tracks[1].title, "Episode");
        assert_eq!(page.tracks[1].artist, "Show");
        assert!(!page.tracks[1].playable);
        assert_eq!(
            page.next_page_url.as_deref(),
            Some("hm:/context-resolve/v1/next")
        );
    }

    #[test]
    fn hydrates_track_metadata_from_protobuf() {
        use librespot::protocol::metadata::{Album, Artist, Image, ImageGroup};
        use protobuf::MessageField;

        let mut image = Image::new();
        image.file_id = Some(vec![0xab; 20]);
        image.width = Some(300);
        image.height = Some(300);
        let mut cover_group = ImageGroup::new();
        cover_group.image.push(image);

        let mut album = Album::new();
        album.name = Some("Album".to_string());
        album.cover_group = MessageField::some(cover_group);

        let mut artist = Artist::new();
        artist.name = Some("Artist".to_string());
        artist.gid = Some(vec![0x12; 16]);

        let mut message = TrackMessage::new();
        message.name = Some("Title".to_string());
        message.album = MessageField::some(album);
        message.artist.push(artist);
        message.duration = Some(123_456);
        message.explicit = Some(true);

        let hydrated = hydrated_track_from_message(&message).expect("hydrated track");
        assert_eq!(hydrated.title, "Title");
        assert_eq!(hydrated.artist, "Artist");
        assert_eq!(hydrated.artist_credits.len(), 1);
        assert_eq!(hydrated.artist_credits[0].name, "Artist");
        let id = SpotifyId::from_raw(&[0x12; 16])
            .unwrap()
            .to_base62()
            .unwrap();
        assert_eq!(
            hydrated.artist_credits[0].uri,
            format!("spotify:artist:{id}")
        );
        assert_eq!(hydrated.album, "Album");
        assert_eq!(hydrated.duration_ms, 123_456);
        assert!(hydrated.explicit);
        let expected_image = format!("https://i.scdn.co/image/{}", "ab".repeat(20));
        assert_eq!(hydrated.image_url.as_deref(), Some(expected_image.as_str()));
    }

    #[test]
    fn reads_added_at_from_collection_context() {
        let mut track = ContextTrack::new();
        track.uri = Some("spotify:track:one".to_string());
        track
            .metadata
            .insert("added_at".to_string(), "1791840911000".to_string());

        let track = track_from_context(&track, &HashMap::new());
        assert_eq!(track.added_at_ms, Some(1_791_840_911_000));
        assert_eq!(parse_timestamp_ms("1791840911"), Some(1_791_840_911_000));
        assert_eq!(parse_timestamp_ms("0"), None);
    }

    #[test]
    fn builds_image_url_from_raw_picture_id() {
        let expected = format!("https://i.scdn.co/image/{}", "ab".repeat(20));
        assert_eq!(
            image_url_from_picture(&[0xab; 20]).as_deref(),
            Some(expected.as_str())
        );
        assert_eq!(image_url_from_picture(&[]), None);
        assert_eq!(image_url_from_picture(&[0xab; 19]), None);
    }

    #[test]
    fn sanitizes_html_descriptions() {
        assert_eq!(
            html_to_text("<p>The <b>best</b> of 2024 &amp; more</p>"),
            "The best of 2024 & more"
        );
    }
}
