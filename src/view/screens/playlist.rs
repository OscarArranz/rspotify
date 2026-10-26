//! Spotify-style playlist detail screen.
//!
//! The whole page is one variable-height GPUI list: the header scrolls away and
//! track rows stay virtualized. Data comes from the native playlist/context
//! endpoints in `crate::lib::spotify`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::view::components::smooth_scroll::{SmoothScrollState, list_scroll_overlay};
use futures::StreamExt;
use futures::channel::{mpsc, oneshot};
use gpui::{
    AnyElement, AppContext, Context, Entity, InteractiveElement, IntoElement, ListAlignment,
    ListScrollEvent, ListState, ParentElement, Render, StatefulInteractiveElement, Styled,
    Subscription, Window, div, img, linear_color_stop, linear_gradient, list, px, rgb, rgba, svg,
};
use gpui_component::input::{Input, InputEvent, InputState};

use crate::lib::spotify::{
    NativeSpotify, PlaybackSnapshot, PlaylistDetail, PlaylistPage, PlaylistSource,
};

const TRACK_OFFSET: usize = 3;
const ALBUM_WIDTH: f32 = 160.0;
const DATE_WIDTH: f32 = 140.0;
const DURATION_WIDTH: f32 = 56.0;
const PANEL_BG: u32 = 0x121212;

#[derive(Clone, Debug)]
enum LoadState {
    Loading,
    Loaded,
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlaylistSort {
    Custom,
    Title,
    Album,
    DateAdded,
}

impl PlaylistSort {
    fn next(self) -> Self {
        match self {
            Self::Custom => Self::Title,
            Self::Title => Self::Album,
            Self::Album => Self::DateAdded,
            Self::DateAdded => Self::Custom,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Custom => "Spotify order",
            Self::Title => "Title",
            Self::Album => "Album",
            Self::DateAdded => "Recently added",
        }
    }
}

pub(crate) struct PlaylistScreen {
    source: PlaylistSource,
    client: NativeSpotify,
    state: LoadState,
    detail: Option<PlaylistDetail>,
    list_state: ListState,
    playback: PlaybackSnapshot,
    search_input: Option<Entity<InputState>>,
    _search_subscription: Option<Subscription>,
    search_open: bool,
    search_query: String,
    sort: PlaylistSort,
    visible_indices: Vec<usize>,
    next_page_loading: bool,
    next_page_error: Option<String>,
    action_error: Option<String>,
    scroll_animation: SmoothScrollState,
}

impl PlaylistScreen {
    pub(crate) fn new(
        source: PlaylistSource,
        client: NativeSpotify,
        cx: &mut Context<Self>,
    ) -> Self {
        let list_state = ListState::new(1, ListAlignment::Top, px(1600.0));
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
            detail: None,
            list_state,
            search_input: None,
            _search_subscription: None,
            search_open: false,
            search_query: String::new(),
            sort: PlaylistSort::Custom,
            visible_indices: Vec::new(),
            next_page_loading: false,
            next_page_error: None,
            action_error: None,
            scroll_animation: SmoothScrollState::new(cx.entity_id()),
        };
        if matches!(this.source, PlaylistSource::LikedSongs) {
            if let Some(detail) = this.client.loaded_liked_songs_detail() {
                this.apply_detail(detail, cx);
            }
        }
        this.start_load(cx);
        this.start_playback_poll(cx);
        this
    }

    fn start_load(&mut self, cx: &mut Context<Self>) {
        self.state = LoadState::Loading;
        if self.detail.is_none() {
            self.visible_indices.clear();
        }
        self.next_page_loading = false;
        self.next_page_error = None;
        self.action_error = None;
        self.scroll_animation.cancel();
        self.list_state.reset(self.item_count().max(1));

        let client = self.client.clone();
        let source = self.source.clone();
        let has_detail = self.detail.is_some();
        let (tx, mut rx) = mpsc::unbounded();
        std::thread::spawn(move || {
            if !has_detail {
                if let Some(detail) = client.cached_playlist_detail(&source) {
                    let _ = tx.unbounded_send(Ok(detail));
                }
            }
            let result = client.playlist_detail(source.clone());
            if let Ok(detail) = &result {
                client.cache_playlist_detail(&source, detail);
            }
            let _ = tx.unbounded_send(result);
        });

        cx.spawn(async move |this, cx| {
            while let Some(result) = rx.next().await {
                let _ = this.update(cx, |this, cx| match result {
                    Ok(detail) => this.apply_detail(detail, cx),
                    Err(error) => {
                        if this.detail.is_some() {
                            this.action_error = Some(format!("Refresh failed: {error}"));
                            this.state = LoadState::Loaded;
                        } else {
                            this.state = LoadState::Error(error.to_string());
                        }
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_detail(&mut self, detail: PlaylistDetail, cx: &mut Context<Self>) {
        self.detail = Some(detail);
        self.action_error = None;
        self.rebuild_visible();
        self.state = LoadState::Loaded;
        self.scroll_animation.cancel();
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
        TRACK_OFFSET
            + self.visible_indices.len()
            + usize::from(
                self.detail.as_ref().is_some_and(|detail| {
                    !detail.tracks.is_empty() && self.visible_indices.is_empty()
                }),
            )
    }

    fn is_filtered(&self) -> bool {
        !self.search_query.trim().is_empty() || self.sort != PlaylistSort::Custom
    }

    fn on_scroll(&mut self, event: &ListScrollEvent, cx: &mut Context<Self>) {
        if self.next_page_loading
            || !matches!(self.state, LoadState::Loaded)
            || self.visible_indices.is_empty()
            || !self.search_query.trim().is_empty()
        {
            return;
        }
        if event.visible_range.end + 6 < self.item_count() {
            return;
        }
        let Some(next_page_url) = self
            .detail
            .as_ref()
            .and_then(|detail| detail.next_page_url.clone())
        else {
            return;
        };
        self.load_next_page(next_page_url, cx);
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

    fn load_next_page(&mut self, next_page_url: String, cx: &mut Context<Self>) {
        self.next_page_loading = true;
        self.next_page_error = None;
        let client = self.client.clone();
        let (tx, rx) = oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(client.playlist_next_page(&next_page_url));
        });

        cx.spawn(async move |this, cx| {
            let Ok(result) = rx.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.next_page_loading = false;
                match result {
                    Ok(page) => this.append_page(page, cx),
                    Err(error) => {
                        this.next_page_error = Some(error.to_string());
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn append_page(&mut self, page: PlaylistPage, cx: &mut Context<Self>) {
        if self.detail.is_none() {
            return;
        }
        let old_item_count = self.item_count();
        let Some(detail) = self.detail.as_mut() else {
            return;
        };
        let old_len = detail.tracks.len();
        detail.tracks.extend(page.tracks);
        detail.next_page_url = page.next_page_url;
        let new_len = detail.tracks.len();
        let added_at = &detail.added_at;
        for track in &mut detail.tracks[old_len..new_len] {
            if let Some(timestamp) = added_at.get(&track.uri).copied() {
                track.added_at_ms = Some(timestamp);
            }
        }
        if detail.next_page_url.is_none() && detail.track_count.is_none() {
            detail.track_count = Some(new_len);
        }

        if self.is_filtered() {
            let scroll_top = self.list_state.logical_scroll_top();
            self.rebuild_visible();
            let new_item_count = self.item_count() - TRACK_OFFSET;
            self.list_state
                .splice(TRACK_OFFSET..old_item_count, new_item_count);
            self.list_state.scroll_to(scroll_top);
        } else {
            let added = new_len - old_len;
            if added > 0 {
                self.visible_indices.extend(old_len..new_len);
                self.list_state
                    .splice(old_item_count..old_item_count, added);
            }
        }
        if let Some(detail) = self.detail.as_ref() {
            let client = self.client.clone();
            let source = self.source.clone();
            let detail = detail.clone();
            std::thread::spawn(move || client.cache_playlist_detail(&source, &detail));
        }
        cx.notify();
    }

    fn rebuild_visible(&mut self) {
        let Some(detail) = self.detail.as_ref() else {
            self.visible_indices.clear();
            return;
        };
        let query = self.search_query.trim().to_lowercase();
        let mut indices = detail
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, track)| {
                query.is_empty()
                    || track.title.to_lowercase().contains(&query)
                    || track.artist.to_lowercase().contains(&query)
                    || track.album.to_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();

        match self.sort {
            PlaylistSort::Custom => {}
            PlaylistSort::Title => {
                let mut keyed = indices
                    .iter()
                    .map(|&index| (index, detail.tracks[index].title.to_lowercase()))
                    .collect::<Vec<_>>();
                keyed.sort_by(|left, right| left.1.cmp(&right.1));
                indices = keyed.into_iter().map(|(index, _)| index).collect();
            }
            PlaylistSort::Album => {
                let mut keyed = indices
                    .iter()
                    .map(|&index| (index, detail.tracks[index].album.to_lowercase()))
                    .collect::<Vec<_>>();
                keyed.sort_by(|left, right| left.1.cmp(&right.1));
                indices = keyed.into_iter().map(|(index, _)| index).collect();
            }
            PlaylistSort::DateAdded => {
                indices.sort_by_key(|&index| {
                    std::cmp::Reverse(detail.tracks[index].added_at_ms.unwrap_or(i64::MIN))
                });
            }
        }
        self.visible_indices = indices;
    }

    fn toggle_play(&mut self, cx: &mut Context<Self>) {
        let Some(context_uri) = self
            .detail
            .as_ref()
            .map(|detail| detail.context_uri.clone())
        else {
            return;
        };
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

    fn cycle_sort(&mut self, cx: &mut Context<Self>) {
        self.sort = self.sort.next();
        self.rebuild_visible();
        self.scroll_animation.cancel();
        self.list_state.reset(self.item_count());
        cx.notify();
    }

    fn toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_open {
            self.search_open = false;
            self.search_query.clear();
            if let Some(input) = &self.search_input {
                input.update(cx, |input, cx| input.set_value("", window, cx));
            }
            window.blur();
            self.rebuild_visible();
            self.scroll_animation.cancel();
            self.list_state.reset(self.item_count());
            cx.notify();
            return;
        }

        if self.search_input.is_none() {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search in playlist"));
            let subscription = cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.search_query = input.read(cx).value().to_string();
                    this.rebuild_visible();
                    this.scroll_animation.cancel();
                    this.list_state.reset(this.item_count());
                    cx.notify();
                }
            });
            self.search_input = Some(input);
            self._search_subscription = Some(subscription);
        }

        self.search_open = true;
        if let Some(input) = &self.search_input {
            input.update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    fn render_placeholder(
        &self,
        header_height: f32,
        cover_size: f32,
        message: &str,
        retry: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let subtitle = if retry {
            "Could not load playlist"
        } else {
            "Loading…"
        };
        let body = div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(14.0))
            .px(px(24.0))
            .text_color(rgb(0xb3b3b3))
            .child(
                div()
                    .max_w(px(560.0))
                    .text_size(px(14.0))
                    .child(message.to_string()),
            )
            .children(retry.then(|| {
                div()
                    .id("playlist-retry")
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
            }));

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(self.render_preview_header(header_height, cover_size, subtitle))
            .child(body)
            .into_any_element()
    }

    fn render_header(&self, header_height: f32, cover_size: f32) -> AnyElement {
        let Some(detail) = self.detail.as_ref() else {
            return self.render_preview_header(header_height, cover_size, "");
        };
        let title_size = (header_height * 0.28).clamp(48.0, 96.0);
        let mut metadata = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(14.0))
            .text_color(rgb(0xffffff))
            .child(self.render_owner_avatar(detail))
            .child(
                div()
                    .max_w(px(240.0))
                    .truncate()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(detail.owner_name.clone()),
            );
        if let Some(track_count) = detail.track_count {
            metadata = metadata.child(bullet()).child(format!(
                "{} {}",
                with_commas(track_count),
                if track_count == 1 { "song" } else { "songs" }
            ));
        }
        if let Some(duration) = detail.total_duration_ms {
            metadata = metadata
                .child(bullet())
                .child(format_total_duration(duration));
        }

        div()
            .w_full()
            .h(px(header_height))
            .px(px(24.0))
            .pb(px(24.0))
            .flex()
            .items_end()
            .gap(px(24.0))
            .bg(linear_gradient(
                180.0,
                linear_color_stop(rgb(detail.accent), 0.0),
                linear_color_stop(rgb(PANEL_BG), 1.0),
            ))
            .child(render_cover(
                detail.image_url.as_deref(),
                cover_size,
                detail.is_liked_songs,
                detail.accent,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("Playlist"),
                    )
                    .child(
                        div()
                            .text_size(px(title_size))
                            .font_weight(gpui::FontWeight::BLACK)
                            .line_height(px(title_size * 0.9))
                            .truncate()
                            .child(detail.name.clone()),
                    )
                    .child(metadata)
                    .children((!detail.description.is_empty()).then(|| {
                        div()
                            .max_w(px(720.0))
                            .max_h(px(40.0))
                            .overflow_hidden()
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .text_color(rgb(0xb3b3b3))
                            .child(detail.description.clone())
                    })),
            )
            .into_any_element()
    }

    fn render_preview_header(
        &self,
        header_height: f32,
        cover_size: f32,
        message: &str,
    ) -> AnyElement {
        let title_size = (header_height * 0.28).clamp(48.0, 96.0);
        let liked = matches!(self.source, PlaylistSource::LikedSongs);
        let accent = if liked { 0x4b1fd0 } else { 0x3a2d64 };
        let owner = self.source.preview_owner();

        let mut metadata = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(14.0))
            .text_color(rgb(0xb3b3b3));
        if !owner.is_empty() {
            metadata = metadata
                .child(
                    div()
                        .max_w(px(240.0))
                        .truncate()
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(owner.to_string()),
                )
                .child(bullet());
        }
        metadata = metadata.child(if message.is_empty() {
            "Loading…".to_string()
        } else {
            message.to_string()
        });

        div()
            .w_full()
            .h(px(header_height))
            .px(px(24.0))
            .pb(px(24.0))
            .flex()
            .items_end()
            .gap(px(24.0))
            .bg(linear_gradient(
                180.0,
                linear_color_stop(rgb(accent), 0.0),
                linear_color_stop(rgb(PANEL_BG), 1.0),
            ))
            .child(render_cover(
                self.source.preview_image(),
                cover_size,
                liked,
                accent,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("Playlist"),
                    )
                    .child(
                        div()
                            .text_size(px(title_size))
                            .font_weight(gpui::FontWeight::BLACK)
                            .line_height(px(title_size * 0.9))
                            .truncate()
                            .child(self.source.preview_name().to_string()),
                    )
                    .child(metadata),
            )
            .into_any_element()
    }

    fn render_owner_avatar(&self, detail: &PlaylistDetail) -> AnyElement {
        match detail.owner_image_url.clone() {
            Some(url) => img(url).size(px(24.0)).rounded(px(12.0)).into_any_element(),
            None => div()
                .size(px(24.0))
                .rounded(px(12.0))
                .bg(rgb(0x3e3e3e))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(11.0))
                .font_weight(gpui::FontWeight::BOLD)
                .child(detail.owner_name.chars().next().unwrap_or('?').to_string())
                .into_any_element(),
        }
    }

    fn render_action_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let active_context = self.detail.as_ref().is_some_and(|detail| {
            self.playback.context_uri.as_deref() == Some(detail.context_uri.as_str())
        });
        let playing = active_context && self.playback.is_playing;
        let play_icon = if playing {
            "icons/pause.svg"
        } else {
            "icons/play.svg"
        };
        let shuffle_color = if self.playback.shuffle {
            rgb(0x1ed760)
        } else {
            rgb(0xb3b3b3)
        };

        let left = div()
            .flex()
            .items_center()
            .gap(px(18.0))
            .child(
                div()
                    .id("playlist-play")
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
                            .path(play_icon)
                            .size(px(24.0))
                            .text_color(rgb(0x000000)),
                    ),
            )
            .child(
                div()
                    .id("playlist-shuffle")
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
                            .path("icons/download.svg")
                            .size(px(20.0))
                            .text_color(rgb(0xffffff)),
                    ),
            )
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
                            .text_color(rgb(0xffffff)),
                    ),
            );

        let search_control = if self.search_open {
            let mut search = div()
                .w(px(220.0))
                .h(px(32.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .rounded(px(16.0))
                .bg(rgb(0x242424))
                .border_1()
                .border_color(rgb(0x3e3e3e));
            if let Some(input) = &self.search_input {
                search = search
                    .child(
                        svg()
                            .path("icons/search.svg")
                            .size(px(16.0))
                            .text_color(rgb(0xb3b3b3)),
                    )
                    .child(
                        Input::new(input)
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false)
                            .cleanable(true)
                            .flex_1()
                            .min_w(px(0.0))
                            .text_color(rgb(0xffffff)),
                    )
                    .child(
                        div()
                            .id("playlist-search-close")
                            .flex_shrink_0()
                            .size(px(20.0))
                            .rounded(px(10.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .text_size(px(16.0))
                            .text_color(rgb(0xb3b3b3))
                            .hover(|style| style.bg(rgba(0xffffff1a)))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.toggle_search(window, cx)),
                            )
                            .child("×"),
                    );
            }
            search.into_any_element()
        } else {
            div()
                .id("playlist-search")
                .size(px(32.0))
                .rounded(px(16.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|style| style.bg(rgba(0xffffff1a)))
                .on_click(cx.listener(|this, _, window, cx| this.toggle_search(window, cx)))
                .child(
                    svg()
                        .path("icons/search.svg")
                        .size(px(18.0))
                        .text_color(rgb(0xb3b3b3)),
                )
                .into_any_element()
        };

        let right = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(search_control)
            .child(
                div()
                    .id("playlist-sort")
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(px(16.0))
                    .text_size(px(13.0))
                    .text_color(rgb(0xb3b3b3))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgba(0xffffff1a)))
                    .on_click(cx.listener(|this, _, _, cx| this.cycle_sort(cx)))
                    .child(self.sort.label())
                    .child(
                        svg()
                            .path("icons/sort.svg")
                            .size(px(16.0))
                            .text_color(rgb(0xb3b3b3)),
                    ),
            )
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
                            .path("icons/list.svg")
                            .size(px(18.0))
                            .text_color(rgb(0xffffff)),
                    ),
            );

        let error = self
            .action_error
            .clone()
            .or_else(|| self.next_page_error.clone());

        div()
            .w_full()
            .px(px(24.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .bg(rgb(PANEL_BG))
            .child(
                div()
                    .h(px(48.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(left)
                    .child(right),
            )
            .children(error.map(|error| {
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(0xf15e59))
                    .truncate()
                    .child(error)
            }))
            .into_any_element()
    }

    fn render_column_header(&self) -> AnyElement {
        div()
            .w_full()
            .h(px(36.0))
            .px(px(24.0))
            .border_b_1()
            .border_color(rgba(0xffffff1a))
            .flex()
            .items_center()
            .gap(px(16.0))
            .text_size(px(12.0))
            .text_color(rgb(0xb3b3b3))
            .child(div().w(px(24.0)).flex().justify_end().child("#"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .flex_1()
                    .min_w(px(0.0))
                    .child(div().w(px(40.0)).flex_shrink_0())
                    .child(div().flex_1().min_w(px(0.0)).child("Title")),
            )
            .child(div().w(px(ALBUM_WIDTH)).child("Album"))
            .child(div().w(px(DATE_WIDTH)).child("Date added"))
            .child(
                div().w(px(DURATION_WIDTH)).flex().justify_end().child(
                    svg()
                        .path("icons/clock.svg")
                        .size(px(16.0))
                        .text_color(rgb(0xb3b3b3)),
                ),
            )
            .into_any_element()
    }

    fn render_item(
        &mut self,
        index: usize,
        header_height: f32,
        cover_size: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match index {
            0 => self.render_header(header_height, cover_size),
            1 => self.render_action_bar(cx),
            2 => self.render_column_header(),
            _ => {
                let track_index = self.visible_indices.get(index - TRACK_OFFSET).copied();
                match track_index {
                    Some(track_index) => self.render_track(track_index, index - TRACK_OFFSET, cx),
                    None => self.render_no_results(),
                }
            }
        }
    }

    fn render_no_results(&self) -> AnyElement {
        div()
            .h(px(96.0))
            .w_full()
            .px(px(24.0))
            .flex()
            .items_center()
            .text_size(px(14.0))
            .text_color(rgb(0xb3b3b3))
            .child("No results found in this playlist.")
            .into_any_element()
    }

    fn render_track(
        &self,
        track_index: usize,
        display_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(detail) = self.detail.as_ref() else {
            return div().into_any_element();
        };
        let Some(track) = detail.tracks.get(track_index) else {
            return div().into_any_element();
        };
        let is_active = self
            .playback
            .now_playing
            .as_ref()
            .is_some_and(|now| now.uri == track.uri);
        let is_playing = is_active && self.playback.is_playing;
        let title_color = if is_active {
            rgb(0x1ed760)
        } else {
            rgb(0xffffff)
        };
        let context_uri = detail.context_uri.clone();
        let track_uri = track.uri.clone();
        let playable = track.playable;

        let index_cell = div()
            .w(px(24.0))
            .flex()
            .justify_end()
            .text_size(px(14.0))
            .text_color(rgb(0xb3b3b3))
            .child(if is_playing {
                equalizer()
            } else {
                div()
                    .text_color(if is_active {
                        rgb(0x1ed760)
                    } else {
                        rgb(0xb3b3b3)
                    })
                    .child((display_index + 1).to_string())
                    .into_any_element()
            });

        let title = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .min_w(px(0.0))
            .child(
                div()
                    .truncate()
                    .text_size(px(15.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(title_color)
                    .child(track.title.clone()),
            )
            .children(track.explicit.then(|| {
                div()
                    .size(px(15.0))
                    .flex_shrink_0()
                    .rounded(px(2.0))
                    .bg(rgb(0xb3b3b3))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(9.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(0x121212))
                    .child("E")
            }));

        let row = div()
            .id(("playlist-track", track_index))
            .w_full()
            .h(px(56.0))
            .px(px(24.0))
            .flex()
            .items_center()
            .gap(px(16.0))
            .rounded(px(4.0))
            .cursor_pointer()
            .hover(|style| style.bg(rgba(0xffffff1a)))
            .on_click(cx.listener(move |this, _, _, cx| {
                if !playable {
                    return;
                }
                this.action_error = this
                    .client
                    .play_context_at(&context_uri, &track_uri)
                    .err()
                    .map(|error| error.to_string());
                cx.notify();
            }))
            .child(index_cell)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .flex_1()
                    .min_w(px(0.0))
                    .child(render_cover(
                        track.image_url.as_deref(),
                        40.0,
                        false,
                        detail.accent,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .flex_1()
                            .min_w(px(0.0))
                            .child(title)
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.0))
                                    .text_color(rgb(0xb3b3b3))
                                    .child(track.artist.clone()),
                            ),
                    ),
            )
            .child(
                div()
                    .w(px(ALBUM_WIDTH))
                    .truncate()
                    .text_size(px(13.0))
                    .text_color(rgb(0xb3b3b3))
                    .child(track.album.clone()),
            )
            .child(
                div()
                    .w(px(DATE_WIDTH))
                    .truncate()
                    .text_size(px(13.0))
                    .text_color(rgb(0xb3b3b3))
                    .child(
                        track
                            .added_at_ms
                            .map(format_relative_date)
                            .unwrap_or_default(),
                    ),
            )
            .child(
                div()
                    .w(px(DURATION_WIDTH))
                    .flex()
                    .justify_end()
                    .text_size(px(13.0))
                    .text_color(rgb(0xb3b3b3))
                    .child(format_track_duration(track.duration_ms)),
            );

        if playable {
            row.into_any_element()
        } else {
            row.opacity(0.55).into_any_element()
        }
    }

    fn render_empty_page(
        &self,
        header_height: f32,
        cover_size: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let liked = self
            .detail
            .as_ref()
            .is_some_and(|detail| detail.is_liked_songs);
        let (title, message) = if liked {
            (
                "You haven't liked any songs yet.",
                "Songs you like will appear here.",
            )
        } else {
            (
                "This playlist is empty.",
                "Add tracks to this playlist to see them here.",
            )
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(self.render_header(header_height, cover_size))
            .child(self.render_action_bar(cx))
            .child(self.render_column_header())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .py(px(64.0))
                    .gap(px(8.0))
                    .text_color(rgb(0xb3b3b3))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(title),
                    )
                    .child(div().text_size(px(14.0)).child(message)),
            )
            .into_any_element()
    }
}

impl Render for PlaylistScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let viewport_width: f32 = viewport.width.into();
        let viewport_height: f32 = viewport.height.into();
        let header_height = (viewport_height * 0.40).clamp(186.0, 336.0);
        let cover_size = if viewport_width >= 1280.0 {
            232.0
        } else {
            192.0
        };

        let page = div()
            .size_full()
            .bg(rgb(PANEL_BG))
            .rounded_lg()
            .overflow_hidden();

        let content = if matches!(&self.state, LoadState::Loading) && self.detail.is_none() {
            self.render_placeholder(header_height, cover_size, "Loading playlist…", false, cx)
        } else if let LoadState::Error(error) = &self.state {
            self.render_placeholder(header_height, cover_size, error, true, cx)
        } else if self
            .detail
            .as_ref()
            .map_or(true, |detail| detail.tracks.is_empty())
        {
            self.render_empty_page(header_height, cover_size, cx)
        } else {
            div()
                .relative()
                .size_full()
                .child(
                    list(
                        self.list_state.clone(),
                        cx.processor(move |this, index, _, cx| {
                            this.render_item(index, header_height, cover_size, cx)
                        }),
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
                ))
                .into_any_element()
        };

        page.child(content).into_any_element()
    }
}

pub(crate) fn render_cover(url: Option<&str>, size: f32, liked: bool, accent: u32) -> AnyElement {
    match url {
        Some(url) if !url.is_empty() => img(url.to_string())
            .size(px(size))
            .rounded(px(4.0))
            .flex_shrink_0()
            .into_any_element(),
        _ => {
            let (from, to, icon) = if liked {
                (0x4b1fd0, 0xa786ff, "icons/heart.svg")
            } else {
                (accent, 0x121212, "icons/list.svg")
            };
            div()
                .size(px(size))
                .rounded(px(4.0))
                .flex_shrink_0()
                .bg(linear_gradient(
                    135.0,
                    linear_color_stop(rgb(from), 0.0),
                    linear_color_stop(rgb(to), 1.0),
                ))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path(icon)
                        .size(px(size * 0.34))
                        .text_color(rgb(0xffffff)),
                )
                .into_any_element()
        }
    }
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

fn bullet() -> AnyElement {
    div()
        .text_color(rgb(0xb3b3b3))
        .child("•")
        .into_any_element()
}

fn format_track_duration(duration_ms: u64) -> String {
    if duration_ms == 0 {
        return String::new();
    }
    let total_seconds = duration_ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn format_total_duration(duration_ms: u64) -> String {
    let minutes = duration_ms / 60_000;
    let hours = minutes / 60;
    let minutes = minutes % 60;
    if hours > 0 {
        format!("{hours} hr {minutes} min")
    } else {
        format!("{minutes} min")
    }
}

fn format_relative_date(timestamp_ms: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(timestamp_ms);
    let days = now.saturating_sub(timestamp_ms) / 86_400_000;
    match days {
        days if days <= 0 => "Today".to_string(),
        1 => "Yesterday".to_string(),
        days if days < 7 => format!("{days} days ago"),
        days if days < 30 => {
            let weeks = days / 7;
            format!("{weeks} week{} ago", if weeks == 1 { "" } else { "s" })
        }
        days if days < 365 => {
            let months = days / 30;
            format!("{months} month{} ago", if months == 1 { "" } else { "s" })
        }
        days => {
            let years = days / 365;
            format!("{years} year{} ago", if years == 1 { "" } else { "s" })
        }
    }
}

fn with_commas(value: usize) -> String {
    let text = value.to_string();
    let mut result = String::with_capacity(text.len() + text.len() / 3);
    for (index, character) in text.chars().enumerate() {
        if index > 0 && (text.len() - index) % 3 == 0 {
            result.push(',');
        }
        result.push(character);
    }
    result
}
