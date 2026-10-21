//! Native Spotify client: OAuth login, librespot session, Pathfinder search and
//! Spotify Connect playback.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use librespot::connect::{ConnectConfig, LoadRequest, LoadRequestOptions, PlayingTrack, Spirc};
use librespot::core::{
    Session, SessionConfig, authentication::Credentials, cache::Cache, config::DeviceType,
};
use librespot::oauth::OAuthClientBuilder;
use librespot::playback::{
    audio_backend,
    config::{AudioFormat, PlayerConfig},
    mixer::{Mixer, MixerConfig, softmixer::SoftMixer},
    player::{Player, PlayerEvent},
};
use librespot::protocol::playlist4_external::SelectedListContent;
use protobuf::Message;
use serde_json::Value;

use super::error::SpotifyError;
use super::search::{self, SearchResults};
use super::store::SessionStore;

/// Spotify's own desktop client id. The same id is used by librespot for keymaster
/// requests and is allowed to use the loopback OAuth redirect below.
pub const NATIVE_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";
/// Loopback redirect used by the official desktop client / librespot.
pub const NATIVE_REDIRECT_URI: &str = "http://127.0.0.1:8898/login";

/// Scopes the official desktop client requests for full playback + library access.
const OAUTH_SCOPES: &[&str] = &[
    "app-remote-control",
    "playlist-modify",
    "playlist-modify-private",
    "playlist-modify-public",
    "playlist-read",
    "playlist-read-collaborative",
    "playlist-read-private",
    "streaming",
    "ugc-image-upload",
    "user-follow-modify",
    "user-follow-read",
    "user-library-modify",
    "user-library-read",
    "user-modify",
    "user-modify-playback-state",
    "user-modify-private",
    "user-personalized",
    "user-read-birthdate",
    "user-read-currently-playing",
    "user-read-email",
    "user-read-play-history",
    "user-read-playback-position",
    "user-read-playback-state",
    "user-read-private",
    "user-read-recently-played",
    "user-top-read",
];

const CONNECT_DEVICE_NAME: &str = "rspotify";
const OAUTH_SUCCESS_HTML: &str = r#"<!doctype html>
<html>
  <body style="margin:0;height:100vh;display:flex;align-items:center;justify-content:center;background:#121212;color:#fff;font-family:sans-serif">
    <div style="text-align:center">
      <h1 style="color:#1ed760">Login successful</h1>
      <p>You can close this tab and return to rspotify.</p>
    </div>
  </body>
</html>"#;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NowPlaying {
    pub uri: String,
    pub title: String,
    pub artists: String,
    pub album: String,
    pub cover_url: Option<String>,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackSnapshot {
    pub now_playing: Option<NowPlaying>,
    pub is_playing: bool,
    pub position_ms: u64,
    pub volume_percent: u8,
    pub shuffle: bool,
    pub repeat: bool,
}

impl Default for PlaybackSnapshot {
    fn default() -> Self {
        Self {
            now_playing: None,
            is_playing: false,
            position_ms: 0,
            volume_percent: 50,
            shuffle: false,
            repeat: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct UserProfile {
    pub name: String,
    pub image_url: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct LibraryPlaylist {
    pub uri: String,
    pub name: String,
    pub owner: String,
    pub image_url: Option<String>,
}

struct PlaybackInner {
    snapshot: PlaybackSnapshot,
    updated_at: Instant,
}

struct NativeInner {
    session: Session,
    spirc: Spirc,
    playback: Arc<Mutex<PlaybackInner>>,
    runtime: Arc<tokio::runtime::Runtime>,
    username: String,
}

/// Cloneable handle to an authenticated native Spotify session.
#[derive(Clone)]
pub struct NativeSpotify {
    inner: Arc<NativeInner>,
}

impl NativeSpotify {
    /// Performs the interactive desktop OAuth flow and connects a librespot session.
    ///
    /// This is blocking and must run on a background thread. It opens the user's
    /// browser at `accounts.spotify.com`, receives the loopback callback and then
    /// authenticates against `login5.spotify.com` through librespot.
    pub fn authenticate() -> Result<Self, SpotifyError> {
        let mut store = SessionStore::load();
        let device_id = store.ensure_device_id();

        let client_id =
            std::env::var("RSPOTIFY_NATIVE_CLIENT_ID").unwrap_or_else(|_| NATIVE_CLIENT_ID.into());
        let redirect_uri =
            std::env::var("RSPOTIFY_REDIRECT_URI").unwrap_or_else(|_| NATIVE_REDIRECT_URI.into());

        let oauth = OAuthClientBuilder::new(&client_id, &redirect_uri, OAUTH_SCOPES.to_vec())
            .open_in_browser()
            .with_custom_message(OAUTH_SUCCESS_HTML)
            .build()?;

        // Reuse a stored refresh token when possible, otherwise show the browser flow.
        let mut token = match store.refresh_token() {
            Some(refresh) => match oauth.refresh_token(refresh) {
                Ok(token) => token,
                Err(error) => {
                    eprintln!("stored Spotify session expired ({error}); signing in again");
                    oauth.get_access_token()?
                }
            },
            None => oauth.get_access_token()?,
        };

        if token.refresh_token.is_empty() {
            if let Some(previous) = store.refresh_token() {
                token.refresh_token = previous.to_string();
            }
        }
        if !token.refresh_token.is_empty() {
            store.set_refresh_token(&token.refresh_token);
        }

        Self::connect(token.access_token, device_id, client_id)
    }

    fn connect(
        access_token: String,
        device_id: String,
        client_id: String,
    ) -> Result<Self, SpotifyError> {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|error| SpotifyError::Session(error.to_string()))?,
        );

        let cache_dir = session_cache_dir();
        std::fs::create_dir_all(&cache_dir)?;
        restrict_private_dir(&cache_dir);
        let cache = Cache::new(
            Some(cache_dir.as_path()),
            Some(cache_dir.as_path()),
            None,
            None,
        )?;

        let session_config = SessionConfig {
            client_id,
            device_id,
            tmp_dir: cache_dir,
            autoplay: Some(true),
            ..Default::default()
        };
        // librespot captures `tokio::runtime::Handle::current()`, so the session must be
        // constructed while the runtime context is active.
        let session = {
            let _guard = runtime.enter();
            Session::new(session_config, Some(cache))
        };

        let credentials = Credentials::with_access_token(access_token);
        let mixer = Arc::new(SoftMixer::open(MixerConfig::default())?);
        let player_config = PlayerConfig {
            position_update_interval: Some(Duration::from_millis(500)),
            ..Default::default()
        };
        let soft_volume = mixer.get_soft_volume();
        let sink_builder = audio_backend::find(None).ok_or_else(|| {
            SpotifyError::Playback("no librespot audio backend is available".into())
        })?;
        let player = Player::new(player_config, session.clone(), soft_volume, move || {
            sink_builder(None, AudioFormat::S16)
        });

        let connect_config = ConnectConfig {
            name: CONNECT_DEVICE_NAME.to_string(),
            device_type: DeviceType::Computer,
            initial_volume: 50 * (u16::MAX / 100),
            ..Default::default()
        };

        let mixin: Arc<dyn Mixer> = mixer;
        let (spirc, spirc_task) = runtime
            .block_on(Spirc::new(
                connect_config,
                session.clone(),
                credentials,
                player.clone(),
                mixin,
            ))
            .map_err(|error| SpotifyError::Session(error.to_string()))?;
        runtime.spawn(spirc_task);

        let playback = Arc::new(Mutex::new(PlaybackInner {
            snapshot: PlaybackSnapshot::default(),
            updated_at: Instant::now(),
        }));
        spawn_player_event_loop(&runtime, &player, playback.clone());

        let username = session.username();

        Ok(Self {
            inner: Arc::new(NativeInner {
                session,
                spirc,
                playback,
                runtime,
                username,
            }),
        })
    }

    pub fn username(&self) -> &str {
        &self.inner.username
    }

    /// Searches Spotify's own Pathfinder endpoint. Blocking; call from a worker thread.
    pub fn search(&self, query: &str) -> Result<SearchResults, SpotifyError> {
        self.inner
            .runtime
            .block_on(search::search(&self.inner.session, query))
    }

    /// Fetches the current user's profile and avatar through Spotify's native endpoints.
    pub fn user_profile(&self) -> Result<UserProfile, SpotifyError> {
        self.inner.runtime.block_on(async {
            let username = self.inner.session.username();
            let bytes = self
                .inner
                .session
                .spclient()
                .get_user_profile(&username, None, None)
                .await
                .map_err(|error| SpotifyError::Api(error.to_string()))?;
            let value: Value = serde_json::from_slice(&bytes)
                .map_err(|error| SpotifyError::Api(error.to_string()))?;

            Ok(UserProfile {
                name: value
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(&username)
                    .to_string(),
                image_url: image_from_json(&value),
            })
        })
    }

    /// Fetches the user's saved playlists from Spotify's native rootlist endpoint.
    ///
    /// The endpoint returns a protobuf `SelectedListContent` (the same structure the
    /// official client decodes). Playlist names and artwork come from the decorated
    /// `meta_items` that accompany each rootlist entry.
    pub fn library_playlists(&self) -> Result<Vec<LibraryPlaylist>, SpotifyError> {
        self.inner.runtime.block_on(async {
            let mut playlists = Vec::new();
            let mut from = 0usize;
            let page_size = 100usize;

            loop {
                let bytes = self
                    .inner
                    .session
                    .spclient()
                    .get_rootlist(from, Some(page_size))
                    .await
                    .map_err(|error| SpotifyError::Api(error.to_string()))?;
                let list = SelectedListContent::parse_from_bytes(&bytes)
                    .map_err(|error| SpotifyError::Api(error.to_string()))?;

                let Some(contents) = list.contents.as_ref() else {
                    break;
                };
                if contents.items.is_empty() {
                    break;
                }

                for (index, item) in contents.items.iter().enumerate() {
                    let uri = item.uri.clone().unwrap_or_default();
                    if !uri.starts_with("spotify:playlist:") {
                        continue;
                    }
                    let attributes = contents
                        .meta_items
                        .get(index)
                        .and_then(|meta| meta.attributes.as_ref());
                    let name = attributes
                        .and_then(|attributes| attributes.name.clone())
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| "Playlist".to_string());
                    let image_url = attributes.and_then(|attributes| {
                        attributes
                            .picture_size
                            .iter()
                            .find_map(|size| size.url.clone().filter(|url| !url.is_empty()))
                    });

                    playlists.push(LibraryPlaylist {
                        uri,
                        name,
                        owner: String::new(),
                        image_url,
                    });
                }

                from += contents.items.len();
                let total = list.length().max(0) as usize;
                if from >= total {
                    break;
                }
            }

            Ok(playlists)
        })
    }

    /// A consistent snapshot of local playback, with position extrapolated to now.
    pub fn playback(&self) -> PlaybackSnapshot {
        let inner = self.inner.playback.lock().unwrap();
        let mut snapshot = inner.snapshot.clone();
        if snapshot.is_playing {
            let elapsed = inner.updated_at.elapsed().as_millis() as u64;
            let duration = snapshot
                .now_playing
                .as_ref()
                .map(|track| track.duration_ms)
                .unwrap_or(0);
            snapshot.position_ms = snapshot.position_ms.saturating_add(elapsed);
            if duration > 0 {
                snapshot.position_ms = snapshot.position_ms.min(duration);
            }
        }
        snapshot
    }

    /// Plays a single track URI, making this client the active Connect device.
    pub fn play_track(&self, uri: &str) -> Result<(), SpotifyError> {
        self.play_uris(vec![uri.to_string()])
    }

    /// Plays an album, playlist, artist or show context URI.
    pub fn play_context(&self, uri: &str) -> Result<(), SpotifyError> {
        let _ = self.inner.spirc.activate();
        let options = LoadRequestOptions {
            start_playing: true,
            ..Default::default()
        };
        self.inner
            .spirc
            .load(LoadRequest::from_context_uri(uri.to_string(), options))
            .map_err(|error| SpotifyError::Playback(error.to_string()))
    }

    fn play_uris(&self, uris: Vec<String>) -> Result<(), SpotifyError> {
        if uris.is_empty() {
            return Ok(());
        }
        let _ = self.inner.spirc.activate();
        let options = LoadRequestOptions {
            start_playing: true,
            playing_track: Some(PlayingTrack::Uri(uris[0].clone())),
            ..Default::default()
        };
        self.inner
            .spirc
            .load(LoadRequest::from_tracks(uris, options))
            .map_err(|error| SpotifyError::Playback(error.to_string()))
    }

    pub fn play_pause(&self) -> Result<(), SpotifyError> {
        self.inner
            .spirc
            .play_pause()
            .map_err(|error| SpotifyError::Playback(error.to_string()))
    }

    pub fn play(&self) -> Result<(), SpotifyError> {
        self.inner
            .spirc
            .play()
            .map_err(|error| SpotifyError::Playback(error.to_string()))
    }

    pub fn pause(&self) -> Result<(), SpotifyError> {
        self.inner
            .spirc
            .pause()
            .map_err(|error| SpotifyError::Playback(error.to_string()))
    }

    pub fn next(&self) -> Result<(), SpotifyError> {
        self.inner
            .spirc
            .next()
            .map_err(|error| SpotifyError::Playback(error.to_string()))
    }

    pub fn previous(&self) -> Result<(), SpotifyError> {
        self.inner
            .spirc
            .prev()
            .map_err(|error| SpotifyError::Playback(error.to_string()))
    }

    pub fn seek(&self, position_ms: u64) -> Result<(), SpotifyError> {
        self.inner
            .spirc
            .set_position_ms(position_ms.min(u32::MAX as u64) as u32)
            .map_err(|error| SpotifyError::Playback(error.to_string()))
    }

    pub fn set_volume_percent(&self, percent: u8) -> Result<(), SpotifyError> {
        let value = (percent.min(100) as u32 * u16::MAX as u32 / 100) as u16;
        self.inner
            .spirc
            .set_volume(value)
            .map_err(|error| SpotifyError::Playback(error.to_string()))?;
        let mut playback = self.inner.playback.lock().unwrap();
        playback.snapshot.volume_percent = percent.min(100);
        Ok(())
    }

    pub fn set_shuffle(&self, shuffle: bool) -> Result<(), SpotifyError> {
        self.inner
            .spirc
            .shuffle(shuffle)
            .map_err(|error| SpotifyError::Playback(error.to_string()))
    }

    pub fn set_repeat(&self, repeat: bool) -> Result<(), SpotifyError> {
        self.inner
            .spirc
            .repeat(repeat)
            .map_err(|error| SpotifyError::Playback(error.to_string()))
    }
}

fn spawn_player_event_loop(
    runtime: &Arc<tokio::runtime::Runtime>,
    player: &Arc<Player>,
    playback: Arc<Mutex<PlaybackInner>>,
) {
    let mut events = player.get_player_event_channel();
    runtime.spawn(async move {
        while let Some(event) = events.recv().await {
            let mut inner = playback.lock().unwrap();
            match event {
                PlayerEvent::TrackChanged { audio_item } => {
                    let (artists, album) = match &audio_item.unique_fields {
                        librespot::metadata::audio::UniqueFields::Track {
                            artists, album, ..
                        } => (
                            artists
                                .iter()
                                .map(|artist| artist.name.clone())
                                .collect::<Vec<_>>()
                                .join(", "),
                            album.clone(),
                        ),
                        librespot::metadata::audio::UniqueFields::Episode { show_name, .. } => {
                            (show_name.clone(), "Episode".to_string())
                        }
                        librespot::metadata::audio::UniqueFields::Local {
                            artists, album, ..
                        } => (
                            artists.clone().unwrap_or_default(),
                            album.clone().unwrap_or_default(),
                        ),
                    };
                    inner.snapshot.now_playing = Some(NowPlaying {
                        uri: audio_item.uri.clone(),
                        title: audio_item.name.clone(),
                        artists,
                        album,
                        cover_url: audio_item.covers.first().map(|cover| cover.url.clone()),
                        duration_ms: audio_item.duration_ms as u64,
                    });
                    inner.updated_at = Instant::now();
                }
                PlayerEvent::Playing { position_ms, .. } => {
                    inner.snapshot.is_playing = true;
                    inner.snapshot.position_ms = position_ms as u64;
                    inner.updated_at = Instant::now();
                }
                PlayerEvent::Paused { position_ms, .. } => {
                    inner.snapshot.is_playing = false;
                    inner.snapshot.position_ms = position_ms as u64;
                    inner.updated_at = Instant::now();
                }
                PlayerEvent::Seeked { position_ms, .. }
                | PlayerEvent::PositionCorrection { position_ms, .. }
                | PlayerEvent::PositionChanged { position_ms, .. } => {
                    inner.snapshot.position_ms = position_ms as u64;
                    inner.updated_at = Instant::now();
                }
                PlayerEvent::Loading { position_ms, .. } => {
                    inner.snapshot.position_ms = position_ms as u64;
                    inner.updated_at = Instant::now();
                }
                PlayerEvent::Stopped { .. } => {
                    inner.snapshot.is_playing = false;
                    inner.snapshot.position_ms = 0;
                    inner.updated_at = Instant::now();
                }
                PlayerEvent::Unavailable { .. } => {
                    inner.snapshot.is_playing = false;
                }
                PlayerEvent::VolumeChanged { volume } => {
                    inner.snapshot.volume_percent = ((volume as u32 * 100) / u16::MAX as u32) as u8;
                }
                PlayerEvent::ShuffleChanged { shuffle } => {
                    inner.snapshot.shuffle = shuffle;
                }
                PlayerEvent::RepeatChanged { context, .. } => {
                    inner.snapshot.repeat = context;
                }
                _ => {}
            }
        }
    });
}

fn image_from_json(value: &Value) -> Option<String> {
    if let Some(uri) = value
        .get("image_uri")
        .or_else(|| value.get("imageUri"))
        .or_else(|| value.get("image_url"))
        .or_else(|| value.get("imageUrl"))
        .and_then(Value::as_str)
        .filter(|uri| !uri.is_empty())
    {
        return Some(image_url_from_id(uri));
    }

    for pointer in [
        "/images/0/url",
        "/images/0/sources/0/url",
        "/images/items/0/sources/0/url",
        "/avatarImage/sources/0/url",
    ] {
        if let Some(url) = value.pointer(pointer).and_then(Value::as_str) {
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
    }
    None
}

fn image_url_from_id(id: &str) -> String {
    let id = id.strip_prefix("spotify:image:").unwrap_or(id);
    if id.starts_with("http://") || id.starts_with("https://") {
        id.to_string()
    } else {
        format!("https://i.scdn.co/image/{id}")
    }
}

fn session_cache_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("RSPOTIFY_STATE_DIR") {
        return std::path::PathBuf::from(dir);
    }
    let cache = std::env::var("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|home| std::path::Path::new(&home).join(".cache")))
        .unwrap_or_else(|_| std::env::temp_dir());
    cache.join("rspotify")
}

#[cfg(unix)]
fn restrict_private_dir(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn restrict_private_dir(_path: &std::path::Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manual smoke test. Requires a previously authorised account in
    /// `~/.cache/rspotify/session.json` and network access.
    #[test]
    #[ignore = "requires a logged-in Spotify account and network access"]
    fn smoke_native_login_and_search() {
        let client = NativeSpotify::authenticate().expect("native login");
        let results = client.search("daft punk").expect("pathfinder search");
        assert!(
            !results.is_empty(),
            "search returned no results: {results:#?}"
        );
        eprintln!(
            "search ok: {} tracks, {} artists, {} albums",
            results.tracks.len(),
            results.artists.len(),
            results.albums.len()
        );
        let track = results.tracks.first().expect("track result");
        assert!(!track.title.is_empty() && !track.uri.is_empty());
        assert!(!track.subtitle.is_empty());
        assert!(track.image.is_some());
        assert!(track.duration_ms.unwrap_or(0) > 0);
        let artist = results.artists.first().expect("artist result");
        assert!(!artist.title.is_empty() && !artist.uri.is_empty());
        assert!(artist.image.is_some());
        let album = results.albums.first().expect("album result");
        assert!(!album.title.is_empty() && !album.uri.is_empty());
        assert!(album.image.is_some());
        let playlist = results.playlists.first().expect("playlist result");
        assert!(!playlist.title.is_empty() && !playlist.uri.is_empty());
        assert!(playlist.image.is_some());
        if let Ok(profile) = client.user_profile() {
            eprintln!(
                "profile ok: {} (avatar: {})",
                profile.name,
                profile.image_url.is_some()
            );
        }
        let playlists = client.library_playlists().expect("native rootlist library");
        eprintln!("library ok: {} playlists", playlists.len());
        assert!(!playlists.is_empty());
        assert!(playlists.iter().all(|playlist| {
            playlist.uri.starts_with("spotify:playlist:") && !playlist.name.is_empty()
        }));
        eprintln!("playback: {:#?}", client.playback());
    }

    #[test]
    #[ignore = "requires a logged-in Spotify account and audio output"]
    fn smoke_native_playback() {
        let client = NativeSpotify::authenticate().expect("native login");
        let results = client.search("daft punk get lucky").expect("search");
        let track = results.tracks.first().expect("a track").clone();
        eprintln!("playing {} ({})", track.title, track.uri);

        client.set_volume_percent(8).expect("volume");
        client.play_track(&track.uri).expect("load track");

        let mut last = String::new();
        for _ in 0..24 {
            std::thread::sleep(Duration::from_millis(500));
            let snapshot = client.playback();
            let line = format!(
                "playing={} position={}ms track={:?}",
                snapshot.is_playing,
                snapshot.position_ms,
                snapshot.now_playing.as_ref().map(|track| &track.title)
            );
            if line != last {
                eprintln!("{line}");
                last = line;
            }
            if snapshot.is_playing && snapshot.position_ms > 2_000 {
                break;
            }
        }

        client.pause().expect("pause");
        std::thread::sleep(Duration::from_millis(700));
        let snapshot = client.playback();
        assert!(
            !snapshot.is_playing,
            "playback did not pause: {snapshot:#?}"
        );
        eprintln!("paused at {}ms", snapshot.position_ms);
    }
}
