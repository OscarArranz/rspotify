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
    /// Last known non-secret profile details used to render the app before session refresh.
    #[serde(default)]
    pub profile: Option<StoredProfile>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredProfile {
    pub name: String,
    pub image_url: Option<String>,
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

    pub fn profile(&self) -> Option<&StoredProfile> {
        self.data.profile.as_ref()
    }

    pub fn has_restorable_session(&self) -> bool {
        self.refresh_token().is_some()
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

    pub fn clear_refresh_token(&mut self) {
        self.data.refresh_token = None;
        self.data.profile = None;
        if let Err(error) = self.save() {
            eprintln!("failed to clear Spotify refresh token: {error}");
        }
    }

    pub fn set_profile(&mut self, profile: StoredProfile) {
        self.data.profile = Some(profile);
        if let Err(error) = self.save() {
            eprintln!("failed to persist Spotify profile: {error}");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_profile_round_trips_and_legacy_session_remains_readable() {
        let session = StoredSession {
            device_id: "device-id".into(),
            refresh_token: Some("refresh-token".into()),
            profile: Some(StoredProfile {
                name: "Listener".into(),
                image_url: Some("https://images.example/avatar".into()),
            }),
        };
        let encoded = serde_json::to_string(&session).unwrap();
        let decoded: StoredSession = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.profile, session.profile);

        let legacy: StoredSession =
            serde_json::from_str(r#"{"device_id":"old-device","refresh_token":"old-token"}"#)
                .unwrap();
        assert!(legacy.profile.is_none());
    }
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
