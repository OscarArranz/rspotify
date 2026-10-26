//! Native rootlist library model. Presentation order is not listening recency.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use librespot::core::{Session, SpotifyUri};
use librespot::metadata::{Metadata, Track};
use librespot::protocol::playlist4_external::{ListAttributes, SelectedListContent};
use protobuf::Message;

use super::error::SpotifyError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LibraryKind {
    LikedSongs,
    Playlist,
    Folder,
    Album,
    Artist,
    Show,
}

impl LibraryKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::LikedSongs | Self::Playlist => "Playlist",
            Self::Folder => "Folder",
            Self::Album => "Album",
            Self::Artist => "Artist",
            Self::Show => "Podcast",
        }
    }
}

#[derive(Clone, Debug)]
pub struct LibraryEntry {
    pub uri: String,
    pub name: String,
    pub owner: String,
    pub owner_username: String,
    pub image_url: Option<String>,
    pub covers: Vec<String>,
    pub kind: LibraryKind,
    pub parent: Option<String>,
    pub added_at: Option<i64>,
    pub last_played_at: Option<i64>,
    pub pinned: Option<bool>,
    pub type_label: String,
}

impl LibraryEntry {
    pub fn liked_songs(username: &str) -> Self {
        Self {
            uri: format!("spotify:user:{username}:collection"),
            name: "Liked Songs".into(),
            owner: String::new(),
            owner_username: username.to_string(),
            image_url: None,
            covers: Vec::new(),
            kind: LibraryKind::LikedSongs,
            parent: None,
            added_at: None,
            last_played_at: None,
            pinned: None,
            type_label: "Playlist".into(),
        }
    }

    pub fn subtitle(&self) -> String {
        if self.owner.is_empty() || matches!(self.kind, LibraryKind::Folder | LibraryKind::Artist) {
            self.type_label.clone()
        } else {
            format!("{} • {}", self.type_label, self.owner)
        }
    }

    pub(crate) fn pin_priority(&self) -> bool {
        self.pinned.unwrap_or(self.kind == LibraryKind::LikedSongs)
    }

    pub(crate) fn pin_uri(&self) -> String {
        if self.kind == LibraryKind::LikedSongs {
            "spotify:collection:tracks".into()
        } else if self.kind == LibraryKind::Folder {
            self.uri
                .split(':')
                .nth(2)
                .map(|id| format!("spotify:folder:{id}"))
                .unwrap_or_else(|| self.uri.clone())
        } else {
            self.uri.clone()
        }
    }

    pub(crate) fn share_url(&self) -> String {
        if self.kind == LibraryKind::LikedSongs {
            return "https://open.spotify.com/collection/tracks".into();
        }
        if let Some((kind, id)) = self
            .uri
            .strip_prefix("spotify:")
            .and_then(|uri| uri.split_once(':'))
        {
            if matches!(kind, "playlist" | "album" | "artist" | "show") {
                return format!("https://open.spotify.com/{kind}/{id}");
            }
        }
        self.uri.clone()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LibrarySort {
    #[default]
    Custom,
    Recents,
    RecentlyAdded,
    Alphabetical,
    Creator,
}

impl LibrarySort {
    pub const ALL: [Self; 5] = [
        Self::Recents,
        Self::Custom,
        Self::RecentlyAdded,
        Self::Alphabetical,
        Self::Creator,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Custom => "Custom order",
            Self::Recents => "Recents",
            Self::RecentlyAdded => "Recently added",
            Self::Alphabetical => "Alphabetical",
            Self::Creator => "Creator",
        }
    }
}

/// Folders are retained in Spotify's flat start/end-marker order, including across pages.
#[derive(Default)]
pub(crate) struct RootlistDecoder {
    pub entries: Vec<LibraryEntry>,
    folders: Vec<String>,
    seen: HashSet<String>,
}

impl RootlistDecoder {
    pub fn append(&mut self, list: &SelectedListContent) {
        let Some(contents) = list.contents.as_ref() else {
            return;
        };
        for (index, item) in contents.items.iter().enumerate() {
            let uri = item.uri();
            if uri.starts_with("spotify:end-group:") {
                self.folders.pop();
                continue;
            }
            let kind = if uri.starts_with("spotify:start-group:") {
                LibraryKind::Folder
            } else if uri.starts_with("spotify:playlist:") {
                LibraryKind::Playlist
            } else {
                continue;
            };
            let meta = contents.meta_items.get(index);
            let attributes = meta.and_then(|meta| meta.attributes.as_ref());
            if self.seen.insert(uri.to_string()) {
                self.entries.push(LibraryEntry {
                    uri: uri.to_string(),
                    name: attributes
                        .and_then(|a| a.name.clone())
                        .filter(|s| !s.is_empty())
                        .or_else(|| folder_name(uri))
                        .unwrap_or_else(|| {
                            if kind == LibraryKind::Folder {
                                "Folder"
                            } else {
                                "Playlist"
                            }
                            .into()
                        }),
                    owner: meta
                        .and_then(|m| m.owner_username.clone())
                        .unwrap_or_default(),
                    owner_username: meta
                        .and_then(|m| m.owner_username.clone())
                        .unwrap_or_default(),
                    image_url: attributes.and_then(attribute_image),
                    covers: Vec::new(),
                    kind,
                    parent: self.folders.last().cloned(),
                    added_at: item
                        .attributes
                        .as_ref()
                        .and_then(|a| a.timestamp)
                        .filter(|t| *t > 0),
                    last_played_at: None,
                    pinned: None,
                    type_label: kind.label().to_string(),
                });
            }
            if kind == LibraryKind::Folder {
                self.folders.push(uri.to_string());
            }
        }
    }
}

fn folder_name(uri: &str) -> Option<String> {
    // The final component is a URI-escaped label, not another Spotify identity.
    let name = uri.strip_prefix("spotify:start-group:")?.split_once(':')?.1;
    let mut bytes = Vec::new();
    let mut chars = name.as_bytes().iter().copied();
    while let Some(byte) = chars.next() {
        if byte == b'%' {
            let high = (chars.next()? as char).to_digit(16)?;
            let low = (chars.next()? as char).to_digit(16)?;
            bytes.push((high * 16 + low) as u8);
        } else if byte == b'+' {
            bytes.push(b' ');
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8(bytes).ok().filter(|s| !s.is_empty())
}

fn attribute_image(attributes: &ListAttributes) -> Option<String> {
    attributes
        .picture_size
        .iter()
        .filter_map(|picture| {
            let url = normalize_image(picture.url.as_deref()?)?;
            Some((picture.target_name().parse::<u32>().unwrap_or(0), url))
        })
        .max_by_key(|(size, _)| *size)
        .map(|(_, url)| url)
        .or_else(|| {
            attributes
                .picture
                .as_deref()
                .filter(|p| p.len() == 20)
                .map(image_id)
        })
}

fn normalize_image(url: &str) -> Option<String> {
    match url.strip_prefix("spotify:image:") {
        Some(id) if !id.is_empty() => Some(format!("https://i.scdn.co/image/{id}")),
        None if url.len() == 40 && url.bytes().all(|byte| byte.is_ascii_hexdigit()) => {
            Some(format!("https://i.scdn.co/image/{url}"))
        }
        None if url.starts_with("https://") || url.starts_with("http://") => Some(url.to_string()),
        _ => None,
    }
}

fn image_id(bytes: &[u8]) -> String {
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("https://i.scdn.co/image/{hex}")
}

pub(crate) async fn fetch(session: &Session) -> Result<Vec<LibraryEntry>, SpotifyError> {
    let mut decoder = RootlistDecoder::default();
    let mut from = 0;
    let mut revision = None;
    let mut previous_page = Vec::new();
    loop {
        let bytes = tokio::time::timeout(
            Duration::from_secs(20),
            session.spclient().get_rootlist(from, Some(100)),
        )
        .await
        .map_err(|_| SpotifyError::Api("Spotify library request timed out".into()))?
        .map_err(|error| SpotifyError::Api(error.to_string()))?;
        let page = SelectedListContent::parse_from_bytes(&bytes)
            .map_err(|error| SpotifyError::Api(error.to_string()))?;
        let count = page.contents.as_ref().map_or(0, |c| c.items.len());
        if count == 0 {
            break;
        }
        if let Some(current) = page.revision.as_ref() {
            if revision
                .as_ref()
                .is_some_and(|expected| expected != current)
            {
                return Err(SpotifyError::Api(
                    "Your Library changed while loading. Please refresh.".into(),
                ));
            }
            revision = Some(current.clone());
        }
        let page_uris: Vec<_> = page
            .contents
            .as_ref()
            .unwrap()
            .items
            .iter()
            .map(|item| item.uri().to_owned())
            .collect();
        if page_uris == previous_page {
            return Err(SpotifyError::Api(
                "Spotify returned a repeated library page. Please refresh.".into(),
            ));
        }
        previous_page = page_uris;
        decoder.append(&page);
        from += count;
        if page
            .length
            .is_some_and(|length| from >= length.max(0) as usize)
        {
            break;
        }
    }
    let mut entries = vec![LibraryEntry::liked_songs(&session.username())];
    entries.extend(decoder.entries);
    // This native endpoint returns protobuf even when Accept: application/json is sent.
    if let Ok(recents) = fetch_recents(session).await {
        for entry in &mut entries {
            entry.last_played_at = recents.get(&entry.uri).copied();
        }
    }
    Ok(entries)
}

/// Wire layout from Spotify's shipped recently_played_backend.proto (not built by librespot).
pub(crate) fn parse_recents(bytes: &[u8]) -> Result<HashMap<String, i64>, SpotifyError> {
    fn decode(bytes: &[u8]) -> protobuf::Result<HashMap<String, i64>> {
        let mut input = protobuf::CodedInputStream::from_bytes(bytes);
        let mut unknown = protobuf::UnknownFields::new();
        let mut recents = HashMap::<String, i64>::new();
        while let Some(tag) = input.read_raw_tag_or_eof()? {
            if tag != 10 {
                protobuf::rt::read_unknown_or_skip_group(tag, &mut input, &mut unknown)?;
                continue;
            }
            let context = input.read_bytes()?;
            let mut context = protobuf::CodedInputStream::from_bytes(&context);
            let mut fields = protobuf::UnknownFields::new();
            let mut uri = String::new();
            let mut timestamp = 0;
            while let Some(tag) = context.read_raw_tag_or_eof()? {
                match tag {
                    10 => uri = context.read_string()?,
                    16 => timestamp = context.read_int64()?,
                    _ => protobuf::rt::read_unknown_or_skip_group(tag, &mut context, &mut fields)?,
                }
            }
            if uri.starts_with("spotify:") && timestamp > 0 {
                recents
                    .entry(uri)
                    .and_modify(|previous| *previous = (*previous).max(timestamp))
                    .or_insert(timestamp);
            }
        }
        Ok(recents)
    }
    decode(bytes)
        .map_err(|error| SpotifyError::Api(format!("Invalid native recents response: {error}")))
}

pub(crate) async fn fetch_recents(session: &Session) -> Result<HashMap<String, i64>, SpotifyError> {
    let bytes = tokio::time::timeout(
        Duration::from_secs(8),
        session.spclient().request(
            &reqwest::Method::GET,
            "/recently-played/v3/recently-played?limit=50&filter=default,collection-new-episodes",
            None,
            None,
        ),
    )
    .await
    .map_err(|_| SpotifyError::Api("Native recents timed out".into()))?
    .map_err(|error| SpotifyError::Api(error.to_string()))?;
    parse_recents(&bytes)
}

/// Per-refresh cache; display names never replace raw owner identifiers in the rootlist.
#[derive(Default)]
pub(crate) struct LibraryHydrator {
    profiles: HashMap<String, String>,
}

/// Enrich a bounded batch on a worker. Errors retain usable original metadata.
pub(crate) async fn hydrate(
    session: &Session,
    entries: &mut [LibraryEntry],
    cache: &mut LibraryHydrator,
) -> usize {
    let mut failures = 0;
    let mut cover_failures = 0;
    let mut profile_failures = 0;
    for entry in entries.iter_mut() {
        if !matches!(entry.kind, LibraryKind::Playlist | LibraryKind::LikedSongs) {
            continue;
        }
        if entry.kind == LibraryKind::LikedSongs {
            entry.owner = session.username();
        }
        if entry.kind == LibraryKind::Playlist
            && (entry.image_url.is_none() || entry.owner.is_empty())
        {
            let result =
                tokio::time::timeout(Duration::from_secs(15), hydrate_cover(session, entry)).await;
            if !matches!(result, Ok(Ok(()))) {
                failures += 1;
                cover_failures += 1;
                if super::library_catalog::diagnostic_logging() {
                    // Class only: never print the playlist identity or the response body.
                    match &result {
                        Err(_) => eprintln!("library hydrate: cover timeout"),
                        Ok(Err(error)) => {
                            let text = error.to_string();
                            let class = if text.contains("404") {
                                "not-found"
                            } else if text.contains("403") {
                                "forbidden"
                            } else if text.contains("429") {
                                "rate-limited"
                            } else {
                                "api"
                            };
                            eprintln!("library hydrate: cover failure class={class}");
                        }
                        Ok(Ok(())) => {}
                    }
                }
            }
        }
        if !entry.owner.is_empty() {
            let username = entry.owner.clone();
            if let Some(name) = cache.profiles.get(&username) {
                entry.owner = name.clone();
            } else {
                let result = tokio::time::timeout(
                    Duration::from_secs(8),
                    session.spclient().get_user_profile(&username, None, None),
                )
                .await;
                let mut resolved = false;
                if let Ok(Ok(bytes)) = result {
                    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                        if let Some(name) = value
                            .get("name")
                            .and_then(|n| n.as_str())
                            .filter(|s| !s.is_empty())
                        {
                            entry.owner = name.to_string();
                            resolved = true;
                        }
                    }
                }
                if !resolved {
                    failures += 1;
                    profile_failures += 1;
                }
                cache.profiles.insert(username, entry.owner.clone());
            }
        }
    }
    if failures > 0 && super::library_catalog::diagnostic_logging() {
        eprintln!(
            "library hydrate: entries={} failures={failures} (cover={cover_failures} profile={profile_failures})",
            entries.len()
        );
    }
    failures
}

async fn hydrate_cover(session: &Session, entry: &mut LibraryEntry) -> Result<(), SpotifyError> {
    let uri = SpotifyUri::from_uri(&entry.uri).map_err(|e| SpotifyError::Api(e.to_string()))?;
    let SpotifyUri::Playlist { id, .. } = uri else {
        return Ok(());
    };
    let bytes = session
        .spclient()
        .get_playlist(&id)
        .await
        .map_err(|e| SpotifyError::Api(e.to_string()))?;
    let playlist = SelectedListContent::parse_from_bytes(&bytes)
        .map_err(|e| SpotifyError::Api(e.to_string()))?;
    if entry.owner.is_empty() {
        entry.owner = playlist.owner_username().to_string();
        entry.owner_username = entry.owner.clone();
    }
    if entry.name == "Playlist" {
        if let Some(name) = playlist
            .attributes
            .as_ref()
            .and_then(|a| a.name.as_ref())
            .filter(|s| !s.is_empty())
        {
            entry.name = name.clone();
        }
    }
    if entry.image_url.is_none() {
        entry.image_url = playlist.attributes.as_ref().and_then(attribute_image);
    }
    if entry.image_url.is_some() {
        return Ok(());
    }
    if let Some(contents) = playlist.contents.as_ref() {
        // Only inspect the first eight tracks; never fetch a whole playlist to build a thumbnail.
        for item in contents.items.iter().take(8) {
            let Ok(uri) = SpotifyUri::from_uri(item.uri()) else {
                continue;
            };
            if !matches!(uri, SpotifyUri::Track { .. }) {
                continue;
            }
            if let Ok(track) = Track::get(session, &uri).await {
                if let Some(image) = track.album.covers.first() {
                    let url = format!("https://i.scdn.co/image/{}", image.id);
                    if !entry.covers.contains(&url) {
                        entry.covers.push(url);
                    }
                }
            }
            if entry.covers.len() == 4 {
                break;
            }
        }
    }
    Ok(())
}

pub fn visible_entries(
    entries: &[LibraryEntry],
    query: &str,
    folder: Option<&str>,
    sort: LibrarySort,
) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    let mut visible: Vec<_> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            let matches_query = query.is_empty()
                || format!("{} {}", entry.name, entry.owner)
                    .to_lowercase()
                    .contains(&query);
            let matches_folder = if query.is_empty() {
                entry.parent.as_deref() == folder
            } else {
                true
            };
            matches_query && matches_folder
        })
        .map(|(index, _)| index)
        .collect();
    visible.sort_by(|&a, &b| {
        let a = &entries[a];
        let b = &entries[b];
        // Liked Songs is a special collection, not fabricated account pin state.
        (!a.pin_priority())
            .cmp(&(!b.pin_priority()))
            .then_with(|| match sort {
                LibrarySort::Custom => std::cmp::Ordering::Equal,
                LibrarySort::Recents => b
                    .last_played_at
                    .max(b.added_at)
                    .cmp(&a.last_played_at.max(a.added_at)),
                LibrarySort::RecentlyAdded => b.added_at.cmp(&a.added_at),
                LibrarySort::Alphabetical => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                LibrarySort::Creator => a.owner.to_lowercase().cmp(&b.owner.to_lowercase()),
            })
    });
    visible
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_folder_names_decode_plus_and_percent_escaping() {
        assert_eq!(
            folder_name("spotify:start-group:000000ab:Super+Mario+Bros"),
            Some("Super Mario Bros".into())
        );
        assert_eq!(
            folder_name("spotify:start-group:000000ab:A%2BB"),
            Some("A+B".into())
        );
    }

    #[test]
    fn recents_wire_decoder_keeps_latest_timestamp_and_ignores_future_fields() {
        let mut response = Vec::new();
        {
            let mut outer = protobuf::CodedOutputStream::vec(&mut response);
            for timestamp in [100_i64, 300, 200] {
                let mut item = Vec::new();
                {
                    let mut inner = protobuf::CodedOutputStream::vec(&mut item);
                    inner.write_string(1, "spotify:playlist:fixture").unwrap();
                    inner.write_int64(2, timestamp).unwrap();
                    inner.write_bool(20, true).unwrap();
                    inner.flush().unwrap();
                }
                outer.write_bytes(1, &item).unwrap();
            }
            outer.write_int32(3, 3).unwrap();
            outer.flush().unwrap();
        }
        let recents = parse_recents(&response).unwrap();
        assert_eq!(recents.get("spotify:playlist:fixture"), Some(&300));
        assert!(parse_recents(&[0xff]).is_err());
    }

    #[test]
    fn real_pin_state_overrides_the_initial_collection_fallback() {
        let mut decoder = RootlistDecoder::default();
        decoder.append(&page(&["spotify:playlist:a", "spotify:playlist:b"]));
        let mut entries = decoder.entries;
        entries.push(LibraryEntry::liked_songs("fixture-account"));
        for entry in &mut entries {
            entry.pinned = Some(false);
        }
        entries[1].pinned = Some(true);
        entries[0].last_played_at = Some(200);
        entries[2].last_played_at = Some(100);
        assert_eq!(
            visible_entries(&entries, "", None, LibrarySort::Recents),
            vec![1, 0, 2]
        );
        assert_eq!(entries[2].pin_uri(), "spotify:collection:tracks");
    }
    use librespot::protocol::playlist4_external::{Item, ItemAttributes, ListItems, MetaItem};

    fn page(uris: &[&str]) -> SelectedListContent {
        SelectedListContent {
            contents: Some(ListItems {
                items: uris
                    .iter()
                    .map(|uri| Item {
                        uri: Some((*uri).into()),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        }
    }

    #[test]
    fn folders_span_pages_and_do_not_corrupt_parent_order() {
        let mut decoder = RootlistDecoder::default();
        decoder.append(&page(&[
            "spotify:start-group:one:My%20Music",
            "spotify:playlist:first",
            "spotify:start-group:two:Nested",
        ]));
        decoder.append(&page(&[
            "spotify:playlist:second",
            "spotify:end-group:two",
            "spotify:end-group:one",
            "spotify:playlist:third",
            "spotify:playlist:third",
        ]));
        assert_eq!(decoder.entries.len(), 5);
        assert_eq!(decoder.entries[0].name, "My Music");
        assert_eq!(
            decoder.entries[1].parent.as_deref(),
            Some("spotify:start-group:one:My%20Music")
        );
        assert_eq!(
            decoder.entries[3].parent.as_deref(),
            Some("spotify:start-group:two:Nested")
        );
        assert_eq!(decoder.entries[4].parent, None);
    }

    #[test]
    fn decorated_owner_picture_and_added_timestamp_are_preserved() {
        let mut p = page(&["spotify:playlist:first"]);
        let contents = p.contents.as_mut().unwrap();
        contents.items[0].attributes = Some(ItemAttributes {
            timestamp: Some(123),
            ..Default::default()
        })
        .into();
        contents.meta_items.push(MetaItem {
            owner_username: Some("fixture-owner".into()),
            attributes: Some(ListAttributes {
                name: Some("Fixture mix".into()),
                picture: Some(vec![0xab; 20]),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        });
        let mut decoder = RootlistDecoder::default();
        decoder.append(&p);
        let entry = &decoder.entries[0];
        assert_eq!(entry.owner, "fixture-owner");
        assert_eq!(entry.added_at, Some(123));
        assert_eq!(
            entry.image_url.as_deref(),
            Some("https://i.scdn.co/image/abababababababababababababababababababab")
        );
    }

    #[test]
    fn search_sort_and_folder_filter_are_stable() {
        let mut decoder = RootlistDecoder::default();
        decoder.append(&page(&[
            "spotify:playlist:a",
            "spotify:start-group:x:Folder",
            "spotify:playlist:b",
            "spotify:end-group:x",
            "spotify:playlist:c",
        ]));
        decoder.entries[0].name = "Zulu".into();
        decoder.entries[2].name = "EXAMPLE MIX".into();
        decoder.entries[2].owner = "Fixture".into();
        decoder.entries[3].name = "Alpha".into();
        let e = &decoder.entries;
        assert_eq!(
            visible_entries(e, "fixture", None, LibrarySort::Custom),
            vec![2]
        );
        assert_eq!(
            visible_entries(e, "example mix", None, LibrarySort::Custom),
            vec![2]
        );
        assert_eq!(
            visible_entries(e, "", None, LibrarySort::Alphabetical),
            vec![3, 1, 0]
        );
        assert_eq!(
            visible_entries(
                e,
                "",
                Some("spotify:start-group:x:Folder"),
                LibrarySort::Custom
            ),
            vec![2]
        );
        assert_eq!(
            visible_entries(e, "", None, LibrarySort::Custom),
            vec![0, 1, 3]
        );
    }

    #[test]
    fn missing_metadata_and_invalid_pictures_are_safe() {
        let mut decoder = RootlistDecoder::default();
        decoder.append(&SelectedListContent::default());
        decoder.append(&page(&[
            "spotify:unknown:a",
            "spotify:end-group:unknown",
            "spotify:playlist:a",
        ]));
        assert_eq!(decoder.entries.len(), 1);
        assert_eq!(decoder.entries[0].name, "Playlist");
        assert!(
            attribute_image(&ListAttributes {
                picture: Some(vec![1; 5]),
                ..Default::default()
            })
            .is_none()
        );
        assert_eq!(folder_name("spotify:start-group:x:Bad%zz"), None);
    }

    #[test]
    fn artwork_uses_largest_valid_remote_image_and_rejects_local_paths() {
        use librespot::protocol::playlist4_external::PictureSize;
        let attributes = ListAttributes {
            picture_size: vec![
                PictureSize {
                    target_name: Some("64".into()),
                    url: Some("https://i.scdn.co/image/small".into()),
                    ..Default::default()
                },
                PictureSize {
                    target_name: Some("640".into()),
                    url: Some("spotify:image:large".into()),
                    ..Default::default()
                },
                PictureSize {
                    target_name: Some("999".into()),
                    url: Some("file:///not-an-image".into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            attribute_image(&attributes).as_deref(),
            Some("https://i.scdn.co/image/large")
        );
        assert_eq!(normalize_image(""), None);
        assert_eq!(normalize_image("spotify:image:"), None);
        assert_eq!(normalize_image("/not-an-image"), None);
        assert_eq!(
            normalize_image(&"ab".repeat(20)),
            Some(format!("https://i.scdn.co/image/{}", "ab".repeat(20)))
        );
    }

    #[test]
    fn added_and_creator_sort_keep_unknown_dates_last_and_ties_stable() {
        let mut decoder = RootlistDecoder::default();
        decoder.append(&page(&[
            "spotify:playlist:a",
            "spotify:playlist:b",
            "spotify:playlist:c",
        ]));
        decoder.entries[0].owner = "Zulu".into();
        decoder.entries[1].owner = "Alpha".into();
        decoder.entries[2].owner = "Alpha".into();
        decoder.entries[0].added_at = Some(10);
        decoder.entries[2].added_at = Some(20);
        assert_eq!(
            visible_entries(&decoder.entries, "", None, LibrarySort::RecentlyAdded),
            vec![2, 0, 1]
        );
        assert_eq!(
            visible_entries(&decoder.entries, "", None, LibrarySort::Creator),
            vec![1, 2, 0]
        );
        assert_eq!(
            visible_entries(&decoder.entries, "  alpha  ", None, LibrarySort::Custom),
            vec![1, 2]
        );
    }

    #[test]
    fn liked_songs_is_in_the_same_list_and_filtered_normally() {
        let mut decoder = RootlistDecoder::default();
        decoder.append(&page(&["spotify:playlist:a"]));
        decoder
            .entries
            .push(LibraryEntry::liked_songs("fixture-account"));
        assert_eq!(
            visible_entries(&decoder.entries, "", None, LibrarySort::Custom),
            vec![1, 0]
        );
        assert_eq!(
            visible_entries(&decoder.entries, "liked", None, LibrarySort::Custom),
            vec![1]
        );
        assert_eq!(decoder.entries[1].subtitle(), "Playlist");
        assert_eq!(
            decoder.entries[1].uri,
            "spotify:user:fixture-account:collection"
        );
    }
}
