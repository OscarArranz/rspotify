//! Feed-backed native Home. UI state and scroll positions live in this entity.

use std::{collections::HashMap, sync::Arc};

use futures::channel::oneshot;
use gpui::{
    AnyElement, AppContext, Context, FontWeight, InteractiveElement, IntoElement, ListAlignment,
    ListOffset, ListState, ObjectFit, ParentElement, Render, RenderImage, ScrollHandle,
    SharedString, StatefulInteractiveElement, Styled, StyledImage, Window, canvas, div, img, list,
    point, prelude::FluentBuilder, px, rgb, svg,
};

use crate::view::components::track_menu::{TrackMenuTarget, track_menu};
use crate::{
    lib::spotify::{
        NativeSpotify, PlaylistSource, SpotifyError,
        home::{HomeFeed, HomeItem, HomePanel, HomeShelf},
    },
    router::StackRouterHandle,
    state::AppState,
    view::{
        components::{
            smooth_scroll::{
                SmoothScrollElement, SmoothScrollState, list_scroll_overlay, nested_scroll_region,
            },
            virtual_list::virtual_horizontal_list,
        },
        screens::playlist::{PlaylistScreen, render_cover},
    },
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum HomeListMode {
    Loading,
    Failed,
    Feed,
    Expanded(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct HomeListLayout {
    mode: HomeListMode,
    revision: u64,
    shelf_loading: bool,
    width: u32,
    shelves: usize,
    panels: usize,
    shortcuts: usize,
    items: usize,
    columns: usize,
    tail: bool,
}

impl HomeListLayout {
    fn empty_feed(self) -> bool {
        self.shelves == 0 && self.shortcuts == 0
    }

    fn grid_rows(self) -> usize {
        self.items.div_ceil(self.columns).max(1)
    }

    fn row_count(self) -> usize {
        match self.mode {
            HomeListMode::Loading => 4,
            HomeListMode::Failed => 1,
            HomeListMode::Feed => 2 + self.shelves + self.panels + usize::from(self.empty_feed()),
            HomeListMode::Expanded(_) => 2 + self.grid_rows() + usize::from(self.tail),
        }
    }
}

pub(crate) struct HomeScreen {
    client: Option<NativeSpotify>,
    feed: HomeFeed,
    facet: String,
    loading: bool,
    failed: bool,
    message: Option<String>,
    generation: u64,
    content_revision: u64,
    width: f32,
    list: ListState,
    list_layout: Option<HomeListLayout>,
    feed_scroll_top: ListOffset,
    scroll_state: SmoothScrollState,
    shelf_scrolls: Vec<ScrollHandle>,
    shelf_states: Vec<SmoothScrollState>,
    expanded: Option<usize>,
    shelf_loading: bool,
    hovered_panel: Option<usize>,
    previews: HashMap<usize, Option<Arc<RenderImage>>>,
    preview_pending: std::collections::HashSet<usize>,
    #[cfg(test)]
    rendered_rows: Vec<usize>,
    #[cfg(test)]
    rendered_cards: std::cell::Cell<usize>,
    #[cfg(test)]
    rendered_card_ids: std::cell::RefCell<Vec<String>>,
    _auth: gpui::Subscription,
}

impl HomeScreen {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let auth = cx.observe_global::<AppState>(|this, cx| this.attach(cx));
        let mut screen = Self {
            client: None,
            feed: HomeFeed::default(),
            facet: String::new(),
            loading: false,
            failed: false,
            message: None,
            generation: 0,
            content_revision: 0,
            width: 800.0,
            list: ListState::new(1, ListAlignment::Top, px(256.0)),
            list_layout: None,
            feed_scroll_top: ListOffset::default(),
            scroll_state: SmoothScrollState::new(cx.entity_id()),
            shelf_scrolls: Vec::new(),
            shelf_states: Vec::new(),
            expanded: None,
            shelf_loading: false,
            hovered_panel: None,
            previews: HashMap::new(),
            preview_pending: Default::default(),
            #[cfg(test)]
            rendered_rows: Vec::new(),
            #[cfg(test)]
            rendered_cards: std::cell::Cell::new(0),
            #[cfg(test)]
            rendered_card_ids: std::cell::RefCell::new(Vec::new()),
            _auth: auth,
        };
        screen.attach(cx);
        screen
    }

    fn attach(&mut self, cx: &mut Context<Self>) {
        if self.client.is_none() {
            self.client = cx
                .try_global::<AppState>()
                .and_then(AppState::spotify_client);
            if self.client.is_some() {
                self.load(cx);
            }
        }
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        let facet = self.facet.clone();
        self.loading = true;
        self.failed = false;
        self.message = None;
        self.hovered_panel = None;
        self.previews.clear();
        self.preview_pending.clear();
        self.shelf_loading = false;
        cx.spawn(async move |this, cx| {
            let (tx, rx) = oneshot::channel();
            std::thread::spawn(move || {
                let _ = tx.send(client.home(&facet));
            });
            let result = rx.await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(Ok(feed)) => {
                        this.shelf_scrolls =
                            feed.shelves.iter().map(|_| ScrollHandle::new()).collect();
                        this.shelf_states = feed
                            .shelves
                            .iter()
                            .map(|_| SmoothScrollState::new(cx.entity_id()))
                            .collect();
                        this.feed = feed;
                        this.content_revision += 1;
                        this.expanded = None;
                        this.scroll_state.cancel();
                        this.list_layout = None;
                        this.feed_scroll_top = ListOffset::default();
                    }
                    _ => this.failed = true,
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn play(&mut self, item: HomeItem, cx: &mut Context<Self>) {
        if !item.playable {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.hovered_panel = None;
        cx.spawn(async move |this, cx| {
            let (tx, rx) = oneshot::channel();
            std::thread::spawn(move || {
                let result = if item.kind == "LikedSongs" {
                    client.play_context(&client.liked_songs_context_uri())
                } else if matches!(item.kind.as_str(), "Track" | "Episode") {
                    client.play_track(&item.uri)
                } else {
                    client.play_context(&item.uri)
                };
                let _ = tx.send(result);
            });
            let message = match rx.await {
                Ok(Ok(())) => None,
                Ok(Err(SpotifyError::Reconnecting)) => Some(SpotifyError::Reconnecting.to_string()),
                Ok(Err(SpotifyError::Playback(error))) => {
                    Some(format!("Couldn't start playback: {error}"))
                }
                Ok(Err(error)) => Some(format!("Couldn't start playback: {error}")),
                Err(_) => Some("Couldn't start playback. Try again.".into()),
            };
            if let Some(message) = message {
                let _ = this.update(cx, |this, cx| {
                    this.message = Some(message);
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn open(&mut self, item: HomeItem, cx: &mut Context<Self>) {
        self.hovered_panel = None;
        if matches!(
            item.kind.as_str(),
            "Playlist" | "LikedSongs" | "Album" | "Artist" | "Podcast"
        ) {
            let Some(client) = self.client.clone() else {
                return;
            };
            let Some(router) = cx.try_global::<StackRouterHandle>().cloned() else {
                return;
            };
            let source = if item.kind == "LikedSongs" {
                PlaylistSource::LikedSongs
            } else if item.kind == "Playlist" {
                PlaylistSource::Playlist {
                    uri: item.uri.clone(),
                    name: item.title,
                    owner: String::new(),
                    image_url: item.image,
                }
            } else {
                let kind = match item.kind.as_str() {
                    "Album" => "Album",
                    "Artist" => "Artist",
                    _ => "Podcast",
                };
                PlaylistSource::Context {
                    uri: item.uri.clone(),
                    name: item.title,
                    owner: item.subtitle,
                    image_url: item.image,
                    kind,
                }
            };
            let screen = cx.new(|cx| PlaylistScreen::new(source, client, cx));
            router.push_for_source(screen.into(), item.uri, cx);
        } else {
            self.play(item, cx);
        }
    }

    fn preview(&mut self, index: usize, hovered: bool, cx: &mut Context<Self>) {
        self.hovered_panel = hovered.then_some(index);
        if !hovered || self.previews.contains_key(&index) {
            cx.notify();
            return;
        }
        let Some(url) = self
            .feed
            .panels
            .get(index)
            .and_then(|p| p.video_url.clone())
        else {
            return;
        };
        self.previews.insert(index, None);
        self.preview_pending.insert(index);
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let (tx, rx) = oneshot::channel();
            std::thread::spawn(move || {
                let _ = tx.send(super::home_video::decode(&url));
            });
            let preview = rx.await.ok().flatten();
            let _ = this.update(cx, |this, cx| {
                if generation == this.generation {
                    this.preview_pending.remove(&index);
                    this.previews.insert(index, preview);
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn expand_shelf(&mut self, index: usize, more: bool, cx: &mut Context<Self>) {
        if self.shelf_loading {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(shelf) = self.feed.shelves.get(index) else {
            return;
        };
        let offset = if more {
            let Some(offset) = shelf.next_offset else {
                return;
            };
            offset
        } else {
            0
        };
        let uri = shelf.uri.clone();
        let generation = self.generation;
        self.expanded = Some(index);
        self.hovered_panel = None;
        self.shelf_loading = true;
        cx.spawn(async move |this, cx| {
            let (tx, rx) = oneshot::channel();
            std::thread::spawn(move || {
                let _ = tx.send(client.home_section(&uri, offset));
            });
            let result = rx.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.shelf_loading = false;
                match result {
                    Ok(Ok((items, next))) => {
                        let shelf = &mut this.feed.shelves[index];
                        if !more {
                            shelf.items = items;
                        } else {
                            for item in items {
                                if !shelf.items.iter().any(|i| i.uri == item.uri) {
                                    shelf.items.push(item);
                                }
                            }
                        }
                        shelf.next_offset = next;
                    }
                    _ => {
                        this.message =
                            Some("Couldn't load the full shelf. Try Show all again.".into())
                    }
                }
                this.content_revision += 1;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn play_button(
        &self,
        item: &HomeItem,
        id: String,
        size: f32,
        cx: &Context<Self>,
    ) -> AnyElement {
        let item = item.clone();
        div()
            .id(SharedString::from(id))
            .size(px(size))
            .rounded_full()
            .bg(rgb(0x1ed760))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0x3be477)))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.play(item.clone(), cx);
            }))
            .child(
                svg()
                    .path("icons/play.svg")
                    .size(px(size * 0.45))
                    .text_color(rgb(0x000000)),
            )
            .into_any_element()
    }

    fn share_button(&self, item: &HomeItem, id: String, cx: &Context<Self>) -> AnyElement {
        let uri = item.uri.clone();
        div()
            .id(SharedString::from(id))
            .size(px(40.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(rgb(0xb3b3b3))
            .hover(|s| s.text_color(rgb(0xffffff)))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                let parts: Vec<_> = uri.split(':').collect();
                let link = if parts.len() == 3 {
                    format!("https://open.spotify.com/{}/{}", parts[1], parts[2])
                } else {
                    uri.clone()
                };
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(link));
                this.message = Some("Link copied".into());
                cx.notify();
            }))
            .child(svg().path("icons/share.svg").size(px(24.0)))
            .into_any_element()
    }

    fn card(&self, item: &HomeItem, id: String, size: f32, cx: &Context<Self>) -> AnyElement {
        #[cfg(test)]
        self.rendered_cards.set(self.rendered_cards.get() + 1);
        #[cfg(test)]
        self.rendered_card_ids.borrow_mut().push(id.clone());
        let open = item.clone();
        let card = div()
            .id(SharedString::from(id.clone()))
            .group("home-card")
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(size + 24.0))
            .p(px(12.0))
            .gap(px(8.0))
            .rounded(px(8.0))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0x242424)))
            .on_click(cx.listener(move |this, _, _, cx| this.open(open.clone(), cx)))
            .child(
                div()
                    .relative()
                    .size(px(size))
                    .rounded(px(if item.kind == "Artist" {
                        size / 2.0
                    } else {
                        6.0
                    }))
                    .overflow_hidden()
                    .child(render_cover(
                        item.image.as_deref(),
                        size,
                        item.kind == "LikedSongs",
                        0x262626,
                    ))
                    .when(item.playable, |s| {
                        s.child(
                            div()
                                .absolute()
                                .bottom(px(8.0))
                                .right(px(8.0))
                                .opacity(0.0)
                                .group_hover("home-card", |s| s.opacity(1.0))
                                .child(self.play_button(item, format!("{id}-play"), 48.0, cx)),
                        )
                    }),
            )
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_ellipsis()
                    .child(item.title.clone()),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0xb3b3b3))
                    .h(px(40.0))
                    .overflow_hidden()
                    .child(item.subtitle.clone()),
            );
        track_menu(
            card,
            format!("home-track-menu-{id}"),
            TrackMenuTarget::new(item.uri.clone(), item.title.clone()),
            None,
        )
    }

    fn shelf(&self, index: usize, shelf: &HomeShelf, cx: &Context<Self>) -> AnyElement {
        let size = if self.width < 650.0 { 150.0 } else { 180.0 };
        let spotlight = matches!(
            shelf.layout.as_str(),
            "HomeSpotlightSectionData" | "HomePromotionSectionData"
        );
        let handle = self.shelf_scrolls[index].clone();
        let left = handle.clone();
        let right = handle.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .w_full()
            .when(!shelf.subtitle.is_empty(), |s| {
                s.child(
                    div()
                        .px(px(12.0))
                        .text_sm()
                        .text_color(rgb(0xb3b3b3))
                        .child(shelf.subtitle.clone()),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(12.0))
                    .gap(px(16.0))
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::BOLD)
                            .child(shelf.title.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(12.0))
                            .items_center()
                            .child(
                                div()
                                    .id(("shelf-prev", index))
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        let offset = left.offset();
                                        left.set_offset(point(
                                            (offset.x + px(420.0)).min(px(0.0)),
                                            offset.y,
                                        ));
                                        cx.refresh_windows();
                                    })
                                    .child(svg().path("icons/back.svg").size(px(20.0))),
                            )
                            .child(
                                div()
                                    .id(("shelf-next", index))
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        let offset = right.offset();
                                        right.set_offset(point(
                                            (offset.x - px(420.0)).max(-right.max_offset().width),
                                            offset.y,
                                        ));
                                        cx.refresh_windows();
                                    })
                                    .child(svg().path("icons/forward.svg").size(px(20.0))),
                            )
                            .child(
                                div()
                                    .id(("shelf-all", index))
                                    .cursor_pointer()
                                    .text_sm()
                                    .text_color(rgb(0xb3b3b3))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.expand_shelf(index, false, cx)
                                    }))
                                    .child("Show all"),
                            ),
                    ),
            )
            .child(if spotlight {
                self.featured(&shelf.items[0], index, cx)
            } else {
                div()
                    .id(("home-shelf", index))
                    .relative()
                    .flex()
                    .w_full()
                    .overflow_x_scroll()
                    .smooth_scroll(handle.clone(), self.shelf_states[index].clone())
                    .child(nested_scroll_region(
                        handle,
                        Some(self.shelf_states[index].clone()),
                        self.scroll_state.clone(),
                    ))
                    .when(shelf.items.len() > 50, |row| {
                        row.child(virtual_horizontal_list(
                            cx.entity(),
                            ("home-shelf-cards", index),
                            shelf.items.len(),
                            size + 24.0,
                            0.0,
                            move |this, i, _, cx| {
                                this.card(
                                    &this.feed.shelves[index].items[i],
                                    format!("shelf-{index}-{i}"),
                                    size,
                                    cx,
                                )
                            },
                        ))
                    })
                    .when(shelf.items.len() <= 50, |row| {
                        row.children(shelf.items.iter().enumerate().map(|(i, item)| {
                            self.card(item, format!("shelf-{index}-{i}"), size, cx)
                        }))
                    })
                    .into_any_element()
            })
            .into_any_element()
    }

    fn featured(&self, item: &HomeItem, index: usize, cx: &Context<Self>) -> AnyElement {
        let open = item.clone();
        let size = if self.width < 650.0 { 140.0 } else { 220.0 };
        let card = div()
            .id(("featured", index))
            .flex()
            .items_center()
            .gap(px(24.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0x242424)))
            .on_click(cx.listener(move |this, _, _, cx| this.open(open.clone(), cx)))
            .child(render_cover(item.image.as_deref(), size, false, 0x262626))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xb3b3b3))
                            .child(item.kind.clone()),
                    )
                    .child(
                        div()
                            .text_3xl()
                            .font_weight(FontWeight::BOLD)
                            .truncate()
                            .child(item.title.clone()),
                    )
                    .child(div().text_color(rgb(0xb3b3b3)).child(item.subtitle.clone()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(16.0))
                            .when(item.playable, |s| {
                                s.child(self.play_button(
                                    item,
                                    format!("featured-play-{index}"),
                                    48.0,
                                    cx,
                                ))
                            })
                            .child(self.share_button(item, format!("featured-share-{index}"), cx)),
                    ),
            );
        track_menu(
            card,
            format!("home-featured-menu-{index}"),
            TrackMenuTarget::new(item.uri.clone(), item.title.clone()),
            None,
        )
    }

    fn panel(&self, index: usize, panel: &HomePanel, cx: &Context<Self>) -> AnyElement {
        let width = (self.width - 100.0).clamp(220.0, 620.0);
        let height = (width * 1.05).min(650.0);
        let open = panel.item.clone();
        let preview = self
            .previews
            .get(&index)
            .and_then(|p| p.clone())
            .filter(|_| self.hovered_panel == Some(index));
        let poster = panel.poster.as_ref().or(panel.item.image.as_ref());
        let media = if let Some(image) = preview {
            img(image)
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element()
        } else if let Some(url) = poster {
            img(url.clone())
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element()
        } else {
            div().size_full().bg(rgb(0x263d36)).into_any_element()
        };
        let body = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .items_center()
            .w_full()
            .child(
                div()
                    .w(px(width))
                    .text_sm()
                    .text_color(rgb(0xb3b3b3))
                    .child(panel.label.clone()),
            )
            .child(
                div()
                    .id(("home-panel", index))
                    .relative()
                    .w(px(width))
                    .h(px(height))
                    .rounded(px(12.0))
                    .overflow_hidden()
                    .bg(rgb(0x202020))
                    .cursor_pointer()
                    .on_hover(
                        cx.listener(move |this, hovered, _, cx| this.preview(index, *hovered, cx)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.open(open.clone(), cx)))
                    .child(div().absolute().size_full().child(media))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .w_full()
                            .p(px(24.0))
                            .bg(gpui::rgba(0x00000099))
                            .flex()
                            .gap(px(16.0))
                            .items_center()
                            .child(render_cover(
                                panel.item.image.as_deref(),
                                80.0,
                                false,
                                0x262626,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_2xl()
                                            .font_weight(FontWeight::BOLD)
                                            .truncate()
                                            .child(panel.item.title.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(0xb3b3b3))
                                            .child(panel.item.kind.clone()),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .absolute()
                            .bottom_0()
                            .w_full()
                            .p(px(24.0))
                            .bg(gpui::rgba(0x000000bb))
                            .flex()
                            .flex_col()
                            .gap(px(20.0))
                            .child(div().child(panel.item.subtitle.clone()))
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .items_center()
                                    .child(
                                        div().text_sm().child(
                                            if self.preview_pending.contains(&index) {
                                                "Preparing muted preview…"
                                            } else if self
                                                .previews
                                                .get(&index)
                                                .is_some_and(Option::is_none)
                                            {
                                                "Preview unavailable · artwork shown"
                                            } else if panel.video_url.is_some() {
                                                "Muted Canvas · hover to preview"
                                            } else {
                                                "Recommended for you"
                                            },
                                        ),
                                    )
                                    .when(panel.item.playable, |s| {
                                        s.child(self.play_button(
                                            &panel.item,
                                            format!("panel-play-{index}"),
                                            56.0,
                                            cx,
                                        ))
                                    }),
                            ),
                    ),
            );
        track_menu(
            body,
            format!("home-panel-menu-{index}"),
            TrackMenuTarget::new(panel.item.uri.clone(), panel.item.title.clone()),
            None,
        )
    }
    fn list_layout(&self) -> HomeListLayout {
        let mode = if self.loading {
            HomeListMode::Loading
        } else if self.failed {
            HomeListMode::Failed
        } else if let Some(index) = self.expanded {
            HomeListMode::Expanded(index)
        } else {
            HomeListMode::Feed
        };
        let shelf = self.expanded.and_then(|index| self.feed.shelves.get(index));
        HomeListLayout {
            mode,
            revision: self.content_revision,
            shelf_loading: self.shelf_loading,
            width: self.width.to_bits(),
            shelves: self.feed.shelves.len(),
            panels: self.feed.panels.len(),
            shortcuts: self.feed.shortcuts.len().min(8),
            items: shelf.map_or(0, |shelf| shelf.items.len()),
            columns: (((self.width - 40.0) / 204.0).floor() as usize).max(1),
            tail: self.shelf_loading || shelf.is_some_and(|shelf| shelf.next_offset.is_some()),
        }
    }

    fn sync_list_layout(&mut self) {
        let layout = self.list_layout();
        if self.list_layout == Some(layout) {
            return;
        }
        let old_top = self.list.logical_scroll_top();
        let mut top = ListOffset::default();
        if let Some(old) = self.list_layout {
            if old.mode == layout.mode {
                top = old_top;
                if matches!(layout.mode, HomeListMode::Expanded(_))
                    && old.columns != layout.columns
                    && top.item_ix >= 2
                    && top.item_ix < 2 + old.grid_rows()
                {
                    // Keep the same leading card in view when wrapping changes.
                    let first_card = (top.item_ix - 2) * old.columns;
                    top.item_ix = 2 + first_card / layout.columns;
                }
            } else if old.mode == HomeListMode::Feed
                && matches!(layout.mode, HomeListMode::Expanded(_))
            {
                self.feed_scroll_top = old_top;
            } else if matches!(old.mode, HomeListMode::Expanded(_))
                && layout.mode == HomeListMode::Feed
            {
                top = self.feed_scroll_top;
            }
        }
        top.item_ix = top.item_ix.min(layout.row_count().saturating_sub(1));
        self.scroll_state.cancel();
        self.list.reset(layout.row_count());
        self.list.scroll_to(top);
        self.list_layout = Some(layout);
    }

    fn shortcuts(&self, cx: &Context<Self>) -> AnyElement {
        let columns = if self.width >= 1050.0 { 4 } else { 2 };
        let shortcut_width =
            ((self.width - 64.0 - (columns - 1) as f32 * 8.0) / columns as f32).max(100.0);
        div()
            .flex()
            .flex_wrap()
            .gap(px(8.0))
            .px(px(12.0))
            .children(
                self.feed
                    .shortcuts
                    .iter()
                    .take(8)
                    .enumerate()
                    .map(|(i, item)| {
                        let open = item.clone();
                        let id = format!("shortcut-{i}");
                        let shortcut = div()
                            .id(SharedString::from(id.clone()))
                            .group("shortcut")
                            .relative()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .w(px(shortcut_width))
                            .h(px(64.0))
                            .rounded(px(5.0))
                            .overflow_hidden()
                            .bg(rgb(0x2d2d2d))
                            .hover(|s| s.bg(rgb(0x454545)))
                            .cursor_pointer()
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.open(open.clone(), cx)),
                            )
                            .child(render_cover(
                                item.image.as_deref(),
                                64.0,
                                item.kind == "LikedSongs",
                                0x313131,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .pr(px(8.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_ellipsis()
                                    .child(item.title.clone()),
                            )
                            .when(item.playable, |s| {
                                s.child(
                                    div()
                                        .absolute()
                                        .right(px(8.0))
                                        .opacity(0.0)
                                        .group_hover("shortcut", |s| s.opacity(1.0))
                                        .child(self.play_button(
                                            item,
                                            format!("{id}-play"),
                                            40.0,
                                            cx,
                                        )),
                                )
                            });
                        track_menu(
                            shortcut,
                            format!("home-shortcut-menu-{i}"),
                            TrackMenuTarget::new(item.uri.clone(), item.title.clone()),
                            None,
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_row(&mut self, row: usize, cx: &Context<Self>) -> AnyElement {
        #[cfg(test)]
        self.rendered_rows.push(row);
        let layout = self
            .list_layout
            .expect("Home list layout is synchronized before layout");
        let last = row + 1 == layout.row_count();
        let (body, gap) = match layout.mode {
            HomeListMode::Loading => (
                if row == 0 {
                    div().text_color(rgb(0xb3b3b3)).child("Loading your Home…")
                } else {
                    div()
                        .h(px(200.0))
                        .w_full()
                        .rounded(px(8.0))
                        .bg(rgb(0x242424))
                }
                .into_any_element(),
                28.0,
            ),
            HomeListMode::Failed => (
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .p(px(12.0))
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::BOLD)
                            .child("Couldn't load Home"),
                    )
                    .child("Your session or Spotify's Home service may be temporarily unavailable.")
                    .child(
                        div()
                            .id("home-retry")
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| this.load(cx)))
                            .child("Try again"),
                    )
                    .into_any_element(),
                0.0,
            ),
            HomeListMode::Expanded(index) => {
                let shelf = &self.feed.shelves[index];
                let body = match row {
                    0 => div()
                        .id("home-shelf-back")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.expanded = None;
                            cx.notify();
                        }))
                        .child("Back to Home")
                        .into_any_element(),
                    1 => div()
                        .text_3xl()
                        .font_weight(FontWeight::BOLD)
                        .child(shelf.title.clone())
                        .into_any_element(),
                    _ if row < 2 + layout.grid_rows() => {
                        let start = (row - 2) * layout.columns;
                        let end = (start + layout.columns).min(shelf.items.len());
                        div()
                            .flex()
                            .children((start..end).map(|i| {
                                self.card(&shelf.items[i], format!("expanded-{i}"), 180.0, cx)
                            }))
                            .into_any_element()
                    }
                    _ if self.shelf_loading => {
                        div().child("Loading recommendations…").into_any_element()
                    }
                    _ => div()
                        .id("home-load-more")
                        .cursor_pointer()
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.expand_shelf(index, true, cx)),
                        )
                        .child("Load more")
                        .into_any_element(),
                };
                let gap = if row < 2 || row + 1 == 2 + layout.grid_rows() {
                    16.0
                } else {
                    0.0
                };
                (body, gap)
            }
            HomeListMode::Feed => {
                let panel_start = 1 + layout.shelves;
                let status_row = panel_start + layout.panels;
                let body = if row == 0 {
                    self.shortcuts(cx)
                } else if row < panel_start {
                    self.shelf(row - 1, &self.feed.shelves[row - 1], cx)
                } else if row < status_row {
                    self.panel(row - panel_start, &self.feed.panels[row - panel_start], cx)
                } else if row == status_row && layout.empty_feed() {
                    div()
                        .child("No recommendations available for this category yet.")
                        .into_any_element()
                } else {
                    div()
                        .px(px(12.0))
                        .py(px(24.0))
                        .text_sm()
                        .text_color(rgb(0xb3b3b3))
                        .child("Playback requires Spotify Premium.")
                        .into_any_element()
                };
                (body, 32.0)
            }
        };
        div()
            .w_full()
            .pb(px(if last { 0.0 } else { gap }))
            .child(body)
            .into_any_element()
    }
}

impl Render for HomeScreen {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_list_layout();
        #[cfg(test)]
        {
            self.rendered_rows.clear();
            self.rendered_cards.set(0);
            self.rendered_card_ids.borrow_mut().clear();
        }
        let entity = cx.entity().downgrade();
        let measure = canvas(
            move |bounds, _, cx| {
                let _ = entity.update(cx, |this, cx| {
                    let width: f32 = bounds.size.width.into();
                    if (this.width - width).abs() > 1.0 {
                        this.width = width;
                        cx.notify();
                    }
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let mut chips = vec![(String::new(), "All".to_string())];
        chips.extend(
            self.feed
                .chips
                .iter()
                .map(|c| (c.id.clone(), c.label.clone())),
        );
        div()
            .relative()
            .size_full()
            .min_w(px(0.0))
            .bg(rgb(0x181818))
            .rounded(px(8.0))
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(measure)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(32.0))
                    .py(px(16.0))
                    .bg(rgb(0x291c19))
                    .children(chips.into_iter().map(|(id, label)| {
                        let active = self.facet == id;
                        div()
                            .id(SharedString::from(format!("home-chip-{id}")))
                            .rounded_full()
                            .px(px(16.0))
                            .py(px(9.0))
                            .bg(rgb(if active { 0xffffff } else { 0x38302e }))
                            .text_color(rgb(if active { 0x000000 } else { 0xffffff }))
                            .cursor_pointer()
                            .hover(|s| s.opacity(0.85))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.facet != id {
                                    this.facet = id.clone();
                                    this.load(cx);
                                }
                            }))
                            .child(label)
                    })),
            )
            .children(self.message.clone().map(|message| {
                div()
                    .px(px(32.0))
                    .py(px(8.0))
                    .text_color(rgb(0xffce81))
                    .child(message)
            }))
            .child(
                div()
                    .id("home-scroll")
                    .relative()
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .child(
                        list(
                            self.list.clone(),
                            cx.processor(|this, row, _, cx| this.render_row(row, cx)),
                        )
                        .size_full()
                        .px(px(20.0))
                        .py(px(12.0)),
                    )
                    .child(list_scroll_overlay(
                        self.list.clone(),
                        self.scroll_state.clone(),
                        |_, _| {},
                    )),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn home_renders_scrollable_shelves_and_survives_resize(cx: &mut TestAppContext) {
        let (home, cx) = cx.add_window_view(|_, cx| {
            let mut home = HomeScreen::new(cx);
            let item = HomeItem {
                uri: "spotify:playlist:example".into(),
                title: "Example playlist".into(),
                subtitle: "Example description".into(),
                kind: "Playlist".into(),
                playable: true,
                image: None,
            };
            home.feed.shortcuts = vec![item.clone(); 8];
            home.feed.shelves = vec![
                HomeShelf {
                    title: "Recommendations".into(),
                    items: vec![item.clone(); 10],
                    ..Default::default()
                };
                3
            ];
            home.feed.panels = vec![HomePanel {
                label: "Made for you".into(),
                item,
                video_url: None,
                poster: None,
            }];
            home.shelf_scrolls = (0..3).map(|_| ScrollHandle::new()).collect();
            home.shelf_states = (0..3)
                .map(|_| SmoothScrollState::new(cx.entity_id()))
                .collect();
            home
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        home.read_with(cx, |home, _| {
            assert!(home.list.max_offset_for_scrollbar().height > px(0.0));
            assert!(home.shelf_scrolls[0].max_offset().width > px(0.0));
        });
        cx.simulate_resize(gpui::size(px(500.0), px(700.0)));
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        home.update(cx, |home, cx| {
            home.expanded = Some(0);
            cx.notify();
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        home.read_with(cx, |home, _| {
            assert!(home.list.max_offset_for_scrollbar().height > px(0.0))
        });
    }

    fn fixture(
        shelves: usize,
        cards: usize,
        panels: usize,
        cx: &mut Context<HomeScreen>,
    ) -> HomeScreen {
        let mut home = HomeScreen::new(cx);
        let item = HomeItem {
            uri: "spotify:playlist:offline-fixture".into(),
            title: "Offline recommendation".into(),
            subtitle: "A description that occupies the existing card subtitle area.".into(),
            kind: "Playlist".into(),
            playable: true,
            image: None,
        };
        home.feed.shortcuts = vec![item.clone(); 8];
        home.feed.shelves = (0..shelves)
            .map(|index| HomeShelf {
                title: format!("Recommendations {index}"),
                items: vec![item.clone(); cards],
                next_offset: Some(cards as u64),
                ..Default::default()
            })
            .collect();
        home.feed.panels = (0..panels)
            .map(|index| HomePanel {
                label: format!("Made for you {index}"),
                item: item.clone(),
                video_url: None,
                poster: None,
            })
            .collect();
        home.shelf_scrolls = (0..shelves).map(|_| ScrollHandle::new()).collect();
        home.shelf_states = (0..shelves)
            .map(|_| SmoothScrollState::new(cx.entity_id()))
            .collect();
        home
    }

    fn assert_row_in_view(home: &HomeScreen, row: usize) {
        let bounds = home
            .list
            .bounds_for_item(row)
            .expect("row has been measured");
        let viewport = home.list.viewport_bounds();
        assert!(
            bounds.bottom() > viewport.top(),
            "{row}: {bounds:?} / {viewport:?}"
        );
        assert!(
            bounds.top() < viewport.bottom(),
            "{row}: {bounds:?} / {viewport:?}"
        );
    }

    #[gpui::test]
    fn large_home_feed_builds_visible_shelves_and_distant_panels(cx: &mut TestAppContext) {
        let (home, cx) = cx.add_window_view(|_, cx| fixture(120, 10, 80, cx));
        cx.simulate_resize(gpui::size(px(900.0), px(600.0)));
        for _ in 0..2 {
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
        }
        home.read_with(cx, |home, _| {
            assert!(home.rendered_rows.len() < 30);
            assert!(home.rendered_cards.get() < 150);
            assert_row_in_view(home, 0);
        });
        let distant_panel = 1 + 120 + 60;
        home.update(cx, |home, cx| {
            home.list.scroll_to(ListOffset {
                item_ix: distant_panel,
                offset_in_item: px(0.0),
            });
            cx.notify();
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        home.read_with(cx, |home, _| {
            assert!(home.rendered_rows.contains(&distant_panel));
            assert!(home.rendered_rows.len() < 30);
            assert_row_in_view(home, distant_panel);
        });
        home.update(cx, |home, cx| {
            home.list.scroll_to(ListOffset {
                item_ix: home.list.item_count() - 1,
                offset_in_item: px(0.0),
            });
            cx.notify();
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        home.read_with(cx, |home, _| {
            let footer = home.list.item_count() - 1;
            assert!(home.rendered_rows.contains(&footer));
            assert_row_in_view(home, footer);
            let bounds = home.list.bounds_for_item(footer).unwrap();
            assert!(bounds.bottom() <= home.list.viewport_bounds().bottom());
        });
    }

    #[gpui::test]
    fn expanded_home_virtualizes_rows_and_reflows_distant_scroll_on_resize(
        cx: &mut TestAppContext,
    ) {
        let (home, cx) = cx.add_window_view(|_, cx| {
            let mut home = fixture(1, 2000, 0, cx);
            home.expanded = Some(0);
            home
        });
        cx.simulate_resize(gpui::size(px(900.0), px(600.0)));
        for _ in 0..2 {
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
        }
        home.read_with(cx, |home, _| {
            assert_eq!(home.list_layout.unwrap().columns, 4);
            assert!(home.rendered_cards.get() < 100);
            assert_row_in_view(home, 0);
        });
        home.update(cx, |home, cx| {
            home.list.scroll_to(ListOffset {
                item_ix: 202,
                offset_in_item: px(0.0),
            });
            cx.notify();
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        home.read_with(cx, |home, _| {
            assert_row_in_view(home, 202);
            assert!(
                home.rendered_card_ids
                    .borrow()
                    .iter()
                    .any(|id| id == "expanded-800")
            );
            assert!(home.rendered_cards.get() < 100);
        });
        cx.simulate_resize(gpui::size(px(500.0), px(600.0)));
        for _ in 0..2 {
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
        }
        home.read_with(cx, |home, _| {
            assert_eq!(home.list_layout.unwrap().columns, 2);
            assert_row_in_view(home, 402);
            assert!(
                home.rendered_card_ids
                    .borrow()
                    .iter()
                    .any(|id| id == "expanded-800")
            );
            assert!(home.rendered_cards.get() < 100);
        });
        home.update(cx, |home, cx| {
            home.feed.shelves[0].items.truncate(25);
            home.feed.shelves[0].next_offset = None;
            home.content_revision += 1;
            cx.notify();
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        home.read_with(cx, |home, _| {
            assert_eq!(home.list.item_count(), 15);
            assert!(home.rendered_rows.iter().all(|row| *row < 15));
            assert!(
                home.rendered_card_ids
                    .borrow()
                    .iter()
                    .any(|id| id == "expanded-24")
            );
            assert_row_in_view(home, 14);
            let end = home.list.bounds_for_item(14).unwrap();
            assert!(end.bottom() <= home.list.viewport_bounds().bottom());
        });
    }

    #[gpui::test]
    fn large_home_shelf_builds_only_clipped_cards_and_reaches_distant_cards(
        cx: &mut TestAppContext,
    ) {
        let (home, cx) = cx.add_window_view(|_, cx| fixture(1, 500, 0, cx));
        cx.simulate_resize(gpui::size(px(900.0), px(600.0)));
        for _ in 0..2 {
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
        }
        home.read_with(cx, |home, _| {
            assert!(home.rendered_cards.get() < 50);
            assert!(home.shelf_scrolls[0].max_offset().width > px(90_000.0));
            assert_row_in_view(home, 1);
        });
        home.update(cx, |home, cx| {
            home.shelf_scrolls[0].set_offset(point(px(-204.0 * 300.0), px(0.0)));
            cx.notify();
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        home.read_with(cx, |home, _| {
            assert!(home.rendered_cards.get() < 50);
            assert!(
                home.rendered_card_ids
                    .borrow()
                    .iter()
                    .any(|id| id == "shelf-0-300")
            );
            assert!(
                !home
                    .rendered_card_ids
                    .borrow()
                    .iter()
                    .any(|id| id == "shelf-0-100")
            );
            assert_row_in_view(home, 1);
        });
    }
}
