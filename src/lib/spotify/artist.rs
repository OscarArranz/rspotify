//! Native artist profiles through Spotify's Pathfinder endpoint.
//!
//! The installed desktop client requests `queryArtistOverview` with an artist URI
//! and a nullable locale. The response is deliberately parsed section by section:
//! optional artist content varies by account, market, and artist, and a missing
//! section must not invalidate the rest of the profile.
//!
//! Field paths below were validated against a real, read-only overview response
//! from the installed client's persisted query. The parser keeps aliases for a few
//! fields because the desktop schema has changed names across releases.

use std::collections::HashSet;

use librespot::core::Session;
use serde_json::{Value, json};

use super::error::SpotifyError;

const PATHFINDER_URL: &str = "https://api-partner.spotify.com/pathfinder/v1/query";
pub const ARTIST_OVERVIEW_HASH: &str =
    "1ac33ddab5d39a3a9c27802774e6d78b9405cc188c6f75aed007df2a32737c72";
const IMAGE_BASE: &str = "https://i.scdn.co/image/";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtistProfile {
    pub uri: String,
    pub name: String,
    pub verified: bool,
    pub header_image_url: Option<String>,
    pub avatar_image_url: Option<String>,
    pub biography: Option<String>,
    pub monthly_listeners: Option<u64>,
    pub followers: Option<u64>,
    pub world_rank: Option<u64>,
    pub top_cities: Vec<ArtistCity>,
    pub gallery: Vec<String>,
    /// `profile.saved` from the overview: `None` when the response omits it.
    pub following: Option<bool>,
    pub popular_tracks: Vec<ArtistTrack>,
    pub liked_music: Option<ArtistLikedMusic>,
    pub artist_pick: Option<ArtistCard>,
    pub discography: ArtistDiscography,
    pub featuring: Vec<ArtistCard>,
    pub music_videos: Vec<ArtistVideo>,
    pub concerts: Vec<ArtistConcert>,
    pub merchandise: Vec<ArtistMerch>,
    pub discovered_on: Vec<ArtistCard>,
    pub related_artists: Vec<ArtistCard>,
    pub appears_on: Vec<ArtistCard>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtistCity {
    pub city: String,
    pub country: String,
    pub listeners: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtistTrack {
    pub uri: String,
    pub title: String,
    pub artists: String,
    pub album: String,
    pub image_url: Option<String>,
    pub duration_ms: u64,
    /// Play count supplied by the overview, when present.
    pub playcount: Option<u64>,
    pub explicit: bool,
    pub playable: bool,
    pub music_video: bool,
    pub liked: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtistLikedMusic {
    pub uri: Option<String>,
    pub track_count: Option<u64>,
    pub release_count: Option<u64>,
    pub image_url: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtistCard {
    pub uri: String,
    pub name: String,
    pub subtitle: String,
    /// Release year, when the overview supplies it.
    pub year: Option<u32>,
    /// Optional editorial note, used by the artist pick.
    pub note: Option<String>,
    pub image_url: Option<String>,
    pub kind: ArtistCardKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ArtistCardKind {
    #[default]
    Release,
    Playlist,
    Artist,
    Video,
    Merch,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtistDiscography {
    /// `discography.popularReleasesAlbums`
    pub popular: Vec<ArtistCard>,
    /// `discography.albums`
    pub albums: Vec<ArtistCard>,
    /// `discography.singles`
    pub singles: Vec<ArtistCard>,
    /// `discography.compilations`
    pub compilations: Vec<ArtistCard>,
    pub popular_total: Option<u64>,
    pub albums_total: Option<u64>,
    pub singles_total: Option<u64>,
    pub compilations_total: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtistVideo {
    pub uri: String,
    pub title: String,
    pub artists: String,
    pub image_url: Option<String>,
    pub explicit: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtistConcert {
    pub uri: String,
    pub city: String,
    pub venue: String,
    pub date_label: String,
    pub time_label: String,
    pub ticket_url: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtistMerch {
    pub name: String,
    pub description: String,
    pub price: Option<String>,
    pub image_url: Option<String>,
    pub url: Option<String>,
}

pub async fn fetch_artist_profile(
    session: &Session,
    uri: &str,
) -> Result<ArtistProfile, SpotifyError> {
    let token = session
        .login5()
        .auth_token()
        .await
        .map_err(|error| SpotifyError::Auth(error.to_string()))?;

    let variables = json!({ "uri": uri, "locale": null });
    let extensions = json!({
        "persistedQuery": {
            "version": 1,
            "sha256Hash": ARTIST_OVERVIEW_HASH,
        }
    });

    let response = reqwest::Client::new()
        .get(PATHFINDER_URL)
        .query(&[
            ("operationName", "queryArtistOverview"),
            ("variables", &variables.to_string()),
            ("extensions", &extensions.to_string()),
        ])
        .header("Authorization", format!("Bearer {}", token.access_token))
        .header("App-Platform", "WebPlayer")
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("Spotify-App-Version", "1.2.95.453")
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(SpotifyError::Api(format!(
            "artist overview returned HTTP {status}: {}",
            truncate(&body, 240)
        )));
    }

    let value: Value = serde_json::from_str(&body)
        .map_err(|error| SpotifyError::Api(format!("invalid artist overview JSON: {error}")))?;
    if let Some(errors) = value.get("errors") {
        return Err(SpotifyError::Api(format!(
            "artist overview GraphQL error: {}",
            truncate(&errors.to_string(), 240)
        )));
    }
    if let Some(filter) = std::env::var_os("RSPOTIFY_ARTIST_DIAGNOSTICS") {
        let filter = filter.to_string_lossy();
        diagnose_schema(&value, &mut HashSet::new(), "", 0, &filter);
        diagnose_scalars(&value);
    }
    let artist = value
        .pointer("/data/artistUnion")
        .ok_or_else(|| SpotifyError::Api("artist overview is missing artistUnion".into()))?;
    parse_artist(artist, uri).ok_or_else(|| {
        SpotifyError::Api("artist overview did not contain a usable artist entity".into())
    })
}

pub fn parse_artist(artist: &Value, requested_uri: &str) -> Option<ArtistProfile> {
    let typename = string_at(artist, &["__typename"]).unwrap_or_default();
    if !typename.is_empty() && typename != "Artist" {
        return None;
    }

    let uri = string_at(artist, &["uri"])
        .filter(|uri| !uri.is_empty())
        .unwrap_or_else(|| requested_uri.to_string());
    let name = string_at(artist, &["profile/name", "name", "profile/displayName"])
        .filter(|name| !name.is_empty())?;

    let header_image_url = image_at(
        artist,
        &[
            "headerImage/data/sources",
            "headerImage/sources",
            "visualIdentity/wideFullBleedImage/data/sources",
            "visuals/headerImage/sources",
            "visuals/heroImage/sources",
            "headerImage",
        ],
    );
    let avatar_image_url = image_at(
        artist,
        &[
            "visuals/avatarImage/sources",
            "avatarImage/sources",
            "profile/avatarImage/sources",
        ],
    );

    let mut profile = ArtistProfile {
        uri,
        name,
        verified: bool_at(artist, &["profile/verified", "isVerified"]),
        header_image_url,
        avatar_image_url,
        biography: biography_at(artist),
        monthly_listeners: u64_at(artist, &["stats/monthlyListeners"]),
        followers: u64_at(artist, &["stats/followers", "followers/totalCount"]),
        world_rank: u64_at(artist, &["stats/worldRank", "stats/chartPosition/position"]),
        top_cities: parse_cities(artist),
        gallery: gallery_images(artist),
        following: option_bool_at(artist, &["profile/saved", "saved", "profile/isFollowing"]),
        ..ArtistProfile::default()
    };

    profile.popular_tracks = parse_popular_tracks(artist);
    profile.liked_music = parse_liked_music(artist);
    profile.artist_pick = parse_pick(artist);
    profile.discography = parse_discography(artist);
    profile.featuring = parse_cards(
        artist,
        &[
            "relatedContent/featuringV2/items",
            "relatedContent/featuring/items",
            "profile/playlistsV2/items",
        ],
        ArtistCardKind::Playlist,
    );
    profile.music_videos = parse_videos(artist);
    profile.concerts = parse_concerts(artist);
    profile.merchandise = parse_merch(artist);
    profile.discovered_on = parse_cards(
        artist,
        &[
            "relatedContent/discoveredOnV2/items",
            "relatedContent/discoveredOn/items",
        ],
        ArtistCardKind::Playlist,
    );
    profile.related_artists = parse_cards(
        artist,
        &[
            "relatedContent/relatedArtists/items",
            "relatedArtists/items",
        ],
        ArtistCardKind::Artist,
    );
    profile.appears_on = parse_release_cards(
        artist,
        &["relatedContent/appearsOn/items", "appearsOn/items"],
    );

    Some(profile)
}

fn parse_popular_tracks(artist: &Value) -> Vec<ArtistTrack> {
    let Some(items) = array_at(
        artist,
        &[
            "discography/topTracks/items",
            "topTracks/items",
            "profile/topTracks/items",
        ],
    ) else {
        return Vec::new();
    };
    let mut tracks = Vec::new();
    for item in items {
        let data = unwrap_item(item);
        let track = data.get("track").map(unwrap_item).unwrap_or(data);
        let uri = string_at(track, &["uri", "trackUri"]).unwrap_or_default();
        if !uri.starts_with("spotify:track:") {
            continue;
        }
        let title = string_at(track, &["name", "title", "trackName"]).unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let artists = joined_names(track, &["artists/items", "artists"]);
        let album =
            string_at(track, &["albumOfTrack/name", "album/name", "albumName"]).unwrap_or_default();
        let image_url = image_at(
            track,
            &[
                "albumOfTrack/coverArt/sources",
                "album/coverArt/sources",
                "coverArt/sources",
            ],
        );
        let duration_ms = u64_at(
            track,
            &["duration/totalMilliseconds", "duration/ms", "durationMs"],
        )
        .unwrap_or(0);
        let video_count =
            u64_at(track, &["associationsV3/videoAssociations/totalCount"]).unwrap_or(0);
        tracks.push(ArtistTrack {
            uri,
            title,
            artists,
            album,
            image_url,
            duration_ms,
            playcount: u64_at(track, &["playcount"]),
            explicit: string_at(track, &["contentRating/label"])
                .is_some_and(|label| label.eq_ignore_ascii_case("explicit"))
                || bool_at(track, &["explicit"]),
            playable: option_bool_at(track, &["playability/playable", "playable", "isPlayable"])
                .unwrap_or(true),
            music_video: video_count > 0
                || bool_at(track, &["hasVideo", "isMusicVideo"])
                || track.get("video").is_some_and(|value| !value.is_null()),
            liked: bool_at(track, &["liked", "isLiked", "saved"]),
        });
    }
    tracks
}

fn parse_liked_music(artist: &Value) -> Option<ArtistLikedMusic> {
    let value = first_at(
        artist,
        &["profile/likedMusic", "likedMusic", "profile/yourLikedMusic"],
    )?;
    if value.is_null() {
        return None;
    }
    let track_count = u64_at(value, &["trackCount", "tracks/totalCount", "songsCount"])
        .or_else(|| u64_at(artist, &["profile/likedMusicTrackCount"]));
    let release_count = u64_at(
        value,
        &["releaseCount", "releases/totalCount", "albumCount"],
    );
    let uri = string_at(value, &["uri", "contextUri"]);
    let image_url = image_at(
        value,
        &[
            "images/items/0/sources",
            "coverArt/sources",
            "image/sources",
        ],
    );
    (track_count.is_some() || release_count.is_some() || uri.is_some()).then_some(
        ArtistLikedMusic {
            uri,
            track_count,
            release_count,
            image_url,
        },
    )
}

fn parse_pick(artist: &Value) -> Option<ArtistCard> {
    let value = first_at(
        artist,
        &[
            "profile/pinnedItem",
            "profile/artistPick",
            "artistPick",
            "profile/pick",
        ],
    )?;
    if value.is_null() {
        return None;
    }
    // `pinnedItem` wraps the real entity in `itemV2.data`.
    let data = first_at(value, &["itemV2/data", "itemV2", "data", "item/data"])
        .map(unwrap_item)
        .unwrap_or_else(|| value.get("item").map(unwrap_item).unwrap_or(value));
    let uri = string_at(data, &["uri", "data/uri"]).unwrap_or_default();
    let name = string_at(data, &["name", "data/name", "title"]).unwrap_or_default();
    if uri.is_empty() && name.is_empty() {
        return None;
    }
    let kind = card_kind_from_uri(&uri);
    let subtitle = string_at(value, &["subtitle", "type"])
        .or_else(|| string_at(data, &["type", "description", "albumType"]))
        .unwrap_or_else(|| kind_label(kind).to_string());
    let image_url = image_at(data, &["coverArt/sources"])
        .or_else(|| image_at(value, &["thumbnailImage/data/sources"]))
        .or_else(|| image_at(data, &["images/items/0/sources"]));
    Some(ArtistCard {
        uri,
        name,
        subtitle,
        year: u64_at(data, &["date/year"]).map(|year| year as u32),
        note: string_at(value, &["comment"])
            .map(|comment| html_to_text(&comment))
            .filter(|comment| !comment.trim().is_empty()),
        image_url,
        kind,
    })
}

fn parse_discography(artist: &Value) -> ArtistDiscography {
    let popular = parse_release_group(
        artist,
        &[
            "discography/popularReleasesAlbums/items",
            "discography/popularReleases/items",
        ],
    );
    let albums = parse_release_group(
        artist,
        &["discography/albums/items", "discography/albumsV2/items"],
    );
    let singles = parse_release_group(
        artist,
        &[
            "discography/singles/items",
            "discography/singlesV2/items",
            "discography/singlesAndEps/items",
        ],
    );
    let compilations = parse_release_group(
        artist,
        &[
            "discography/compilations/items",
            "discography/compilationsV2/items",
        ],
    );
    ArtistDiscography {
        popular_total: len_at(artist, &["discography/popularReleasesAlbums/totalCount"]),
        albums_total: len_at(artist, &["discography/albums/totalCount"]),
        singles_total: len_at(artist, &["discography/singles/totalCount"]),
        compilations_total: len_at(artist, &["discography/compilations/totalCount"]),
        popular,
        albums,
        singles,
        compilations,
    }
}

fn parse_release_group(artist: &Value, paths: &[&str]) -> Vec<ArtistCard> {
    let Some(items) = array_at(artist, &paths) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let data = unwrap_item(item);
            // Discography groups wrap each release in `releases.items`.
            let release = data
                .get("releases")
                .and_then(|releases| releases.get("items"))
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .map(unwrap_item)
                .unwrap_or(data);
            card_from_value(release, ArtistCardKind::Release)
        })
        .collect()
}

fn parse_cards(artist: &Value, paths: &[&str], kind: ArtistCardKind) -> Vec<ArtistCard> {
    let Some(items) = array_at(artist, &paths) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| card_from_value(unwrap_item(item), kind))
        .collect()
}

/// Release lists such as appears-on wrap each entry in `releases.items`.
fn parse_release_cards(artist: &Value, paths: &[&str]) -> Vec<ArtistCard> {
    let Some(items) = array_at(artist, &paths) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let data = unwrap_item(item);
            let release = data
                .get("releases")
                .and_then(|releases| releases.get("items"))
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .map(unwrap_item)
                .unwrap_or(data);
            card_from_value(release, ArtistCardKind::Release)
        })
        .collect()
}

fn card_from_value(data: &Value, kind: ArtistCardKind) -> Option<ArtistCard> {
    let uri = string_at(data, &["uri", "data/uri"]).unwrap_or_default();
    let name = string_at(
        data,
        &["name", "profile/name", "data/name", "title", "data/title"],
    )
    .unwrap_or_default();
    if uri.is_empty() && name.is_empty() {
        return None;
    }
    let kind = if kind == ArtistCardKind::Release {
        card_kind_from_uri(&uri)
    } else {
        kind
    };
    let subtitle = string_at(
        data,
        &[
            "description",
            "subtitle",
            "type",
            "albumType",
            "ownerV2/data/name",
            "owner/name",
            "publisher/name",
        ],
    )
    .unwrap_or_else(|| {
        if kind == ArtistCardKind::Artist {
            "Artist".to_string()
        } else {
            String::new()
        }
    });
    let image_url = image_at(
        data,
        &[
            "coverArt/sources",
            "images/items/0/sources",
            "images/0/sources",
            "visuals/avatarImage/sources",
            "avatarImage/sources",
            "albumOfTrack/coverArt/sources",
            "sources",
        ],
    );
    Some(ArtistCard {
        uri,
        name,
        subtitle,
        year: u64_at(data, &["date/year"]).map(|year| year as u32),
        note: None,
        image_url,
        kind,
    })
}

fn parse_videos(artist: &Value) -> Vec<ArtistVideo> {
    let paths = [
        "relatedMusicVideos/items",
        "unmappedMusicVideos/items",
        "relatedContent/watchFeed/items",
        "watchFeed/items",
        "profile/musicVideos/items",
        "musicVideos/items",
    ];
    let mut videos = Vec::new();
    for path in paths {
        let Some(items) = array_at(artist, &[path]) else {
            continue;
        };
        for item in items {
            let data = unwrap_item(item);
            let video = data.get("video").map(unwrap_item).unwrap_or(data);
            let uri = string_at(video, &["uri", "data/uri"]).unwrap_or_default();
            let title = string_at(video, &["name", "title", "data/name"]).unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            if videos
                .iter()
                .any(|existing: &ArtistVideo| existing.title == title && existing.uri == uri)
            {
                continue;
            }
            videos.push(ArtistVideo {
                uri,
                title,
                artists: joined_names(video, &["artists/items", "artists"]),
                image_url: image_at(
                    video,
                    &[
                        "thumbnail/sources",
                        "thumbnailImage/data/sources",
                        "coverArt/sources",
                        "images/items/0/sources",
                    ],
                ),
                explicit: string_at(video, &["contentRating/label"])
                    .is_some_and(|label| label.eq_ignore_ascii_case("explicit"))
                    || bool_at(video, &["explicit"]),
            });
        }
    }
    // The overview can expose only a "music videos" entrypoint without a
    // titled feed. Surface that entrypoint as a single non-playable card.
    if videos.is_empty() {
        if let Some(entrypoint) = value_at(artist, "watchFeedEntrypoint") {
            let uri = string_at(entrypoint, &["entrypointUri"]).unwrap_or_default();
            let image = image_at(entrypoint, &["thumbnailImage/data/sources"]);
            if !uri.is_empty() || image.is_some() {
                videos.push(ArtistVideo {
                    uri,
                    title: "Music videos".to_string(),
                    artists: String::new(),
                    image_url: image,
                    explicit: false,
                });
            }
        }
    }
    videos
}

fn parse_concerts(artist: &Value) -> Vec<ArtistConcert> {
    let paths = [
        "goods/concerts/items",
        "concerts/items",
        "profile/concerts/items",
        "upcomingConcerts/items",
    ];
    let Some(items) = array_at(artist, &paths) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let data = unwrap_item(item);
            let venue = first_at(data, &["venue", "venueV2", "location/venue"])
                .map(unwrap_item)
                .unwrap_or(data);
            let city = string_at(
                venue,
                &["location/city", "city", "location/name", "address/city"],
            )
            .unwrap_or_default();
            let venue_name =
                string_at(venue, &["location/name", "name", "venueName"]).unwrap_or_default();
            let date_label = string_at(
                data,
                &[
                    "startDateIsoString",
                    "date/isoString",
                    "date",
                    "startDate",
                    "startDate/isoString",
                    "date/day",
                ],
            )
            .unwrap_or_default();
            let time_label = string_at(data, &["time", "startTime", "date/time", "doorsOpenTime"])
                .unwrap_or_default();
            if city.is_empty() && venue_name.is_empty() && date_label.is_empty() {
                return None;
            }
            Some(ArtistConcert {
                uri: string_at(data, &["uri"]).unwrap_or_default(),
                city,
                venue: venue_name,
                date_label,
                time_label,
                ticket_url: string_at(data, &["ticketUrl", "url", "tickets/url", "venue/url"]),
            })
        })
        .collect()
}

fn parse_merch(artist: &Value) -> Vec<ArtistMerch> {
    let paths = ["goods/merch/items", "goods/items", "merch/items"];
    let Some(items) = array_at(artist, &paths) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let data = unwrap_item(item);
            let name = string_at(data, &["nameV2", "name", "title"]).unwrap_or_default();
            if name.is_empty() {
                return None;
            }
            Some(ArtistMerch {
                name,
                description: string_at(data, &["description", "subtitle"]).unwrap_or_default(),
                price: string_at(data, &["price"]),
                image_url: image_at(
                    data,
                    &["image/sources", "images/items/0/sources", "imageUrl"],
                ),
                url: string_at(data, &["url", "externalUrl", "shopUrl"]),
            })
        })
        .collect()
}

fn parse_cities(artist: &Value) -> Vec<ArtistCity> {
    let Some(items) = array_at(artist, &["stats/topCities/items", "topCities/items"]) else {
        return Vec::new();
    };
    items
        .iter()
        .map(|item| ArtistCity {
            city: string_at(item, &["city", "name"]).unwrap_or_default(),
            country: string_at(item, &["country", "countryCode"]).unwrap_or_default(),
            listeners: u64_at(item, &["numberOfListeners", "listeners"]),
        })
        .collect()
}

fn gallery_images(artist: &Value) -> Vec<String> {
    let Some(items) = array_at(
        artist,
        &[
            "visuals/gallery/items",
            "visuals/galleryImages/items",
            "gallery/items",
        ],
    ) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| image_at(item, &["sources"]))
        .collect()
}

fn biography_at(artist: &Value) -> Option<String> {
    let pointer = [
        "profile/biography/text",
        "profile/bio/text",
        "biography/text",
        "profile/biography",
        "biography",
    ]
    .iter()
    .find_map(|pointer| value_at(artist, pointer))?;
    let text = pointer
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| {
            pointer
                .as_object()?
                .get("text")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .map(|text| html_to_text(&text));
    text.filter(|text| !text.trim().is_empty())
}

fn unwrap_item(value: &Value) -> &Value {
    let candidate = value.get("item").unwrap_or(value);
    let data = candidate.get("data").unwrap_or(candidate);
    if data.is_object() { data } else { candidate }
}

fn first_at<'a>(value: &'a Value, pointers: &[&str]) -> Option<&'a Value> {
    pointers.iter().find_map(|pointer| value_at(value, pointer))
}

fn array_at<'a>(value: &'a Value, pointers: &[&str]) -> Option<&'a Vec<Value>> {
    pointers
        .iter()
        .find_map(|pointer| value_at(value, pointer).and_then(Value::as_array))
}

fn string_at(value: &Value, pointers: &[&str]) -> Option<String> {
    pointers
        .iter()
        .find_map(|pointer| value_at(value, pointer).and_then(Value::as_str))
        .map(ToOwned::to_owned)
}

fn bool_at(value: &Value, pointers: &[&str]) -> bool {
    option_bool_at(value, pointers).unwrap_or(false)
}

fn option_bool_at(value: &Value, pointers: &[&str]) -> Option<bool> {
    pointers
        .iter()
        .find_map(|pointer| value_at(value, pointer).and_then(Value::as_bool))
}

fn u64_at(value: &Value, pointers: &[&str]) -> Option<u64> {
    pointers.iter().find_map(|pointer| {
        value_at(value, pointer)
            .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
    })
}

fn len_at(value: &Value, pointers: &[&str]) -> Option<u64> {
    u64_at(value, pointers)
}

/// serde_json requires pointer paths to start with `/`. Accept the shorter
/// `a/b` form used throughout this module.
fn value_at<'a>(value: &'a Value, pointer: &str) -> Option<&'a Value> {
    if pointer.is_empty() || pointer.starts_with('/') {
        return value.pointer(pointer);
    }
    let mut owned = String::with_capacity(pointer.len() + 1);
    owned.push('/');
    owned.push_str(pointer);
    value.pointer(&owned)
}

fn image_at(value: &Value, pointers: &[&str]) -> Option<String> {
    for pointer in pointers {
        if let Some(sources) = value_at(value, pointer).and_then(Value::as_array) {
            if let Some(url) = best_image(sources) {
                return Some(url);
            }
        }
        if let Some(url) = value_at(value, pointer).and_then(Value::as_str) {
            let normalized = image_url_from_id(url);
            if !normalized.is_empty() {
                return Some(normalized);
            }
        }
    }
    None
}

fn best_image(sources: &[Value]) -> Option<String> {
    sources
        .iter()
        .max_by_key(|source| {
            u64_at(source, &["width", "maxWidth"])
                .unwrap_or(0)
                .saturating_mul(u64_at(source, &["height", "maxHeight"]).unwrap_or(0))
        })
        .and_then(|source| source.get("url").and_then(Value::as_str))
        .filter(|url| !url.is_empty())
        .map(ToOwned::to_owned)
}

fn joined_names(value: &Value, pointers: &[&str]) -> String {
    let Some(items) = array_at(value, pointers) else {
        return String::new();
    };
    let names = items
        .iter()
        .filter_map(|item| {
            let data = unwrap_item(item);
            string_at(data, &["profile/name", "name", "data/profile/name"])
        })
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    names.join(", ")
}

fn card_kind_from_uri(uri: &str) -> ArtistCardKind {
    if uri.contains(":artist:") {
        ArtistCardKind::Artist
    } else if uri.contains(":playlist:") {
        ArtistCardKind::Playlist
    } else if uri.contains(":video:") {
        ArtistCardKind::Video
    } else {
        ArtistCardKind::Release
    }
}

fn kind_label(kind: ArtistCardKind) -> &'static str {
    match kind {
        ArtistCardKind::Release => "Release",
        ArtistCardKind::Playlist => "Playlist",
        ArtistCardKind::Artist => "Artist",
        ArtistCardKind::Video => "Video",
        ArtistCardKind::Merch => "Merch",
    }
}

fn image_url_from_id(value: &str) -> String {
    let value = value.trim();
    if value.starts_with("http://") || value.starts_with("https://") {
        value.to_string()
    } else if let Some(id) = value.strip_prefix("spotify:image:") {
        format!("{IMAGE_BASE}{id}")
    } else if value.len() == 40 && value.chars().all(|c| c.is_ascii_hexdigit()) {
        format!("{IMAGE_BASE}{value}")
    } else {
        String::new()
    }
}

fn html_to_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        format!("{}…", value.chars().take(max).collect::<String>())
    }
}

/// Print only JSON paths and array lengths. Values are deliberately excluded so a
/// diagnostic run never writes artist names, URIs, or other account content.
fn diagnose_schema(
    value: &Value,
    seen: &mut HashSet<String>,
    path: &str,
    depth: usize,
    filter: &str,
) {
    if depth > 11 {
        return;
    }
    let filtered = !filter.is_empty() && filter != "all";
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                if seen.insert(child_path.clone()) && (!filtered || child_path.contains(filter)) {
                    match child {
                        Value::Object(_) => eprintln!("artist-schema {child_path} object"),
                        Value::Array(items) => {
                            eprintln!("artist-schema {child_path}[] len={}", items.len());
                        }
                        Value::Null => eprintln!("artist-schema {child_path} null"),
                        Value::Bool(_) => eprintln!("artist-schema {child_path} bool"),
                        Value::Number(_) => eprintln!("artist-schema {child_path} number"),
                        Value::String(_) => eprintln!("artist-schema {child_path} string"),
                    }
                }
                diagnose_schema(child, seen, &child_path, depth + 1, filter);
            }
        }
        Value::Array(items) => {
            if let Some(first) = items.first() {
                diagnose_schema(first, seen, &format!("{path}[]"), depth + 1, filter);
            }
        }
        _ => {}
    }
}

/// Report the JSON kind (and string length only) of fields whose exact type is
/// ambiguous. No values are printed.
fn diagnose_scalars(value: &Value) {
    for path in [
        "data.artistUnion.headerImage",
        "data.artistUnion.visualIdentity.wideFullBleedImage",
        "data.artistUnion.profile.pinnedItem",
        "data.artistUnion.saved",
        "data.artistUnion.relatedContent.appearsOn.items",
        "data.artistUnion.relatedMusicVideos.items",
        "data.artistUnion.unmappedMusicVideos.items",
        "data.artistUnion.goods.concerts.items",
        "data.artistUnion.goods.merch.items",
        "data.artistUnion.profile.playlistsV2.items",
    ] {
        let Some(found) = value_at(value, &path.replace('.', "/")) else {
            eprintln!("artist-field {path} absent");
            continue;
        };
        let kind = match found {
            Value::Null => "null".to_string(),
            Value::Bool(_) => "bool".to_string(),
            Value::Number(_) => "number".to_string(),
            Value::String(text) => format!("string(len={})", text.len()),
            Value::Array(items) => format!("array(len={})", items.len()),
            Value::Object(map) => format!("object(keys={})", map.len()),
        };
        eprintln!("artist-field {path} {kind}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sparse_artist_overview_without_inventing_sections() {
        let json = json!({
            "data": {
                "artistUnion": {
                    "__typename": "Artist",
                    "uri": "spotify:artist:one",
                    "profile": { "name": "Example", "verified": true },
                    "visuals": {
                        "headerImage": { "sources": [
                            { "url": "https://img/small", "width": 320, "height": 180 },
                            { "url": "https://img/large", "width": 1280, "height": 720 }
                        ]},
                        "avatarImage": { "sources": [
                            { "url": "https://img/avatar", "width": 300, "height": 300 }
                        ]}
                    },
                    "stats": { "monthlyListeners": 1234, "followers": 55, "worldRank": 10 }
                }
            }
        });
        let profile =
            parse_artist(&json["data"]["artistUnion"], "spotify:artist:one").expect("artist");
        assert_eq!(profile.name, "Example");
        assert!(profile.verified);
        assert_eq!(
            profile.header_image_url.as_deref(),
            Some("https://img/large")
        );
        assert_eq!(
            profile.avatar_image_url.as_deref(),
            Some("https://img/avatar")
        );
        assert_eq!(profile.monthly_listeners, Some(1234));
        assert_eq!(profile.followers, Some(55));
        assert_eq!(profile.world_rank, Some(10));
        assert!(profile.popular_tracks.is_empty());
        assert!(profile.discography.albums.is_empty());
        assert!(profile.related_artists.is_empty());
    }

    #[test]
    fn maps_observed_overview_sections() {
        let json = json!({
            "data": { "artistUnion": {
                "__typename": "Artist",
                "uri": "spotify:artist:one",
                "headerImage": "0123456789abcdef0123456789abcdef01234567",
                "saved": true,
                "profile": {
                    "name": "Example",
                    "biography": { "text": "<p>Hello &amp; welcome</p>" },
                    "pinnedItem": {
                        "type": "album",
                        "comment": "<p>Listen to this</p>",
                        "subtitle": "Album",
                        "itemV2": { "data": {
                            "uri": "spotify:album:pick",
                            "name": "Pinned",
                            "coverArt": { "sources": [
                                { "url": "https://img/pick", "width": 300, "height": 300 }
                            ]}
                        }}
                    }
                },
                "visuals": {
                    "avatarImage": { "sources": [
                        { "url": "https://img/avatar", "width": 300, "height": 300 }
                    ]},
                    "gallery": { "items": [
                        { "sources": [{ "url": "https://img/gallery" }] }
                    ]}
                },
                "stats": {
                    "monthlyListeners": 999,
                    "followers": 88,
                    "worldRank": 7,
                    "topCities": { "items": [
                        { "city": "Madrid", "country": "ES", "numberOfListeners": 12 }
                    ]}
                },
                "discography": {
                    "topTracks": { "items": [
                        {
                            "track": {
                                "uri": "spotify:track:one",
                                "name": "Song",
                                "artists": { "items": [
                                    { "profile": { "name": "Example" } },
                                    { "profile": { "name": "Guest" } }
                                ]},
                                "albumOfTrack": {
                                    "uri": "spotify:album:record",
                                    "coverArt": { "sources": [
                                        { "url": "https://img/cover", "width": 300, "height": 300 }
                                    ]}
                                },
                                "duration": { "totalMilliseconds": 200000 },
                                "contentRating": { "label": "EXPLICIT" },
                                "playability": { "playable": false },
                                "associationsV3": { "videoAssociations": { "totalCount": 1 } }
                            }
                        }
                    ]},
                    "popularReleasesAlbums": { "totalCount": 2, "items": [
                        { "uri": "spotify:album:popular", "name": "Popular", "type": "ALBUM" }
                    ]},
                    "albums": { "totalCount": 3, "items": [
                        { "releases": { "items": [{
                            "uri": "spotify:album:one",
                            "name": "Album",
                            "type": "ALBUM",
                            "coverArt": { "sources": [{ "url": "https://img/album" }] }
                        }]}}
                    ]},
                    "singles": { "totalCount": 2, "items": [
                        { "releases": { "items": [{
                            "uri": "spotify:album:single",
                            "name": "Single",
                            "type": "SINGLE",
                            "coverArt": { "sources": [{ "url": "https://img/single" }] }
                        }]}}
                    ]},
                    "compilations": { "totalCount": 1, "items": [
                        { "releases": { "items": [{
                            "uri": "spotify:album:comp",
                            "name": "Compilation",
                            "type": "COMPILATION"
                        }]}}
                    ]}
                },
                "relatedContent": {
                    "relatedArtists": { "items": [
                        { "uri": "spotify:artist:two", "profile": { "name": "Other" } }
                    ]},
                    "featuringV2": { "items": [
                        { "data": {
                            "uri": "spotify:playlist:feat",
                            "name": "Featuring",
                            "description": "A playlist",
                            "images": { "items": [
                                { "sources": [{ "url": "https://img/feat" }] }
                            ]}
                        }}
                    ]},
                    "discoveredOnV2": { "items": [
                        { "data": { "uri": "spotify:playlist:disc", "name": "Discovered" } }
                    ]},
                    "appearsOn": { "items": [
                        { "releases": { "items": [{
                            "uri": "spotify:album:appears",
                            "name": "Appears",
                            "type": "ALBUM"
                        }], "totalCount": 1 }}
                    ]}
                },
                "relatedMusicVideos": { "items": [
                    { "video": {
                        "uri": "spotify:video:one",
                        "name": "Video",
                        "contentRating": { "label": "EXPLICIT" },
                        "thumbnail": { "sources": [{ "url": "https://img/video" }] }
                    }}
                ]},
                "goods": {
                    "concerts": { "items": [
                        { "data": {
                            "festival": false,
                            "title": "Live",
                            "location": { "city": "Barcelona", "name": "Venue" },
                            "startDateIsoString": "2026-06-03T20:00:00Z"
                        }}
                    ]},
                    "merch": { "items": [
                        { "nameV2": "Hoodie", "description": "Cotton", "price": "45 EUR", "url": "https://shop" }
                    ]}
                }
            }}
        });
        let profile =
            parse_artist(&json["data"]["artistUnion"], "spotify:artist:one").expect("artist");
        assert_eq!(profile.biography.as_deref(), Some("Hello & welcome"));
        assert_eq!(
            profile.header_image_url.as_deref(),
            Some("https://i.scdn.co/image/0123456789abcdef0123456789abcdef01234567")
        );
        assert_eq!(profile.following, Some(true));
        assert_eq!(
            profile.artist_pick.as_ref().map(|pick| pick.name.as_str()),
            Some("Pinned")
        );
        assert_eq!(
            profile
                .artist_pick
                .as_ref()
                .and_then(|pick| pick.note.as_deref()),
            Some("Listen to this")
        );
        assert_eq!(profile.gallery.len(), 1);
        assert_eq!(profile.top_cities.len(), 1);

        assert_eq!(profile.popular_tracks.len(), 1);
        let track = &profile.popular_tracks[0];
        assert_eq!(track.artists, "Example, Guest");
        assert!(track.explicit && track.music_video);
        assert!(!track.playable);
        assert_eq!(track.duration_ms, 200_000);

        assert_eq!(profile.discography.popular.len(), 1);
        assert_eq!(profile.discography.albums.len(), 1);
        assert_eq!(profile.discography.singles.len(), 1);
        assert_eq!(profile.discography.compilations.len(), 1);
        assert_eq!(profile.discography.albums_total, Some(3));

        assert_eq!(profile.related_artists[0].name, "Other");
        assert_eq!(profile.featuring[0].name, "Featuring");
        assert_eq!(profile.discovered_on[0].name, "Discovered");
        assert_eq!(profile.appears_on[0].name, "Appears");
        assert_eq!(profile.music_videos[0].title, "Video");
        assert_eq!(profile.concerts[0].venue, "Venue");
        assert_eq!(profile.concerts[0].city, "Barcelona");
        assert_eq!(profile.concerts[0].date_label, "2026-06-03T20:00:00Z");
        assert_eq!(profile.merchandise[0].name, "Hoodie");
        assert_eq!(profile.merchandise[0].price.as_deref(), Some("45 EUR"));
    }

    #[test]
    fn non_artist_union_is_rejected() {
        let json = json!({ "__typename": "Album", "uri": "spotify:album:x", "name": "x" });
        assert!(parse_artist(&json, "spotify:artist:x").is_none());
    }

    #[test]
    fn missing_name_is_rejected() {
        let json = json!({ "__typename": "Artist", "uri": "spotify:artist:x" });
        assert!(parse_artist(&json, "spotify:artist:x").is_none());
    }
}
