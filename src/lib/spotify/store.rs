//! Small on-disk store for the persistent device id and OAuth refresh token.
//!
//! The file lives under the user's cache directory and is written with owner-only
//! permissions on Unix. It never contains the account password.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::SpotifyError;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct StoredSession {
    /// Stable Spotify Connect device id, generated on first launch.
    pub device_id: String,
    /// OAuth refresh token (not an access token).
    pub refresh_token: Option<String>,
}

pub struct SessionStore {
    path: PathBuf,
    data: StoredSession,
}

impl SessionStore {
    pub fn load() -> Self {
        let path = state_file_path();
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|contents| serde_json::from_str::<StoredSession>(&contents).ok())
            .unwrap_or_default();

        Self { path, data }
    }

    pub fn refresh_token(&self) -> Option<&str> {
        self.data.refresh_token.as_deref()
    }

    pub fn ensure_device_id(&mut self) -> String {
        if self.data.device_id.is_empty() {
            self.data.device_id = format!("{:032x}", rand::random::<u128>());
            if let Err(error) = self.save() {
                eprintln!("failed to persist Spotify device id: {error}");
            }
        }
        self.data.device_id.clone()
    }

    pub fn set_refresh_token(&mut self, token: &str) {
        self.data.refresh_token = Some(token.to_string());
        if let Err(error) = self.save() {
            eprintln!("failed to persist Spotify refresh token: {error}");
        }
    }

    pub fn save(&self) -> Result<(), SpotifyError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
            restrict_dir(parent);
        }

        let json = serde_json::to_string_pretty(&self.data)
            .map_err(|error| SpotifyError::Storage(error.to_string()))?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        restrict_file(&tmp);
        fs::rename(&tmp, &self.path)?;

        Ok(())
    }
}

fn state_file_path() -> PathBuf {
    if let Ok(dir) = std::env::var("RSPOTIFY_STATE_DIR") {
        return Path::new(&dir).join("session.json");
    }

    let cache = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|home| Path::new(&home).join(".cache")))
        .unwrap_or_else(|_| std::env::temp_dir());

    cache.join("rspotify").join("session.json")
}

#[cfg(unix)]
fn restrict_dir(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn restrict_dir(_path: &Path) {}

#[cfg(unix)]
fn restrict_file(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_file(_path: &Path) {}
