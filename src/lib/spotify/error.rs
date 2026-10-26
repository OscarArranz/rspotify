//! Errors returned by the native Spotify client.

#[derive(Debug, thiserror::Error)]
pub enum SpotifyError {
    #[error("OAuth error: {0}")]
    OAuth(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Stored Spotify credentials were rejected")]
    InvalidRefreshToken,

    #[error("Session error: {0}")]
    Session(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Spotify API error: {0}")]
    Api(String),

    #[error("Playback error: {0}")]
    Playback(String),

    #[error("Storage error: {0}")]
    Storage(String),
}

impl From<librespot::core::Error> for SpotifyError {
    fn from(value: librespot::core::Error) -> Self {
        Self::Session(value.to_string())
    }
}

impl From<reqwest::Error> for SpotifyError {
    fn from(value: reqwest::Error) -> Self {
        Self::Http(value.to_string())
    }
}

impl From<librespot::oauth::OAuthError> for SpotifyError {
    fn from(value: librespot::oauth::OAuthError) -> Self {
        Self::OAuth(value.to_string())
    }
}

impl From<std::io::Error> for SpotifyError {
    fn from(value: std::io::Error) -> Self {
        Self::Storage(value.to_string())
    }
}
