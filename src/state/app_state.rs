//! Application-wide global state.
//!
//! Similar to React Context, this provides a way to share state across
//! all components without prop drilling.

use gpui::Global;

use crate::lib::auth::SpotifyToken;

/// Authentication state for the application.
#[derive(Debug, Clone, Default)]
pub struct AuthState {
    /// The current Spotify access token.
    pub access_token: Option<String>,
    /// The Spotify refresh token for obtaining new access tokens.
    pub refresh_token: Option<String>,
    /// Token type (usually "Bearer").
    pub token_type: Option<String>,
    /// Granted OAuth scopes.
    pub scope: Option<String>,
    /// Token expiration time in seconds.
    pub expires_in: Option<u64>,
}

impl AuthState {
    /// Returns true if the user is authenticated.
    pub fn is_authenticated(&self) -> bool {
        self.access_token.is_some()
    }

    /// Creates an AuthState from a SpotifyToken.
    pub fn from_token(token: &SpotifyToken) -> Self {
        Self {
            access_token: Some(token.access_token.clone()),
            refresh_token: Some(token.refresh_token.clone()),
            token_type: Some(token.token_type.clone()),
            scope: Some(token.scope.clone()),
            expires_in: Some(token.expires_in),
        }
    }

    /// Clears the authentication state (logout).
    pub fn clear(&mut self) {
        self.access_token = None;
        self.refresh_token = None;
        self.token_type = None;
        self.scope = None;
        self.expires_in = None;
    }
}

/// Global application state accessible from any component.
///
/// Usage (similar to React's useContext):
/// ```ignore
/// // Read state
/// let app_state = cx.global::<AppState>();
/// if app_state.auth.is_authenticated() {
///     // user is logged in
/// }
///
/// // Update state
/// cx.update_global::<AppState, _>(|state, _cx| {
///     state.auth = AuthState::from_token(&token);
/// });
/// ```
#[derive(Debug, Clone, Default)]
pub struct AppState {
    /// Authentication state (tokens, user info).
    pub auth: AuthState,
}

impl Global for AppState {}

impl AppState {
    /// Creates a new default AppState.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the authentication state from a Spotify token.
    pub fn set_auth(&mut self, token: &SpotifyToken) {
        self.auth = AuthState::from_token(token);
    }

    /// Clears authentication (logout).
    pub fn logout(&mut self) {
        self.auth.clear();
    }
}
