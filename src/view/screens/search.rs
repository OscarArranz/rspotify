//! Search results backed by Spotify's own Pathfinder GraphQL endpoint.

use futures::channel::oneshot;
use gpui::{
    AnyElement, App, AppContext, Context, InteractiveElement, IntoElement, ListAlignment,
    ListState, ParentElement, Render, ScrollHandle, StatefulInteractiveElement, Styled, Window,
    div, img, list, px, rgb, svg,
};

use crate::lib::spotify::{NativeSpotify, SearchItem, SearchResults};
use crate::router::StackRouterHandle;
use crate::state::AppState;
use crate::view::components::smooth_scroll::{
    SmoothScrollElement, SmoothScrollState, list_scroll_overlay, nested_scroll_region,
};
use crate::view::components::track_menu::{TrackMenuTarget, track_menu};
use crate::view::components::track_title::{open_album, track_title};
use crate::view::screens::artist::{ArtistScreen, ArtistSource};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchTab {
    All,
    Songs,
    Artists,
    Albums,
    Playlists,
    Podcasts,
}

impl SearchTab {
    const ALL: [Self; 6] = [
        Self::All,
        Self::Songs,
        Self::Artists,
        Self::Albums,
        Self::Playlists,
        Self::Podcasts,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Songs => "Songs",
            Self::Artists => "Artists",
            Self::Albums => "Albums",
            Self::Playlists => "Playlists",
            Self::Podcasts => "Podcasts & Shows",
        }
    }
}

enum SearchStatus {
    Idle,
    Loading,
    Loaded,
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchRow {
    Tabs,
    Status,
    Heading(&'static str),
    TopResult,
    Carousel(SearchTab),
    Track(usize),
}

pub(crate) struct SearchScreen {
    query: String,
    tab: SearchTab,
    client: NativeSpotify,
    results: SearchResults,
    status: SearchStatus,
    request_generation: u64,
    vertical_list: ListState,
    rows: Vec<SearchRow>,
    featured_scroll: ScrollHandle,
    playlist_scroll: ScrollHandle,
    podcast_scroll: ScrollHandle,
    artist_scroll: ScrollHandle,
    vertical_scroll_state: SmoothScrollState,
    featured_scroll_state: SmoothScrollState,
    playlist_scroll_state: SmoothScrollState,
    podcast_scroll_state: SmoothScrollState,
    artist_scroll_state: SmoothScrollState,
}

impl SearchScreen {
    pub(crate) fn new(
        query: String,
        tab: SearchTab,
        client: NativeSpotify,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut screen = Self {
            query,
            tab,
            client,
            results: SearchResults::default(),
            status: SearchStatus::Idle,
            request_generation: 0,
            vertical_list: ListState::new(1, ListAlignment::Top, px(200.0)),
            rows: Vec::new(),
            featured_scroll: ScrollHandle::new(),
            playlist_scroll: ScrollHandle::new(),
            podcast_scroll: ScrollHandle::new(),
            artist_scroll: ScrollHandle::new(),
            vertical_scroll_state: SmoothScrollState::new(cx.entity_id()),
            featured_scroll_state: SmoothScrollState::new(cx.entity_id()),
            playlist_scroll_state: SmoothScrollState::new(cx.entity_id()),
            podcast_scroll_state: SmoothScrollState::new(cx.entity_id()),
            artist_scroll_state: SmoothScrollState::new(cx.entity_id()),
        };
        screen.fetch_results(cx);
        screen
    }

    fn select_tab(&mut self, tab: SearchTab, cx: &mut Context<Self>) {
        if self.tab != tab {
            self.tab = tab;
            self.reset_list();
            cx.notify();
        }
    }

    fn fetch_results(&mut self, cx: &mut Context<Self>) {
        let query = self.query.clone();
        let request_query = query.clone();
        let tab = self.tab;
        let client = self.client.clone();
        self.request_generation = self.request_generation.wrapping_add(1);
        let generation = self.request_generation;
        self.status = SearchStatus::Loading;
        self.results = SearchResults::default();
        self.reset_list();
        cx.notify();

        cx.spawn(async move |this, cx| {
            let (tx, rx) = oneshot::channel();
            std::thread::spawn(move || {
                let result = client.search(&request_query);
                let _ = tx.send(result);
            });

            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| {
                    if this.query != query
                        || this.tab != tab
                        || this.request_generation != generation
                    {
                        return;
                    }
                    match result {
                        Ok(results) => {
                            this.results = results;
                            this.status = SearchStatus::Loaded;
                        }
                        Err(error) => this.status = SearchStatus::Error(error.to_string()),
                    }
                    this.reset_list();
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn reset_list(&mut self) {
        self.rows = search_rows(self.tab, &self.status, &self.results);
        self.vertical_scroll_state.cancel();
        self.vertical_list.reset(self.rows.len());
    }
}

fn artwork(image: &Option<String>, size: f32) -> gpui::AnyElement {
    match image {
        Some(url) => img(url.clone())
            .size(px(size))
            .rounded(px(6.0))
            .into_any_element(),
        None => div()
            .size(px(size))
            .rounded(px(6.0))
            .bg(rgb(0x282828))
            .into_any_element(),
    }
}

fn track_row(item: &SearchItem, index: usize, client: &NativeSpotify) -> impl IntoElement {
    let client = client.clone();
    let uri = item.uri.clone();
    let is_song = matches!(item.kind, "Song" | "Episode");
    let row = div()
        .id(("search-track", index))
        .flex()
        .items_center()
        .gap(px(12.0))
        .px(px(10.0))
        .py(px(7.0))
        .rounded(px(5.0))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x292929)))
        .on_click(move |_, _, _| {
            if is_song {
                let _ = client.play_track(&uri);
            } else {
                let _ = client.play_context(&uri);
            }
        })
        .child(artwork(&item.image, 40.0))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.0))
                .child(track_title(&item.title, &item.uri, None))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0xb3b3b3))
                        .child(format!("{} · {}", item.kind, item.subtitle)),
                ),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(0xb3b3b3))
                .child(item.duration_ms.map(format_duration).unwrap_or_default()),
        );
    track_menu(
        row,
        format!("search-track-menu-{index}"),
        TrackMenuTarget::new(item.uri.clone(), item.title.clone()),
        None,
    )
}

fn album_card(item: &SearchItem, index: usize, client: &NativeSpotify) -> impl IntoElement {
    let client = client.clone();
    let uri = item.uri.clone();
    let title = item.title.clone();
    let image = item.image.clone();
    let is_artist = item.kind == "Artist";
    let is_album = item.kind == "Album";
    div()
        .id((item.kind, index))
        .w(px(148.0))
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(10.0))
        .rounded(px(6.0))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x242424)))
        .on_click(move |_, _, cx| {
            if is_artist {
                open_artist_profile(&uri, &title, image.clone(), cx);
            } else if is_album {
                open_album(&uri, cx);
            } else {
                let _ = client.play_context(&uri);
            }
        })
        .child(artwork(&item.image, 128.0))
        .child(item.title.clone())
        .child(
            div()
                .text_sm()
                .text_color(rgb(0xb3b3b3))
                .child(match item.kind {
                    "Artist" => "Artist".to_string(),
                    "Playlist" => format!("By {}", item.subtitle),
                    _ => format!("By {}", item.subtitle),
                }),
        )
}

/// Open a dedicated artist profile from search without starting playback.
pub(crate) fn open_artist_profile(uri: &str, name: &str, image: Option<String>, cx: &mut App) {
    let Some(client) = cx
        .try_global::<AppState>()
        .and_then(AppState::spotify_client)
    else {
        return;
    };
    let Some(router) = cx.try_global::<StackRouterHandle>().cloned() else {
        return;
    };
    if router.current_source(cx) == Some(uri) {
        return;
    }
    let source = ArtistSource::new(uri.to_string(), name.to_string(), image);
    let screen = cx.new(|cx| ArtistScreen::new(source, client, cx));
    router.push_for_source(screen.into(), uri.to_string(), cx);
}

fn search_rows(tab: SearchTab, status: &SearchStatus, results: &SearchResults) -> Vec<SearchRow> {
    let mut rows = vec![SearchRow::Tabs];
    if !matches!(status, SearchStatus::Loaded) || results.is_empty() {
        rows.push(SearchRow::Status);
    }
    if tab == SearchTab::All && results.top_result.is_some() {
        rows.extend([SearchRow::Heading("Top result"), SearchRow::TopResult]);
    }
    for (category, title, items) in [
        (SearchTab::Albums, "Featuring", &results.albums),
        (SearchTab::Songs, "Songs", &results.tracks),
        (SearchTab::Playlists, "Playlists", &results.playlists),
        (SearchTab::Podcasts, "Podcasts & Shows", &results.podcasts),
        (SearchTab::Artists, "Artists", &results.artists),
    ] {
        if (tab == SearchTab::All || tab == category) && !items.is_empty() {
            rows.push(SearchRow::Heading(title));
            if category == SearchTab::Songs {
                rows.extend((0..items.len().min(10)).map(SearchRow::Track));
            } else {
                rows.push(SearchRow::Carousel(category));
            }
        }
    }
    rows
}

impl SearchScreen {
    fn render_row(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let content = match self.rows[index] {
            SearchRow::Tabs => div()
                .flex()
                .gap(px(8.0))
                .children(SearchTab::ALL.into_iter().map(|tab| {
                    let selected = self.tab == tab;
                    div()
                        .id(tab.label())
                        .px(px(14.0))
                        .py(px(8.0))
                        .rounded(px(18.0))
                        .cursor_pointer()
                        .bg(rgb(if selected { 0xffffff } else { 0x292929 }))
                        .text_color(rgb(if selected { 0x000000 } else { 0xffffff }))
                        .text_sm()
                        .on_click(cx.listener(move |this, _, _, cx| this.select_tab(tab, cx)))
                        .child(tab.label())
                }))
                .into_any_element(),
            SearchRow::Status => {
                let message = match &self.status {
                    SearchStatus::Idle | SearchStatus::Loading => "Searching…".to_string(),
                    SearchStatus::Loaded => "No results found.".to_string(),
                    SearchStatus::Error(error) => format!("Search failed: {error}"),
                };
                div()
                    .py(px(24.0))
                    .text_color(rgb(0xb3b3b3))
                    .child(message)
                    .into_any_element()
            }
            SearchRow::Heading(title) => section_heading(title, &self.query).into_any_element(),
            SearchRow::TopResult => top_result_card(
                self.results.top_result.as_ref().expect("top result row"),
                &self.client,
            ),
            SearchRow::Track(track) => {
                track_row(&self.results.tracks[track], track, &self.client).into_any_element()
            }
            SearchRow::Carousel(category) => {
                let (id, items, handle, motion) = match category {
                    SearchTab::Albums => (
                        "featured-albums",
                        &self.results.albums,
                        &self.featured_scroll,
                        &self.featured_scroll_state,
                    ),
                    SearchTab::Playlists => (
                        "playlist-results",
                        &self.results.playlists,
                        &self.playlist_scroll,
                        &self.playlist_scroll_state,
                    ),
                    SearchTab::Podcasts => (
                        "podcast-results",
                        &self.results.podcasts,
                        &self.podcast_scroll,
                        &self.podcast_scroll_state,
                    ),
                    SearchTab::Artists => (
                        "artist-results",
                        &self.results.artists,
                        &self.artist_scroll,
                        &self.artist_scroll_state,
                    ),
                    _ => unreachable!("only card categories have carousels"),
                };
                div()
                    .id(id)
                    .relative()
                    .flex()
                    .gap(px(8.0))
                    .overflow_x_scroll()
                    .smooth_scroll(handle.clone(), motion.clone())
                    .child(nested_scroll_region(
                        handle.clone(),
                        Some(motion.clone()),
                        self.vertical_scroll_state.clone(),
                    ))
                    .children(
                        items
                            .iter()
                            .take(8)
                            .enumerate()
                            .map(|(i, item)| album_card(item, i, &self.client)),
                    )
                    .into_any_element()
            }
        };
        // Preserve the original page padding and 20px gap between direct children.
        div()
            .w_full()
            .px(px(24.0))
            .pt(px(if index == 0 { 24.0 } else { 20.0 }))
            .pb(px(if index + 1 == self.rows.len() {
                24.0
            } else {
                0.0
            }))
            .child(content)
            .into_any_element()
    }
}

fn top_result_card(item: &SearchItem, client: &NativeSpotify) -> AnyElement {
    let top_uri = item.uri.clone();
    let top_title = item.title.clone();
    let top_image = item.image.clone();
    let top_is_song = matches!(item.kind, "Song" | "Episode");
    let top_is_artist = item.kind == "Artist";
    let top_is_album = item.kind == "Album";
    let album_play_client = client.clone();
    let album_play_uri = item.uri.clone();
    let client = client.clone();
    let card = div()
        .id("top-result-card")
        .flex()
        .items_center()
        .gap(px(16.0))
        .p(px(16.0))
        .rounded(px(8.0))
        .bg(rgb(0x292929))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x333333)))
        .on_click(move |_, _, cx| {
            if top_is_artist {
                open_artist_profile(&top_uri, &top_title, top_image.clone(), cx);
            } else if top_is_song {
                let _ = client.play_track(&top_uri);
            } else if top_is_album {
                open_album(&top_uri, cx);
            } else {
                let _ = client.play_context(&top_uri);
            }
        })
        .child(artwork(&item.image, 72.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .min_w(px(0.0))
                .child(track_title(&item.title, &item.uri, None))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0xb3b3b3))
                        .child(item.subtitle.clone()),
                ),
        )
        .child(div().flex_1())
        .child(
            div()
                .id("top-result-play")
                .size(px(44.0))
                .rounded_full()
                .bg(rgb(0x1ed760))
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(0x000000))
                .font_weight(gpui::FontWeight::BOLD)
                .on_click(move |_, _, cx| {
                    if top_is_album {
                        cx.stop_propagation();
                        let _ = album_play_client.play_context(&album_play_uri);
                    }
                })
                .child(
                    svg()
                        .path("icons/play.svg")
                        .size(px(18.0))
                        .text_color(rgb(0x000000)),
                ),
        );
    track_menu(
        card,
        "search-top-track-menu",
        TrackMenuTarget::new(item.uri.clone(), item.title.clone()),
        None,
    )
}

impl Render for SearchScreen {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("search-content")
            .relative()
            .size_full()
            .bg(rgb(0x121212))
            .rounded_lg()
            .overflow_hidden()
            .child(
                list(
                    self.vertical_list.clone(),
                    cx.processor(|this, index, _, cx| this.render_row(index, cx)),
                )
                .size_full(),
            )
            .child(list_scroll_overlay(
                self.vertical_list.clone(),
                self.vertical_scroll_state.clone(),
                |_, _| {},
            ))
    }
}

fn format_duration(duration_ms: u64) -> String {
    let total_seconds = duration_ms / 1000;
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}

fn section_heading(title: &str, query: &str) -> impl IntoElement {
    div()
        .text_xl()
        .font_weight(gpui::FontWeight::BOLD)
        .child(if title == "Featuring" {
            format!("Featuring {query}")
        } else {
            title.to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_tabs_keep_sections_isolated_and_track_cap_when_results_shrink() {
        let mut results = SearchResults {
            top_result: Some(SearchItem::default()),
            tracks: vec![SearchItem::default(); 100],
            albums: vec![SearchItem::default(); 100],
            artists: vec![SearchItem::default(); 100],
            ..Default::default()
        };
        let rows = search_rows(SearchTab::Songs, &SearchStatus::Loaded, &results);
        let mut expected = vec![SearchRow::Tabs, SearchRow::Heading("Songs")];
        expected.extend((0..10).map(SearchRow::Track));
        assert_eq!(rows, expected);
        assert_eq!(
            search_rows(SearchTab::Albums, &SearchStatus::Loaded, &results),
            vec![
                SearchRow::Tabs,
                SearchRow::Heading("Featuring"),
                SearchRow::Carousel(SearchTab::Albums)
            ],
        );
        results.tracks.truncate(1);
        assert_eq!(
            search_rows(SearchTab::Songs, &SearchStatus::Loaded, &results),
            vec![
                SearchRow::Tabs,
                SearchRow::Heading("Songs"),
                SearchRow::Track(0)
            ],
        );
        assert_eq!(
            search_rows(
                SearchTab::All,
                &SearchStatus::Loaded,
                &SearchResults::default()
            ),
            vec![SearchRow::Tabs, SearchRow::Status],
        );
    }
}
