//! Spotify Authorization Code with PKCE Flow implementation.
//!
//! This module provides authentication functionality for the Spotify Web API
//! using the Authorization Code with PKCE flow, which is recommended for
//! applications where a client secret cannot be securely stored.

mod pkce;
mod server;
mod token;

pub use pkce::PkceChallenge;
pub use token::{SpotifyToken, TokenError};

use server::CallbackServer;
use std::time::Duration;

/// Scopes for Spotify API access.
/// Add more scopes as needed for your application.
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

/// Main authentication client for Spotify.
pub struct SpotifyAuth {
    config: SpotifyAuthConfig,
}

impl SpotifyAuth {
    pub fn new(config: SpotifyAuthConfig) -> Self {
        Self { config }
    }

    /// Performs the full OAuth PKCE flow:
    /// 1. Generates PKCE challenge
    /// 2. Opens browser for user authorization
    /// 3. Starts local server to receive callback
    /// 4. Exchanges authorization code for tokens
    ///
    /// This is a blocking call that should be run in a background thread.
    pub fn authenticate(&self) -> Result<SpotifyToken, AuthError> {
        let pkce = PkceChallenge::generate();
        let auth_url = self.build_auth_url(&pkce);

        // Start the callback server before opening the browser
        let port = self.extract_port()?;
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
        let token = token::exchange_code(
            &self.config.client_id,
            &code,
            &self.config.redirect_uri,
            &pkce.verifier,
        )?;

        Ok(token)
    }

    /// Refreshes an existing token using the refresh token.
    /// This is a blocking call.
    pub fn refresh_token(&self, refresh_token: &str) -> Result<SpotifyToken, AuthError> {
        let token = token::refresh_token(&self.config.client_id, refresh_token)?;
        Ok(token)
    }

    fn build_auth_url(&self, pkce: &PkceChallenge) -> String {
        let scopes = self.config.scopes.join(" ");
        let state = pkce::generate_random_string(16);

        format!(
            "https://accounts.spotify.com/authorize?\
            client_id={}&\
            response_type=code&\
            redirect_uri={}&\
            code_challenge_method=S256&\
            code_challenge={}&\
            scope={}&\
            state={}",
            urlencoding::encode(&self.config.client_id),
            urlencoding::encode(&self.config.redirect_uri),
            urlencoding::encode(&pkce.challenge),
            urlencoding::encode(&scopes),
            urlencoding::encode(&state),
        )
    }

    fn extract_port(&self) -> Result<u16, AuthError> {
        let url = url::Url::parse(&self.config.redirect_uri)
            .map_err(|e| AuthError::Config(format!("Invalid redirect URI: {}", e)))?;

        url.port()
            .ok_or_else(|| AuthError::Config("Redirect URI must specify a port".to_string()))
    }
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
    Token(#[from] TokenError),
}
