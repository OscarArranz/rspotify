//! Worker-only native track actions. Transport evidence is recorded in the spec 016 ledger.

use librespot::core::{Session, SpotifyId, SpotifyUri};
use librespot::protocol::{
    extension_kind::ExtensionKind,
    metadata::Track,
    playlist4_external::{self as wire, SelectedListContent},
};
use protobuf::{EnumOrUnknown, Message, MessageField};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ArtistCredit, PlaylistSource, SpotifyError};

#[derive(Clone, Debug)]
pub struct TrackMenuInfo {
    pub artists: Vec<ArtistCredit>,
}

#[derive(Clone, Debug)]
pub struct EditablePlaylist {
    pub uri: String,
    pub name: String,
}

/// An immutable native occurrence, not a URI-wide removal key.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaylistOccurrence {
    pub playlist_uri: String,
    pub track_uri: String,
    pub item_id: Vec<u8>,
    pub revision: Vec<u8>,
    pub position: i32,
}

#[derive(Clone, Debug)]
pub struct TrackCredits {
    pub title: String,
    pub groups: Vec<CreditGroup>,
    pub source_names: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CreditGroup {
    pub role: String,
    pub names: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SleepTimerChoice {
    #[default]
    Off,
    Minutes(u16),
    EndOfTrack,
}

fn api_error(error: impl std::fmt::Display) -> SpotifyError {
    SpotifyError::Api(error.to_string())
}

fn track_uri(uri: &str) -> Result<SpotifyUri, SpotifyError> {
    let parsed = SpotifyUri::from_uri(uri).map_err(api_error)?;
    if !matches!(parsed, SpotifyUri::Track { .. }) {
        return Err(api_error("This action requires a Spotify song"));
    }
    Ok(parsed)
}

pub(crate) async fn menu_info(session: &Session, uri: &str) -> Result<TrackMenuInfo, SpotifyError> {
    let parsed = track_uri(uri)?;
    let bytes = session
        .spclient()
        .get_metadata(ExtensionKind::TRACK_V4, &parsed)
        .await
        .map_err(api_error)?;
    let track = Track::parse_from_bytes(&bytes).map_err(api_error)?;
    let artists = track
        .artist
        .iter()
        .filter_map(|artist| {
            let id = SpotifyId::from_raw(artist.gid()).ok()?.to_base62().ok()?;
            Some(ArtistCredit {
                name: artist.name().to_owned(),
                uri: format!("spotify:artist:{id}"),
            })
        })
        .collect();
    Ok(TrackMenuInfo { artists })
}

pub(crate) async fn credits(session: &Session, uri: &str) -> Result<TrackCredits, SpotifyError> {
    let SpotifyUri::Track { id } = track_uri(uri)? else {
        unreachable!()
    };
    let endpoint = format!(
        "/track-credits-view/v0/experimental/{}/credits",
        id.to_base62().map_err(api_error)?
    );
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    let bytes = session
        .spclient()
        .request(&reqwest::Method::GET, &endpoint, Some(headers), None)
        .await
        .map_err(api_error)?;
    decode_credits(&serde_json::from_slice(&bytes).map_err(api_error)?)
}

fn decode_credits(value: &Value) -> Result<TrackCredits, SpotifyError> {
    let title = value
        .get("trackTitle")
        .and_then(Value::as_str)
        .ok_or_else(|| api_error("Invalid native song credits title"))?;
    let roles = value
        .get("roleCredits")
        .and_then(Value::as_array)
        .ok_or_else(|| api_error("Invalid native song credits roles"))?;
    let mut groups = Vec::new();
    for group in roles.iter().chain(
        value
            .get("extendedCredits")
            .and_then(Value::as_array)
            .into_iter()
            .flatten(),
    ) {
        let role = group
            .get("roleTitle")
            .and_then(Value::as_str)
            .ok_or_else(|| api_error("Invalid song credit role"))?;
        let artists = group
            .get("artists")
            .and_then(Value::as_array)
            .ok_or_else(|| api_error("Invalid song credit contributors"))?;
        let names = artists
            .iter()
            .map(|artist| {
                artist
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| api_error("Invalid song credit contributor name"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        groups.push(CreditGroup {
            role: role.to_owned(),
            names,
        });
    }
    let source_names = value
        .get("sourceNames")
        .and_then(Value::as_array)
        .ok_or_else(|| api_error("Invalid native song credits sources"))?
        .iter()
        .map(|source| {
            source
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| api_error("Invalid song credits source"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(TrackCredits {
        title: title.to_owned(),
        groups,
        source_names,
    })
}

pub(crate) async fn radio(session: &Session, uri: &str) -> Result<PlaylistSource, SpotifyError> {
    let bytes = session
        .spclient()
        .get_radio_for_track(&track_uri(uri)?)
        .await
        .map_err(api_error)?;
    let value: Value = serde_json::from_slice(&bytes).map_err(api_error)?;
    radio_destination(&value)
}

fn radio_destination(value: &Value) -> Result<PlaylistSource, SpotifyError> {
    let destination = value
        .get("mediaItems")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("uri"))
        .and_then(Value::as_str)
        .ok_or_else(|| api_error("Song radio returned no destination"))?;
    let parsed = SpotifyUri::from_uri(destination).map_err(api_error)?;
    if !matches!(parsed, SpotifyUri::Playlist { .. }) {
        return Err(api_error("Song radio returned an unsupported destination"));
    }
    Ok(PlaylistSource::Playlist {
        uri: destination.to_owned(),
        name: "Song Radio".into(),
        owner: String::new(),
        image_url: None,
    })
}

pub(crate) async fn playlist_content(
    session: &Session,
    uri: &str,
) -> Result<SelectedListContent, SpotifyError> {
    let SpotifyUri::Playlist { id, .. } = SpotifyUri::from_uri(uri).map_err(api_error)? else {
        return Err(api_error("This action requires a genuine Spotify playlist"));
    };
    let bytes = session
        .spclient()
        .get_playlist(&id)
        .await
        .map_err(api_error)?;
    SelectedListContent::parse_from_bytes(&bytes).map_err(api_error)
}

/// Denied/deleted library entries cannot be targets; transport failures still reach the UI.
pub(crate) async fn editable_content(
    session: &Session,
    uri: &str,
) -> Result<Option<SelectedListContent>, SpotifyError> {
    let SpotifyUri::Playlist { id, .. } = SpotifyUri::from_uri(uri).map_err(api_error)? else {
        return Ok(None);
    };
    match session.spclient().get_playlist(&id).await {
        Ok(bytes) => SelectedListContent::parse_from_bytes(&bytes)
            .map(Some)
            .map_err(api_error),
        Err(error)
            if matches!(
                error.kind,
                librespot::core::error::ErrorKind::PermissionDenied
                    | librespot::core::error::ErrorKind::NotFound
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(api_error(error)),
    }
}

fn require_editable(content: &SelectedListContent) -> Result<(), SpotifyError> {
    if !content
        .capabilities
        .as_ref()
        .is_some_and(|capabilities| capabilities.can_edit_items())
    {
        return Err(api_error(
            "You do not have permission to edit this playlist",
        ));
    }
    if content.revision().is_empty() || content.multiple_heads() || content.changes_require_resync()
    {
        return Err(api_error(
            "Playlist revision is not synchronized; reload it before editing",
        ));
    }
    Ok(())
}

pub(crate) fn occurrences(uri: &str, content: &SelectedListContent) -> Vec<PlaylistOccurrence> {
    if require_editable(content).is_err() {
        return Vec::new();
    }
    let Some(contents) = content.contents.as_ref() else {
        return Vec::new();
    };
    contents
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let item_id = item.attributes.as_ref()?.item_id();
            if item_id.is_empty() {
                return None;
            }
            Some(PlaylistOccurrence {
                playlist_uri: uri.to_owned(),
                track_uri: item.uri().to_owned(),
                item_id: item_id.to_vec(),
                revision: content.revision().to_vec(),
                position: contents.pos().checked_add(i32::try_from(index).ok()?)?,
            })
        })
        .collect()
}

fn changes(content: &SelectedListContent, op: wire::Op) -> Result<wire::ListChanges, SpotifyError> {
    require_editable(content)?;
    Ok(wire::ListChanges {
        base_revision: Some(content.revision().to_vec()),
        deltas: vec![wire::Delta {
            ops: vec![op],
            ..Default::default()
        }],
        want_resulting_revisions: Some(true),
        want_sync_result: Some(true),
        ..Default::default()
    })
}

pub(crate) fn append_changes(
    content: &SelectedListContent,
    uri: &str,
) -> Result<wire::ListChanges, SpotifyError> {
    track_uri(uri)?;
    changes(
        content,
        wire::Op {
            kind: Some(EnumOrUnknown::new(wire::op::Kind::ADD)),
            add: MessageField::some(wire::Add {
                add_last: Some(true),
                items: vec![wire::Item {
                    uri: Some(uri.to_owned()),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        },
    )
}

pub(crate) fn removal_changes(
    content: &SelectedListContent,
    occurrence: &PlaylistOccurrence,
) -> Result<wire::ListChanges, SpotifyError> {
    require_editable(content)?;
    if content.revision() != occurrence.revision {
        return Err(api_error(
            "Playlist changed since this occurrence was loaded; reload before removing it",
        ));
    }
    let contents = content
        .contents
        .as_ref()
        .ok_or_else(|| api_error("Playlist items are unavailable"))?;
    let index = occurrence
        .position
        .checked_sub(contents.pos())
        .and_then(|index| usize::try_from(index).ok())
        .ok_or_else(|| api_error("Playlist occurrence position is unavailable"))?;
    let item = contents
        .items
        .get(index)
        .ok_or_else(|| api_error("Playlist occurrence is no longer available"))?;
    if occurrence.item_id.is_empty()
        || item.uri() != occurrence.track_uri
        || item
            .attributes
            .as_ref()
            .is_none_or(|attributes| attributes.item_id() != occurrence.item_id)
    {
        return Err(api_error(
            "Playlist occurrence identity changed; reload before removing it",
        ));
    }
    changes(
        content,
        wire::Op {
            kind: Some(EnumOrUnknown::new(wire::op::Kind::REM)),
            rem: MessageField::some(wire::Rem {
                from_index: Some(occurrence.position),
                length: Some(1),
                ..Default::default()
            }),
            ..Default::default()
        },
    )
}

pub(crate) async fn apply_changes(
    session: &Session,
    uri: &str,
    changes: &wire::ListChanges,
) -> Result<Vec<u8>, SpotifyError> {
    let SpotifyUri::Playlist { id, .. } = SpotifyUri::from_uri(uri).map_err(api_error)? else {
        return Err(api_error("Invalid playlist URI"));
    };
    let endpoint = format!(
        "/playlist/v2/playlist/{}/changes",
        id.to_base62().map_err(api_error)?
    );
    let bytes = session
        .spclient()
        .request_with_protobuf(&reqwest::Method::POST, &endpoint, None, changes)
        .await
        .map_err(api_error)?;
    let response = SelectedListContent::parse_from_bytes(&bytes).map_err(api_error)?;
    acknowledged_revision(response)
}

fn acknowledged_revision(response: SelectedListContent) -> Result<Vec<u8>, SpotifyError> {
    if response.multiple_heads()
        || response.changes_require_resync()
        || response.resulting_revisions.len() != 1
        || response.resulting_revisions[0].is_empty()
    {
        return Err(api_error(
            "Playlist edit was not acknowledged with a consistent resulting revision; reload the playlist",
        ));
    }
    Ok(response.resulting_revisions.into_iter().next().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn failed_or_ambiguous_edits_never_report_success() {
        assert!(acknowledged_revision(SelectedListContent::default()).is_err());
        let mut response = SelectedListContent {
            resulting_revisions: vec![vec![3]],
            ..Default::default()
        };
        assert_eq!(acknowledged_revision(response.clone()).unwrap(), vec![3]);
        response.changes_require_resync = Some(true);
        assert!(acknowledged_revision(response).is_err());
        let response = SelectedListContent {
            resulting_revisions: vec![vec![3], vec![4]],
            ..Default::default()
        };
        assert!(acknowledged_revision(response).is_err());
    }

    fn editable() -> SelectedListContent {
        SelectedListContent {
            revision: Some(vec![1, 2]),
            capabilities: MessageField::some(
                librespot::protocol::playlist_permission::Capabilities {
                    can_edit_items: Some(true),
                    ..Default::default()
                },
            ),
            contents: MessageField::some(wire::ListItems {
                items: vec![1, 2]
                    .into_iter()
                    .map(|id| wire::Item {
                        uri: Some("spotify:track:duplicate".into()),
                        attributes: MessageField::some(wire::ItemAttributes {
                            item_id: Some(vec![id]),
                            ..Default::default()
                        }),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn duplicate_removal_requires_exact_item_and_revision() {
        let mut content = editable();
        let items = occurrences("spotify:playlist:test", &content);
        let second = &items[1];
        let request = removal_changes(&content, second).unwrap();
        let rem = request.deltas[0].ops[0].rem.as_ref().unwrap();
        assert_eq!(rem.from_index(), 1);
        assert_eq!(rem.length(), 1);
        assert!(rem.items.is_empty());
        assert_eq!(request.base_revision(), &[1, 2]);
        content.revision = Some(vec![3]);
        assert!(removal_changes(&content, second).is_err());
    }

    #[test]
    fn permissions_and_missing_identity_never_enable_removal() {
        let mut content = editable();
        content.capabilities.as_mut().unwrap().can_edit_items = Some(false);
        assert!(occurrences("spotify:playlist:test", &content).is_empty());
        content.capabilities.as_mut().unwrap().can_edit_items = Some(true);
        let mut occurrence = occurrences("spotify:playlist:test", &content).remove(0);
        occurrence.item_id = vec![9];
        assert!(removal_changes(&content, &occurrence).is_err());
        content.changes_require_resync = Some(true);
        assert!(occurrences("spotify:playlist:test", &content).is_empty());
    }

    #[test]
    fn append_uses_revision_checked_tail_operation() {
        let content = editable();
        let request = append_changes(&content, "spotify:track:0000000000000000000000").unwrap();
        let decoded =
            wire::ListChanges::parse_from_bytes(&request.write_to_bytes().unwrap()).unwrap();
        assert_eq!(decoded.base_revision(), &[1, 2]);
        let add = decoded.deltas[0].ops[0].add.as_ref().unwrap();
        assert!(add.add_last());
        assert_eq!(add.items.len(), 1);
        assert_eq!(add.items[0].uri(), "spotify:track:0000000000000000000000");
        assert!(decoded.deltas[0].ops[0].rem.is_none());
    }

    #[test]
    fn credits_preserve_actual_roles_and_extended_contributors() {
        let credits = decode_credits(&json!({"trackTitle":"Song", "roleCredits":[{"roleTitle":"Writers","artists":[{"name":"Writer"}]}], "extendedCredits":[{"roleTitle":"Engineer","artists":[{"name":"Engineer"}]}], "sourceNames":["Label"]})).unwrap();
        assert_eq!(credits.groups[0].role, "Writers");
        assert_eq!(credits.groups[1].names, vec!["Engineer"]);
        assert_eq!(credits.source_names, vec!["Label"]);
        assert!(decode_credits(&json!({"artists":[{"name":"Artist"}]})).is_err());
    }

    #[test]
    fn radio_uses_returned_media_destination_only() {
        let source = radio_destination(
            &json!({"mediaItems":[{"uri":"spotify:playlist:0000000000000000000000"}]}),
        )
        .unwrap();
        assert!(
            matches!(source, PlaylistSource::Playlist { uri, .. } if uri == "spotify:playlist:0000000000000000000000")
        );
        assert!(radio_destination(&json!({"uri":"spotify:playlist:not-the-response"})).is_err());
    }
}
