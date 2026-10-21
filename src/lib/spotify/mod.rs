//! Native Spotify integration.
//!
//! This module talks to the same services as Spotify's own desktop client:
//! - OAuth login at `accounts.spotify.com` with the desktop client id
//! - session/keymaster/login5 through librespot
//! - search through `api-partner.spotify.com/pathfinder/v1/query`
//! - playback through Spotify Connect (librespot) and the local audio backend

mod client;
mod error;
mod search;
mod store;

pub use client::{LibraryPlaylist, NativeSpotify, PlaybackSnapshot};
pub use search::{SearchItem, SearchResults};
