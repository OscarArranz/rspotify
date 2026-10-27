//! Personalized Home through the desktop Pathfinder operation.

use librespot::core::Session;
use serde_json::{Value, json};

use super::{SpotifyError, search::PATHFINDER_URL};

const HOME_HASH: &str = "76243c78b0e20ecdbe41b794dec8cbe73f75e585b0a7201b8d2e84578412847a";

#[derive(Clone, Debug, Default)]
pub(crate) struct HomeItem {
    pub uri: String,
    pub title: String,
    pub subtitle: String,
    pub image: Option<String>,
    pub kind: String,
    pub playable: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct HomeShelf {
    pub uri: String,
    pub title: String,
    pub subtitle: String,
    pub layout: String,
    pub items: Vec<HomeItem>,
    pub next_offset: Option<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct HomeChip {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct HomeFeed {
    pub chips: Vec<HomeChip>,
    pub shortcuts: Vec<HomeItem>,
    pub shelves: Vec<HomeShelf>,
    pub panels: Vec<HomePanel>,
}

#[derive(Clone, Debug)]
pub(crate) struct HomePanel {
    pub label: String,
    pub item: HomeItem,
    pub video_url: Option<String>,
    pub poster: Option<String>,
}

fn text(value: &Value, path: &str) -> String {
    value
        .pointer(path)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn label(value: &Value) -> String {
    value
        .get("transformedLabel")
        .and_then(Value::as_str)
        .or_else(|| value.get("translatedBaseText").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string()
}

fn sources(value: &Value, path: &str) -> Option<String> {
    value
        .pointer(path)?
        .as_array()?
        .iter()
        .filter_map(|s| {
            s.get("url")
                .and_then(Value::as_str)
                .map(|url| (s.get("width").and_then(Value::as_u64).unwrap_or(300), url))
        })
        .filter(|(_, url)| url.starts_with("https://"))
        .min_by_key(|(width, _)| width.abs_diff(400))
        .map(|(_, url)| url.to_string())
}

fn plain_text(text: &str) -> String {
    let mut inside = false;
    text.chars()
        .filter(|c| match c {
            '<' => {
                inside = true;
                false
            }
            '>' => {
                inside = false;
                false
            }
            _ => !inside,
        })
        .collect::<String>()
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
}

fn item(value: &Value) -> Option<HomeItem> {
    if text(value, "/uri") == "spotify:collection:tracks" {
        return Some(HomeItem {
            uri: "spotify:collection:tracks".into(),
            title: "Liked Songs".into(),
            subtitle: "Your saved tracks".into(),
            kind: "LikedSongs".into(),
            playable: true,
            image: None,
        });
    }
    let data = value.pointer("/content/data")?;
    let kind = text(data, "/__typename");
    if !matches!(
        kind.as_str(),
        "Playlist" | "Album" | "Artist" | "Episode" | "Podcast" | "Track"
    ) {
        return None;
    }
    let uri = text(data, "/uri");
    if !uri.starts_with("spotify:") {
        return None;
    }
    let title = if kind == "Artist" {
        text(data, "/profile/name")
    } else {
        text(data, "/name")
    };
    let subtitle = match kind.as_str() {
        "Album" => data
            .pointer("/artists/items")
            .and_then(Value::as_array)
            .map(|artists| {
                artists
                    .iter()
                    .map(|a| text(a, "/profile/name"))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default(),
        "Playlist" => {
            let description = plain_text(&text(data, "/description"));
            if description.is_empty() {
                format!("By {}", text(data, "/ownerV2/data/name"))
            } else {
                description
            }
        }
        "Episode" => text(data, "/podcastV2/data/name"),
        _ => kind.clone(),
    };
    let image = sources(data, "/coverArt/sources")
        .or_else(|| sources(data, "/images/items/0/sources"))
        .or_else(|| sources(data, "/visuals/avatarImage/sources"));
    Some(HomeItem {
        uri,
        title,
        subtitle,
        image,
        kind: kind.clone(),
        playable: data
            .pointer("/playability/playable")
            .and_then(Value::as_bool)
            .unwrap_or(true)
            && matches!(
                kind.as_str(),
                "Playlist" | "Album" | "Artist" | "Episode" | "Track"
            ),
    })
}

pub(crate) fn parse(value: &Value) -> Result<HomeFeed, SpotifyError> {
    let home = value
        .pointer("/data/home")
        .filter(|v| text(v, "/__typename") == "HomeResponsePayload")
        .ok_or_else(|| SpotifyError::Api("Home is temporarily unavailable".into()))?;
    let mut feed = HomeFeed::default();
    for chip in home
        .get("homeChips")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let id = text(chip, "/id");
        let name = label(&chip["label"]);
        if !id.is_empty() && !name.is_empty() {
            feed.chips.push(HomeChip { id, label: name });
        }
    }
    for section in home
        .pointer("/sectionContainer/sections/items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let layout = text(section, "/data/__typename");
        let items: Vec<_> = section
            .pointer("/sectionItems/items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(item)
            .collect();
        if layout == "HomeShortsSectionData" {
            feed.shortcuts.extend(items);
            continue;
        }
        if items.is_empty() {
            continue;
        }
        let title = label(&section["data"]["title"]);
        let subtitle = label(&section["data"]["subtitle"]);
        if layout == "HomeFeedBaselineSectionData" {
            feed.panels.extend(items.into_iter().map(|item| HomePanel {
                label: title.clone(),
                item,
                video_url: None,
                poster: None,
            }));
        } else if !title.is_empty() {
            let next_offset = section
                .pointer("/sectionItems/pagingInfo/nextOffset")
                .and_then(Value::as_u64)
                .filter(|v| *v > 0);
            feed.shelves.push(HomeShelf {
                uri: text(section, "/uri"),
                title,
                subtitle,
                layout,
                items,
                next_offset,
            });
        }
    }
    Ok(feed)
}

pub(super) async fn section(
    session: &Session,
    uri: &str,
    offset: u64,
) -> Result<(Vec<HomeItem>, Option<u64>), SpotifyError> {
    let value = query(
        session,
        "homeSection",
        HOME_HASH,
        json!({
            "uri": uri, "homeEndUserIntegration": "INTEGRATION_DESKTOP", "timeZone": "UTC",
            "sp_t": "", "sectionItemsOffset": offset, "sectionItemsLimit": 40,
            "includeEpisodeContentRatingsV2": true,
        }),
    )
    .await?;
    let section = value
        .pointer("/data/homeSections/sections/0/sectionItems")
        .ok_or_else(|| SpotifyError::Api("Home shelf is unavailable".into()))?;
    let items = section
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(item)
        .collect();
    let next = section
        .pointer("/pagingInfo/nextOffset")
        .and_then(Value::as_u64)
        .filter(|next| *next > offset);
    Ok((items, next))
}

pub(super) async fn fetch(session: &Session, facet: &str) -> Result<HomeFeed, SpotifyError> {
    let mut feed = parse(&fetch_raw(session, facet).await?)?;
    // When the account has no baseline sections, expose previews of actual returned
    // recommendations rather than manufacturing personalized playlists.
    if feed.panels.is_empty() {
        feed.panels = feed
            .shelves
            .iter()
            .filter_map(|s| {
                s.items
                    .iter()
                    .find(|i| matches!(i.kind.as_str(), "Playlist" | "Album"))
                    .map(|item| HomePanel {
                        label: s.title.clone(),
                        item: item.clone(),
                        video_url: None,
                        poster: None,
                    })
            })
            .take(3)
            .collect();
    }
    feed.panels.truncate(3);
    let uris: Vec<_> = feed.panels.iter().map(|p| p.item.uri.clone()).collect();
    if !uris.is_empty() {
        if let Ok(value) = preview_raw(session, &uris).await {
            apply_previews(&mut feed.panels, &value);
        }
    }
    Ok(feed)
}

fn apply_previews(panels: &mut [HomePanel], value: &Value) {
    for wrapper in value
        .pointer("/data/lookup")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let uri = text(wrapper, "/_uri");
        let Some(panel) = panels.iter_mut().find(|p| p.item.uri == uri) else {
            continue;
        };
        for track in wrapper
            .pointer("/data/previewItems/items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let url = text(track, "/data/canvas/url");
            if trusted_video_url(&url) {
                panel.video_url = Some(url);
                panel.poster = sources(track, "/data/canvas/thumbnail/sources");
                break;
            }
        }
    }
}

pub(crate) fn trusted_video_url(url: &str) -> bool {
    reqwest::Url::parse(url).ok().is_some_and(|url| {
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url
                .host_str()
                .is_some_and(|host| host.ends_with(".scdn.co") || host.ends_with(".spotifycdn.com"))
    })
}

pub(super) async fn fetch_raw(session: &Session, facet: &str) -> Result<Value, SpotifyError> {
    let variables = json!({
        "homeEndUserIntegration": "INTEGRATION_DESKTOP", "timeZone": "UTC",
        "sp_t": "", "facet": facet, "sectionItemsLimit": 10,
        "includeEpisodeContentRatingsV2": true,
    });
    query(session, "home", HOME_HASH, variables).await
}

pub(super) async fn preview_raw(session: &Session, uris: &[String]) -> Result<Value, SpotifyError> {
    query(
        session,
        "feedBaselineLookup",
        "a950fb7c4ecdcaf2aad2f3ca9ee9c3aa4b9c43c97e1d07d05148c4d355bea7fc",
        json!({"uris":uris}),
    )
    .await
}

async fn query(
    session: &Session,
    operation: &str,
    hash: &str,
    variables: Value,
) -> Result<Value, SpotifyError> {
    let token = session
        .login5()
        .auth_token()
        .await
        .map_err(|_| SpotifyError::Auth("Home authentication failed".into()))?;
    let response = reqwest::Client::new()
        .get(PATHFINDER_URL)
        .query(&[
            ("operationName", operation),
            ("variables", &variables.to_string()),
            (
                "extensions",
                &json!({"persistedQuery":{"version":1,"sha256Hash":hash}}).to_string(),
            ),
        ])
        .bearer_auth(token.access_token)
        .header("App-Platform", "WebPlayer")
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(SpotifyError::Api(format!(
            "Home returned HTTP {}",
            response.status()
        )));
    }
    let value: Value = response.json().await?;
    if value
        .get("errors")
        .and_then(Value::as_array)
        .is_some_and(|errors| !errors.is_empty())
        && value.get("data").is_none_or(Value::is_null)
    {
        // Never include service payloads or account metadata in errors.
        return Err(SpotifyError::Api("Home query was rejected".into()));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playlist(uri: &str) -> Value {
        json!({"content":{"data":{"__typename":"Playlist", "uri":uri, "name":"Example playlist",
            "description":"A <a href='https://example.test'>test</a> &amp; music",
            "images":{"items":[{"sources":[{"url":"https://i.scdn.co/image/example", "width":300}]}]}}}})
    }

    fn response(sections: Vec<Value>) -> Value {
        json!({"data":{"home":{"__typename":"HomeResponsePayload", "homeChips":[
            {"id":"music-chip", "label":{"transformedLabel":null,"translatedBaseText":"Music"}}],
            "sectionContainer":{"sections":{"items":sections}}}}})
    }

    #[test]
    fn parses_shortcuts_ordered_shelves_chips_and_artwork() {
        let feed = parse(&response(vec![
            json!({"data":{"__typename":"HomeShortsSectionData"}, "sectionItems":{"items":[playlist("spotify:playlist:a")]}}),
            json!({"uri":"spotify:section:example", "data":{"__typename":"HomeGenericSectionData","title":{"transformedLabel":"Made for you"}},
                "sectionItems":{"items":[playlist("spotify:playlist:b")],"pagingInfo":{"nextOffset":10}}}),
        ])).unwrap();
        assert_eq!(feed.shortcuts[0].uri, "spotify:playlist:a");
        assert_eq!(feed.shelves[0].items[0].uri, "spotify:playlist:b");
        assert_eq!(feed.shelves[0].title, "Made for you");
        assert_eq!(feed.shelves[0].next_offset, Some(10));
        assert_eq!(feed.chips[0].label, "Music");
        assert_eq!(feed.shelves[0].items[0].subtitle, "A test & music");
        assert!(feed.shortcuts[0].image.is_some());
    }

    #[test]
    fn skips_unavailable_content_and_preserves_prerelease_restrictions() {
        let blocked = json!({"content":{"data":{"__typename":"Album","uri":"spotify:album:example", "name":"Not out yet", "playability":{"playable":false}}}});
        let feed = parse(&response(vec![json!({"data":{"__typename":"HomeGenericSectionData","title":{"transformedLabel":"Releases"}},
            "sectionItems":{"items":[{"content":{"data":{"__typename":"NotFound"}}},blocked]}})])).unwrap();
        assert_eq!(feed.shelves[0].items.len(), 1);
        assert!(!feed.shelves[0].items[0].playable);
        assert!(parse(&json!({"errors":[]})).is_err());
        assert!(parse(&response(vec![])).unwrap().shelves.is_empty());
    }

    #[test]
    fn baseline_panels_are_not_misclassified_as_shelves() {
        let feed = parse(&response(vec![json!({"data":{"__typename":"HomeFeedBaselineSectionData","title":{"transformedLabel":"Made for you"}},
            "sectionItems":{"items":[playlist("spotify:playlist:a")]}})])).unwrap();
        assert_eq!(feed.panels.len(), 1);
        assert!(feed.shelves.is_empty());
    }

    #[test]
    fn preview_lookup_matches_context_and_rejects_untrusted_urls() {
        let mut panels = vec![HomePanel {
            label: "Examples".into(),
            item: item(&playlist("spotify:playlist:a")).unwrap(),
            video_url: None,
            poster: None,
        }];
        apply_previews(
            &mut panels,
            &json!({"data":{"lookup":[{"_uri":"spotify:playlist:a", "data":{"previewItems":{"items":[
                {"data":{"canvas":{"url":"https://example.com/video"}}},
                {"data":{"canvas":{"url":"https://canvaz.scdn.co/example.mp4", "thumbnail":{"sources":[{"url":"https://i.scdn.co/image/example"}]}}}}
            ]}}}]}}),
        );
        assert_eq!(
            panels[0].video_url.as_deref(),
            Some("https://canvaz.scdn.co/example.mp4")
        );
        assert!(panels[0].poster.is_some());
        for url in [
            "http://canvaz.scdn.co/a",
            "file:///a",
            "https://scdn.co.attacker.test/a",
            "https://user@canvaz.scdn.co/a",
        ] {
            assert!(!trusted_video_url(url));
        }
    }
}
