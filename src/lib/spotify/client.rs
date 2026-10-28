//! Native Spotify client: OAuth login, librespot session, Pathfinder search and
//! Spotify Connect playback.

use std::{
    collections::HashSet,
    path::Path,
    sync::{
        Arc, Mutex, RwLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

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

use super::artist::{self, ArtistProfile};
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
/// How long worker-only reads wait for an in-flight reconnect before returning `Reconnecting`.
const RECONNECT_WAIT: Duration = Duration::from_secs(8);
const RECONNECT_MIN_BACKOFF: Duration = Duration::from_secs(2);
const RECONNECT_MAX_BACKOFF: Duration = Duration::from_secs(30);
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

/// Repeat behavior requested from the local Connect device.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RepeatMode {
    #[default]
    Off,
    /// Repeat the whole context (playlist, album, or track list).
    Context,
    /// Repeat the current track.
    Track,
}

impl RepeatMode {
    pub fn is_active(self) -> bool {
        !matches!(self, Self::Off)
    }

    /// Advances through the Spotify repeat cycle: off, context, track.
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Context,
            Self::Context => Self::Track,
            Self::Track => Self::Off,
        }
    }

    fn from_flags(context: bool, track: bool) -> Self {
        if track {
            Self::Track
        } else if context {
            Self::Context
        } else {
            Self::Off
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackSnapshot {
    pub now_playing: Option<NowPlaying>,
    pub is_playing: bool,
    pub position_ms: u64,
    pub volume_percent: u8,
    pub shuffle: bool,
    pub repeat: RepeatMode,
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
            repeat: RepeatMode::Off,
            context_uri: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LikedStatus {
    /// Membership has not loaded yet, or the item is not a Spotify track.
    Unknown,
    Liked,
    NotLiked,
}

struct LikedCache {
    tracks: HashSet<String>,
    loaded: bool,
    loading: bool,
    revision: u64,
    last_attempt: Option<Instant>,
    refresh_generation: u64,
}

impl Default for LikedCache {
    fn default() -> Self {
        Self {
            tracks: HashSet::new(),
            loaded: false,
            loading: false,
            revision: 0,
            last_attempt: None,
            refresh_generation: 0,
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
    /// The live librespot connection. `None` while a reconnect is in progress.
    connection: RwLock<Option<SessionConnection>>,
    /// Wakes the session supervisor when a command observes a lost connection.
    reconnect_now: tokio::sync::Notify,
    /// Set when Spotify rejects the stored refresh token; the UI returns to sign-in.
    signed_out: AtomicBool,
    /// A play/load request issued while disconnected, applied after the next reconnect.
    pending_load: Mutex<Option<LoadRequest>>,
    /// Created once and re-pointed at each reconnected session via `Player::set_session`.
    player: Mutex<Arc<Player>>,
    mixer: Arc<SoftMixer>,
    playback: Arc<Mutex<PlaybackInner>>,
    runtime: Arc<tokio::runtime::Runtime>,
    username: String,
    liked_songs_detail: Mutex<Option<PlaylistDetail>>,
    liked: Arc<Mutex<LikedCache>>,
}

/// One established librespot session. `alive` stops polling tasks that belong to a dead session.
#[derive(Clone)]
struct SessionConnection {
    session: Session,
    spirc: Spirc,
    alive: Arc<AtomicBool>,
}

struct OpenedConnection {
    connection: SessionConnection,
    spirc_task: tokio::task::JoinHandle<()>,
}

/// Playback state captured before a connection is replaced.
struct ResumeTarget {
    context_uri: Option<String>,
    track_uri: String,
    position_ms: u64,
}

enum ReconnectFailure {
    SignedOut,
    Retry(String),
}

impl From<SpotifyError> for ReconnectFailure {
    fn from(error: SpotifyError) -> Self {
        Self::Retry(error.to_string())
    }
}

impl From<std::io::Error> for ReconnectFailure {
    fn from(error: std::io::Error) -> Self {
        Self::Retry(error.to_string())
    }
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

        let mixer = Arc::new(SoftMixer::open(MixerConfig::default())?);
        let initial_volume = initial_volume.min(100);
        let initial_volume_value = volume_value(initial_volume);
        mixer.set_volume(initial_volume_value);

        // librespot captures `tokio::runtime::Handle::current()`, so the session must be
        // constructed while the runtime context is active.
        let (session, player) = {
            let _guard = runtime.enter();
            let session = build_session(&cache_dir, &device_id, &client_id)?;
            let player = create_player(session.clone(), &mixer)?;
            (session, player)
        };

        let playback = Arc::new(Mutex::new(PlaybackInner {
            snapshot: PlaybackSnapshot {
                volume_percent: initial_volume,
                ..PlaybackSnapshot::default()
            },
            updated_at: Instant::now(),
            local_player_active: false,
        }));

        let opened = runtime.block_on(connect_spirc(
            &runtime,
            session,
            access_token,
            player.clone(),
            mixer.clone(),
            playback.clone(),
            initial_volume_value,
        ))?;
        spawn_player_event_loop(&runtime, &player, playback.clone());

        let username = opened.connection.session.username();
        // Authentication already runs on a worker. Decode the collection here so opening
        // Liked Songs never needs to render a placeholder while waiting on disk I/O.
        let liked_songs_detail =
            playlist::read_cached_detail(&username, &PlaylistSource::LikedSongs);

        let client = Self {
            inner: Arc::new(NativeInner {
                connection: RwLock::new(Some(opened.connection)),
                reconnect_now: tokio::sync::Notify::new(),
                signed_out: AtomicBool::new(false),
                pending_load: Mutex::new(None),
                player: Mutex::new(player),
                mixer,
                playback,
                runtime,
                username,
                liked_songs_detail: Mutex::new(liked_songs_detail),
                liked: Arc::new(Mutex::new(LikedCache::default())),
            }),
        };
        spawn_session_supervisor(&client, opened.spirc_task);
        client.ensure_liked_tracks_loaded();
        Ok(client)
    }

    /// True while a librespot session is established and usable.
    #[cfg(test)]
    pub fn is_connected(&self) -> bool {
        self.inner.live_connection().is_some()
    }

    /// True once Spotify has rejected the stored refresh token and sign-in is required.
    pub fn is_signed_out(&self) -> bool {
        self.inner.signed_out.load(Ordering::Relaxed)
    }

    /// Returns the live connection without waiting. UI-thread callers use this.
    fn connection_now(&self) -> Result<SessionConnection, SpotifyError> {
        match self.inner.live_connection() {
            Some(connection) => Ok(connection),
            None => {
                self.inner.reconnect_now.notify_one();
                Err(SpotifyError::Reconnecting)
            }
        }
    }

    /// Worker-only: waits briefly for an in-flight reconnect before returning the connection.
    fn connection_blocking(&self) -> Result<SessionConnection, SpotifyError> {
        self.inner.runtime.block_on(async {
            let deadline = Instant::now() + RECONNECT_WAIT;
            loop {
                if let Some(connection) = self.inner.live_connection() {
                    return Ok(connection);
                }
                self.inner.reconnect_now.notify_one();
                if Instant::now() >= deadline {
                    return Err(SpotifyError::Reconnecting);
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
        })
    }

    /// Sends a transport command to the current Spirc, treating a closed channel as a lost
    /// session instead of surfacing librespot's internal error.
    fn with_spirc<T>(
        &self,
        command: impl FnOnce(&Spirc) -> Result<T, librespot::core::Error>,
    ) -> Result<T, SpotifyError> {
        let connection = self.connection_now()?;
        command(&connection.spirc).map_err(|_| {
            self.inner.reconnect_now.notify_one();
            SpotifyError::Reconnecting
        })
    }

    /// Queues a load when the session is down and replays it after the next reconnect.
    fn send_load(&self, request: LoadRequest) -> Result<(), SpotifyError> {
        let connection = match self.connection_now() {
            Ok(connection) => connection,
            Err(error) => {
                *self.inner.pending_load.lock().unwrap() = Some(request);
                return Err(error);
            }
        };
        let _ = connection.spirc.activate();
        if connection.spirc.load(request.clone()).is_err() {
            self.inner.reconnect_now.notify_one();
            *self.inner.pending_load.lock().unwrap() = Some(request);
            return Err(SpotifyError::Reconnecting);
        }
        Ok(())
    }

    pub fn username(&self) -> &str {
        &self.inner.username
    }

    /// Fetches the user's saved library from Spotify's native rootlist endpoint.
    ///
    /// The endpoint returns a protobuf `SelectedListContent` (the same structure the
    /// official client decodes). Names and artwork come from the decorated `meta_items`
    /// that accompany each rootlist entry.
    ///
    /// Blocking native fetch; callers must run this on a worker.
    pub fn library_entries(&self) -> Result<Vec<super::library::LibraryEntry>, SpotifyError> {
        let session = self.connection_blocking()?.session;
        self.inner.runtime.block_on(super::library::fetch(&session))
    }

    pub(crate) fn library_catalog(&self) -> super::library_catalog::LibrarySupplement {
        match self.connection_blocking() {
            Ok(connection) => self
                .inner
                .runtime
                .block_on(super::library_catalog::fetch(&connection.session)),
            Err(_) => super::library_catalog::LibrarySupplement {
                entries: Vec::new(),
                loaded_categories: HashSet::new(),
                pins: None,
                failures: 1,
                recents_available: false,
            },
        }
    }

    /// Blocking account mutation; only explicit user Pin/Unpin actions may invoke this worker API.
    pub(crate) fn set_library_pinned(
        &self,
        uri: &str,
        pinned: bool,
    ) -> Result<std::collections::HashSet<String>, SpotifyError> {
        let session = self.connection_blocking()?.session;
        self.inner
            .runtime
            .block_on(super::collection::set_pinned(&session, uri, pinned))
    }

    /// Enrich a small visible batch without holding up the first library render. Best-effort:
    /// during a reconnect it reports failures instead of stalling the per-entry loop.
    pub(crate) fn hydrate_library_entries(
        &self,
        entries: &mut [super::library::LibraryEntry],
        cache: &mut super::library::LibraryHydrator,
    ) -> usize {
        let Some(connection) = self.inner.live_connection() else {
            self.inner.reconnect_now.notify_one();
            return usize::from(!entries.is_empty());
        };
        self.inner
            .runtime
            .block_on(super::library::hydrate(&connection.session, entries, cache))
    }

    /// Start the one-per-session liked membership load. Retries are throttled; the GPUI thread
    /// never waits on the network here.
    pub fn ensure_liked_tracks_loaded(&self) {
        let Some(session) = self
            .inner
            .live_connection()
            .map(|connection| connection.session)
        else {
            self.inner.reconnect_now.notify_one();
            return;
        };
        {
            let mut cache = self.inner.liked.lock().unwrap();
            if cache.loaded || cache.loading {
                return;
            }
            if cache
                .last_attempt
                .is_some_and(|attempt| attempt.elapsed() < Duration::from_secs(10))
            {
                return;
            }
            cache.loading = true;
            cache.last_attempt = Some(Instant::now());
        }
        let liked = self.inner.liked.clone();
        self.inner.runtime.spawn(async move {
            let result = super::collection::snapshot(&session, "collection").await;
            let mut cache = liked.lock().unwrap();
            cache.loading = false;
            if let Ok(items) = result {
                cache.tracks = super::collection::liked_track_uris(&items);
                cache.loaded = true;
                cache.revision = cache.revision.wrapping_add(1);
            }
        });
    }

    /// Monotonic counter for liked membership changes, safe to poll from the UI thread.
    pub fn liked_revision(&self) -> u64 {
        self.inner.liked.lock().unwrap().revision
    }

    /// True once the first authoritative liked-track snapshot has loaded.
    pub fn liked_loaded(&self) -> bool {
        self.inner.liked.lock().unwrap().loaded
    }

    pub fn liked_status(&self, uri: &str) -> LikedStatus {
        let cache = self.inner.liked.lock().unwrap();
        if !cache.loaded || !uri.starts_with("spotify:track:") {
            return LikedStatus::Unknown;
        }
        if cache.tracks.contains(uri) {
            LikedStatus::Liked
        } else {
            LikedStatus::NotLiked
        }
    }

    /// Blocking account mutation for an explicit like/unlike action. Workers only.
    pub fn set_liked(&self, uri: &str, liked: bool) -> Result<(), SpotifyError> {
        if !uri.starts_with("spotify:track:") {
            return Err(SpotifyError::Api(
                "Only Spotify tracks can be saved to Liked Songs".into(),
            ));
        }
        if !self.inner.liked.lock().unwrap().loaded {
            return Err(SpotifyError::Api(
                "Liked Songs are still loading; try again in a moment".into(),
            ));
        }
        let session = self.connection_blocking()?.session;
        self.inner
            .runtime
            .block_on(super::collection::set_liked(&session, uri, liked))?;
        {
            let mut cache = self.inner.liked.lock().unwrap();
            if cache.loaded {
                if liked {
                    cache.tracks.insert(uri.to_string());
                } else {
                    cache.tracks.remove(uri);
                }
                cache.revision = cache.revision.wrapping_add(1);
            }
        }
        if !liked {
            self.prune_liked_songs_detail(uri);
        }
        self.schedule_liked_refresh();
        Ok(())
    }

    /// Remove an unliked track from the retained Liked Songs page and its disk cache so it
    /// cannot briefly reappear before the next network refresh.
    fn prune_liked_songs_detail(&self, uri: &str) {
        let updated = {
            let mut guard = self.inner.liked_songs_detail.lock().unwrap();
            let Some(detail) = guard.as_mut() else {
                return;
            };
            if detail.remove_track_uri(uri) == 0 {
                return;
            }
            detail.clone()
        };
        playlist::write_cached_detail(&self.inner.username, &PlaylistSource::LikedSongs, &updated);
    }

    /// Re-read membership after a write burst. Only the newest scheduled refresh performs the
    /// request, so rapid toggles do not each fetch the whole collection.
    fn schedule_liked_refresh(&self) {
        let generation = {
            let mut cache = self.inner.liked.lock().unwrap();
            cache.refresh_generation = cache.refresh_generation.wrapping_add(1);
            cache.refresh_generation
        };
        let inner = self.inner.clone();
        let liked = self.inner.liked.clone();
        self.inner.runtime.spawn(async move {
            tokio::time::sleep(Duration::from_millis(1_200)).await;
            if liked.lock().unwrap().refresh_generation != generation {
                return;
            }
            let Some(session) = inner.live_connection().map(|connection| connection.session) else {
                inner.reconnect_now.notify_one();
                return;
            };
            let Ok(items) = super::collection::snapshot(&session, "collection").await else {
                return;
            };
            let tracks = super::collection::liked_track_uris(&items);
            let mut cache = liked.lock().unwrap();
            if cache.refresh_generation != generation {
                return;
            }
            if !cache.loaded || cache.tracks != tracks {
                cache.tracks = tracks;
                cache.loaded = true;
                cache.revision = cache.revision.wrapping_add(1);
            }
        });
    }

    /// Searches Spotify's own Pathfinder endpoint. Blocking; call from a worker thread.
    pub fn search(&self, query: &str) -> Result<SearchResults, SpotifyError> {
        let session = self.connection_blocking()?.session;
        self.inner.runtime.block_on(search::search(&session, query))
    }

    /// Fetches the current user's profile and avatar through Spotify's native endpoints.
    pub fn user_profile(&self) -> Result<UserProfile, SpotifyError> {
        let session = self.connection_blocking()?.session;
        self.inner.runtime.block_on(async move {
            let username = session.username();
            let bytes = session
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

    pub(crate) fn home(&self, facet: &str) -> Result<super::home::HomeFeed, SpotifyError> {
        let session = self.connection_blocking()?.session;
        self.inner
            .runtime
            .block_on(super::home::fetch(&session, facet))
    }

    pub(crate) fn home_section(
        &self,
        uri: &str,
        offset: u64,
    ) -> Result<(Vec<super::home::HomeItem>, Option<u64>), SpotifyError> {
        let session = self.connection_blocking()?.session;
        self.inner
            .runtime
            .block_on(super::home::section(&session, uri, offset))
    }

    /// Fetches a saved playlist, or the current user's Liked Songs, plus its first context page.
    pub fn playlist_detail(&self, source: PlaylistSource) -> Result<PlaylistDetail, SpotifyError> {
        let session = self.connection_blocking()?.session;
        self.inner
            .runtime
            .block_on(playlist::fetch_playlist_detail(&session, source))
    }

    /// Fetches an artist's profile overview.
    ///
    /// Must be called from a background worker: it blocks the calling thread on the
    /// Pathfinder request. The returned profile keeps every section independent so a
    /// missing or malformed section does not invalidate the rest of the page.
    pub fn artist_profile(&self, uri: &str) -> Result<ArtistProfile, SpotifyError> {
        let session = self.connection_blocking()?.session;
        self.inner
            .runtime
            .block_on(artist::fetch_artist_profile(&session, uri))
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
        let session = self.connection_blocking()?.session;
        self.inner
            .runtime
            .block_on(playlist::fetch_playlist_page(&session, next_page_url))
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
        let options = LoadRequestOptions {
            start_playing: true,
            ..Default::default()
        };
        let result = self.send_load(LoadRequest::from_context_uri(uri.to_string(), options));
        if result.is_ok() {
            self.set_context_uri(Some(uri.to_string()));
        }
        result
    }

    /// Plays a context starting at a specific track.
    pub fn play_context_at(&self, context_uri: &str, track_uri: &str) -> Result<(), SpotifyError> {
        let options = LoadRequestOptions {
            start_playing: true,
            playing_track: Some(PlayingTrack::Uri(track_uri.to_string())),
            ..Default::default()
        };
        let result = self.send_load(LoadRequest::from_context_uri(
            context_uri.to_string(),
            options,
        ));
        if result.is_ok() {
            self.set_context_uri(Some(context_uri.to_string()));
        }
        result
    }

    fn play_uris(&self, uris: Vec<String>) -> Result<(), SpotifyError> {
        if uris.is_empty() {
            return Ok(());
        }
        let options = LoadRequestOptions {
            start_playing: true,
            playing_track: Some(PlayingTrack::Uri(uris[0].clone())),
            ..Default::default()
        };
        let result = self.send_load(LoadRequest::from_tracks(uris, options));
        if result.is_ok() {
            self.set_context_uri(None);
        }
        result
    }

    fn set_context_uri(&self, context_uri: Option<String>) {
        let mut playback = self.inner.playback.lock().unwrap();
        playback.snapshot.context_uri = context_uri;
    }

    pub fn play_pause(&self) -> Result<(), SpotifyError> {
        let snapshot = self.playback();
        if snapshot.is_playing {
            self.with_spirc(Spirc::play_pause)
        } else {
            self.start_snapshot_playback(snapshot)
        }
    }

    pub fn play(&self) -> Result<(), SpotifyError> {
        self.start_snapshot_playback(self.playback())
    }

    fn start_snapshot_playback(&self, snapshot: PlaybackSnapshot) -> Result<(), SpotifyError> {
        let Some(track) = snapshot.now_playing else {
            return self.with_spirc(Spirc::play);
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

        self.send_load(request)
    }

    pub fn pause(&self) -> Result<(), SpotifyError> {
        self.with_spirc(Spirc::pause)
    }

    pub fn next(&self) -> Result<(), SpotifyError> {
        self.with_spirc(Spirc::next)
    }

    pub fn previous(&self) -> Result<(), SpotifyError> {
        self.with_spirc(Spirc::prev)
    }

    pub fn seek(&self, position_ms: u64) -> Result<(), SpotifyError> {
        let position_ms = position_ms.min(u32::MAX as u64) as u32;
        self.with_spirc(|spirc| spirc.set_position_ms(position_ms))
    }

    pub fn set_volume_percent(&self, percent: u8) -> Result<(), SpotifyError> {
        let percent = percent.min(100);
        let value = volume_value(percent);
        let result = self.with_spirc(|spirc| spirc.set_volume(value));
        // Remember the requested volume even if the session is being rebuilt.
        let mut store = SessionStore::load();
        store.set_volume_percent(percent);
        let mut playback = self.inner.playback.lock().unwrap();
        playback.snapshot.volume_percent = percent;
        result
    }

    /// Applies volume directly to the local soft mixer without sending a Connect command.
    /// Use this for responsive drag previews, then call `set_volume_percent` on release.
    pub fn set_local_volume_percent(&self, percent: u8) {
        self.inner.mixer.set_volume(volume_value(percent.min(100)));
    }

    pub fn set_shuffle(&self, shuffle: bool) -> Result<(), SpotifyError> {
        self.with_spirc(|spirc| spirc.shuffle(shuffle))
    }

    pub fn set_repeat_mode(&self, mode: RepeatMode) -> Result<(), SpotifyError> {
        self.with_spirc(|spirc| spirc.repeat(matches!(mode, RepeatMode::Context)))?;
        self.with_spirc(|spirc| spirc.repeat_track(matches!(mode, RepeatMode::Track)))
    }
}

/// Converts a 0–100 volume percentage to librespot's 16-bit mixer range.
fn volume_value(percent: u8) -> u16 {
    (percent.min(100) as u32 * u16::MAX as u32 / 100) as u16
}

impl NativeInner {
    /// A connection that has not yet been invalidated. Empty during a reconnect.
    fn live_connection(&self) -> Option<SessionConnection> {
        self.connection
            .read()
            .unwrap()
            .as_ref()
            .filter(|connection| !connection.session.is_invalid())
            .cloned()
    }

    fn needs_reconnect(&self, spirc_task: Option<&tokio::task::JoinHandle<()>>) -> bool {
        if spirc_task.is_some_and(|task| task.is_finished()) {
            return true;
        }
        if self.player.lock().unwrap().is_invalid() {
            return true;
        }
        match self.connection.read().unwrap().as_ref() {
            None => true,
            Some(connection) => connection.session.is_invalid(),
        }
    }

    /// Drops the current connection and freezes playback before a rebuild.
    fn detach_connection(&self) -> Option<ResumeTarget> {
        let resume = freeze_for_reconnect(&self.playback);
        if let Some(connection) = self.connection.write().unwrap().take() {
            connection.alive.store(false, Ordering::Relaxed);
            if !connection.session.is_invalid() {
                connection.session.shutdown();
            }
        }
        resume
    }

    /// Applies a user load queued during the reconnect, or resumes interrupted playback.
    fn resume_playback(&self, connection: &SessionConnection, resume: Option<ResumeTarget>) {
        let pending = self.pending_load.lock().unwrap().take();
        let request = pending.or_else(|| resume.map(resume_load_request));
        let Some(request) = request else {
            return;
        };
        let _ = connection.spirc.activate();
        if connection.spirc.load(request).is_err() {
            self.reconnect_now.notify_one();
        }
    }
}

/// Captures the position of an interrupted track and pauses the local snapshot. The track,
/// context, and volume stay visible while the session is rebuilt.
fn freeze_for_reconnect(playback: &Mutex<PlaybackInner>) -> Option<ResumeTarget> {
    let mut inner = playback.lock().unwrap();
    let now = Instant::now();
    let snapshot = inner.snapshot_at(now);
    let resume = if snapshot.is_playing {
        snapshot.now_playing.as_ref().map(|track| ResumeTarget {
            context_uri: snapshot.context_uri.clone(),
            track_uri: track.uri.clone(),
            position_ms: snapshot.position_ms,
        })
    } else {
        None
    };
    inner.snapshot.position_ms = snapshot.position_ms;
    inner.snapshot.is_playing = false;
    inner.local_player_active = false;
    inner.updated_at = now;
    resume
}

fn resume_load_request(target: ResumeTarget) -> LoadRequest {
    let track_uri = target.track_uri;
    let options = LoadRequestOptions {
        start_playing: true,
        seek_to: target.position_ms.min(u32::MAX as u64) as u32,
        playing_track: Some(PlayingTrack::Uri(track_uri.clone())),
        ..Default::default()
    };
    match target.context_uri.filter(|uri| !uri.is_empty()) {
        Some(context_uri) => LoadRequest::from_context_uri(context_uri, options),
        None => LoadRequest::from_tracks(vec![track_uri], options),
    }
}

fn build_session(
    cache_dir: &Path,
    device_id: &str,
    client_id: &str,
) -> Result<Session, SpotifyError> {
    let cache = Cache::new(Some(cache_dir), Some(cache_dir), None, None)?;
    let session_config = SessionConfig {
        client_id: client_id.to_string(),
        device_id: device_id.to_string(),
        tmp_dir: cache_dir.to_path_buf(),
        autoplay: Some(true),
        ..Default::default()
    };
    Ok(Session::new(session_config, Some(cache)))
}

fn create_player(session: Session, mixer: &Arc<SoftMixer>) -> Result<Arc<Player>, SpotifyError> {
    let player_config = PlayerConfig {
        position_update_interval: Some(Duration::from_millis(500)),
        ..Default::default()
    };
    let soft_volume = mixer.get_soft_volume();
    let sink_builder = audio_backend::find(None)
        .ok_or_else(|| SpotifyError::Playback("no librespot audio backend is available".into()))?;
    Ok(Player::new(
        player_config,
        session,
        soft_volume,
        move || sink_builder(None, AudioFormat::S16),
    ))
}

/// Builds a Connect device around an already-created `Session` and spawns its task.
async fn connect_spirc(
    runtime: &Arc<tokio::runtime::Runtime>,
    session: Session,
    access_token: String,
    player: Arc<Player>,
    mixer: Arc<SoftMixer>,
    playback: Arc<Mutex<PlaybackInner>>,
    initial_volume: u16,
) -> Result<OpenedConnection, SpotifyError> {
    let connect_config = ConnectConfig {
        name: CONNECT_DEVICE_NAME.to_string(),
        device_type: DeviceType::Computer,
        initial_volume,
        ..Default::default()
    };
    let mixin: Arc<dyn Mixer> = mixer;
    let credentials = Credentials::with_access_token(access_token);
    let (spirc, spirc_task) =
        Spirc::new(connect_config, session.clone(), credentials, player, mixin)
            .await
            .map_err(|error| SpotifyError::Session(error.to_string()))?;

    let connection = SessionConnection {
        session,
        spirc,
        alive: Arc::new(AtomicBool::new(true)),
    };
    spawn_connect_state_event_loop(runtime, &connection, playback);
    Ok(OpenedConnection {
        spirc_task: runtime.spawn(spirc_task),
        connection,
    })
}

/// Watches the live connection and rebuilds it after librespot invalidates the session.
fn spawn_session_supervisor(client: &NativeSpotify, spirc_task: tokio::task::JoinHandle<()>) {
    let weak: Weak<NativeInner> = Arc::downgrade(&client.inner);
    client.inner.runtime.spawn(async move {
        let mut spirc_task = Some(spirc_task);
        let mut backoff = RECONNECT_MIN_BACKOFF;
        // Playback interrupted by the first failure; kept until a session accepts the resume so
        // retrying the reconnect does not keep re-freezing an already paused snapshot.
        let mut resume = None;

        loop {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                _ = inner.reconnect_now.notified() => {}
            }
            if inner.signed_out.load(Ordering::Relaxed) {
                return;
            }
            if !inner.needs_reconnect(spirc_task.as_ref()) {
                continue;
            }

            if let Some(target) = inner.detach_connection() {
                resume = Some(target);
            }
            if let Some(task) = spirc_task.take() {
                task.abort();
            }

            match reconnect(&inner).await {
                Ok(opened) => {
                    let connection = opened.connection;
                    *inner.connection.write().unwrap() = Some(connection.clone());
                    spirc_task = Some(opened.spirc_task);
                    backoff = RECONNECT_MIN_BACKOFF;
                    eprintln!("Spotify session reconnected");
                    inner.resume_playback(&connection, resume.take());
                }
                Err(ReconnectFailure::SignedOut) => {
                    SessionStore::load().clear_refresh_token();
                    inner.signed_out.store(true, Ordering::Relaxed);
                    eprintln!("Spotify session expired; sign-in required");
                    return;
                }
                Err(ReconnectFailure::Retry(error)) => {
                    eprintln!("Spotify session reconnect failed ({error}); retrying");
                    tokio::time::sleep(backoff).await;
                    backoff = next_reconnect_backoff(backoff);
                }
            }
        }
    });
}

fn next_reconnect_backoff(current: Duration) -> Duration {
    (current * 2).min(RECONNECT_MAX_BACKOFF)
}

/// Refreshes the OAuth token and rebuilds the librespot connection.
async fn reconnect(inner: &NativeInner) -> Result<OpenedConnection, ReconnectFailure> {
    let stored_refresh = SessionStore::load()
        .refresh_token()
        .map(str::to_owned)
        .ok_or(ReconnectFailure::SignedOut)?;
    let (client_id, oauth) =
        oauth_client(false).map_err(|error| ReconnectFailure::Retry(error.to_string()))?;
    let token = oauth
        .refresh_token_async(&stored_refresh)
        .await
        .map_err(|error| {
            if is_rejected_refresh(&error) {
                ReconnectFailure::SignedOut
            } else {
                ReconnectFailure::Retry(format!("OAuth refresh failed: {error}"))
            }
        })?;

    let mut store = SessionStore::load();
    let refresh_token = if token.refresh_token.is_empty() {
        stored_refresh
    } else {
        token.refresh_token
    };
    store.set_refresh_token(&refresh_token);
    let device_id = store.ensure_device_id();

    let cache_dir = session_cache_dir();
    std::fs::create_dir_all(&cache_dir)?;
    restrict_private_dir(&cache_dir);
    let session = build_session(&cache_dir, &device_id, &client_id)?;

    let player = {
        let mut guard = inner.player.lock().unwrap();
        if guard.is_invalid() {
            let replacement = create_player(session.clone(), &inner.mixer)?;
            *guard = replacement.clone();
            spawn_player_event_loop(&inner.runtime, &replacement, inner.playback.clone());
            replacement
        } else {
            guard.clone()
        }
    };
    // Match librespot's own reconnect path: point the live player at the new session before the
    // Connect device is built around it.
    player.set_session(session.clone());

    let volume = inner.playback.lock().unwrap().snapshot.volume_percent;
    connect_spirc(
        &inner.runtime,
        session,
        token.access_token,
        player,
        inner.mixer.clone(),
        inner.playback.clone(),
        volume_value(volume),
    )
    .await
    .map_err(ReconnectFailure::from)
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
        PlayerEvent::RepeatChanged { context, track } => {
            inner.snapshot.repeat = RepeatMode::from_flags(context, track);
        }
        _ => {}
    }
}

fn spawn_connect_state_event_loop(
    runtime: &Arc<tokio::runtime::Runtime>,
    connection: &SessionConnection,
    playback: Arc<Mutex<PlaybackInner>>,
) {
    let spirc = connection.spirc.clone();
    let session = connection.session.clone();
    let alive = connection.alive.clone();
    runtime.spawn(async move {
        let mut resolved_uri = String::new();
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            // Stop polling a connection that has been replaced so stale server state cannot
            // overwrite the reconnected session's playback snapshot.
            if !alive.load(Ordering::Relaxed) {
                return;
            }
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

    fn live_session(client: &NativeSpotify) -> Session {
        client
            .connection_blocking()
            .expect("live Spotify session")
            .session
    }

    #[test]
    fn freeze_for_reconnect_pauses_and_keeps_resume_target() {
        let start = Instant::now();
        let playback = Mutex::new(PlaybackInner {
            snapshot: PlaybackSnapshot {
                now_playing: Some(NowPlaying {
                    uri: TEST_TRACK_URI.into(),
                    duration_ms: 180_000,
                    ..Default::default()
                }),
                is_playing: true,
                position_ms: 61_000,
                context_uri: Some("spotify:playlist:example".into()),
                volume_percent: 32,
                ..Default::default()
            },
            updated_at: start,
            local_player_active: true,
        });

        let resume = freeze_for_reconnect(&playback).expect("playing track resumes");

        assert_eq!(resume.track_uri, TEST_TRACK_URI);
        assert_eq!(
            resume.context_uri.as_deref(),
            Some("spotify:playlist:example")
        );
        assert!(resume.position_ms >= 61_000);
        let inner = playback.lock().unwrap();
        assert!(!inner.snapshot.is_playing);
        assert!(!inner.local_player_active);
        assert_eq!(inner.snapshot.position_ms, resume.position_ms);
        assert_eq!(inner.snapshot.volume_percent, 32);
        assert!(inner.snapshot.now_playing.is_some());
    }

    #[test]
    fn freeze_for_reconnect_does_not_resume_paused_playback() {
        let start = Instant::now();
        let playback = Mutex::new(PlaybackInner {
            snapshot: PlaybackSnapshot {
                now_playing: Some(NowPlaying {
                    uri: TEST_TRACK_URI.into(),
                    ..Default::default()
                }),
                is_playing: false,
                position_ms: 12_000,
                ..Default::default()
            },
            updated_at: start,
            local_player_active: true,
        });

        assert!(freeze_for_reconnect(&playback).is_none());
        let inner = playback.lock().unwrap();
        assert!(!inner.local_player_active);
        assert_eq!(inner.snapshot.position_ms, 12_000);
    }

    #[test]
    fn resume_request_uses_context_or_track_and_position() {
        let request = resume_load_request(ResumeTarget {
            context_uri: Some("spotify:album:example".into()),
            track_uri: TEST_TRACK_URI.into(),
            position_ms: 42_000,
        });
        assert!(request.start_playing);
        assert_eq!(request.seek_to, 42_000);
        assert!(matches!(
            &request.playing_track,
            Some(PlayingTrack::Uri(uri)) if uri == TEST_TRACK_URI
        ));

        let request = resume_load_request(ResumeTarget {
            context_uri: Some(String::new()),
            track_uri: TEST_TRACK_URI.into(),
            position_ms: 1_000,
        });
        assert!(request.start_playing);
        assert_eq!(request.seek_to, 1_000);
    }

    #[test]
    fn reconnect_backoff_doubles_and_caps() {
        let mut delay = RECONNECT_MIN_BACKOFF;
        assert_eq!(delay, Duration::from_secs(2));
        delay = next_reconnect_backoff(delay);
        assert_eq!(delay, Duration::from_secs(4));
        delay = next_reconnect_backoff(delay);
        assert_eq!(delay, Duration::from_secs(8));
        delay = next_reconnect_backoff(delay);
        assert_eq!(delay, Duration::from_secs(16));
        delay = next_reconnect_backoff(delay);
        assert_eq!(delay, Duration::from_secs(30));
        delay = next_reconnect_backoff(delay);
        assert_eq!(delay, Duration::from_secs(30));
    }

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
    fn repeat_and_shuffle_events_update_transport_snapshot() {
        let start = Instant::now();
        let mut inner = playback_clock(start);
        apply_player_event(
            &mut inner,
            PlayerEvent::ShuffleChanged { shuffle: true },
            start,
        );
        apply_player_event(
            &mut inner,
            PlayerEvent::RepeatChanged {
                context: true,
                track: false,
            },
            start,
        );
        assert!(inner.snapshot.shuffle);
        assert_eq!(inner.snapshot.repeat, RepeatMode::Context);
        apply_player_event(
            &mut inner,
            PlayerEvent::RepeatChanged {
                context: false,
                track: true,
            },
            start,
        );
        assert_eq!(inner.snapshot.repeat, RepeatMode::Track);
        apply_player_event(
            &mut inner,
            PlayerEvent::RepeatChanged {
                context: false,
                track: false,
            },
            start,
        );
        assert_eq!(inner.snapshot.repeat, RepeatMode::Off);
    }

    #[test]
    fn repeat_mode_cycles_off_context_track() {
        assert_eq!(RepeatMode::Off.next(), RepeatMode::Context);
        assert_eq!(RepeatMode::Context.next(), RepeatMode::Track);
        assert_eq!(RepeatMode::Track.next(), RepeatMode::Off);
        assert!(!RepeatMode::Off.is_active());
        assert!(RepeatMode::Context.is_active());
        assert!(RepeatMode::Track.is_active());
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
        let session = live_session(&client);
        for set in ["artist", "show", "ylpin", "collection", "prerelease"] {
            let page = client
                .inner
                .runtime
                .block_on(super::super::collection::page(&session, set, ""))
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
        let session = live_session(&client);
        let response = client.inner.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(15), session.spclient().request_as_json(
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
    #[ignore = "requires network and a user-approved native account session"]
    fn smoke_native_library_diagnostics() {
        use super::super::library::LibraryHydrator;
        assert!(
            SessionStore::load().refresh_token().is_some(),
            "Authorize rspotify before running this check"
        );
        let client = NativeSpotify::restore().expect("native session restore");
        let entries = client.library_entries().expect("native library rootlist");
        eprintln!("diagnostics: rootlist entries={}", entries.len());
        let catalog = client.library_catalog();
        eprintln!(
            "diagnostics: catalog failures={} recents_available={} categories={} pins={}",
            catalog.failures,
            catalog.recents_available,
            catalog.loaded_categories.len(),
            catalog.pins.as_ref().map(|p| p.len()).unwrap_or(0)
        );
        let mut cache = LibraryHydrator::default();
        let mut failures = 0;
        for entry in entries {
            let mut batch = [entry];
            failures += client.hydrate_library_entries(&mut batch, &mut cache);
        }
        eprintln!("diagnostics: per-entry hydrate failures={failures}");
        // Read-only: no playback, pin, or library mutation is issued.
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
    #[ignore = "requires saved session and read-only account/network access"]
    fn smoke_native_home_schema() {
        let client = NativeSpotify::restore().expect("saved native session");
        let home = client.home("").expect("Home feed");
        assert!(!home.shortcuts.is_empty());
        assert!(!home.shelves.is_empty());
        eprintln!(
            "Home: {} shortcuts, {} shelves, {} chips, {} Canvas URLs",
            home.shortcuts.len(),
            home.shelves.len(),
            home.chips.len(),
            home.panels.iter().filter(|p| p.video_url.is_some()).count()
        );
        let shelf = home.shelves.first().expect("shelf");
        let (items, next) = client.home_section(&shelf.uri, 0).expect("full shelf");
        assert!(!items.is_empty());
        eprintln!(
            "Full shelf: {} items, next page: {}",
            items.len(),
            next.is_some()
        );
        if let Some(offset) = next {
            let (_, after) = client
                .home_section(&shelf.uri, offset)
                .expect("next shelf page");
            assert!(after.is_none_or(|after| after > offset));
        }
        for chip in home.chips {
            let filtered = client.home(&chip.id).expect("category feed");
            eprintln!("Category: {} shelves", filtered.shelves.len());
        }
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
    #[ignore = "requires a saved session and read-only account/network access"]
    fn smoke_native_session_reconnect() {
        let client = NativeSpotify::restore().expect("saved native session");
        assert!(client.is_connected(), "initial session was not established");
        assert!(
            client.search("daft punk").is_ok(),
            "Pathfinder failed before the forced reconnect"
        );

        // Simulates the invalidation librespot performs when the access-point connection
        // is lost. The supervisor should rebuild the session without user interaction.
        live_session(&client).shutdown();

        let deadline = Instant::now() + Duration::from_secs(60);
        while !client.is_connected() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(250));
        }
        assert!(
            client.is_connected(),
            "session was not re-established within 60 seconds"
        );
        assert!(
            !client.is_signed_out(),
            "reconnect incorrectly cleared the session"
        );

        let results = client.search("daft punk").expect("search after reconnect");
        assert!(
            !results.is_empty(),
            "search returned no results after reconnect"
        );
        eprintln!(
            "reconnect ok: {} tracks, {} artists after a forced session shutdown",
            results.tracks.len(),
            results.artists.len()
        );
        // Read-only: no playback or account mutation is issued by this test.
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

    #[test]
    #[ignore = "requires read-only Spotify account/network approval"]
    fn smoke_native_artist_profile() {
        use super::super::library::LibraryKind;

        let client = NativeSpotify::authenticate().expect("native login");
        let catalog = client.library_catalog();
        let limit = std::env::var("RSPOTIFY_ARTIST_PROBE_LIMIT")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(1);
        let artists: Vec<_> = catalog
            .entries
            .iter()
            .filter(|entry| entry.kind == LibraryKind::Artist)
            .take(limit)
            .collect();
        if artists.is_empty() {
            eprintln!("no saved artist available for the read-only profile probe");
            return;
        }

        for (index, entry) in artists.iter().enumerate() {
            let profile = client
                .artist_profile(&entry.uri)
                .expect("native artist overview");
            assert_eq!(profile.uri, entry.uri);
            assert!(!profile.name.is_empty());
            assert_eq!(
                profile.name, entry.name,
                "overview name did not match the library name"
            );

            let counts = [
                ("popular", profile.popular_tracks.len()),
                ("liked", usize::from(profile.liked_music.is_some())),
                ("pick", usize::from(profile.artist_pick.is_some())),
                ("discography-popular", profile.discography.popular.len()),
                ("discography-albums", profile.discography.albums.len()),
                ("discography-singles", profile.discography.singles.len()),
                (
                    "discography-compilations",
                    profile.discography.compilations.len(),
                ),
                ("featuring", profile.featuring.len()),
                ("videos", profile.music_videos.len()),
                ("concerts", profile.concerts.len()),
                ("merch", profile.merchandise.len()),
                ("discovered-on", profile.discovered_on.len()),
                ("related-artists", profile.related_artists.len()),
                ("appears-on", profile.appears_on.len()),
            ];
            eprintln!(
                "native artist #{index}: verified={}, header={}, avatar={}, \
                 monthly-listeners={}, followers={}, rank={}, following={:?}",
                profile.verified,
                profile.header_image_url.is_some(),
                profile.avatar_image_url.is_some(),
                profile.monthly_listeners.is_some(),
                profile.followers.is_some(),
                profile.world_rank.is_some(),
                profile.following,
            );
            eprintln!("native artist #{index} sections: {counts:?}");
        }
        // Read-only: no playback, follow, or other account mutation is sent.
    }
}
