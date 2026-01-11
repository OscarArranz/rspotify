//! Token management for Spotify OAuth.
//!
//! Handles token exchange and refresh operations with the Spotify token endpoint.
//! Uses blocking reqwest since GPUI has its own async executor.

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

const TOKEN_ENDPOINT: &str = "https://accounts.spotify.com/api/token";

/// Spotify access and refresh tokens with expiration tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotifyToken {
    pub access_token: String,
    pub token_type: String,
    pub scope: String,
    pub expires_in: u64,
    pub refresh_token: String,
    #[serde(skip)]
    created_at: Option<Instant>,
}

impl SpotifyToken {
    /// Returns true if the token has expired or will expire within the given buffer.
    pub fn is_expired(&self) -> bool {
        self.is_expired_with_buffer(Duration::from_secs(60))
    }

    /// Returns true if the token has expired or will expire within the given buffer duration.
    pub fn is_expired_with_buffer(&self, buffer: Duration) -> bool {
        match self.created_at {
            Some(created) => {
                let elapsed = created.elapsed();
                let expires_in = Duration::from_secs(self.expires_in);
                elapsed + buffer >= expires_in
            }
            None => false, // If no creation time tracked, assume not expired
        }
    }

    /// Returns the Authorization header value for API requests.
    pub fn auth_header(&self) -> String {
        format!("{} {}", self.token_type, self.access_token)
    }
}

/// Response from Spotify's token endpoint.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    scope: String,
    expires_in: u64,
    refresh_token: Option<String>,
}

/// Error response from Spotify's token endpoint.
#[derive(Debug, Deserialize)]
struct TokenErrorResponse {
    error: String,
    error_description: Option<String>,
}

/// Exchanges an authorization code for access and refresh tokens.
/// This is a blocking call.
pub fn exchange_code(
    client_id: &str,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<SpotifyToken, TokenError> {
    let client = Client::new();

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", code_verifier),
    ];

    let response = client
        .post(TOKEN_ENDPOINT)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&params)
        .send()
        .map_err(|e| TokenError::NetworkError(e.to_string()))?;

    handle_token_response(response, None)
}

/// Refreshes an access token using a refresh token.
/// This is a blocking call.
pub fn refresh_token(client_id: &str, refresh_token: &str) -> Result<SpotifyToken, TokenError> {
    let client = Client::new();

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
    ];

    let response = client
        .post(TOKEN_ENDPOINT)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&params)
        .send()
        .map_err(|e| TokenError::NetworkError(e.to_string()))?;

    handle_token_response(response, Some(refresh_token))
}

fn handle_token_response(
    response: reqwest::blocking::Response,
    existing_refresh_token: Option<&str>,
) -> Result<SpotifyToken, TokenError> {
    let status = response.status();

    if status.is_success() {
        let token_response: TokenResponse = response
            .json()
            .map_err(|e| TokenError::ParseError(e.to_string()))?;

        // Use the new refresh token if provided, otherwise keep the existing one
        let refresh_token = token_response
            .refresh_token
            .or_else(|| existing_refresh_token.map(String::from))
            .ok_or_else(|| TokenError::MissingRefreshToken)?;

        Ok(SpotifyToken {
            access_token: token_response.access_token,
            token_type: token_response.token_type,
            scope: token_response.scope,
            expires_in: token_response.expires_in,
            refresh_token,
            created_at: Some(Instant::now()),
        })
    } else {
        let error_response: TokenErrorResponse = response
            .json()
            .map_err(|e| TokenError::ParseError(e.to_string()))?;

        Err(TokenError::AuthError {
            error: error_response.error,
            description: error_response.error_description,
        })
    }
}

/// Errors that can occur during token operations.
#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Failed to parse response: {0}")]
    ParseError(String),

    #[error("Authentication error: {error} - {}", description.as_deref().unwrap_or("No description"))]
    AuthError {
        error: String,
        description: Option<String>,
    },

    #[error("No refresh token provided in response")]
    MissingRefreshToken,
}
