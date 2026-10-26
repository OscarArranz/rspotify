//! Native Spotify client: OAuth login, librespot session, Pathfinder search and
//! Spotify Connect playback.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use librespot::connect::{ConnectConfig, LoadRequest, LoadRequestOptions, PlayingTrack, Spirc};
use librespot::core::{
    Session, SessionConfig, authentication::Credentials, cache::Cache, config::DeviceType,
};
use librespot::oauth::{OAuthClientBuilder, OAuthError};
use librespot::playback::{
    audio_backend,
    config::{AudioFormat, PlayerConfig},
    mixer::{Mixer, MixerConfig, softmixer::SoftMixer},
    player::{Player, PlayerEvent},
};
use serde_json::Value;

use super::error::SpotifyError;
use super::playlist::{self, PlaylistDetail, PlaylistPage, PlaylistSource};
use super::search::{self, SearchResults};
use super::store::{SessionStore, StoredProfile};

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
    /// The context URI last loaded by rspotify, when playback was started from a playlist,
    /// album, artist, or collection rather than an ad-hoc list of tracks.
    pub context_uri: Option<String>,
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
            context_uri: None,
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
    local_player_active: bool,
}

impl PlaybackInner {
    fn snapshot_at(&self, now: Instant) -> PlaybackSnapshot {
        let mut snapshot = self.snapshot.clone();
        if snapshot.is_playing {
            let elapsed = now.duration_since(self.updated_at).as_millis() as u64;
            snapshot.position_ms = snapshot.position_ms.saturating_add(elapsed);
            if let Some(track) = &snapshot.now_playing {
                if track.duration_ms > 0 {
                    snapshot.position_ms = snapshot.position_ms.min(track.duration_ms);
                }
            }
        }
        snapshot
    }

    fn apply_connect_state(
        &mut self,
        remote: &librespot::connect::RemotePlayerState,
        local_device_id: &str,
        now: Instant,
    ) {
        if self.local_player_active {
            return;
        }
        if remote.track_uri.is_empty() {
            if remote.active_device_id != local_device_id {
                self.snapshot.now_playing = None;
                self.snapshot.is_playing = false;
                self.snapshot.position_ms = 0;
                self.snapshot.context_uri = None;
                self.updated_at = now;
            }
            return;
        }
        if self
            .snapshot
            .now_playing
            .as_ref()
            .is_some_and(|track| track.uri == remote.track_uri)
        {
            if remote.active_device_id != local_device_id {
                self.snapshot.is_playing = false;
                self.snapshot.position_ms = remote.position_ms;
                self.updated_at = now;
            }
            if !remote.context_uri.is_empty() {
                self.snapshot.context_uri = Some(remote.context_uri.clone());
            }
        }
    }
}

struct NativeInner {
    session: Session,
    spirc: Spirc,
    mixer: Arc<SoftMixer>,
    playback: Arc<Mutex<PlaybackInner>>,
    runtime: Arc<tokio::runtime::Runtime>,
    username: String,
    liked_songs_detail: Mutex<Option<PlaylistDetail>>,
}

/// Cloneable handle to an authenticated native Spotify session.
#[derive(Clone)]
pub struct NativeSpotify {
    inner: Arc<NativeInner>,
}

impl NativeSpotify {
    pub fn cached_profile() -> Option<UserProfile> {
        SessionStore::load().profile().map(|profile| UserProfile {
            name: profile.name.clone(),
            image_url: profile.image_url.clone(),
        })
    }

    pub fn has_restorable_session() -> bool {
        SessionStore::load().has_restorable_session()
    }

    pub fn clear_saved_refresh_token() {
        SessionStore::load().clear_refresh_token();
    }

    /// Performs the interactive desktop OAuth flow and connects a librespot session.
    ///
    /// This is blocking and must run on a background thread. It opens the user's
    /// browser at `accounts.spotify.com`, receives the loopback callback and then
    /// authenticates against `login5.spotify.com` through librespot.
    pub fn authenticate() -> Result<Self, SpotifyError> {
        let mut store = SessionStore::load();
        let device_id = store.ensure_device_id();
        let (client_id, oauth) = oauth_client(true)?;

        // Reuse a stored refresh token when possible, otherwise show the browser flow.
        let mut token = match store.refresh_token() {
            Some(refresh) => match oauth.refresh_token(refresh) {
                Ok(token) => token,
                Err(_) => {
                    eprintln!("stored Spotify session was rejected; starting interactive sign-in");
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

        let initial_volume = store.volume_percent();
        Self::connect(token.access_token, device_id, client_id, initial_volume)
    }

    /// Restores a saved session without opening a browser. Must run off the GPUI thread.
    pub fn restore() -> Result<Self, SpotifyError> {
        let mut store = SessionStore::load();
        let refresh = store
            .refresh_token()
            .ok_or_else(|| SpotifyError::Auth("no saved refresh token".into()))?
            .to_owned();
        let device_id = store.ensure_device_id();
        let (client_id, oauth) = oauth_client(false)?;
        let token = oauth.refresh_token(&refresh).map_err(|error| {
            if is_rejected_refresh(&error) {
                SpotifyError::InvalidRefreshToken
            } else {
                SpotifyError::OAuth(error.to_string())
            }
        })?;
        let refresh_token = if token.refresh_token.is_empty() {
            refresh
        } else {
            token.refresh_token
        };
        store.set_refresh_token(&refresh_token);
        let initial_volume = store.volume_percent();
        Self::connect(token.access_token, device_id, client_id, initial_volume)
    }

    /// Fetch and persist the current profile. Profile fetch failure is non-fatal.
    pub fn persist_profile(&self) -> Option<UserProfile> {
        let profile = self.user_profile().ok()?;
        SessionStore::load().set_profile(StoredProfile {
            name: profile.name.clone(),
            image_url: profile.image_url.clone(),
        });
        Some(profile)
    }

    fn connect(
        access_token: String,
        device_id: String,
        client_id: String,
        initial_volume: u8,
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
        let initial_volume = initial_volume.min(100);
        let initial_volume_value = (initial_volume as u32 * u16::MAX as u32 / 100) as u16;
        mixer.set_volume(initial_volume_value);
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
            initial_volume: initial_volume_value,
            ..Default::default()
        };

        let mixin: Arc<dyn Mixer> = mixer.clone();
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
            snapshot: PlaybackSnapshot {
                volume_percent: initial_volume,
                ..PlaybackSnapshot::default()
            },
            updated_at: Instant::now(),
            local_player_active: false,
        }));
        spawn_player_event_loop(&runtime, &player, playback.clone());
        spawn_connect_state_event_loop(&runtime, &spirc, &session, playback.clone());

        let username = session.username();
        // Authentication already runs on a worker. Decode the collection here so opening
        // Liked Songs never needs to render a placeholder while waiting on disk I/O.
        let liked_songs_detail =
            playlist::read_cached_detail(&username, &PlaylistSource::LikedSongs);

        Ok(Self {
            inner: Arc::new(NativeInner {
                session,
                spirc,
                mixer,
                playback,
                runtime,
                username,
                liked_songs_detail: Mutex::new(liked_songs_detail),
            }),
        })
    }

    pub fn username(&self) -> &str {
        &self.inner.username
    }

    /// Blocking native library fetch; callers must run this on a worker.
    pub fn library_entries(&self) -> Result<Vec<super::library::LibraryEntry>, SpotifyError> {
        self.inner
            .runtime
            .block_on(super::library::fetch(&self.inner.session))
    }

    pub(crate) fn library_catalog(&self) -> super::library_catalog::LibrarySupplement {
        self.inner
            .runtime
            .block_on(super::library_catalog::fetch(&self.inner.session))
    }

    /// Blocking account mutation; only explicit user Pin/Unpin actions may invoke this worker API.
    pub(crate) fn set_library_pinned(
        &self,
        uri: &str,
        pinned: bool,
    ) -> Result<std::collections::HashSet<String>, SpotifyError> {
        self.inner.runtime.block_on(super::collection::set_pinned(
            &self.inner.session,
            uri,
            pinned,
        ))
    }

    /// Enrich a small visible batch without holding up the first library render.
    pub(crate) fn hydrate_library_entries(
        &self,
        entries: &mut [super::library::LibraryEntry],
        cache: &mut super::library::LibraryHydrator,
    ) -> usize {
        self.inner
            .runtime
            .block_on(super::library::hydrate(&self.inner.session, entries, cache))
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
        Ok(self
            .library_entries()?
            .into_iter()
            .filter(|entry| entry.kind == super::library::LibraryKind::Playlist)
            .map(|entry| LibraryPlaylist {
                uri: entry.uri,
                name: entry.name,
                owner: entry.owner,
                image_url: entry.image_url,
            })
            .collect())
    }

    /// Fetches a saved playlist, or the current user's Liked Songs, plus its first context page.
    pub fn playlist_detail(&self, source: PlaylistSource) -> Result<PlaylistDetail, SpotifyError> {
        self.inner
            .runtime
            .block_on(playlist::fetch_playlist_detail(&self.inner.session, source))
    }

    pub fn cached_playlist_detail(&self, source: &PlaylistSource) -> Option<PlaylistDetail> {
        playlist::read_cached_detail(&self.inner.username, source)
    }

    pub fn cache_playlist_detail(&self, source: &PlaylistSource, detail: &PlaylistDetail) {
        if matches!(source, PlaylistSource::LikedSongs) {
            *self.inner.liked_songs_detail.lock().unwrap() = Some(detail.clone());
        }
        playlist::write_cached_detail(&self.inner.username, source, detail);
    }

    /// In-memory only: safe to use when constructing the screen on the UI thread.
    pub fn loaded_liked_songs_detail(&self) -> Option<PlaylistDetail> {
        self.inner.liked_songs_detail.lock().unwrap().clone()
    }

    /// Fetches one additional context page for an already-loaded playlist.
    pub fn playlist_next_page(&self, next_page_url: &str) -> Result<PlaylistPage, SpotifyError> {
        self.inner.runtime.block_on(playlist::fetch_playlist_page(
            &self.inner.session,
            next_page_url,
        ))
    }

    /// The native context URI for the current user's Liked Songs.
    pub fn liked_songs_context_uri(&self) -> String {
        format!("spotify:user:{}:collection", self.inner.username)
    }

    /// A consistent snapshot of local playback, with position extrapolated to now.
    pub fn playback(&self) -> PlaybackSnapshot {
        let inner = self.inner.playback.lock().unwrap();
        inner.snapshot_at(Instant::now())
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
            .map_err(|error| SpotifyError::Playback(error.to_string()))?;
        self.set_context_uri(Some(uri.to_string()));
        Ok(())
    }

    /// Plays a context starting at a specific track.
    pub fn play_context_at(&self, context_uri: &str, track_uri: &str) -> Result<(), SpotifyError> {
        let _ = self.inner.spirc.activate();
        let options = LoadRequestOptions {
            start_playing: true,
            playing_track: Some(PlayingTrack::Uri(track_uri.to_string())),
            ..Default::default()
        };
        self.inner
            .spirc
            .load(LoadRequest::from_context_uri(
                context_uri.to_string(),
                options,
            ))
            .map_err(|error| SpotifyError::Playback(error.to_string()))?;
        self.set_context_uri(Some(context_uri.to_string()));
        Ok(())
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
            .map_err(|error| SpotifyError::Playback(error.to_string()))?;
        self.set_context_uri(None);
        Ok(())
    }

    fn set_context_uri(&self, context_uri: Option<String>) {
        let mut playback = self.inner.playback.lock().unwrap();
        playback.snapshot.context_uri = context_uri;
    }

    pub fn play_pause(&self) -> Result<(), SpotifyError> {
        let snapshot = self.playback();
        if snapshot.is_playing {
            self.inner
                .spirc
                .play_pause()
                .map_err(|error| SpotifyError::Playback(error.to_string()))
        } else {
            self.start_snapshot_playback(snapshot)
        }
    }

    pub fn play(&self) -> Result<(), SpotifyError> {
        self.start_snapshot_playback(self.playback())
    }

    fn start_snapshot_playback(&self, snapshot: PlaybackSnapshot) -> Result<(), SpotifyError> {
        let Some(track) = snapshot.now_playing else {
            return self
                .inner
                .spirc
                .play()
                .map_err(|error| SpotifyError::Playback(error.to_string()));
        };

        let options = LoadRequestOptions {
            start_playing: true,
            seek_to: snapshot.position_ms.min(u32::MAX as u64) as u32,
            playing_track: Some(PlayingTrack::Uri(track.uri.clone())),
            ..Default::default()
        };
        let request = match snapshot.context_uri {
            Some(context_uri) if !context_uri.is_empty() => {
                LoadRequest::from_context_uri(context_uri, options)
            }
            _ => LoadRequest::from_tracks(vec![track.uri], options),
        };

        self.inner
            .spirc
            .activate()
            .map_err(|error| SpotifyError::Playback(error.to_string()))?;
        self.inner
            .spirc
            .load(request)
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
        let mut store = SessionStore::load();
        store.set_volume_percent(percent.min(100));
        let mut playback = self.inner.playback.lock().unwrap();
        playback.snapshot.volume_percent = percent.min(100);
        Ok(())
    }

    /// Applies volume directly to the local soft mixer without sending a Connect command.
    /// Use this for responsive drag previews, then call `set_volume_percent` on release.
    pub fn set_local_volume_percent(&self, percent: u8) {
        let value = (percent.min(100) as u32 * u16::MAX as u32 / 100) as u16;
        self.inner.mixer.set_volume(value);
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
            apply_player_event(&mut inner, event, Instant::now());
        }
    });
}

fn apply_player_event(inner: &mut PlaybackInner, event: PlayerEvent, now: Instant) {
    match event {
        PlayerEvent::TrackChanged { audio_item } => {
            let (artists, album) = match &audio_item.unique_fields {
                librespot::metadata::audio::UniqueFields::Track { artists, album, .. } => (
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
                librespot::metadata::audio::UniqueFields::Local { artists, album, .. } => (
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
        }
        PlayerEvent::Playing { position_ms, .. } => {
            inner.local_player_active = true;
            inner.snapshot.is_playing = true;
            inner.snapshot.position_ms = position_ms as u64;
            inner.updated_at = now;
        }
        PlayerEvent::Paused { position_ms, .. } => {
            inner.local_player_active = true;
            inner.snapshot.is_playing = false;
            inner.snapshot.position_ms = position_ms as u64;
            inner.updated_at = now;
        }
        PlayerEvent::Seeked { position_ms, .. }
        | PlayerEvent::PositionCorrection { position_ms, .. }
        | PlayerEvent::PositionChanged { position_ms, .. } => {
            inner.snapshot.position_ms = position_ms as u64;
            inner.updated_at = now;
        }
        PlayerEvent::Loading { position_ms, .. } => {
            inner.local_player_active = true;
            inner.snapshot.is_playing = false;
            inner.snapshot.position_ms = position_ms as u64;
            inner.updated_at = now;
        }
        PlayerEvent::Stopped { .. } => {
            inner.local_player_active = false;
            inner.snapshot.is_playing = false;
            inner.snapshot.position_ms = 0;
            inner.snapshot.now_playing = None;
            inner.updated_at = now;
        }
        PlayerEvent::Unavailable { .. } => {
            inner.local_player_active = false;
            inner.snapshot.position_ms = inner.snapshot_at(now).position_ms;
            inner.updated_at = now;
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

fn spawn_connect_state_event_loop(
    runtime: &Arc<tokio::runtime::Runtime>,
    spirc: &Spirc,
    session: &Session,
    playback: Arc<Mutex<PlaybackInner>>,
) {
    let spirc = spirc.clone();
    let session = session.clone();
    runtime.spawn(async move {
        let mut resolved_uri = String::new();
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let Some(remote) = spirc.remote_player_state() else {
                continue;
            };

            // Connect can still describe the previous device during a local handoff.
            // The player event stream owns transport state until it stops.
            if remote.active_device_id == session.device_id()
                || playback.lock().unwrap().local_player_active
            {
                resolved_uri.clear();
                continue;
            }

            if remote.track_uri.is_empty() {
                if remote.active_device_id != session.device_id() {
                    let mut inner = playback.lock().unwrap();
                    inner.apply_connect_state(&remote, session.device_id(), Instant::now());
                    resolved_uri.clear();
                }
                continue;
            }

            let current_uri = playback
                .lock()
                .unwrap()
                .snapshot
                .now_playing
                .as_ref()
                .map(|track| track.uri.clone());
            if current_uri.as_deref() != Some(remote.track_uri.as_str())
                && resolved_uri != remote.track_uri
            {
                resolved_uri = remote.track_uri.clone();
                let metadata = playlist::fetch_track_metadata(&session, &remote.track_uri)
                    .await
                    .ok()
                    .flatten();
                if let Some(now_playing) = now_playing_from_remote(&remote, metadata) {
                    let mut inner = playback.lock().unwrap();
                    // A newer server update or local player event may have arrived while
                    // metadata was being fetched; only apply a still-current server item.
                    if let Some(latest) = spirc.remote_player_state() {
                        apply_remote_now_playing(
                            &mut inner,
                            &latest,
                            session.device_id(),
                            now_playing,
                        );
                    }
                }
            }

            // Refresh after the possible await above before updating remote timing/context.
            let Some(remote) = spirc.remote_player_state() else {
                continue;
            };
            if remote.active_device_id == session.device_id() {
                resolved_uri.clear();
                continue;
            }
            let mut inner = playback.lock().unwrap();
            inner.apply_connect_state(&remote, session.device_id(), Instant::now());
        }
    });
}

fn apply_remote_now_playing(
    inner: &mut PlaybackInner,
    remote: &librespot::connect::RemotePlayerState,
    local_device_id: &str,
    now_playing: NowPlaying,
) {
    // Hydration may finish after the server changes tracks or playback returns locally.
    if remote.active_device_id == local_device_id
        || inner.local_player_active
        || remote.track_uri != now_playing.uri
    {
        return;
    }
    inner.snapshot.now_playing = Some(now_playing);
    inner.snapshot.context_uri =
        (!remote.context_uri.is_empty()).then_some(remote.context_uri.clone());
}

fn now_playing_from_remote(
    remote: &librespot::connect::RemotePlayerState,
    hydrated: Option<playlist::HydratedTrack>,
) -> Option<NowPlaying> {
    let title = hydrated
        .as_ref()
        .map(|metadata| metadata.title.clone())
        .or_else(|| remote_metadata_value(remote, &["title", "name"]))?;
    let artists = hydrated
        .as_ref()
        .map(|metadata| metadata.artist.clone())
        .unwrap_or_else(|| {
            remote_metadata_value(remote, &["artist", "artist_name", "artists"]).unwrap_or_default()
        });
    let album = hydrated
        .as_ref()
        .map(|metadata| metadata.album.clone())
        .unwrap_or_else(|| {
            remote_metadata_value(remote, &["album", "album_name"]).unwrap_or_default()
        });
    let cover_url = hydrated
        .as_ref()
        .and_then(|metadata| metadata.image_url.clone())
        .or_else(|| {
            remote_metadata_value(
                remote,
                &[
                    "image_url",
                    "image_uri",
                    "album_image_url",
                    "cover_url",
                    "imageUrl",
                ],
            )
            .map(|url| image_url_from_id(&url))
            .filter(|url| !url.is_empty())
        });

    Some(NowPlaying {
        uri: remote.track_uri.clone(),
        title,
        artists,
        album,
        cover_url,
        duration_ms: hydrated
            .as_ref()
            .map(|metadata| metadata.duration_ms)
            .filter(|duration| *duration > 0)
            .unwrap_or(remote.duration_ms),
    })
}

fn remote_metadata_value(
    remote: &librespot::connect::RemotePlayerState,
    keys: &[&str],
) -> Option<String> {
    keys.iter()
        .find_map(|key| remote.metadata.get(*key))
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
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
        let url = image_url_from_id(uri);
        if !url.is_empty() {
            return Some(url);
        }
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
    if id.is_empty() {
        String::new()
    } else if id.starts_with("http://") || id.starts_with("https://") {
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

fn oauth_client(
    interactive: bool,
) -> Result<(String, librespot::oauth::OAuthClient), SpotifyError> {
    let client_id =
        std::env::var("RSPOTIFY_NATIVE_CLIENT_ID").unwrap_or_else(|_| NATIVE_CLIENT_ID.into());
    let redirect_uri =
        std::env::var("RSPOTIFY_REDIRECT_URI").unwrap_or_else(|_| NATIVE_REDIRECT_URI.into());
    let mut builder = OAuthClientBuilder::new(&client_id, &redirect_uri, OAUTH_SCOPES.to_vec());
    if interactive {
        builder = builder
            .open_in_browser()
            .with_custom_message(OAUTH_SUCCESS_HTML);
    }
    Ok((client_id, builder.build()?))
}

fn is_rejected_refresh(error: &OAuthError) -> bool {
    match error {
        OAuthError::ExchangeCode { e } => {
            let normalized = e.to_ascii_lowercase();
            normalized.contains("invalid_grant") || normalized.contains("invalid_token")
        }
        _ => false,
    }
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

    const TEST_TRACK_URI: &str = "spotify:track:0000000000000000000000";

    fn playback_clock(now: Instant) -> PlaybackInner {
        PlaybackInner {
            snapshot: PlaybackSnapshot {
                now_playing: Some(NowPlaying {
                    uri: TEST_TRACK_URI.into(),
                    duration_ms: 180_000,
                    ..Default::default()
                }),
                position_ms: 60_000,
                ..Default::default()
            },
            updated_at: now,
            local_player_active: false,
        }
    }

    fn test_track_id() -> librespot::core::SpotifyUri {
        librespot::core::SpotifyUri::from_uri(TEST_TRACK_URI).unwrap()
    }

    #[test]
    fn stale_connect_polls_cannot_rewind_local_playback() {
        let start = Instant::now();
        let mut inner = playback_clock(start);
        apply_player_event(
            &mut inner,
            PlayerEvent::Playing {
                play_request_id: 1,
                track_id: test_track_id(),
                position_ms: 60_000,
            },
            start,
        );
        let remote = librespot::connect::RemotePlayerState {
            track_uri: TEST_TRACK_URI.into(),
            active_device_id: "previous-spotify-device".into(),
            position_ms: 12_000,
            ..Default::default()
        };
        for tick in 1..=6 {
            let now = start + Duration::from_millis(tick * 500);
            inner.apply_connect_state(&remote, "rspotify-device", now);
            let snapshot = inner.snapshot_at(now);
            assert!(snapshot.is_playing);
            assert_eq!(snapshot.position_ms, 60_000 + tick * 500);
            apply_player_event(
                &mut inner,
                PlayerEvent::PositionChanged {
                    play_request_id: 1,
                    track_id: test_track_id(),
                    position_ms: (60_000 + tick * 500) as u32,
                },
                now,
            );
            assert_eq!(
                inner
                    .snapshot_at(now + Duration::from_millis(250))
                    .position_ms,
                60_000 + tick * 500 + 250
            );
        }
        // A stale empty remote selection must not clear the local track either.
        inner.apply_connect_state(&Default::default(), "rspotify-device", start);
        assert!(inner.snapshot.now_playing.is_some());
    }

    #[test]
    fn local_connect_context_poll_does_not_reset_position_anchor() {
        let start = Instant::now();
        let mut inner = playback_clock(start);
        inner.snapshot.is_playing = true;
        let remote = librespot::connect::RemotePlayerState {
            track_uri: TEST_TRACK_URI.into(),
            active_device_id: "rspotify-device".into(),
            context_uri: "spotify:playlist:example".into(),
            ..Default::default()
        };
        for tick in 1..=6 {
            let now = start + Duration::from_millis(tick * 500);
            inner.apply_connect_state(&remote, "rspotify-device", now);
            assert_eq!(inner.snapshot_at(now).position_ms, 60_000 + tick * 500);
        }
        assert_eq!(inner.updated_at, start);
        assert_eq!(inner.snapshot.context_uri, Some(remote.context_uri));
    }

    #[test]
    fn local_pause_and_backward_seek_remain_authoritative() {
        let start = Instant::now();
        let mut inner = playback_clock(start);
        apply_player_event(
            &mut inner,
            PlayerEvent::Paused {
                play_request_id: 1,
                track_id: test_track_id(),
                position_ms: 65_000,
            },
            start,
        );
        let remote = librespot::connect::RemotePlayerState {
            track_uri: TEST_TRACK_URI.into(),
            active_device_id: "previous-device".into(),
            position_ms: 12_000,
            ..Default::default()
        };
        let later = start + Duration::from_secs(2);
        inner.apply_connect_state(&remote, "rspotify-device", later);
        assert_eq!(inner.snapshot_at(later).position_ms, 65_000);
        apply_player_event(
            &mut inner,
            PlayerEvent::Seeked {
                play_request_id: 1,
                track_id: test_track_id(),
                position_ms: 10_000,
            },
            later,
        );
        assert_eq!(inner.snapshot_at(later).position_ms, 10_000);
        apply_player_event(
            &mut inner,
            PlayerEvent::Playing {
                play_request_id: 1,
                track_id: test_track_id(),
                position_ms: 10_000,
            },
            later,
        );
        inner.apply_connect_state(&remote, "rspotify-device", later);
        assert_eq!(
            inner
                .snapshot_at(later + Duration::from_secs(1))
                .position_ms,
            11_000
        );
    }

    #[test]
    fn loading_claims_local_clock_and_stopping_releases_it() {
        let start = Instant::now();
        let mut inner = playback_clock(start);
        inner.snapshot.is_playing = true;
        apply_player_event(
            &mut inner,
            PlayerEvent::Loading {
                play_request_id: 1,
                track_id: test_track_id(),
                position_ms: 20_000,
            },
            start,
        );
        assert!(inner.local_player_active);
        assert!(!inner.snapshot.is_playing);
        assert_eq!(
            inner
                .snapshot_at(start + Duration::from_secs(1))
                .position_ms,
            20_000
        );
        apply_player_event(
            &mut inner,
            PlayerEvent::Stopped {
                play_request_id: 1,
                track_id: test_track_id(),
            },
            start,
        );
        assert!(!inner.local_player_active);
        let remote = librespot::connect::RemotePlayerState {
            track_uri: TEST_TRACK_URI.into(),
            active_device_id: "other-device".into(),
            position_ms: 42_000,
            metadata: [("title".into(), "Remote track".into())].into(),
            ..Default::default()
        };
        inner.snapshot.now_playing = now_playing_from_remote(&remote, None);
        inner.apply_connect_state(&remote, "rspotify-device", start);
        assert_eq!(inner.snapshot.position_ms, 42_000);
        assert!(!inner.snapshot.is_playing);
    }

    #[test]
    fn extrapolated_playback_is_clamped_to_track_duration() {
        let start = Instant::now();
        let mut inner = playback_clock(start);
        inner.snapshot.is_playing = true;
        assert_eq!(
            inner
                .snapshot_at(start + Duration::from_secs(200))
                .position_ms,
            180_000
        );
    }

    #[test]
    fn stale_connect_metadata_cannot_replace_the_next_local_song() {
        let next_song = NowPlaying {
            uri: "spotify:track:next".into(),
            title: "Next song".into(),
            ..Default::default()
        };
        let snapshot = PlaybackSnapshot {
            now_playing: Some(next_song),
            is_playing: true,
            position_ms: 250,
            context_uri: Some("spotify:playlist:local".into()),
            ..Default::default()
        };
        let mut inner = PlaybackInner {
            snapshot: snapshot.clone(),
            updated_at: Instant::now(),
            local_player_active: true,
        };
        let updated_at = inner.updated_at;
        let remote = librespot::connect::RemotePlayerState {
            active_device_id: "rspotify-device".into(),
            track_uri: "spotify:track:finished".into(),
            ..Default::default()
        };
        let finished_song = NowPlaying {
            uri: remote.track_uri.clone(),
            title: "Finished song".into(),
            ..Default::default()
        };

        // Also covers hydration started remotely but completed after local activation.
        apply_remote_now_playing(&mut inner, &remote, "rspotify-device", finished_song);

        assert_eq!(inner.snapshot, snapshot);
        assert_eq!(inner.updated_at, updated_at);
    }

    #[test]
    fn remote_hydration_applies_only_to_the_current_remote_song() {
        let mut inner = PlaybackInner {
            snapshot: PlaybackSnapshot::default(),
            updated_at: Instant::now(),
            local_player_active: false,
        };
        let remote = librespot::connect::RemotePlayerState {
            active_device_id: "other-device".into(),
            track_uri: "spotify:track:current".into(),
            context_uri: "spotify:playlist:remote".into(),
            ..Default::default()
        };
        let stale_song = NowPlaying {
            uri: "spotify:track:previous".into(),
            ..Default::default()
        };
        apply_remote_now_playing(&mut inner, &remote, "rspotify-device", stale_song);
        assert_eq!(inner.snapshot, PlaybackSnapshot::default());

        let current_song = NowPlaying {
            uri: remote.track_uri.clone(),
            title: "Current remote song".into(),
            ..Default::default()
        };
        apply_remote_now_playing(&mut inner, &remote, "rspotify-device", current_song.clone());
        assert_eq!(inner.snapshot.now_playing, Some(current_song));
        assert_eq!(
            inner.snapshot.context_uri.as_deref(),
            Some(remote.context_uri.as_str())
        );
    }

    #[test]
    #[ignore = "requires read-only Spotify account/network approval"]
    fn smoke_native_library_context_navigation() {
        use super::super::library::LibraryKind;
        let client = NativeSpotify::authenticate().expect("native login");
        let catalog = client.library_catalog();
        for kind in [LibraryKind::Album, LibraryKind::Artist, LibraryKind::Show] {
            let Some(entry) = catalog.entries.iter().find(|entry| entry.kind == kind) else {
                continue;
            };
            let detail = client
                .playlist_detail(PlaylistSource::Context {
                    uri: entry.uri.clone(),
                    name: entry.name.clone(),
                    owner: entry.owner.clone(),
                    image_url: entry.image_url.clone(),
                    kind: kind.label(),
                })
                .expect("native catalog context page");
            assert_eq!(detail.context_uri, entry.uri);
            assert_eq!(detail.name, entry.name);
            assert!(detail.tracks.iter().all(|track| !track.uri.is_empty()));
            eprintln!(
                "native library {kind:?} context: {} loaded rows, has-next={}",
                detail.tracks.len(),
                detail.next_page_url.is_some()
            );
        }
        // No playback or mutation commands are invoked.
    }

    #[test]
    #[ignore = "requires read-only Spotify account/network approval"]
    fn smoke_native_library_catalog() {
        use super::super::library::LibraryKind;
        let client = NativeSpotify::authenticate().expect("native login");
        let mut entries = client.library_entries().expect("native rootlist");
        let supplement = client.library_catalog();
        assert_eq!(
            supplement.failures, 0,
            "native category/pin/decoration requests failed"
        );
        let (categories, _) = supplement.apply(&mut entries);
        for kind in [LibraryKind::Album, LibraryKind::Artist, LibraryKind::Show] {
            assert!(
                categories.contains(&kind),
                "category contract was not loaded"
            );
            let category: Vec<_> = entries.iter().filter(|e| e.kind == kind).collect();
            assert!(
                category
                    .iter()
                    .all(|e| !e.name.is_empty() && e.name != kind.label()),
                "catalog names remained placeholders"
            );
            eprintln!(
                "native library category {kind:?}: {} decorated entries",
                category.len()
            );
        }
        assert!(
            entries.iter().all(|e| e.pinned.is_some()),
            "pin membership was not loaded"
        );
        eprintln!("native library pin membership loaded successfully");
        // Read-only: no playback, pin/unpin, creation or editing commands are sent.
    }

    #[test]
    #[ignore = "requires read-only Spotify account/network approval"]
    fn smoke_native_library_collection_contract() {
        let client = NativeSpotify::authenticate().expect("native login");
        for set in ["artist", "show", "ylpin", "collection", "prerelease"] {
            let page = client
                .inner
                .runtime
                .block_on(super::super::collection::page(
                    &client.inner.session,
                    set,
                    "",
                ))
                .expect("native collection page");
            let mut types = std::collections::BTreeMap::new();
            let mut contexts = std::collections::BTreeMap::new();
            for item in &page.items {
                *types
                    .entry(item.uri.split(':').nth(1).unwrap_or("unknown").to_string())
                    .or_insert(0usize) += 1;
                if let Some(context) = &item.context_uri {
                    *contexts
                        .entry(context.split(':').nth(1).unwrap_or("unknown").to_string())
                        .or_insert(0usize) += 1;
                }
            }
            eprintln!(
                "native collection {set}: {} items, types={types:?}, context-types={contexts:?}, removed={}, has-next={}",
                page.items.len(),
                page.items.iter().filter(|i| i.removed).count(),
                !page.next.is_empty()
            );
        }
    }

    #[test]
    #[ignore = "requires read-only Spotify account/network approval"]
    fn smoke_native_library_recents_contract() {
        let client = NativeSpotify::authenticate().expect("native login");
        let response = client.inner.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(15), client.inner.session.spclient().request_as_json(
                &reqwest::Method::GET,
                "/recently-played/v3/recently-played?limit=50&filter=default,collection-new-episodes",
                None, None,
            )).await
        });
        let bytes = response
            .expect("bounded recents request")
            .expect("observed native recents endpoint");
        let recents =
            super::super::library::parse_recents(&bytes).expect("native recents protobuf");
        assert!(!recents.is_empty(), "no native recents to validate");
        assert!(
            recents
                .values()
                .all(|timestamp| *timestamp > 100_000_000_000 && *timestamp < 10_000_000_000_000),
            "native recency timestamps are not epoch milliseconds"
        );
        eprintln!(
            "native recents protobuf: {} valid context/timestamp records",
            recents.len()
        );
    }

    #[test]
    #[ignore = "requires an existing Spotify login and read-only network/account approval"]
    fn smoke_native_library_sidebar() {
        use super::super::library::{LibraryHydrator, LibraryKind};
        assert!(
            SessionStore::load().refresh_token().is_some(),
            "Authorize rspotify before running this check"
        );
        let client = NativeSpotify::authenticate().expect("native login");
        let entries = client.library_entries().expect("native library rootlist");
        assert_eq!(
            entries.first().map(|e| e.kind),
            Some(LibraryKind::LikedSongs)
        );
        let identities: std::collections::HashSet<_> =
            entries.iter().map(|e| e.uri.as_str()).collect();
        assert_eq!(identities.len(), entries.len());
        assert!(
            entries
                .iter()
                .all(|entry| !entry.name.is_empty() && !entry.uri.is_empty())
        );
        assert!(entries.iter().all(|entry| {
            entry
                .parent
                .as_ref()
                .is_none_or(|parent| identities.contains(parent.as_str()))
        }));
        let mut sample: Vec<_> = entries
            .iter()
            .filter(|e| e.kind != LibraryKind::Folder)
            .take(4)
            .cloned()
            .collect();
        let mut cache = LibraryHydrator::default();
        let failures = client.hydrate_library_entries(&mut sample, &mut cache);
        assert_eq!(
            failures, 0,
            "sampled native library metadata requests failed"
        );
        assert!(
            sample.iter().all(|entry| !entry.owner.is_empty()),
            "sampled playlist owners were not resolved"
        );
        let owners = sample.iter().filter(|e| !e.owner.is_empty()).count();
        let covers = sample
            .iter()
            .filter(|e| e.image_url.is_some() || !e.covers.is_empty())
            .count();
        eprintln!(
            "library sidebar: {} entries, {} folders; sample {} owners, {} covers, {} metadata failures",
            entries.len(),
            entries
                .iter()
                .filter(|e| e.kind == LibraryKind::Folder)
                .count(),
            owners,
            covers,
            failures
        );
        // No playback or mutation commands are sent by this test.
    }

    #[test]
    fn builds_track_display_from_connect_server_metadata() {
        let remote = librespot::connect::RemotePlayerState {
            track_uri: "spotify:track:example".into(),
            position_ms: 12_000,
            metadata: [
                ("title".into(), "Server track".into()),
                ("artist_name".into(), "Server artist".into()),
                ("album_name".into(), "Server album".into()),
            ]
            .into(),
            ..Default::default()
        };

        let track = now_playing_from_remote(&remote, None).expect("server metadata");
        assert_eq!(track.uri, "spotify:track:example");
        assert_eq!(track.title, "Server track");
        assert_eq!(track.artists, "Server artist");
        assert_eq!(track.album, "Server album");
    }

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

    #[test]
    #[ignore = "requires a logged-in Spotify account and network access"]
    fn smoke_native_liked_songs_metadata() {
        let client = NativeSpotify::authenticate().expect("native login");
        let detail = client
            .playlist_detail(PlaylistSource::LikedSongs)
            .expect("liked songs detail");
        assert!(!detail.tracks.is_empty(), "liked songs returned no tracks");

        let hydrated = detail
            .tracks
            .iter()
            .filter(|track| track.title != "Unknown track" && track.image_url.is_some())
            .count();
        eprintln!(
            "liked songs: {} tracks, {} hydrated, count={:?}",
            detail.tracks.len(),
            hydrated,
            detail.track_count
        );
        assert!(
            detail
                .tracks
                .iter()
                .take(20)
                .all(|track| track.title != "Unknown track" && track.image_url.is_some()),
            "first page metadata was not hydrated"
        );
        assert!(
            hydrated * 10 >= detail.tracks.len() * 9,
            "less than 90% of liked songs were hydrated"
        );

        let playlists = client.library_playlists().expect("rootlist");
        eprintln!(
            "library: {} playlists, {} with artwork",
            playlists.len(),
            playlists
                .iter()
                .filter(|playlist| playlist.image_url.is_some())
                .count()
        );
    }
}
