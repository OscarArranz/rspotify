//! Spotify-style artist profile screen.
//!
//! The page is one variable-height GPUI list so the hero scrolls away while long
//! discography and shelf sections stay virtualized. All data comes from the native
//! `queryArtistOverview` payload in `crate::lib::spotify::artist`.

use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc;
use gpui::{
    AnyElement, AppContext, Context, InteractiveElement, IntoElement, ListAlignment,
    ListScrollEvent, ListState, ObjectFit, ParentElement, Render, StatefulInteractiveElement,
    Styled, StyledImage, Window, div, img, linear_color_stop, linear_gradient, list, px, rgb, rgba,
    svg,
};

use crate::lib::spotify::{
    ArtistCard, ArtistCardKind, ArtistConcert, ArtistMerch, ArtistProfile, ArtistTrack,
    ArtistVideo, NativeSpotify, PlaybackSnapshot, PlaylistSource,
};
use crate::router::StackRouterHandle;
use crate::view::components::smooth_scroll::{SmoothScrollState, list_scroll_overlay};
use crate::view::screens::playlist::PlaylistScreen;

const HERO_HEIGHT: f32 = 340.0;
const ACTION_HEIGHT: f32 = 84.0;
const PANEL_BG: u32 = 0x121212;
const SECTION_GAP: f32 = 28.0;
const CARD_WIDTH: f32 = 168.0;
const POPULAR_COLLAPSED: usize = 5;
const DISCOGRAPHY_COLLAPSED: usize = 6;

/// Preview data available before the overview request finishes.
#[derive(Clone, Debug)]
pub(crate) struct ArtistSource {
    pub uri: String,
    pub name: String,
    pub image_url: Option<String>,
}

impl ArtistSource {
    pub(crate) fn new(
        uri: impl Into<String>,
        name: impl Into<String>,
        image_url: Option<String>,
    ) -> Self {
        Self {
            uri: uri.into(),
            name: name.into(),
            image_url,
        }
    }
}

#[derive(Clone, Debug)]
enum LoadState {
    Loading,
    Loaded,
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiscographyFilter {
    Popular,
    Albums,
    Singles,
}

impl DiscographyFilter {
    fn label(self) -> &'static str {
        match self {
            Self::Popular => "Popular releases",
            Self::Albums => "Albums",
            Self::Singles => "Singles and EPs",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtistSection {
    Popular,
    Liked,
    Pick,
    Discography,
    Featuring,
    Videos,
    Tour,
    About,
    Merch,
    DiscoveredOn,
    RelatedArtists,
    AppearsOn,
}

pub(crate) struct ArtistScreen {
    source: ArtistSource,
    client: NativeSpotify,
    state: LoadState,
    profile: Option<ArtistProfile>,
    sections: Vec<ArtistSection>,
    list_state: ListState,
    playback: PlaybackSnapshot,
    popular_expanded: bool,
    discography_filter: DiscographyFilter,
    discography_expanded: bool,
    about_expanded: bool,
    action_error: Option<String>,
    section_error: Option<String>,
    compact_visible: bool,
    request_generation: u64,
    scroll_animation: SmoothScrollState,
}

impl ArtistScreen {
    pub(crate) fn new(source: ArtistSource, client: NativeSpotify, cx: &mut Context<Self>) -> Self {
        let list_state = ListState::new(2, ListAlignment::Top, px(1600.0));
        let weak = cx.entity().downgrade();
        list_state.set_scroll_handler(move |event, _, cx| {
            if let Some(entity) = weak.upgrade() {
                let _ = entity.update(cx, |this, cx| this.on_scroll(event, cx));
            }
        });

        let mut this = Self {
            playback: client.playback(),
            source,
            client,
            state: LoadState::Loading,
            profile: None,
            sections: Vec::new(),
            list_state,
            popular_expanded: false,
            discography_filter: DiscographyFilter::Popular,
            discography_expanded: false,
            about_expanded: false,
            action_error: None,
            section_error: None,
            compact_visible: false,
            request_generation: 0,
            scroll_animation: SmoothScrollState::new(cx.entity_id()),
        };
        this.start_load(cx);
        this.start_playback_poll(cx);
        this
    }

    fn start_load(&mut self, cx: &mut Context<Self>) {
        self.state = LoadState::Loading;
        self.action_error = None;
        self.section_error = None;
        self.compact_visible = false;
        self.request_generation = self.request_generation.wrapping_add(1);
        let generation = self.request_generation;
        self.list_state.reset(self.item_count());
        self.scroll_animation.cancel();

        let client = self.client.clone();
        let uri = self.source.uri.clone();
        let (tx, mut rx) = mpsc::unbounded();
        std::thread::spawn(move || {
            let result = client.artist_profile(&uri);
            let _ = tx.unbounded_send(result);
        });

        cx.spawn(async move |this, cx| {
            while let Some(result) = rx.next().await {
                let _ = this.update(cx, |this, cx| {
                    if this.request_generation != generation {
                        return;
                    }
                    match result {
                        Ok(profile) => this.apply_profile(profile, cx),
                        Err(error) => {
                            if this.profile.is_some() {
                                this.section_error = Some(error.to_string());
                                this.state = LoadState::Loaded;
                            } else {
                                this.state = LoadState::Error(error.to_string());
                            }
                            cx.notify();
                        }
                    }
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_profile(&mut self, profile: ArtistProfile, cx: &mut Context<Self>) {
        if self.source.name.is_empty() {
            self.source.name = profile.name.clone();
        }
        if self.source.image_url.is_none() {
            self.source.image_url = profile
                .header_image_url
                .clone()
                .or_else(|| profile.avatar_image_url.clone());
        }
        self.sections = sections_for(&profile);
        if self.discography_filter == DiscographyFilter::Popular
            && profile.discography.popular.is_empty()
        {
            self.discography_filter = if !profile.discography.albums.is_empty() {
                DiscographyFilter::Albums
            } else {
                DiscographyFilter::Singles
            };
        }
        self.profile = Some(profile);
        self.state = LoadState::Loaded;
        self.section_error = None;
        self.list_state.reset(self.item_count());
        cx.notify();
    }

    fn start_playback_poll(&self, cx: &mut Context<Self>) {
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let snapshot = client.playback();
                let updated = this.update(cx, |this, cx| {
                    let relevant_change = this.playback.now_playing != snapshot.now_playing
                        || this.playback.is_playing != snapshot.is_playing
                        || this.playback.context_uri != snapshot.context_uri
                        || this.playback.shuffle != snapshot.shuffle;
                    this.playback = snapshot;
                    if relevant_change {
                        cx.notify();
                    }
                });
                if updated.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn item_count(&self) -> usize {
        if matches!(self.state, LoadState::Loaded) && self.profile.is_some() {
            2 + self.sections.len()
        } else {
            2
        }
    }

    fn on_scroll(&mut self, event: &ListScrollEvent, cx: &mut Context<Self>) {
        let compact = event.is_scrolled && event.visible_range.start > 0;
        if compact != self.compact_visible {
            self.compact_visible = compact;
            cx.notify();
        }
    }

    fn on_animated_scroll(&mut self, cx: &mut Context<Self>) {
        let top = self.list_state.logical_scroll_top().item_ix;
        let bottom = self.list_state.viewport_bounds().bottom();
        let mut end = top;
        while end < self.item_count() {
            match self.list_state.bounds_for_item(end) {
                Some(bounds) if bounds.top() < bottom => end += 1,
                _ => break,
            }
        }
        self.on_scroll(
            &ListScrollEvent {
                visible_range: top..end,
                count: self.item_count(),
                is_scrolled: top > 0,
            },
            cx,
        );
    }

    fn reset_preserving_scroll(&mut self) {
        let top = self.list_state.logical_scroll_top();
        self.scroll_animation.cancel();
        self.list_state.reset(self.item_count());
        self.list_state.scroll_to(top);
    }

    fn toggle_play(&mut self, cx: &mut Context<Self>) {
        let context_uri = self.source.uri.clone();
        let active = self.playback.context_uri.as_deref() == Some(context_uri.as_str());
        let result = if active && self.playback.is_playing {
            self.client.pause()
        } else if active {
            self.client.play()
        } else {
            self.client.play_context(&context_uri)
        };
        self.action_error = result.err().map(|error| error.to_string());
        cx.notify();
    }

    fn toggle_shuffle(&mut self, cx: &mut Context<Self>) {
        let result = self.client.set_shuffle(!self.playback.shuffle);
        self.action_error = result.err().map(|error| error.to_string());
        cx.notify();
    }

    fn play_track_at(&mut self, track_uri: &str, cx: &mut Context<Self>) {
        let result = self.client.play_context_at(&self.source.uri, track_uri);
        self.action_error = result.err().map(|error| error.to_string());
        cx.notify();
    }

    fn toggle_popular(&mut self, cx: &mut Context<Self>) {
        self.popular_expanded = !self.popular_expanded;
        self.reset_preserving_scroll();
        cx.notify();
    }

    fn toggle_discography(&mut self, cx: &mut Context<Self>) {
        self.discography_expanded = !self.discography_expanded;
        self.reset_preserving_scroll();
        cx.notify();
    }

    fn select_discography(&mut self, filter: DiscographyFilter, cx: &mut Context<Self>) {
        if self.discography_filter == filter {
            return;
        }
        self.discography_filter = filter;
        self.discography_expanded = false;
        self.reset_preserving_scroll();
        cx.notify();
    }

    fn toggle_about(&mut self, cx: &mut Context<Self>) {
        self.about_expanded = !self.about_expanded;
        self.reset_preserving_scroll();
        cx.notify();
    }

    fn set_action_error(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.action_error = Some(message.into());
        cx.notify();
    }

    fn open_artist(&mut self, card: &ArtistCard, cx: &mut Context<Self>) {
        let Some(router) = cx.try_global::<StackRouterHandle>().cloned() else {
            return;
        };
        if card.uri.is_empty() || router.current_source(cx) == Some(card.uri.as_str()) {
            return;
        }
        let source = ArtistSource::new(card.uri.clone(), card.name.clone(), card.image_url.clone());
        let client = self.client.clone();
        let screen = cx.new(|cx| ArtistScreen::new(source, client, cx));
        router.push_for_source(screen.into(), card.uri.clone(), cx);
    }

    fn open_context(&mut self, card: &ArtistCard, cx: &mut Context<Self>) {
        let Some(router) = cx.try_global::<StackRouterHandle>().cloned() else {
            return;
        };
        if card.uri.is_empty() || router.current_source(cx) == Some(card.uri.as_str()) {
            return;
        }
        let kind = match card.kind {
            ArtistCardKind::Playlist => "Playlist",
            ArtistCardKind::Release => "Album",
            _ => "Context",
        };
        let source = PlaylistSource::Context {
            uri: card.uri.clone(),
            name: card.name.clone(),
            owner: if card.subtitle.is_empty() {
                "Spotify".to_string()
            } else {
                card.subtitle.clone()
            },
            image_url: card.image_url.clone(),
            kind,
        };
        let client = self.client.clone();
        let screen = cx.new(|cx| PlaylistScreen::new(source, client, cx));
        router.push_for_source(screen.into(), card.uri.clone(), cx);
    }

    fn open_card(&mut self, card: &ArtistCard, cx: &mut Context<Self>) {
        if card.uri.starts_with("spotify:track:") {
            let result = self.client.play_track(&card.uri);
            self.action_error = result.err().map(|error| error.to_string());
            cx.notify();
            return;
        }
        match card.kind {
            ArtistCardKind::Artist => self.open_artist(card, cx),
            ArtistCardKind::Video => {
                self.set_action_error("Video playback is not available in rspotify.", cx)
            }
            ArtistCardKind::Merch => {
                self.set_action_error("Merch checkout is not available in rspotify.", cx)
            }
            ArtistCardKind::Release | ArtistCardKind::Playlist => self.open_context(card, cx),
        }
    }

    fn render_track_row(
        &self,
        index: usize,
        track: &ArtistTrack,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let playing = self
            .playback
            .now_playing
            .as_ref()
            .is_some_and(|now| now.uri == track.uri);
        let title = track.title.clone();
        let artists = track.artists.clone();
        let uri = track.uri.clone();
        let duration = format_duration(track.duration_ms);
        let liked = track.liked;
        let explicit = track.explicit;
        let has_video = track.music_video;

        div()
            .id(("artist-track", index))
            .flex()
            .items_center()
            .gap(px(14.0))
            .h(px(56.0))
            .px(px(12.0))
            .rounded(px(5.0))
            .cursor_pointer()
            .hover(|style| style.bg(rgba(0xffffff12)))
            .on_click(cx.listener(move |this, _, _, cx| this.play_track_at(&uri, cx)))
            .child(
                div()
                    .w(px(20.0))
                    .flex()
                    .justify_end()
                    .text_size(px(15.0))
                    .text_color(rgb(0xb3b3b3))
                    .child(if playing {
                        equalizer()
                    } else {
                        div().child((index + 1).to_string()).into_any_element()
                    }),
            )
            .child(track_artwork(track, 40.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .text_color(rgb(if playing { 0x1ed760 } else { 0xffffff }))
                            .truncate()
                            .child(title),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(13.0))
                            .text_color(rgb(0xb3b3b3))
                            .children(explicit.then(explicit_badge))
                            .children(has_video.then(|| video_badge()))
                            .child(div().truncate().child(artists)),
                    ),
            )
            .child(
                svg()
                    .path("icons/heart.svg")
                    .size(px(16.0))
                    .text_color(if liked { rgb(0x1ed760) } else { rgb(0xb3b3b3) }),
            )
            .child(
                div()
                    .w(px(46.0))
                    .text_size(px(13.0))
                    .text_color(rgb(0xb3b3b3))
                    .text_align(gpui::TextAlign::Right)
                    .child(duration),
            )
            .child(
                svg()
                    .path("icons/more.svg")
                    .size(px(16.0))
                    .text_color(rgb(0xb3b3b3)),
            )
            .into_any_element()
    }

    fn render_popular(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(profile) = self.profile.as_ref() else {
            return div().into_any_element();
        };
        let tracks = &profile.popular_tracks;
        let visible = if self.popular_expanded {
            tracks.len()
        } else {
            tracks.len().min(POPULAR_COLLAPSED)
        };
        let can_expand = tracks.len() > POPULAR_COLLAPSED;
        let mut rows = Vec::with_capacity(visible);
        for (index, track) in tracks.iter().take(visible).enumerate() {
            rows.push(self.render_track_row(index, track, cx));
        }
        let expanded = self.popular_expanded;

        section(
            "Popular",
            can_expand.then(|| {
                text_button(
                    "popular-toggle",
                    if expanded { "Show less" } else { "See more" },
                    cx.listener(|this, _, _, cx| this.toggle_popular(cx)),
                )
            }),
            div().flex().flex_col().children(rows).into_any_element(),
        )
    }

    fn render_liked(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(liked) = self
            .profile
            .as_ref()
            .and_then(|profile| profile.liked_music.clone())
        else {
            return div().into_any_element();
        };
        let description = match (liked.track_count, liked.release_count) {
            (Some(tracks), Some(releases)) => {
                format!(
                    "{} liked songs · {} releases",
                    format_count(tracks),
                    format_count(releases)
                )
            }
            (Some(tracks), None) => format!("{} liked songs", format_count(tracks)),
            (None, Some(releases)) => format!("{} liked releases", format_count(releases)),
            (None, None) => "Music you liked".to_string(),
        };
        let card = ArtistCard {
            uri: liked.uri.clone().unwrap_or_default(),
            name: "You Liked".to_string(),
            subtitle: description,
            year: None,
            note: None,
            image_url: liked.image_url.clone(),
            kind: ArtistCardKind::Playlist,
        };
        section(
            "You Liked",
            None,
            div()
                .child(self.render_wide_card(&card, "icons/heart.svg", cx))
                .into_any_element(),
        )
    }

    fn render_pick(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(card) = self
            .profile
            .as_ref()
            .and_then(|profile| profile.artist_pick.clone())
        else {
            return div().into_any_element();
        };
        let note = card.note.clone();
        let cover = card.image_url.clone();
        let name = card.name.clone();
        let subtitle = card.subtitle.clone();
        let uri = card.uri.clone();
        let kind = card.kind;

        let mut body = div()
            .id("artist-pick")
            .flex()
            .items_center()
            .gap(px(18.0))
            .p(px(16.0))
            .rounded(px(8.0))
            .bg(rgba(0xffffff0d))
            .cursor_pointer()
            .hover(|style| style.bg(rgba(0xffffff17)))
            .child(cover_art(cover.as_deref(), 96.0, false))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(0x1ed760))
                            .child("ARTIST PICK"),
                    )
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .truncate()
                            .child(name),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(0xb3b3b3))
                            .child(subtitle),
                    )
                    .children(note.map(|note| {
                        div()
                            .pt(px(4.0))
                            .text_size(px(13.0))
                            .text_color(rgb(0xd6d6d6))
                            .child(note)
                    })),
            );

        let card = ArtistCard {
            uri,
            name: String::new(),
            subtitle: String::new(),
            year: None,
            note: None,
            image_url: None,
            kind,
        };
        body = body.on_click(cx.listener(move |this, _, _, cx| this.open_card(&card, cx)));

        section("Artist pick", None, body.into_any_element())
    }

    fn render_discography(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(discography) = self
            .profile
            .as_ref()
            .map(|profile| profile.discography.clone())
        else {
            return div().into_any_element();
        };
        let groups = [
            (DiscographyFilter::Popular, &discography.popular),
            (DiscographyFilter::Albums, &discography.albums),
            (DiscographyFilter::Singles, &discography.singles),
        ];
        let filters = div().flex().items_center().gap(px(8.0)).children(
            groups
                .iter()
                .filter(|(_, items)| !items.is_empty())
                .enumerate()
                .map(|(index, (filter, _))| {
                    let selected = *filter == self.discography_filter;
                    let filter = *filter;
                    div()
                        .id(("discography-filter", index))
                        .px(px(12.0))
                        .py(px(6.0))
                        .rounded(px(14.0))
                        .cursor_pointer()
                        .bg(rgb(if selected { 0xffffff } else { 0x292929 }))
                        .text_color(rgb(if selected { 0x000000 } else { 0xffffff }))
                        .text_size(px(13.0))
                        .hover(|style| style.bg(rgb(if selected { 0xe6e6e6 } else { 0x333333 })))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.select_discography(filter, cx)),
                        )
                        .child(filter.label())
                }),
        );

        let items: Vec<ArtistCard> = match self.discography_filter {
            DiscographyFilter::Popular => discography.popular.clone(),
            DiscographyFilter::Albums => discography.albums.clone(),
            DiscographyFilter::Singles => discography.singles.clone(),
        };
        let collapsed = items.len() > DISCOGRAPHY_COLLAPSED;
        let shown = if self.discography_expanded || !collapsed {
            items.len()
        } else {
            DISCOGRAPHY_COLLAPSED
        };
        let cards = div().flex().flex_wrap().gap(px(16.0)).children(
            items
                .iter()
                .take(shown)
                .enumerate()
                .map(|(index, card)| self.render_card(card, index, false, cx)),
        );
        let expanded = self.discography_expanded;

        let body = div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(filters)
            .child(cards)
            .children((collapsed || expanded).then(|| {
                text_button(
                    "discography-toggle",
                    if expanded { "Show less" } else { "Show all" },
                    cx.listener(|this, _, _, cx| this.toggle_discography(cx)),
                )
            }));

        section("Discography", None, body.into_any_element())
    }

    fn render_shelf_section(
        &mut self,
        title: &'static str,
        cards: Vec<ArtistCard>,
        circular: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if cards.is_empty() {
            return div().into_any_element();
        }
        let shelf = div()
            .id(title)
            .flex()
            .gap(px(12.0))
            .overflow_x_scroll()
            .children(
                cards
                    .iter()
                    .enumerate()
                    .map(|(index, card)| self.render_card(card, index, circular, cx)),
            );
        section(title, None, shelf.into_any_element())
    }

    fn render_videos(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let videos = self
            .profile
            .as_ref()
            .map(|profile| profile.music_videos.clone())
            .unwrap_or_default();
        if videos.is_empty() {
            return div().into_any_element();
        }
        let shelf = div()
            .id("artist-videos")
            .flex()
            .gap(px(12.0))
            .overflow_x_scroll()
            .children(
                videos
                    .iter()
                    .enumerate()
                    .map(|(index, video)| self.render_video_card(video, index, cx)),
            );
        section("Music videos", None, shelf.into_any_element())
    }

    fn render_video_card(
        &mut self,
        video: &ArtistVideo,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = video.title.clone();
        let image = video.image_url.clone();
        let subtitle = if video.artists.is_empty() {
            "Video playback unavailable".to_string()
        } else {
            format!("{} · Video playback unavailable", video.artists)
        };
        let card = ArtistCard {
            uri: video.uri.clone(),
            name: title,
            subtitle,
            year: None,
            note: None,
            image_url: image,
            kind: ArtistCardKind::Video,
        };
        self.render_card_inner(card, index, false, Some("icons/video.svg"), None, cx)
    }

    fn render_tour(&mut self, _cx: &mut Context<Self>) -> AnyElement {
        let concerts = self
            .profile
            .as_ref()
            .map(|profile| profile.concerts.clone())
            .unwrap_or_default();
        if concerts.is_empty() {
            return div().into_any_element();
        }
        let rows = concerts
            .iter()
            .enumerate()
            .map(|(index, concert)| render_concert_row(index, concert));
        section(
            "On Tour",
            None,
            div().flex().flex_col().children(rows).into_any_element(),
        )
    }

    fn render_about(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(profile) = self.profile.as_ref() else {
            return div().into_any_element();
        };
        let biography = profile.biography.clone();
        let monthly = profile.monthly_listeners;
        let followers = profile.followers;
        let rank = profile.world_rank;
        let cities = profile.top_cities.clone();
        let gallery = profile.gallery.clone();
        let expanded = self.about_expanded;

        let can_expand = biography
            .as_ref()
            .is_some_and(|text| text.chars().count() > 620);
        let biography_text = biography.map(|text| {
            if expanded || text.chars().count() <= 620 {
                text
            } else {
                let mut truncated = text.chars().take(620).collect::<String>();
                truncated.push('…');
                truncated
            }
        });
        let stats = div()
            .flex()
            .flex_wrap()
            .gap(px(40.0))
            .children(monthly.map(|value| stat_block("Monthly listeners", format_count(value))))
            .children(followers.map(|value| stat_block("Followers", format_count(value))))
            .children(rank.map(|value| stat_block("World rank", format!("#{value}"))));

        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .p(px(20.0))
            .rounded(px(8.0))
            .bg(rgba(0xffffff0d))
            .child(
                div()
                    .text_size(px(15.0))
                    .text_color(rgb(0xd6d6d6))
                    .line_height(px(23.0))
                    .children(biography_text),
            )
            .child(stats);

        if !gallery.is_empty() {
            card = card.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(0xffffff))
                            .child("Photos"),
                    )
                    .child(
                        div()
                            .id("artist-gallery")
                            .flex()
                            .gap(px(8.0))
                            .overflow_x_scroll()
                            .children(
                                gallery
                                    .iter()
                                    .take(8)
                                    .map(|url| cover_art(Some(url), 104.0, false)),
                            ),
                    ),
            );
        }

        if !cities.is_empty() {
            let city_rows = cities.iter().take(5).enumerate().map(|(index, city)| {
                div()
                    .id(("artist-city", index))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .py(px(4.0))
                    .text_size(px(13.0))
                    .text_color(rgb(0xb3b3b3))
                    .child(
                        div()
                            .w(px(18.0))
                            .text_color(rgb(0x1ed760))
                            .child((index + 1).to_string()),
                    )
                    .child(div().flex_1().child(if city.country.is_empty() {
                        city.city.clone()
                    } else {
                        format!("{}, {}", city.city, city.country)
                    }))
                    .children(city.listeners.map(|listeners| {
                        div().child(format!("{} listeners", format_count(listeners)))
                    }))
            });
            card = card.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .pb(px(4.0))
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(0xffffff))
                            .child("Where people listen"),
                    )
                    .children(city_rows),
            );
        }

        card = card.children(can_expand.then(|| {
            text_button(
                "about-toggle",
                if expanded { "Show less" } else { "Read more" },
                cx.listener(|this, _, _, cx| this.toggle_about(cx)),
            )
        }));

        section("About", None, card.into_any_element())
    }

    fn render_merch(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let items = self
            .profile
            .as_ref()
            .map(|profile| profile.merchandise.clone())
            .unwrap_or_default();
        if items.is_empty() {
            return div().into_any_element();
        }
        let shelf = div()
            .id("artist-merch")
            .flex()
            .gap(px(12.0))
            .overflow_x_scroll()
            .children(
                items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| self.render_merch_card(item, index, cx)),
            );
        section("Merch", None, shelf.into_any_element())
    }

    fn render_merch_card(
        &mut self,
        item: &ArtistMerch,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let card = ArtistCard {
            uri: item.url.clone().unwrap_or_default(),
            name: item.name.clone(),
            subtitle: item.description.clone(),
            year: None,
            note: None,
            image_url: item.image_url.clone(),
            kind: ArtistCardKind::Merch,
        };
        self.render_card_inner(
            card,
            index,
            false,
            Some("icons/external.svg"),
            item.price.clone(),
            cx,
        )
    }

    fn render_card(
        &mut self,
        card: &ArtistCard,
        index: usize,
        circular: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_card_inner(card.clone(), index, circular, None, None, cx)
    }

    fn render_card_inner(
        &mut self,
        card: ArtistCard,
        index: usize,
        circular: bool,
        badge: Option<&'static str>,
        price: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let group = format!("artist-card-{}-{index}", card.kind as u8);
        let group_id = group.clone();
        let action_card = card.clone();
        let subtitle = if card.kind == ArtistCardKind::Artist {
            "Artist".to_string()
        } else if let Some(year) = card.year {
            year.to_string()
        } else if card.subtitle.is_empty() {
            match card.kind {
                ArtistCardKind::Playlist => "Playlist".to_string(),
                ArtistCardKind::Release => "Release".to_string(),
                ArtistCardKind::Video => "Video playback unavailable".to_string(),
                ArtistCardKind::Merch => "External store".to_string(),
                ArtistCardKind::Artist => "Artist".to_string(),
            }
        } else {
            card.subtitle.clone()
        };
        let image = card.image_url.clone();
        let name = card.name.clone();

        div()
            .id(("artist-card", card_id(&card.uri, index)))
            .group(group)
            .w(px(CARD_WIDTH))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(10.0))
            .rounded(px(8.0))
            .cursor_pointer()
            .hover(|style| style.bg(rgba(0xffffff10)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_card(&action_card, cx);
            }))
            .child(
                div()
                    .relative()
                    .size(px(CARD_WIDTH - 20.0))
                    .child(if circular {
                        artist_avatar(image.as_deref(), CARD_WIDTH - 20.0)
                    } else {
                        cover_art(image.as_deref(), CARD_WIDTH - 20.0, false)
                    })
                    .child(
                        div()
                            .absolute()
                            .right(px(8.0))
                            .bottom(px(8.0))
                            .size(px(38.0))
                            .rounded_full()
                            .bg(rgb(0x1ed760))
                            .flex()
                            .items_center()
                            .justify_center()
                            .opacity(0.0)
                            .group_hover(group_id, |style| style.opacity(1.0))
                            .child(
                                svg()
                                    .path(badge.unwrap_or("icons/play.svg"))
                                    .size(px(18.0))
                                    .text_color(rgb(0x000000)),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .truncate()
                    .child(name),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(rgb(0xb3b3b3))
                    .truncate()
                    .child(subtitle),
            )
            .children(price.map(|price| {
                div()
                    .text_size(px(13.0))
                    .text_color(rgb(0xffffff))
                    .child(price)
            }))
            .into_any_element()
    }

    fn render_wide_card(
        &mut self,
        card: &ArtistCard,
        icon: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let action_card = card.clone();
        let name = card.name.clone();
        let subtitle = card.subtitle.clone();
        let image = card.image_url.clone();
        let has_image = image.is_some();
        div()
            .id("artist-wide-card")
            .flex()
            .items_center()
            .gap(px(16.0))
            .p(px(14.0))
            .rounded(px(8.0))
            .bg(rgba(0xffffff0d))
            .cursor_pointer()
            .hover(|style| style.bg(rgba(0xffffff17)))
            .on_click(cx.listener(move |this, _, _, cx| this.open_card(&action_card, cx)))
            .child(
                div()
                    .size(px(64.0))
                    .rounded(px(8.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(if has_image {
                        rgb(0x282828)
                    } else {
                        rgb(0x2a1a4a)
                    })
                    .children(image.map(|url| {
                        img(url)
                            .size(px(64.0))
                            .rounded(px(8.0))
                            .object_fit(ObjectFit::Cover)
                    }))
                    .children(
                        (!has_image)
                            .then(|| svg().path(icon).size(px(24.0)).text_color(rgb(0x1ed760))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(name),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(0xb3b3b3))
                            .child(subtitle),
                    ),
            )
            .into_any_element()
    }

    fn render_hero(&self) -> AnyElement {
        let profile = self.profile.as_ref();
        let name = profile
            .map(|profile| profile.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| self.source.name.clone());
        let verified = profile.is_some_and(|profile| profile.verified);
        let monthly = profile.and_then(|profile| profile.monthly_listeners);
        let image = profile
            .and_then(|profile| {
                profile
                    .header_image_url
                    .clone()
                    .or_else(|| profile.avatar_image_url.clone())
            })
            .or_else(|| self.source.image_url.clone());

        let mut hero = div()
            .relative()
            .w_full()
            .h(px(HERO_HEIGHT))
            .overflow_hidden()
            .bg(linear_gradient(
                180.0,
                linear_color_stop(rgb(0x303030), 0.0),
                linear_color_stop(rgb(PANEL_BG), 1.0),
            ));
        if let Some(url) = image {
            hero = hero.child(
                img(url)
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .opacity(0.55),
            );
        }
        hero.child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(linear_gradient(
                    180.0,
                    linear_color_stop(rgba(0x00000000), 0.0),
                    linear_color_stop(rgb(PANEL_BG), 1.0),
                )),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .px(px(24.0))
                .pb(px(24.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .children(verified.then(|| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(rgb(0xffffff))
                        .child(
                            svg()
                                .path("icons/verified.svg")
                                .size(px(18.0))
                                .text_color(rgb(0x1ed760)),
                        )
                        .child("Verified artist")
                }))
                .child(
                    div()
                        .text_size(px(72.0))
                        .font_weight(gpui::FontWeight::BLACK)
                        .line_height(px(70.0))
                        .truncate()
                        .child(name),
                )
                .children(monthly.map(|listeners| {
                    div()
                        .text_size(px(14.0))
                        .text_color(rgb(0xd6d6d6))
                        .child(format!("{} monthly listeners", format_count(listeners)))
                })),
        )
        .into_any_element()
    }

    fn render_action_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let active = self.playback.context_uri.as_deref() == Some(self.source.uri.as_str());
        let playing = active && self.playback.is_playing;
        let shuffle_color = if self.playback.shuffle {
            rgb(0x1ed760)
        } else {
            rgb(0xb3b3b3)
        };

        div()
            .w_full()
            .h(px(ACTION_HEIGHT))
            .px(px(24.0))
            .flex()
            .items_center()
            .gap(px(18.0))
            .child(
                div()
                    .id("artist-play")
                    .size(px(56.0))
                    .rounded(px(28.0))
                    .bg(rgb(0x1ed760))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(0x3be477)))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_play(cx)))
                    .child(
                        svg()
                            .path(if playing {
                                "icons/pause.svg"
                            } else {
                                "icons/play.svg"
                            })
                            .size(px(24.0))
                            .text_color(rgb(0x000000)),
                    ),
            )
            .child(
                div()
                    .id("artist-shuffle")
                    .size(px(32.0))
                    .rounded(px(16.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|style| style.bg(rgba(0xffffff1a)))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_shuffle(cx)))
                    .child(
                        svg()
                            .path("icons/shuffle.svg")
                            .size(px(20.0))
                            .text_color(shuffle_color),
                    ),
            )
            // Follow/unfollow is intentionally absent: no verified native follow
            // contract is available, and the profile must not fake account state.
            .child(
                div()
                    .size(px(32.0))
                    .rounded(px(16.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .opacity(0.45)
                    .child(
                        svg()
                            .path("icons/more.svg")
                            .size(px(20.0))
                            .text_color(rgb(0xb3b3b3)),
                    ),
            )
            .child(div().flex_1())
            .children(self.action_error.as_ref().map(|error| {
                div()
                    .max_w(px(420.0))
                    .text_size(px(12.0))
                    .text_color(rgb(0xff9c9c))
                    .child(error.clone())
            }))
            .into_any_element()
    }

    fn render_status(&self, cx: &mut Context<Self>) -> AnyElement {
        let (title, message, retry) = match &self.state {
            LoadState::Loading => ("Loading artist…".to_string(), String::new(), false),
            LoadState::Error(error) => (
                "Could not load this artist".to_string(),
                error.clone(),
                true,
            ),
            LoadState::Loaded => (
                "No profile content".to_string(),
                "Spotify did not return profile details for this artist.".to_string(),
                true,
            ),
        };
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(10.0))
            .py(px(64.0))
            .px(px(24.0))
            .text_color(rgb(0xb3b3b3))
            .child(
                div()
                    .text_size(px(18.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(0xffffff))
                    .child(title),
            )
            .children((!message.is_empty()).then(|| {
                div()
                    .max_w(px(520.0))
                    .text_size(px(13.0))
                    .text_align(gpui::TextAlign::Center)
                    .child(message)
            }))
            .children(retry.then(|| {
                div()
                    .id("artist-retry")
                    .mt(px(6.0))
                    .px(px(16.0))
                    .py(px(8.0))
                    .rounded(px(16.0))
                    .bg(rgb(0xffffff))
                    .text_color(rgb(0x000000))
                    .text_size(px(14.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(0xe6e6e6)))
                    .on_click(cx.listener(|this, _, _, cx| this.start_load(cx)))
                    .child("Try again")
            }))
            .into_any_element()
    }

    fn render_compact_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let active = self.playback.context_uri.as_deref() == Some(self.source.uri.as_str());
        let playing = active && self.playback.is_playing;
        let name = self
            .profile
            .as_ref()
            .map(|profile| profile.name.clone())
            .unwrap_or_else(|| self.source.name.clone());
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(64.0))
            .px(px(24.0))
            .flex()
            .items_center()
            .gap(px(16.0))
            .bg(rgba(0x0b0b0bf2))
            .border_b_1()
            .border_color(rgba(0xffffff14))
            .child(
                div()
                    .id("artist-compact-play")
                    .size(px(38.0))
                    .rounded(px(19.0))
                    .bg(rgb(0x1ed760))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(0x3be477)))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_play(cx)))
                    .child(
                        svg()
                            .path(if playing {
                                "icons/pause.svg"
                            } else {
                                "icons/play.svg"
                            })
                            .size(px(18.0))
                            .text_color(rgb(0x000000)),
                    ),
            )
            .child(
                div()
                    .text_size(px(18.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .truncate()
                    .child(name),
            )
            .child(div().flex_1())
            .into_any_element()
    }

    fn render_section(&mut self, section: ArtistSection, cx: &mut Context<Self>) -> AnyElement {
        match section {
            ArtistSection::Popular => self.render_popular(cx),
            ArtistSection::Liked => self.render_liked(cx),
            ArtistSection::Pick => self.render_pick(cx),
            ArtistSection::Discography => self.render_discography(cx),
            ArtistSection::Featuring => {
                let cards = self
                    .profile
                    .as_ref()
                    .map(|profile| profile.featuring.clone())
                    .unwrap_or_default();
                self.render_shelf_section("Featuring", cards, false, cx)
            }
            ArtistSection::Videos => self.render_videos(cx),
            ArtistSection::Tour => self.render_tour(cx),
            ArtistSection::About => self.render_about(cx),
            ArtistSection::Merch => self.render_merch(cx),
            ArtistSection::DiscoveredOn => {
                let cards = self
                    .profile
                    .as_ref()
                    .map(|profile| profile.discovered_on.clone())
                    .unwrap_or_default();
                self.render_shelf_section("Discovered on", cards, false, cx)
            }
            ArtistSection::RelatedArtists => {
                let cards = self
                    .profile
                    .as_ref()
                    .map(|profile| profile.related_artists.clone())
                    .unwrap_or_default();
                self.render_shelf_section("Fans also like", cards, true, cx)
            }
            ArtistSection::AppearsOn => {
                let cards = self
                    .profile
                    .as_ref()
                    .map(|profile| profile.appears_on.clone())
                    .unwrap_or_default();
                self.render_shelf_section("Appears on", cards, false, cx)
            }
        }
    }

    fn render_item(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        match index {
            0 => self.render_hero(),
            1 if matches!(self.state, LoadState::Loaded) => self.render_action_bar(cx),
            1 => self.render_status(cx),
            _ => match self.sections.get(index - 2).copied() {
                Some(section) => {
                    let mut body = self.render_section(section, cx);
                    if index == 1 + self.sections.len() {
                        body = div().pb(px(40.0)).child(body).into_any_element();
                    }
                    body
                }
                None => div().into_any_element(),
            },
        }
    }
}

impl Render for ArtistScreen {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut page = div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(PANEL_BG))
            .rounded_lg()
            .overflow_hidden()
            .child(
                list(
                    self.list_state.clone(),
                    cx.processor(move |this, index, _, cx| this.render_item(index, cx)),
                )
                .size_full(),
            )
            .child(list_scroll_overlay(
                self.list_state.clone(),
                self.scroll_animation.clone(),
                {
                    let weak = cx.entity().downgrade();
                    move |_, cx| {
                        let _ = weak.update(cx, |this, cx| this.on_animated_scroll(cx));
                    }
                },
            ));

        if self.compact_visible {
            page = page.child(self.render_compact_bar(cx));
        }

        if let Some(error) = self.section_error.clone() {
            let top = if self.compact_visible { 76.0 } else { 12.0 };
            page = page.child(
                div()
                    .id("artist-section-error")
                    .absolute()
                    .top(px(top))
                    .right(px(16.0))
                    .max_w(px(440.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .px(px(14.0))
                    .py(px(10.0))
                    .rounded(px(8.0))
                    .bg(rgba(0x5a1f1ff2))
                    .text_size(px(12.0))
                    .text_color(rgb(0xffd6d6))
                    .child(div().flex_1().child(format!(
                        "Some artist details could not be refreshed: {error}"
                    )))
                    .child(
                        div()
                            .id("artist-dismiss-error")
                            .cursor_pointer()
                            .font_weight(gpui::FontWeight::BOLD)
                            .hover(|style| style.text_color(rgb(0xffffff)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.section_error = None;
                                cx.notify();
                            }))
                            .child("Dismiss"),
                    ),
            );
        }

        page
    }
}

fn sections_for(profile: &ArtistProfile) -> Vec<ArtistSection> {
    let mut sections = Vec::new();
    if !profile.popular_tracks.is_empty() {
        sections.push(ArtistSection::Popular);
    }
    if profile.liked_music.is_some() {
        sections.push(ArtistSection::Liked);
    }
    if profile.artist_pick.is_some() {
        sections.push(ArtistSection::Pick);
    }
    let discography = &profile.discography;
    if !discography.popular.is_empty()
        || !discography.albums.is_empty()
        || !discography.singles.is_empty()
    {
        sections.push(ArtistSection::Discography);
    }
    if !profile.featuring.is_empty() {
        sections.push(ArtistSection::Featuring);
    }
    if !profile.music_videos.is_empty() {
        sections.push(ArtistSection::Videos);
    }
    if !profile.concerts.is_empty() {
        sections.push(ArtistSection::Tour);
    }
    if profile.biography.is_some()
        || profile.monthly_listeners.is_some()
        || profile.followers.is_some()
        || profile.world_rank.is_some()
        || !profile.top_cities.is_empty()
    {
        sections.push(ArtistSection::About);
    }
    if !profile.merchandise.is_empty() {
        sections.push(ArtistSection::Merch);
    }
    if !profile.discovered_on.is_empty() {
        sections.push(ArtistSection::DiscoveredOn);
    }
    if !profile.related_artists.is_empty() {
        sections.push(ArtistSection::RelatedArtists);
    }
    if !profile.appears_on.is_empty() {
        sections.push(ArtistSection::AppearsOn);
    }
    sections
}

fn section(title: &'static str, action: Option<AnyElement>, body: AnyElement) -> AnyElement {
    div()
        .w_full()
        .px(px(24.0))
        .pb(px(SECTION_GAP))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(22.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(rgb(0xffffff))
                        .child(title),
                )
                .children(action),
        )
        .child(body)
        .into_any_element()
}

fn text_button(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .px(px(10.0))
        .py(px(4.0))
        .rounded(px(6.0))
        .text_size(px(13.0))
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(rgb(0xb3b3b3))
        .cursor_pointer()
        .hover(|style| style.text_color(rgb(0xffffff)))
        .on_click(on_click)
        .child(label)
        .into_any_element()
}

/// Element ids only accept `(str, usize)` tuples. Derive a stable number from the
/// card URI so repeated cards in different shelves never share state.
fn card_id(uri: &str, index: usize) -> usize {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in uri.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (hash as usize) ^ index.wrapping_mul(0x9e37_79b9)
}

fn cover_art(url: Option<&str>, size: f32, circular: bool) -> AnyElement {
    let rounded = if circular { px(size / 2.0) } else { px(4.0) };
    match url.filter(|url| !url.is_empty()) {
        Some(url) => {
            let mut image = img(url.to_string())
                .size(px(size))
                .flex_shrink_0()
                .object_fit(ObjectFit::Cover);
            image = if circular {
                image.rounded_full()
            } else {
                image.rounded(px(4.0))
            };
            image.into_any_element()
        }
        None => {
            let mut placeholder = div()
                .size(px(size))
                .rounded(rounded)
                .flex_shrink_0()
                .bg(linear_gradient(
                    135.0,
                    linear_color_stop(rgb(0x3a2d64), 0.0),
                    linear_color_stop(rgb(0x121212), 1.0),
                ))
                .flex()
                .items_center()
                .justify_center();
            placeholder = if circular {
                placeholder.rounded_full()
            } else {
                placeholder.rounded(px(4.0))
            };
            placeholder
                .child(
                    svg()
                        .path("icons/browse-outlined.svg")
                        .size(px(size * 0.3))
                        .text_color(rgb(0xffffff)),
                )
                .into_any_element()
        }
    }
}

fn artist_avatar(url: Option<&str>, size: f32) -> AnyElement {
    cover_art(url, size, true)
}

fn track_artwork(track: &ArtistTrack, size: f32) -> AnyElement {
    cover_art(track.image_url.as_deref(), size, false)
}

fn stat_block(label: &'static str, value: String) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .text_size(px(18.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(rgb(0xffffff))
                .child(value),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgb(0xb3b3b3))
                .child(label),
        )
        .into_any_element()
}

fn render_concert_row(index: usize, concert: &ArtistConcert) -> AnyElement {
    let (date, time) = format_concert_date(&concert.date_label);
    div()
        .id(("artist-concert", index))
        .flex()
        .items_center()
        .gap(px(16.0))
        .py(px(10.0))
        .border_b_1()
        .border_color(rgba(0xffffff12))
        .child(
            div()
                .w(px(56.0))
                .flex()
                .flex_col()
                .items_center()
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(rgb(0x1ed760))
                        .child(date),
                )
                .children((!time.is_empty()).then(|| {
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(0xb3b3b3))
                        .child(time)
                })),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(rgb(0xffffff))
                        .truncate()
                        .child(if concert.city.is_empty() {
                            concert.venue.clone()
                        } else {
                            concert.city.clone()
                        }),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(0xb3b3b3))
                        .truncate()
                        .child(concert.venue.clone()),
                ),
        )
        .child(
            div()
                .px(px(12.0))
                .py(px(6.0))
                .rounded(px(14.0))
                .border_1()
                .border_color(rgba(0xffffff3d))
                .text_size(px(12.0))
                .text_color(rgb(0xffffff))
                .child("Tickets"),
        )
        .into_any_element()
}

fn explicit_badge() -> AnyElement {
    div()
        .size(px(15.0))
        .rounded(px(2.0))
        .bg(rgba(0xffffff26))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(9.0))
        .text_color(rgb(0xb3b3b3))
        .child("E")
        .into_any_element()
}

fn video_badge() -> AnyElement {
    svg()
        .path("icons/video.svg")
        .size(px(14.0))
        .text_color(rgb(0xb3b3b3))
        .into_any_element()
}

fn equalizer() -> AnyElement {
    div()
        .flex()
        .items_end()
        .gap(px(2.0))
        .h(px(14.0))
        .w(px(16.0))
        .children([6.0, 12.0, 8.0, 14.0].map(|height| {
            div()
                .w(px(2.0))
                .h(px(height))
                .rounded(px(1.0))
                .bg(rgb(0x1ed760))
        }))
        .into_any_element()
}

fn format_duration(duration_ms: u64) -> String {
    if duration_ms == 0 {
        return String::new();
    }
    let total_seconds = duration_ms / 1000;
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}

fn format_count(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    let len = digits.len();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (len - index) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Format an ISO date as Spotify's short concert label.
fn format_concert_date(iso: &str) -> (String, String) {
    let mut parts = iso.split('T');
    let date = parts.next().unwrap_or("");
    let time = parts.next().unwrap_or("");
    let mut fields = date.split('-');
    let _year = fields.next();
    let month = fields.next().and_then(|month| month.parse::<usize>().ok());
    let day = fields.next().and_then(|day| day.parse::<u32>().ok());
    let month_name = match month {
        Some(1) => "JAN",
        Some(2) => "FEB",
        Some(3) => "MAR",
        Some(4) => "APR",
        Some(5) => "MAY",
        Some(6) => "JUN",
        Some(7) => "JUL",
        Some(8) => "AUG",
        Some(9) => "SEP",
        Some(10) => "OCT",
        Some(11) => "NOV",
        Some(12) => "DEC",
        _ => return (iso.to_string(), String::new()),
    };
    let day = day.unwrap_or(0);
    let time = time
        .get(0..5)
        .filter(|time| time.contains(':'))
        .unwrap_or("")
        .to_string();
    (format!("{month_name} {day}"), time)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_listener_counts_with_separators() {
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1_234), "1,234");
        assert_eq!(format_count(12_345_678), "12,345,678");
    }

    #[test]
    fn formats_concert_dates_and_rejects_unknown_input() {
        assert_eq!(
            format_concert_date("2026-06-03T20:00:00Z"),
            ("JUN 3".to_string(), "20:00".to_string())
        );
        assert_eq!(
            format_concert_date("soon"),
            ("soon".to_string(), String::new())
        );
    }

    #[test]
    fn sections_follow_reference_order_and_skip_missing_data() {
        let profile = ArtistProfile {
            popular_tracks: vec![ArtistTrack::default()],
            biography: Some("bio".into()),
            related_artists: vec![ArtistCard::default()],
            ..ArtistProfile::default()
        };
        assert_eq!(
            sections_for(&profile),
            vec![
                ArtistSection::Popular,
                ArtistSection::About,
                ArtistSection::RelatedArtists
            ]
        );
    }

    #[test]
    fn duration_formatting_matches_track_rows() {
        assert_eq!(format_duration(0), "");
        assert_eq!(format_duration(65_000), "1:05");
        assert_eq!(format_duration(3_600_000), "60:00");
    }
}
