//! Standalone native collection membership and bounded batched catalog decoration.

use std::collections::{HashMap, HashSet};

use librespot::{
    core::Session,
    protocol::{
        extended_metadata::{BatchedEntityRequest, EntityRequest, ExtensionQuery},
        extension_kind::ExtensionKind,
        metadata,
    },
};
use protobuf::{EnumOrUnknown, Message};

use super::{
    collection,
    library::{LibraryEntry, LibraryKind, fetch_recents},
};

pub(crate) struct LibrarySupplement {
    pub entries: Vec<LibraryEntry>,
    pub loaded_categories: HashSet<LibraryKind>,
    pub pins: Option<HashSet<String>>,
    pub failures: usize,
    pub recents_available: bool,
}

impl LibrarySupplement {
    pub fn apply(self, entries: &mut Vec<LibraryEntry>) -> (HashSet<LibraryKind>, usize) {
        let mut existing: HashSet<_> = entries.iter().map(|e| e.uri.clone()).collect();
        entries.extend(
            self.entries
                .into_iter()
                .filter(|entry| existing.insert(entry.uri.clone())),
        );
        if let Some(pins) = self.pins {
            apply_pins(entries, &pins);
        }
        (self.loaded_categories, self.failures)
    }
}

pub(crate) fn apply_pins(entries: &mut [LibraryEntry], pins: &HashSet<String>) {
    for entry in entries {
        entry.pinned = Some(pins.contains(&entry.pin_uri()) || pins.contains(&entry.uri));
    }
}

pub(crate) async fn fetch(session: &Session) -> LibrarySupplement {
    let (albums, artists, shows, pins, recents) = futures::join!(
        collection::snapshot(session, "collection"),
        collection::snapshot(session, "artist"),
        collection::snapshot(session, "show"),
        collection::snapshot(session, "ylpin"),
        fetch_recents(session),
    );
    let mut failed_sources: Vec<&'static str> = Vec::new();
    let recents_available = recents.is_ok();
    if !recents_available {
        failed_sources.push("recents");
    }
    let recents = recents.unwrap_or_default();
    let mut result = LibrarySupplement {
        entries: Vec::new(),
        loaded_categories: HashSet::new(),
        pins: None,
        failures: usize::from(!recents_available),
        recents_available,
    };
    for (kind, records) in [
        (LibraryKind::Album, albums),
        (LibraryKind::Artist, artists),
        (LibraryKind::Show, shows),
    ] {
        match records {
            Ok(records) => {
                result.loaded_categories.insert(kind);
                let prefix = match kind {
                    LibraryKind::Album => "spotify:album:",
                    LibraryKind::Artist => "spotify:artist:",
                    _ => "spotify:show:",
                };
                let mut seen = HashSet::new();
                for item in records
                    .into_iter()
                    .filter(|item| !item.removed && item.uri.starts_with(prefix))
                {
                    if !seen.insert(item.uri.clone()) {
                        continue;
                    }
                    result.entries.push(LibraryEntry {
                        last_played_at: recents.get(&item.uri).copied(),
                        uri: item.uri,
                        name: kind.label().to_string(),
                        owner: String::new(),
                        owner_username: String::new(),
                        image_url: None,
                        covers: Vec::new(),
                        kind,
                        parent: None,
                        // Collection-v2 dates are Unix seconds; rootlist and recents use milliseconds.
                        added_at: (item.added_at > 0).then_some(item.added_at.saturating_mul(1000)),
                        pinned: None,
                        type_label: kind.label().to_string(),
                    });
                }
            }
            Err(_) => {
                result.failures += 1;
                failed_sources.push(kind.label());
            }
        }
    }
    match pins {
        Ok(pins) => {
            result.pins = Some(
                pins.into_iter()
                    .filter(|p| !p.removed)
                    .map(|p| p.uri)
                    .collect(),
            )
        }
        Err(_) => {
            result.failures += 1;
            failed_sources.push("pins");
        }
    }
    result.entries.sort_by(|a, b| {
        b.last_played_at
            .cmp(&a.last_played_at)
            .then_with(|| b.added_at.cmp(&a.added_at))
    });
    for chunk in result.entries.chunks_mut(64) {
        if !hydrate_batch(session, chunk).await {
            result.failures += 1;
            failed_sources.push("decoration");
        }
    }
    if diagnostic_logging() && result.failures > 0 {
        eprintln!(
            "library catalog: failures={} sources={failed_sources:?}",
            result.failures
        );
    }
    result
}

/// Opt-in failure-source logging. Emits counts and source names only, never titles or URIs.
pub(crate) fn diagnostic_logging() -> bool {
    std::env::var_os("RSPOTIFY_LIBRARY_DIAGNOSTICS").is_some()
}

async fn hydrate_batch(session: &Session, entries: &mut [LibraryEntry]) -> bool {
    let request = BatchedEntityRequest {
        entity_request: entries
            .iter()
            .map(|entry| EntityRequest {
                entity_uri: entry.uri.clone(),
                query: vec![ExtensionQuery {
                    extension_kind: EnumOrUnknown::new(match entry.kind {
                        LibraryKind::Album => ExtensionKind::ALBUM_V4,
                        LibraryKind::Artist => ExtensionKind::ARTIST_V4,
                        _ => ExtensionKind::SHOW_V4,
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let response = match tokio::time::timeout(
        std::time::Duration::from_secs(15),
        session.spclient().get_extended_metadata(request),
    )
    .await
    {
        Ok(Ok(response)) => response,
        _ => return false,
    };
    let mut decoded = HashMap::new();
    for array in response.extended_metadata {
        for data in array.extension_data {
            if data
                .header
                .as_ref()
                .is_some_and(|h| h.status_code != 0 && h.status_code != 200)
            {
                continue;
            }
            let Some(any) = data.extension_data.as_ref() else {
                continue;
            };
            let row = match array.extension_kind.enum_value().ok() {
                Some(ExtensionKind::ALBUM_V4) => metadata::Album::parse_from_bytes(&any.value)
                    .ok()
                    .map(|album| {
                        let kind = match album.type_() {
                            metadata::album::Type::SINGLE => "Single",
                            metadata::album::Type::EP => "EP",
                            metadata::album::Type::COMPILATION => "Compilation",
                            _ => "Album",
                        };
                        let upcoming = album
                            .date
                            .as_ref()
                            .and_then(|date| librespot::core::date::Date::try_from(date).ok())
                            .is_some_and(|date| date > librespot::core::date::Date::now_utc());
                        (
                            album.name().to_string(),
                            album
                                .artist
                                .iter()
                                .map(|a| a.name())
                                .filter(|s| !s.is_empty())
                                .collect::<Vec<_>>()
                                .join(", "),
                            album.cover_group.as_ref().and_then(image),
                            if upcoming {
                                "Upcoming album".to_string()
                            } else {
                                kind.to_string()
                            },
                        )
                    }),
                Some(ExtensionKind::ARTIST_V4) => metadata::Artist::parse_from_bytes(&any.value)
                    .ok()
                    .map(|artist| {
                        let portrait = artist
                            .portrait_group
                            .iter()
                            .find_map(image)
                            .or_else(|| artist.portrait.first().and_then(image_url));
                        (
                            artist.name().to_string(),
                            String::new(),
                            portrait,
                            "Artist".to_string(),
                        )
                    }),
                Some(ExtensionKind::SHOW_V4) => metadata::Show::parse_from_bytes(&any.value)
                    .ok()
                    .map(|show| {
                        (
                            show.name().to_string(),
                            show.publisher().to_string(),
                            show.cover_image.as_ref().and_then(image),
                            "Podcast".to_string(),
                        )
                    }),
                _ => None,
            };
            if let Some(row) = row.filter(|row| !row.0.is_empty()) {
                decoded.insert(data.entity_uri, row);
            }
        }
    }
    // A batch can legitimately omit region-restricted or deleted items. Only treat a batch as
    // failed when the service returned nothing usable for it, otherwise the panel would report a
    // metadata error for a single unavailable album.
    let decoded_any = !decoded.is_empty();
    for entry in entries {
        if let Some((name, owner, cover, label)) = decoded.remove(&entry.uri) {
            entry.name = name;
            entry.owner = owner;
            entry.image_url = cover;
            entry.type_label = label;
        }
    }
    decoded_any
}

fn image(group: &metadata::ImageGroup) -> Option<String> {
    group.image.iter().find_map(image_url)
}
fn image_url(image: &metadata::Image) -> Option<String> {
    let id = image.file_id.as_deref().filter(|id| id.len() == 20)?;
    Some(format!(
        "https://i.scdn.co/image/{}",
        id.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}
