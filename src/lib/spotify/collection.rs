//! Native collection-v2 paging. The request contract is evidenced in Spotify's native ELF.
//! Its installed descriptor uses int64 timestamps; librespot's unused proto declares int32.

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use librespot::core::Session;
use protobuf::{CodedInputStream, CodedOutputStream, UnknownFields};
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderValue};

use super::error::SpotifyError;

const MIME: &str = "application/vnd.collection-v2.spotify.proto";

#[derive(Clone, Debug, Default)]
pub(crate) struct CollectionItem {
    pub uri: String,
    pub added_at: i64,
    pub removed: bool,
    pub context_uri: Option<String>,
}

#[derive(Default)]
pub(crate) struct CollectionPage {
    pub items: Vec<CollectionItem>,
    pub next: String,
}

fn parse_item(bytes: &[u8]) -> protobuf::Result<CollectionItem> {
    let mut input = CodedInputStream::from_bytes(bytes);
    let mut unknown = UnknownFields::new();
    let mut item = CollectionItem::default();
    while let Some(tag) = input.read_raw_tag_or_eof()? {
        match tag {
            10 => item.uri = input.read_string()?,
            16 => item.added_at = input.read_int64()?,
            24 => item.removed = input.read_bool()?,
            34 => item.context_uri = Some(input.read_string()?),
            _ => protobuf::rt::read_unknown_or_skip_group(tag, &mut input, &mut unknown)?,
        }
    }
    Ok(item)
}

fn parse_page(bytes: &[u8]) -> protobuf::Result<CollectionPage> {
    let mut input = CodedInputStream::from_bytes(bytes);
    let mut unknown = UnknownFields::new();
    let mut page = CollectionPage::default();
    while let Some(tag) = input.read_raw_tag_or_eof()? {
        match tag {
            10 => page.items.push(parse_item(&input.read_bytes()?)?),
            18 => page.next = input.read_string()?,
            _ => protobuf::rt::read_unknown_or_skip_group(tag, &mut input, &mut unknown)?,
        }
    }
    Ok(page)
}

fn request(username: &str, set: &str, token: &str) -> protobuf::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut output = CodedOutputStream::vec(&mut bytes);
        output.write_string(1, username)?;
        output.write_string(2, set)?;
        if !token.is_empty() {
            output.write_string(3, token)?;
        }
        output.write_int32(4, 300)?;
        output.flush()?;
    }
    Ok(bytes)
}

pub(crate) async fn page(
    session: &Session,
    set: &str,
    token: &str,
) -> Result<CollectionPage, SpotifyError> {
    let body =
        request(&session.username(), set, token).map_err(|e| SpotifyError::Api(e.to_string()))?;
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static(MIME));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(MIME));
    let bytes = tokio::time::timeout(
        Duration::from_secs(15),
        session.spclient().request(
            &reqwest::Method::POST,
            "/collection/v2/paging",
            Some(headers),
            Some(&body),
        ),
    )
    .await
    .map_err(|_| SpotifyError::Api("Native collection paging timed out".into()))?
    .map_err(|e| SpotifyError::Api(e.to_string()))?;
    parse_page(&bytes)
        .map_err(|e| SpotifyError::Api(format!("Invalid native collection page: {e}")))
}

pub(crate) async fn snapshot(
    session: &Session,
    set: &str,
) -> Result<Vec<CollectionItem>, SpotifyError> {
    let mut token = String::new();
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    loop {
        let page = page(session, set, &token).await?;
        items.extend(page.items);
        if page.next.is_empty() {
            break;
        }
        if !seen.insert(page.next.clone()) {
            return Err(SpotifyError::Api("Repeated native collection page".into()));
        }
        token = page.next;
    }
    Ok(latest_items(items))
}

fn latest_items(items: Vec<CollectionItem>) -> Vec<CollectionItem> {
    let mut positions = HashMap::<String, usize>::new();
    let mut current: Vec<CollectionItem> = Vec::new();
    for item in items {
        if let Some(&position) = positions.get(&item.uri) {
            if item.added_at >= current[position].added_at {
                current[position] = item;
            }
        } else {
            positions.insert(item.uri.clone(), current.len());
            current.push(item);
        }
    }
    current
}

fn membership_write(
    username: &str,
    set: &str,
    uri: &str,
    removed: bool,
    timestamp: i64,
    update_id: &str,
) -> protobuf::Result<Vec<u8>> {
    let mut item = Vec::new();
    {
        let mut output = CodedOutputStream::vec(&mut item);
        output.write_string(1, uri)?;
        output.write_int64(2, timestamp)?;
        output.write_bool(3, removed)?;
        output.flush()?;
    }
    let mut bytes = Vec::new();
    {
        let mut output = CodedOutputStream::vec(&mut bytes);
        output.write_string(1, username)?;
        output.write_string(2, set)?;
        output.write_bytes(3, &item)?;
        output.write_string(4, update_id)?;
        output.flush()?;
    }
    Ok(bytes)
}

/// Account mutation, called only from an explicit Pin/Unpin UI action, never by loading/tests.
/// Confirm actual membership with a read after the status-based write acknowledgment.
pub(crate) async fn set_pinned(
    session: &Session,
    uri: &str,
    pinned: bool,
) -> Result<HashSet<String>, SpotifyError> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64;
    let update_id = format!("{:016x}", rand::random::<u64>());
    let body = membership_write(
        &session.username(),
        "ylpin",
        uri,
        !pinned,
        timestamp,
        &update_id,
    )
    .map_err(|e| SpotifyError::Api(e.to_string()))?;
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static(MIME));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(MIME));
    tokio::time::timeout(
        Duration::from_secs(15),
        session.spclient().request(
            &reqwest::Method::POST,
            "/collection/v2/write",
            Some(headers),
            Some(&body),
        ),
    )
    .await
    .map_err(|_| SpotifyError::Api("Pin request timed out; refresh to check its result".into()))?
    .map_err(|e| SpotifyError::Api(e.to_string()))?;
    for attempt in 0..3 {
        let pins: HashSet<_> = snapshot(session, "ylpin")
            .await.map_err(|_| SpotifyError::Api("Pin write was acknowledged but membership could not be verified. Refresh Your Library to check its result.".into()))?
            .into_iter()
            .filter(|i| !i.removed)
            .map(|i| i.uri)
            .collect();
        if pins.contains(uri) == pinned {
            return Ok(pins);
        }
        if attempt < 2 {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
    Err(SpotifyError::Api(
        "Pin change could not be verified. Refresh Your Library to check its result.".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_decoder_preserves_int64_dates_context_and_tombstones() {
        let mut item = Vec::new();
        {
            let mut out = CodedOutputStream::vec(&mut item);
            out.write_string(1, "spotify:album:fixture").unwrap();
            out.write_int64(2, 4_000_000_000).unwrap();
            out.write_bool(3, true).unwrap();
            out.write_string(4, "spotify:album:context").unwrap();
            out.write_bytes(5, &[8, 1]).unwrap(); // newer ItemData, safely ignored
            out.flush().unwrap();
        }
        let decoded = parse_item(&item).unwrap();
        assert_eq!(decoded.added_at, 4_000_000_000);
        assert!(decoded.removed);
        assert_eq!(
            decoded.context_uri.as_deref(),
            Some("spotify:album:context")
        );
        assert!(parse_page(&[0xff]).is_err());
    }

    #[test]
    fn pin_write_has_native_fields_without_making_any_request() {
        let bytes = membership_write(
            "fixture-account",
            "ylpin",
            "spotify:playlist:fixture",
            true,
            4_000_000_000,
            "0123456789abcdef",
        )
        .unwrap();
        let mut input = CodedInputStream::from_bytes(&bytes);
        assert_eq!(input.read_raw_tag_or_eof().unwrap(), Some(10));
        assert_eq!(input.read_string().unwrap(), "fixture-account");
        assert_eq!(input.read_raw_tag_or_eof().unwrap(), Some(18));
        assert_eq!(input.read_string().unwrap(), "ylpin");
        assert_eq!(input.read_raw_tag_or_eof().unwrap(), Some(26));
        let item = parse_item(&input.read_bytes().unwrap()).unwrap();
        assert!(item.removed);
        assert_eq!(item.added_at, 4_000_000_000);
        assert_eq!(input.read_raw_tag_or_eof().unwrap(), Some(34));
        assert_eq!(input.read_string().unwrap(), "0123456789abcdef");
    }

    #[test]
    fn newer_tombstones_replace_older_membership_across_pages() {
        let items = latest_items(vec![
            CollectionItem {
                uri: "spotify:album:fixture".into(),
                added_at: 10,
                ..Default::default()
            },
            CollectionItem {
                uri: "spotify:album:fixture".into(),
                added_at: 20,
                removed: true,
                ..Default::default()
            },
            CollectionItem {
                uri: "spotify:album:fixture".into(),
                added_at: 15,
                ..Default::default()
            },
        ]);
        assert_eq!(items.len(), 1);
        assert!(items[0].removed);
    }
}
