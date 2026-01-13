//! Application-wide global state.
//!
//! Similar to React Context, this provides a way to share state across
//! all components without prop drilling.

use gpui::Global;
use rspotify::AuthCodePkceSpotify;

/// Global application state accessible from any component.
///
/// Usage (similar to React's useContext):
/// ```ignore
/// // Read state
/// let app_state = cx.global::<AppState>();
/// if app_state.is_authenticated() {
///     // user is logged in
/// }
///
/// // Update state
/// cx.update_global::<AppState, _>(|state, _cx| {
///     state.set_spotify_client(client);
/// });
/// ```
#[derive(Clone, Default)]
pub struct AppState {
    /// The authenticated Spotify client.
    /// Wrapped in Arc<Mutex> for thread-safe access and async operations.
    spotify: Option<AuthCodePkceSpotify>,
}

impl Global for AppState {}

impl AppState {
    /// Creates a new default AppState.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if the user is authenticated.
    pub fn is_authenticated(&self) -> bool {
        self.spotify.is_some()
    }

    /// Sets the authenticated Spotify client.
    pub fn set_spotify_client(&mut self, client: AuthCodePkceSpotify) {
        self.spotify = Some(client);
    }

    /// Gets a clone of the Spotify client handle.
    /// Returns None if not authenticated.
    pub fn spotify_client(&self) -> Option<AuthCodePkceSpotify> {
        self.spotify.clone()
    }

    /// Clears authentication (logout).
    pub fn logout(&mut self) {
        self.spotify = None;
    }
}
