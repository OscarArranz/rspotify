//! Search results backed by Spotify's own Pathfinder GraphQL endpoint.

use futures::channel::oneshot;
use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle,
    StatefulInteractiveElement, Styled, Window, div, img, prelude::FluentBuilder, px, rgb, svg,
};

use crate::lib::spotify::{NativeSpotify, SearchItem, SearchResults};
use crate::view::components::smooth_scroll::{ScrollAxis, SmoothScrollElement, SmoothScrollState};

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

pub(crate) struct SearchScreen {
    query: String,
    tab: SearchTab,
    client: NativeSpotify,
    results: SearchResults,
    status: SearchStatus,
    request_generation: u64,
    vertical_scroll: ScrollHandle,
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
            vertical_scroll: ScrollHandle::new(),
            featured_scroll: ScrollHandle::new(),
            playlist_scroll: ScrollHandle::new(),
            podcast_scroll: ScrollHandle::new(),
            artist_scroll: ScrollHandle::new(),
            vertical_scroll_state: SmoothScrollState::default(),
            featured_scroll_state: SmoothScrollState::default(),
            playlist_scroll_state: SmoothScrollState::default(),
            podcast_scroll_state: SmoothScrollState::default(),
            artist_scroll_state: SmoothScrollState::default(),
        };
        screen.fetch_results(cx);
        screen
    }

    fn select_tab(&mut self, tab: SearchTab, cx: &mut Context<Self>) {
        if self.tab != tab {
            self.tab = tab;
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
                    cx.notify();
                });
            }
        })
        .detach();
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
    div()
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
                .child(item.title.clone())
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
        )
}

fn album_card(item: &SearchItem, index: usize, client: &NativeSpotify) -> impl IntoElement {
    let client = client.clone();
    let uri = item.uri.clone();
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
        .on_click(move |_, _, _| {
            let _ = client.play_context(&uri);
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

impl Render for SearchScreen {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let client = self.client.clone();
        let tabs = SearchTab::ALL.into_iter().map(|tab| {
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
        });

        let status = match &self.status {
            SearchStatus::Idle | SearchStatus::Loading => Some("Searching…".to_string()),
            SearchStatus::Loaded if self.results.is_empty() => Some("No results found.".into()),
            SearchStatus::Loaded => None,
            SearchStatus::Error(error) => Some(format!("Search failed: {error}")),
        };

        let show_artists = matches!(self.tab, SearchTab::All | SearchTab::Artists);
        let show_albums = matches!(self.tab, SearchTab::All | SearchTab::Albums);
        let show_tracks = matches!(self.tab, SearchTab::All | SearchTab::Songs);
        let show_playlists = matches!(self.tab, SearchTab::All | SearchTab::Playlists);
        let show_podcasts = matches!(self.tab, SearchTab::All | SearchTab::Podcasts);

        div()
            .id("search-content")
            .flex()
            .flex_col()
            .size_full()
            .gap(px(20.0))
            .p(px(24.0))
            .bg(rgb(0x121212))
            .rounded_lg()
            .overflow_y_scroll()
            .smooth_scroll(
                self.vertical_scroll.clone(),
                ScrollAxis::Vertical,
                self.vertical_scroll_state.clone(),
            )
            .child(div().flex().gap(px(8.0)).children(tabs))
            .children(
                status.map(|message| div().py(px(24.0)).text_color(rgb(0xb3b3b3)).child(message)),
            )
            .when(
                matches!(self.tab, SearchTab::All) && self.results.top_result.is_some(),
                |content| {
                    let item = self.results.top_result.clone().expect("checked above");
                    let top_uri = item.uri.clone();
                    let top_is_song = matches!(item.kind, "Song" | "Episode");
                    let client = client.clone();
                    content
                        .child(
                            div()
                                .text_xl()
                                .font_weight(gpui::FontWeight::BOLD)
                                .child("Top result"),
                        )
                        .child(
                            div()
                                .id("top-result-card")
                                .flex()
                                .items_center()
                                .gap(px(16.0))
                                .p(px(16.0))
                                .rounded(px(8.0))
                                .bg(rgb(0x292929))
                                .cursor_pointer()
                                .hover(|style| style.bg(rgb(0x333333)))
                                .on_click({
                                    let client = client.clone();
                                    move |_, _, _| {
                                        if top_is_song {
                                            let _ = client.play_track(&top_uri);
                                        } else {
                                            let _ = client.play_context(&top_uri);
                                        }
                                    }
                                })
                                .child(artwork(&item.image, 72.0))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(4.0))
                                        .child(item.title.clone())
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
                                        .size(px(44.0))
                                        .rounded_full()
                                        .bg(rgb(0x1ed760))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_color(rgb(0x000000))
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .child(
                                            svg()
                                                .path("icons/play.svg")
                                                .size(px(18.0))
                                                .text_color(rgb(0x000000)),
                                        ),
                                ),
                        )
                },
            )
            .when(show_albums && !self.results.albums.is_empty(), |content| {
                content
                    .child(section_heading("Featuring", &self.query))
                    .child(
                        div()
                            .id("featured-albums")
                            .flex()
                            .gap(px(8.0))
                            .overflow_x_scroll()
                            .smooth_scroll(
                                self.featured_scroll.clone(),
                                ScrollAxis::Horizontal,
                                self.featured_scroll_state.clone(),
                            )
                            .children(
                                self.results
                                    .albums
                                    .iter()
                                    .take(8)
                                    .enumerate()
                                    .map(|(index, item)| album_card(item, index, &client)),
                            ),
                    )
            })
            .when(show_tracks && !self.results.tracks.is_empty(), |content| {
                content
                    .child(section_heading("Songs", &self.query))
                    .children(
                        self.results
                            .tracks
                            .iter()
                            .take(10)
                            .enumerate()
                            .map(|(index, item)| track_row(item, index, &client)),
                    )
            })
            .when(
                show_playlists && !self.results.playlists.is_empty(),
                |content| {
                    content
                        .child(section_heading("Playlists", &self.query))
                        .child(
                            div()
                                .id("playlist-results")
                                .flex()
                                .gap(px(8.0))
                                .overflow_x_scroll()
                                .smooth_scroll(
                                    self.playlist_scroll.clone(),
                                    ScrollAxis::Horizontal,
                                    self.playlist_scroll_state.clone(),
                                )
                                .children(
                                    self.results
                                        .playlists
                                        .iter()
                                        .take(8)
                                        .enumerate()
                                        .map(|(index, item)| album_card(item, index, &client)),
                                ),
                        )
                },
            )
            .when(
                show_podcasts && !self.results.podcasts.is_empty(),
                |content| {
                    content
                        .child(section_heading("Podcasts & Shows", &self.query))
                        .child(
                            div()
                                .id("podcast-results")
                                .flex()
                                .gap(px(8.0))
                                .overflow_x_scroll()
                                .smooth_scroll(
                                    self.podcast_scroll.clone(),
                                    ScrollAxis::Horizontal,
                                    self.podcast_scroll_state.clone(),
                                )
                                .children(
                                    self.results
                                        .podcasts
                                        .iter()
                                        .take(8)
                                        .enumerate()
                                        .map(|(index, item)| album_card(item, index, &client)),
                                ),
                        )
                },
            )
            .when(
                show_artists && !self.results.artists.is_empty(),
                |content| {
                    content
                        .child(section_heading("Artists", &self.query))
                        .child(
                            div()
                                .id("artist-results")
                                .flex()
                                .gap(px(8.0))
                                .overflow_x_scroll()
                                .smooth_scroll(
                                    self.artist_scroll.clone(),
                                    ScrollAxis::Horizontal,
                                    self.artist_scroll_state.clone(),
                                )
                                .children(
                                    self.results
                                        .artists
                                        .iter()
                                        .take(8)
                                        .enumerate()
                                        .map(|(index, item)| album_card(item, index, &client)),
                                ),
                        )
                },
            )
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
