//! Native Spotify integration.
//!
//! This module talks to the same services as Spotify's own desktop client:
//! - OAuth login at `accounts.spotify.com` with the desktop client id
//! - session/keymaster/login5 through librespot
//! - search through `api-partner.spotify.com/pathfinder/v1/query`
//! - playback through Spotify Connect (librespot) and the local audio backend

mod client;
mod collection;
mod error;
pub(crate) mod home;
pub(crate) mod library;
pub(crate) mod library_catalog;
mod playlist;
mod search;
mod store;

pub use client::{LikedStatus, NativeSpotify, PlaybackSnapshot, RepeatMode, UserProfile};
pub use error::SpotifyError;
pub use playlist::{PlaylistDetail, PlaylistPage, PlaylistSource};
pub use search::{SearchItem, SearchResults};
