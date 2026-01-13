//! Spotify Authentication using rspotify's AuthCodePkceSpotify.
//!
//! This module provides authentication functionality for the Spotify Web API
//! using the Authorization Code with PKCE flow via the rspotify library.

mod server;

use rspotify::{AuthCodePkceSpotify, Config, Credentials, OAuth, clients::OAuthClient, scopes};
use server::CallbackServer;
use std::time::Duration;

/// Default scopes for Spotify API access.
pub const DEFAULT_SCOPES: &[&str] = &[
    "user-read-private",
    "user-read-email",
    "playlist-read-private",
    "playlist-read-collaborative",
    "user-library-read",
    "user-read-playback-state",
    "user-modify-playback-state",
    "user-read-currently-playing",
    "streaming",
];

/// Configuration for the Spotify OAuth flow.
#[derive(Debug, Clone)]
pub struct SpotifyAuthConfig {
    pub client_id: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
}

impl SpotifyAuthConfig {
    pub fn new(client_id: impl Into<String>, redirect_uri: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            redirect_uri: redirect_uri.into(),
            scopes: DEFAULT_SCOPES.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
    }
}

/// Performs the full OAuth PKCE flow and returns an authenticated AuthCodePkceSpotify client.
///
/// This is a blocking call that should be run in a background thread.
///
/// # Arguments
/// * `config` - The authentication configuration with client_id, redirect_uri, and scopes
///
/// # Returns
/// An authenticated `AuthCodePkceSpotify` instance ready to make API calls
pub fn authenticate(config: &SpotifyAuthConfig) -> Result<AuthCodePkceSpotify, AuthError> {
    // Create credentials (PKCE doesn't need client secret)
    let creds = Credentials::new_pkce(&config.client_id);

    // Build OAuth with scopes
    let oauth = OAuth {
        redirect_uri: config.redirect_uri.clone(),
        scopes: scopes!(
            "user-read-private",
            "user-read-email",
            "playlist-read-private",
            "playlist-read-collaborative",
            "user-library-read",
            "user-read-playback-state",
            "user-modify-playback-state",
            "user-read-currently-playing",
            "streaming"
        ),
        ..Default::default()
    };

    // Configure with auto token refresh
    let rspotify_config = Config {
        token_refreshing: true,
        ..Default::default()
    };

    // Create the Spotify client
    let mut spotify = AuthCodePkceSpotify::with_config(creds, oauth, rspotify_config);

    // Generate the authorization URL
    let auth_url = spotify
        .get_authorize_url(None)
        .map_err(|e| AuthError::Config(e.to_string()))?;

    // Extract port from redirect URI
    let port = extract_port(&config.redirect_uri)?;

    // Start the callback server before opening the browser
    let server = CallbackServer::new(port);

    // Open the browser for user authorization
    if let Err(e) = open::that(&auth_url) {
        return Err(AuthError::BrowserOpen(e.to_string()));
    }

    // Wait for the callback with the authorization code
    let code = server
        .wait_for_code(Duration::from_secs(300))
        .map_err(|e| AuthError::Callback(e.to_string()))?;

    // Exchange the authorization code for tokens
    // We need to use a runtime since rspotify is async
    let rt = tokio::runtime::Runtime::new().map_err(|e| AuthError::Runtime(e.to_string()))?;

    rt.block_on(async {
        spotify
            .request_token(&code)
            .await
            .map_err(|e| AuthError::Token(e.to_string()))
    })?;

    Ok(spotify)
}

/// Extracts the port from a redirect URI.
fn extract_port(redirect_uri: &str) -> Result<u16, AuthError> {
    let url = url::Url::parse(redirect_uri)
        .map_err(|e| AuthError::Config(format!("Invalid redirect URI: {}", e)))?;

    url.port()
        .ok_or_else(|| AuthError::Config("Redirect URI must specify a port".to_string()))
}

/// Errors that can occur during authentication.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Failed to open browser: {0}")]
    BrowserOpen(String),

    #[error("Callback error: {0}")]
    Callback(String),

    #[error("Token error: {0}")]
    Token(String),

    #[error("Runtime error: {0}")]
    Runtime(String),
}
